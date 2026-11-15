// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use application_core::ai::{self, Hardware};
use application_core::document::{Adjustments, EditState, History};
use application_core::jobs::{JobError, Jobs};
use application_core::models::Registry;
use application_core::portrait::{self, FaceStatus};
use application_core::info::{self, FileInfo};
use application_core::settings::{self, EditorMode, LanguagePreference, Settings, ThemePreference};
use application_core::{AiSession, EngineError, Face, Portrait, Source};
use tauri::ipc::Response;
use tauri::path::BaseDirectory;
use tauri::{Emitter, Manager, State};

/// Longer side of the overview frame sent when an image opens. The canvas
/// shows it immediately, then asks for sharp renders of what is on screen.
const OVERVIEW_MAX_DIMENSION: u32 = 2048;

/// Largest render the UI may request, per side, in device pixels.
const MAX_RENDER_SIDE: u32 = 8192;

/// Longer side of recent-file thumbnails on the Home screen.
const THUMBNAIL_MAX_DIMENSION: u32 = 320;

const NOTICES_FILE: &str = "THIRD_PARTY_NOTICES.txt";

struct OpenDocument {
    /// Increases with every opened image, so late face results for an
    /// image that was replaced are dropped.
    serial: u64,
    /// The original image at full resolution; never modified.
    source: Arc<Source>,
    /// Committed adjustments and their undo history.
    history: History,
    /// Faces found in the image (empty until detection finishes).
    faces: Arc<Vec<Face>>,
    faces_status: FaceStatus,
}

impl OpenDocument {
    /// What a render needs: the source and the faces to retouch.
    fn render_inputs(&self) -> (Arc<Source>, Arc<Vec<Face>>) {
        (self.source.clone(), self.faces.clone())
    }
}

/// Sent to the UI as a "faces" event when detection for an image ends.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FacesEvent {
    serial: u64,
    status: FaceStatus,
}

struct AppState {
    settings_path: PathBuf,
    settings: Mutex<Settings>,
    /// True when the settings file was damaged at startup and was reset.
    settings_recovered: bool,
    /// The image open in the editor.
    document: Mutex<Option<OpenDocument>>,
    /// Highest render request id seen; older requests are dropped unanswered.
    latest_render: AtomicU64,
    /// Background work; updates go to the UI as "job-update" events.
    jobs: Arc<Jobs>,
    /// Folder holding the model registry (bundled with the app).
    models_dir: PathBuf,
    /// Serial of the last opened image.
    document_serial: AtomicU64,
    /// The face detector, loaded on first use and kept.
    detector: Mutex<Option<Arc<AiSession>>>,
}

impl AppState {
    fn face_detector(&self) -> Result<Arc<AiSession>, String> {
        let mut slot = self.detector.lock().map_err(|_| "internal".to_string())?;
        if let Some(detector) = slot.as_ref() {
            return Ok(detector.clone());
        }
        let registry = Registry::load(&self.models_dir).map_err(|e| e.code().to_string())?;
        let detector = Arc::new(portrait::open_detector(&registry)?);
        *slot = Some(detector.clone());
        Ok(detector)
    }
}

impl AppState {
    /// Apply a change to the settings and save them. Saving errors are
    /// reported but the in-memory change is kept, so the app keeps working.
    fn update(&self, change: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
        let mut settings = self.settings.lock().map_err(|_| "internal".to_string())?;
        change(&mut settings);
        settings::save(&self.settings_path, &settings).map_err(|_| "settings_save_failed".to_string())?;
        Ok(settings.clone())
    }
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct Startup {
    settings: Settings,
    settings_recovered: bool,
    version: String,
}

#[tauri::command]
fn startup(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Startup, String> {
    let settings = state.settings.lock().map_err(|_| "internal".to_string())?.clone();
    Ok(Startup {
        settings,
        settings_recovered: state.settings_recovered,
        version: app.package_info().version.to_string(),
    })
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<Settings, String> {
    Ok(state.settings.lock().map_err(|_| "internal".to_string())?.clone())
}

fn engine_error_code(error: EngineError) -> String {
    match error {
        EngineError::OutOfMemory => "out_of_memory",
        EngineError::InvalidArgument => "invalid_request",
        EngineError::DecodeFailed
        | EngineError::Internal
        | EngineError::ModelInvalid
        | EngineError::InferenceFailed => "internal",
    }
    .to_string()
}

/// Open an image for editing at full resolution and return an overview
/// frame of the whole image (see `application_core::encode_frame`). Work runs
/// on a worker thread so the UI stays responsive. Errors are stable codes
/// that the UI maps to localized messages.
#[tauri::command]
async fn open_image(path: String, app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Response, String> {
    let file = PathBuf::from(&path);
    let (source, overview) = tauri::async_runtime::spawn_blocking(move || {
        let source = application_core::open_document(&file).map_err(|e| e.code().to_string())?;
        let overview = source.render_fit(OVERVIEW_MAX_DIMENSION).map_err(engine_error_code)?;
        Ok::<_, String>((source, overview))
    })
    .await
    .map_err(|_| "internal".to_string())??;

    let source = Arc::new(source);
    let serial = state.document_serial.fetch_add(1, Ordering::SeqCst) + 1;
    *state.document.lock().map_err(|_| "internal".to_string())? = Some(OpenDocument {
        serial,
        source: source.clone(),
        history: History::default(),
        faces: Arc::new(Vec::new()),
        faces_status: FaceStatus::Detecting,
    });
    // Find faces in the background; the image is usable meanwhile.
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let result = state
            .face_detector()
            .and_then(|detector| portrait::detect(&source, &detector));
        let status = match &result {
            Ok(faces) => FaceStatus::Ready { count: faces.len() },
            Err(code) => FaceStatus::Unavailable { code: code.clone() },
        };
        if let Ok(mut document) = state.document.lock() {
            if let Some(document) = document.as_mut().filter(|d| d.serial == serial) {
                document.faces = Arc::new(result.unwrap_or_default());
                document.faces_status = status.clone();
                let _ = app.emit("faces", FacesEvent { serial, status });
            }
        }
    });
    // The image opened; failing to save the recent list must not block it.
    let _ = state.update(|s| s.add_recent(&path, now_seconds()));
    Ok(Response::new(application_core::encode_frame(&overview)))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenderRequest {
    /// Increases with every request; a newer request makes older ones moot.
    id: u64,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    out_width: u32,
    out_height: u32,
    /// The adjustments to show, which may be uncommitted while a slider is
    /// being dragged.
    adjustments: Adjustments,
    /// Leftmost output columns shown without adjustments (before/after).
    original_columns: u32,
}

/// Render the part of the open image that is on screen. Requests overtaken
/// by a newer one fail with "superseded", which the UI ignores.
#[tauri::command]
async fn render_view(request: RenderRequest, state: State<'_, AppState>) -> Result<Response, String> {
    if request.out_width == 0
        || request.out_height == 0
        || request.out_width > MAX_RENDER_SIDE
        || request.out_height > MAX_RENDER_SIDE
    {
        return Err("invalid_request".to_string());
    }
    state.latest_render.fetch_max(request.id, Ordering::SeqCst);
    let (source, faces) = state
        .document
        .lock()
        .map_err(|_| "internal".to_string())?
        .as_ref()
        .map(OpenDocument::render_inputs)
        .ok_or_else(|| "no_document".to_string())?;

    let latest = state.inner().latest_render.load(Ordering::SeqCst);
    if request.id < latest {
        return Err("superseded".to_string());
    }
    let smoothing = request.adjustments.clamped().skin_smoothing;
    let adjustments = request.adjustments.to_engine();
    let image = tauri::async_runtime::spawn_blocking(move || {
        source.render_adjusted(
            request.x,
            request.y,
            request.width,
            request.height,
            request.out_width,
            request.out_height,
            Some(Portrait { faces: &faces, smoothing }),
            Some(&adjustments),
            request.original_columns,
        )
    })
    .await
    .map_err(|_| "internal".to_string())?
    .map_err(engine_error_code)?;

    if request.id < state.latest_render.load(Ordering::SeqCst) {
        return Err("superseded".to_string());
    }
    Ok(Response::new(application_core::encode_frame(&image)))
}

/// Render the whole open image, with adjustments, at overview size. The
/// canvas draws it under sharper renders, e.g. at the edges while panning.
#[tauri::command]
async fn render_overview(adjustments: Adjustments, state: State<'_, AppState>) -> Result<Response, String> {
    let (source, faces) = state
        .document
        .lock()
        .map_err(|_| "internal".to_string())?
        .as_ref()
        .map(OpenDocument::render_inputs)
        .ok_or_else(|| "no_document".to_string())?;
    let smoothing = adjustments.clamped().skin_smoothing;
    let adjustments = adjustments.to_engine();
    let image = tauri::async_runtime::spawn_blocking(move || {
        let longer = source.width().max(source.height()).max(1);
        let scale = (OVERVIEW_MAX_DIMENSION as f64 / longer as f64).min(1.0);
        let out_w = ((source.width() as f64 * scale).round() as u32).max(1);
        let out_h = ((source.height() as f64 * scale).round() as u32).max(1);
        source.render_adjusted(
            0.0,
            0.0,
            source.width() as f64,
            source.height() as f64,
            out_w,
            out_h,
            Some(Portrait { faces: &faces, smoothing }),
            Some(&adjustments),
            0,
        )
    })
    .await
    .map_err(|_| "internal".to_string())?
    .map_err(engine_error_code)?;
    Ok(Response::new(application_core::encode_frame(&image)))
}

/// Run `change` on the open document's history and report the result.
fn with_history(state: &AppState, change: impl FnOnce(&mut History)) -> Result<EditState, String> {
    let mut document = state.document.lock().map_err(|_| "internal".to_string())?;
    let document = document.as_mut().ok_or_else(|| "no_document".to_string())?;
    change(&mut document.history);
    Ok(document.history.state())
}

#[tauri::command]
fn edit_state(state: State<'_, AppState>) -> Result<EditState, String> {
    with_history(&state, |_| {})
}

/// Record new adjustments as one undoable step.
#[tauri::command]
fn commit_adjustments(adjustments: Adjustments, state: State<'_, AppState>) -> Result<EditState, String> {
    with_history(&state, |history| history.commit(adjustments))
}

#[tauri::command]
fn undo(state: State<'_, AppState>) -> Result<EditState, String> {
    with_history(&state, |history| {
        history.undo();
    })
}

#[tauri::command]
fn redo(state: State<'_, AppState>) -> Result<EditState, String> {
    with_history(&state, |history| {
        history.redo();
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct FacesReply {
    serial: u64,
    status: FaceStatus,
}

/// Face detection state of the open image (also sent as "faces" events).
#[tauri::command]
fn faces_status(state: State<'_, AppState>) -> Result<FacesReply, String> {
    let document = state.document.lock().map_err(|_| "internal".to_string())?;
    let document = document.as_ref().ok_or_else(|| "no_document".to_string())?;
    Ok(FacesReply {
        serial: document.serial,
        status: document.faces_status.clone(),
    })
}

/// One-click enhancement: replace the light and colour values with values
/// suggested from the image (aimed at faces when there are any), as one
/// undoable step. Skin smoothing is kept.
#[tauri::command]
async fn auto_enhance(state: State<'_, AppState>) -> Result<EditState, String> {
    let (source, faces) = state
        .document
        .lock()
        .map_err(|_| "internal".to_string())?
        .as_ref()
        .map(OpenDocument::render_inputs)
        .ok_or_else(|| "no_document".to_string())?;
    let suggested = tauri::async_runtime::spawn_blocking(move || source.auto_adjust(&faces))
        .await
        .map_err(|_| "internal".to_string())?
        .map_err(engine_error_code)?;
    with_history(&state, |history| history.commit(history.current().with_colour(suggested)))
}

#[tauri::command]
fn hardware_info() -> Hardware {
    ai::hardware()
}

/// Run the AI self-test in the background; progress and the report arrive
/// as "job-update" events. Returns the job id.
#[tauri::command]
fn start_ai_self_test(state: State<'_, AppState>) -> u64 {
    let models_dir = state.models_dir.clone();
    state.jobs.start(move |ctx| {
        let registry = Registry::load(&models_dir).map_err(|e| JobError::new(e.code(), e.to_string()))?;
        let report = ai::self_test(&registry, ctx, 1024)?;
        serde_json::to_value(report).map_err(|e| JobError::new("internal", e.to_string()))
    })
}

#[tauri::command]
fn cancel_job(id: u64, state: State<'_, AppState>) -> bool {
    state.jobs.cancel(id)
}

/// Decode a small preview of a file for the recent-files list.
#[tauri::command]
async fn thumbnail(path: String) -> Result<Response, String> {
    let image = tauri::async_runtime::spawn_blocking(move || {
        application_core::open_image(&PathBuf::from(path), THUMBNAIL_MAX_DIMENSION)
    })
    .await
    .map_err(|_| "internal".to_string())?
    .map_err(|e| e.code().to_string())?;
    Ok(Response::new(application_core::encode_frame(&image)))
}

#[tauri::command]
fn file_info(path: String) -> Result<FileInfo, String> {
    info::file_info(&PathBuf::from(path)).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => "not_found".to_string(),
        _ => "read_failed".to_string(),
    })
}

#[tauri::command]
fn set_theme(theme: ThemePreference, state: State<'_, AppState>) -> Result<Settings, String> {
    state.update(|s| s.theme = theme)
}

#[tauri::command]
fn set_language(language: LanguagePreference, state: State<'_, AppState>) -> Result<Settings, String> {
    state.update(|s| s.language = language)
}

#[tauri::command]
fn set_editor_mode(mode: EditorMode, state: State<'_, AppState>) -> Result<Settings, String> {
    state.update(|s| s.editor_mode = mode)
}

#[tauri::command]
fn remove_recent(path: String, state: State<'_, AppState>) -> Result<Settings, String> {
    state.update(|s| s.remove_recent(&path))
}

#[tauri::command]
fn clear_recent(state: State<'_, AppState>) -> Result<Settings, String> {
    state.update(|s| s.clear_recent())
}

/// Full text of the third-party license notices bundled with the app.
#[tauri::command]
fn third_party_notices(app: tauri::AppHandle) -> Result<String, String> {
    let path = app
        .path()
        .resolve(NOTICES_FILE, BaseDirectory::Resource)
        .map_err(|_| "not_found".to_string())?;
    std::fs::read_to_string(path).map_err(|_| "not_found".to_string())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let loaded = settings::load(&settings_path);
            let models_dir = app.path().resolve("models", BaseDirectory::Resource)?;
            let handle = app.handle().clone();
            let jobs = Jobs::new(move |update| {
                let _ = handle.emit("job-update", update);
            });
            app.manage(AppState {
                settings_path,
                settings: Mutex::new(loaded.settings),
                settings_recovered: loaded.recovered_from_damage,
                document: Mutex::new(None),
                latest_render: AtomicU64::new(0),
                jobs,
                models_dir,
                document_serial: AtomicU64::new(0),
                detector: Mutex::new(None),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            startup,
            get_settings,
            open_image,
            render_view,
            render_overview,
            edit_state,
            commit_adjustments,
            undo,
            redo,
            faces_status,
            auto_enhance,
            thumbnail,
            file_info,
            set_theme,
            set_language,
            set_editor_mode,
            hardware_info,
            start_ai_self_test,
            cancel_job,
            remove_recent,
            clear_recent,
            third_party_notices,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AI Beauty Studio");
}
