// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use application_core::ai::{self, Hardware};
use application_core::background::{self as separation, ModelSlot};
use application_core::document::{Adjustments, EditState, History};
use application_core::export::{self, ExportOptions};
use application_core::files::FileRef;
use application_core::project::{self, ProjectError};
use application_core::recovery::{Recovery, RecoveryInfo};
use application_core::jobs::{JobError, Jobs};
use application_core::models::Registry;
use application_core::portrait::{self, FaceStatus};
use application_core::info::{self, FileInfo};
use application_core::settings::{self, EditorMode, LanguagePreference, Settings, ThemePreference};
use application_core::{AiSession, EngineError, Face, Matte, Portrait, Shape, Source};
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
    /// Where the original photo is, and its fingerprint.
    original: FileRef,
    /// The project file this document was opened from or saved to.
    project_path: Option<PathBuf>,
    /// Committed adjustments and their undo history.
    history: History,
    /// Faces found in the image (empty until detection finishes).
    faces: Arc<Vec<Face>>,
    faces_status: FaceStatus,
    /// Subject/background separation, once it has been run.
    matte: Option<Arc<Matte>>,
    /// 3D shape for lighting, once it has been estimated.
    shape: Option<Arc<Shape>>,
    /// Replacement background images added to this document; number n in
    /// the adjustments is element n - 1.
    background_images: Vec<BackgroundImage>,
}

/// A replacement background image; `source` is None when a project refers
/// to an image that is missing.
#[derive(Clone)]
struct BackgroundImage {
    file: FileRef,
    source: Option<Arc<Source>>,
}

/// Everything a render needs, taken from the document under its lock and
/// used after releasing it.
struct RenderInputs {
    source: Arc<Source>,
    faces: Arc<Vec<Face>>,
    matte: Option<Arc<Matte>>,
    shape: Option<Arc<Shape>>,
    images: Vec<Option<Arc<Source>>>,
}

impl OpenDocument {
    fn render_inputs(&self) -> RenderInputs {
        RenderInputs {
            source: self.source.clone(),
            faces: self.faces.clone(),
            matte: self.matte.clone(),
            shape: self.shape.clone(),
            images: self.background_images.iter().map(|b| b.source.clone()).collect(),
        }
    }

    /// The parts of the document to save, taken under its lock.
    fn save_parts(&self) -> SaveParts {
        SaveParts {
            inputs: self.render_inputs(),
            original: self.original.clone(),
            history: self.history.clone(),
            serial: self.serial,
            faces_known: matches!(self.faces_status, FaceStatus::Ready { .. }),
            backgrounds: self.background_images.iter().map(|b| b.file.clone()).collect(),
            project_path: self.project_path.clone(),
        }
    }
}

/// A copy of what is saved, so writing happens without holding the lock.
struct SaveParts {
    serial: u64,
    inputs: RenderInputs,
    original: FileRef,
    history: History,
    faces_known: bool,
    backgrounds: Vec<FileRef>,
    project_path: Option<PathBuf>,
}

/// Longer side of the preview stored in projects.
const PREVIEW_MAX_DIMENSION: u32 = 512;

impl SaveParts {
    fn preview(&self) -> Option<application_core::RgbaImage> {
        let (w, h) = (self.inputs.source.width(), self.inputs.source.height());
        let (pw, ph) = export::output_size(w, h, Some(PREVIEW_MAX_DIMENSION));
        self.inputs
            .render(&self.history.current(), 0.0, 0.0, w as f64, h as f64, pw, ph, 0)
            .ok()
    }

    fn save(&self, path: &Path, with_preview: bool) -> Result<(), ProjectError> {
        let preview = if with_preview { self.preview() } else { None };
        project::save(path, &self.snapshot(preview.as_ref()))
    }

    fn snapshot<'a>(&'a self, preview: Option<&'a application_core::RgbaImage>) -> project::Snapshot<'a> {
        project::Snapshot {
            original: &self.original,
            image_size: (self.inputs.source.width(), self.inputs.source.height()),
            history: &self.history,
            faces: self.faces_known.then_some(self.inputs.faces.as_slice()),
            matte: self.inputs.matte.as_deref(),
            shape: self.inputs.shape.as_deref(),
            background_images: &self.backgrounds,
            preview,
            saved_at: now_seconds(),
        }
    }
}

impl RenderInputs {
    /// Render with every edit in `adjustments` applied.
    #[allow(clippy::too_many_arguments)]
    fn render(
        &self,
        adjustments: &Adjustments,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        out_w: u32,
        out_h: u32,
        original_columns: u32,
    ) -> Result<application_core::RgbaImage, EngineError> {
        let clamped = adjustments.clamped();
        let colour = clamped.to_engine();
        let image = (clamped.background_image as usize)
            .checked_sub(1)
            .and_then(|i| self.images.get(i))
            .and_then(|s| s.as_deref());
        let background = self.matte.as_deref().and_then(|m| clamped.background(m, image));
        let light = clamped.light(self.matte.as_deref(), self.shape.as_deref());
        self.source.render_adjusted(
            x,
            y,
            w,
            h,
            out_w,
            out_h,
            Some(Portrait {
                faces: &self.faces,
                smoothing: clamped.skin_smoothing,
            }),
            Some(&colour),
            background,
            light,
            original_columns,
        )
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
    /// The background separation model, loaded on first use and kept.
    background_model: Arc<ModelSlot>,
    /// The depth model for 3D lighting, loaded on first use and kept.
    depth_model: Arc<ModelSlot>,
    /// Automatic recovery of unsaved work.
    recovery: Recovery,
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
/// What a newly opened document starts with.
struct Opening {
    source: Arc<Source>,
    original: FileRef,
    project_path: Option<PathBuf>,
    history: History,
    /// Known faces (from a project); None starts detection.
    faces: Option<Vec<Face>>,
    matte: Option<Matte>,
    shape: Option<Shape>,
    background_images: Vec<BackgroundImage>,
}

/// Make `opening` the open document and start face detection if needed.
fn install(app: &tauri::AppHandle, state: &AppState, opening: Opening) -> Result<u64, String> {
    let serial = state.document_serial.fetch_add(1, Ordering::SeqCst) + 1;
    let detect = opening.faces.is_none();
    let (faces, faces_status) = match opening.faces {
        Some(faces) => {
            let count = faces.len();
            (faces, FaceStatus::Ready { count })
        }
        None => (Vec::new(), FaceStatus::Detecting),
    };
    let source = opening.source.clone();
    *state.document.lock().map_err(|_| "internal".to_string())? = Some(OpenDocument {
        serial,
        source: opening.source,
        original: opening.original,
        project_path: opening.project_path,
        history: opening.history,
        faces: Arc::new(faces),
        faces_status,
        matte: opening.matte.map(Arc::new),
        shape: opening.shape.map(Arc::new),
        background_images: opening.background_images,
    });
    // The previous document's recovery no longer applies.
    state.recovery.discard();
    if detect {
        // Find faces in the background; the image is usable meanwhile.
        let app = app.clone();
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
            };
        });
    }
    Ok(serial)
}

#[tauri::command]
async fn open_image(path: String, app: tauri::AppHandle, state: State<'_, AppState>) -> Result<Response, String> {
    let file = PathBuf::from(&path);
    let (source, overview, original) = tauri::async_runtime::spawn_blocking(move || {
        let source = application_core::open_document(&file).map_err(|e| e.code().to_string())?;
        let overview = source.render_fit(OVERVIEW_MAX_DIMENSION).map_err(engine_error_code)?;
        let original = FileRef::of(&file).map_err(|_| "read_failed".to_string())?;
        Ok::<_, String>((source, overview, original))
    })
    .await
    .map_err(|_| "internal".to_string())??;

    install(
        &app,
        &state,
        Opening {
            source: Arc::new(source),
            original,
            project_path: None,
            history: History::default(),
            faces: None,
            matte: None,
            shape: None,
            background_images: Vec::new(),
        },
    )?;
    // The image opened; failing to save the recent list must not block it.
    let _ = state.update(|s| s.add_recent(&path, now_seconds()));
    Ok(Response::new(application_core::encode_frame(&overview)))
}

/// Read a project and open its photo with everything restored. Returns the
/// overview frame, like `open_image`.
fn load_project(path: &Path, original: Option<&Path>) -> Result<(Opening, application_core::RgbaImage), String> {
    let loaded = project::load(path, original).map_err(|e| e.code().to_string())?;
    let source = application_core::open_document(&loaded.original_path).map_err(|e| e.code().to_string())?;
    let overview = source.render_fit(OVERVIEW_MAX_DIMENSION).map_err(engine_error_code)?;
    let background_images = loaded
        .background_images
        .iter()
        .map(|b| BackgroundImage {
            file: FileRef {
                path: b.found.as_ref().map_or_else(|| b.file.path.clone(), |p| p.to_string_lossy().into_owned()),
                ..b.file.clone()
            },
            source: b
                .found
                .as_ref()
                .and_then(|p| application_core::open_document(p).ok())
                .map(Arc::new),
        })
        .collect();
    let original = FileRef {
        path: loaded.original_path.to_string_lossy().into_owned(),
        relative_path: None,
        ..loaded.file.original.clone()
    };
    Ok((
        Opening {
            source: Arc::new(source),
            original,
            project_path: Some(path.to_path_buf()),
            history: loaded.history,
            faces: loaded.faces,
            matte: loaded.matte,
            shape: loaded.shape,
            background_images,
        },
        overview,
    ))
}

/// Open a project. `original` is where the user says the photo is, when
/// the project could not find it (error "original_missing").
#[tauri::command]
async fn open_project(
    path: String,
    original: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Response, String> {
    let file = PathBuf::from(&path);
    let (opening, overview) =
        tauri::async_runtime::spawn_blocking(move || load_project(&file, original.as_deref().map(Path::new)))
            .await
            .map_err(|_| "internal".to_string())??;
    install(&app, &state, opening)?;
    let _ = state.update(|s| s.add_recent(&path, now_seconds()));
    Ok(Response::new(application_core::encode_frame(&overview)))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSummary {
    original_path: String,
    saved_at: u64,
}

/// What a project refers to, without opening it.
#[tauri::command]
fn inspect_project(path: String) -> Result<ProjectSummary, String> {
    let file = project::inspect(Path::new(&path)).map_err(|e| e.code().to_string())?;
    Ok(ProjectSummary {
        original_path: file.original.path,
        saved_at: file.saved_at,
    })
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DocumentStatus {
    serial: u64,
    original_path: String,
    project_path: Option<String>,
    /// Background images a project refers to that could not be found.
    missing_backgrounds: usize,
}

#[tauri::command]
fn document_status(state: State<'_, AppState>) -> Result<DocumentStatus, String> {
    let document = state.document.lock().map_err(|_| "internal".to_string())?;
    let document = document.as_ref().ok_or_else(|| "no_document".to_string())?;
    Ok(DocumentStatus {
        serial: document.serial,
        original_path: document.original.path.clone(),
        project_path: document.project_path.as_ref().map(|p| p.to_string_lossy().into_owned()),
        missing_backgrounds: document.background_images.iter().filter(|b| b.source.is_none()).count(),
    })
}

/// Save the open document as a project: to `path`, or to the project it
/// came from. Returns the path saved to.
#[tauri::command]
async fn save_project(path: Option<String>, state: State<'_, AppState>) -> Result<String, String> {
    let parts = {
        let document = state.document.lock().map_err(|_| "internal".to_string())?;
        document.as_ref().ok_or_else(|| "no_document".to_string())?.save_parts()
    };
    let target = path
        .map(PathBuf::from)
        .or_else(|| parts.project_path.clone())
        .ok_or_else(|| "needs_path".to_string())?;
    let protected: Vec<String> = std::iter::once(parts.original.path.clone())
        .chain(parts.backgrounds.iter().map(|b| b.path.clone()))
        .collect();
    if protected.iter().any(|p| application_core::files::same_file(&target, Path::new(p))) {
        return Err("export_would_overwrite_original".to_string());
    }
    let serial = parts.serial;
    let destination = target.clone();
    tauri::async_runtime::spawn_blocking(move || parts.save(&destination, true))
        .await
        .map_err(|_| "internal".to_string())?
        .map_err(|e| e.code().to_string())?;
    if let Some(document) = state.document.lock().map_err(|_| "internal".to_string())?.as_mut() {
        if document.serial == serial {
            document.project_path = Some(target.clone());
        }
    }
    state.recovery.discard();
    let saved = target.to_string_lossy().into_owned();
    let _ = state.update(|s| s.add_recent(&saved, now_seconds()));
    Ok(saved)
}

/// Save the open document to the recovery slot (called periodically by the
/// UI while there are unsaved changes).
#[tauri::command]
async fn autosave(state: State<'_, AppState>) -> Result<(), String> {
    let parts = {
        let document = state.document.lock().map_err(|_| "internal".to_string())?;
        match document.as_ref() {
            Some(document) => document.save_parts(),
            None => return Ok(()),
        }
    };
    let recovery = state.recovery.clone();
    tauri::async_runtime::spawn_blocking(move || recovery.save(&parts.snapshot(None), parts.project_path.as_deref()))
        .await
        .map_err(|_| "internal".to_string())?
        .map_err(|e| e.code().to_string())
}

/// Unsaved work from a session that ended unexpectedly, if any.
#[tauri::command]
fn recovery_status(state: State<'_, AppState>) -> Option<RecoveryInfo> {
    state.recovery.pending()
}

/// Open the recovered work (as unsaved changes to its project, if it had
/// one). `original` as for `open_project`.
#[tauri::command]
async fn restore_recovery(
    original: Option<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Response, String> {
    let info = state.recovery.pending().ok_or_else(|| "not_found".to_string())?;
    let file = state.recovery.project_path();
    let (mut opening, overview) =
        tauri::async_runtime::spawn_blocking(move || load_project(&file, original.as_deref().map(Path::new)))
            .await
            .map_err(|_| "internal".to_string())??;
    opening.project_path = info.project_path.map(PathBuf::from);
    install(&app, &state, opening)?;
    Ok(Response::new(application_core::encode_frame(&overview)))
}

#[tauri::command]
fn discard_recovery(state: State<'_, AppState>) {
    state.recovery.discard();
}

/// Export the edited image to `path` as a background job; returns the job
/// id. Refused at once if `path` is the original photo.
#[tauri::command]
fn export_image(path: String, options: ExportOptions, state: State<'_, AppState>) -> Result<u64, String> {
    let parts = {
        let document = state.document.lock().map_err(|_| "internal".to_string())?;
        document.as_ref().ok_or_else(|| "no_document".to_string())?.save_parts()
    };
    let target = PathBuf::from(&path);
    let protected: Vec<PathBuf> = std::iter::once(PathBuf::from(&parts.original.path))
        .chain(parts.backgrounds.iter().map(|b| PathBuf::from(&b.path)))
        .collect();
    let protected_refs: Vec<&Path> = protected.iter().map(PathBuf::as_path).collect();
    export::check_target(&target, &protected_refs).map_err(|e| e.code().to_string())?;
    Ok(state.jobs.start(move |ctx| {
        let (w, h) = (parts.inputs.source.width(), parts.inputs.source.height());
        let (out_w, out_h) = export::output_size(w, h, options.long_side);
        ctx.progress(0.05, "render");
        let image = parts
            .inputs
            .render(&parts.history.current(), 0.0, 0.0, w as f64, h as f64, out_w, out_h, 0)
            .map_err(|e| JobError::new(&engine_error_code(e), "render"))?;
        ctx.check()?;
        ctx.progress(0.6, "encode");
        let bytes = export::encode(&image, &options).map_err(|e| JobError::new(e.code(), e.to_string()))?;
        ctx.check()?;
        ctx.progress(0.9, "write");
        let protected_refs: Vec<&Path> = protected.iter().map(PathBuf::as_path).collect();
        export::write(&target, &bytes, &protected_refs).map_err(|e| JobError::new(e.code(), e.to_string()))?;
        Ok(serde_json::json!({
            "path": target.to_string_lossy(),
            "width": out_w,
            "height": out_h,
            "bytes": bytes.len(),
        }))
    }))
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
    let inputs = state
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
    let image = tauri::async_runtime::spawn_blocking(move || {
        inputs.render(
            &request.adjustments,
            request.x,
            request.y,
            request.width,
            request.height,
            request.out_width,
            request.out_height,
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
    let inputs = state
        .document
        .lock()
        .map_err(|_| "internal".to_string())?
        .as_ref()
        .map(OpenDocument::render_inputs)
        .ok_or_else(|| "no_document".to_string())?;
    let image = tauri::async_runtime::spawn_blocking(move || {
        let (width, height) = (inputs.source.width(), inputs.source.height());
        let longer = width.max(height).max(1);
        let scale = (OVERVIEW_MAX_DIMENSION as f64 / longer as f64).min(1.0);
        let out_w = ((width as f64 * scale).round() as u32).max(1);
        let out_h = ((height as f64 * scale).round() as u32).max(1);
        inputs.render(&adjustments, 0.0, 0.0, width as f64, height as f64, out_w, out_h, 0)
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
    let inputs = state
        .document
        .lock()
        .map_err(|_| "internal".to_string())?
        .as_ref()
        .map(OpenDocument::render_inputs)
        .ok_or_else(|| "no_document".to_string())?;
    let suggested = tauri::async_runtime::spawn_blocking(move || inputs.source.auto_adjust(&inputs.faces))
        .await
        .map_err(|_| "internal".to_string())?
        .map_err(engine_error_code)?;
    with_history(&state, |history| history.commit(history.current().with_colour(suggested)))
}

/// Separate subject and background of the open image as a background job
/// ("job-update" events); the result carries the image's serial. Returns
/// the job id.
#[tauri::command]
fn start_background_separation(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<u64, String> {
    let (serial, source) = {
        let document = state.document.lock().map_err(|_| "internal".to_string())?;
        let document = document.as_ref().ok_or_else(|| "no_document".to_string())?;
        (document.serial, document.source.clone())
    };
    let models_dir = state.models_dir.clone();
    let slot = state.background_model.clone();
    Ok(state.jobs.start(move |ctx| {
        let registry = Registry::load(&models_dir).map_err(|e| JobError::new(e.code(), e.to_string()))?;
        let (matte, report) = separation::separate(&registry, &slot, &source, ctx)?;
        let state = app.state::<AppState>();
        let mut document = state.document.lock().map_err(|_| JobError::new("internal", "document lock"))?;
        match document.as_mut().filter(|d| d.serial == serial) {
            Some(document) => document.matte = Some(Arc::new(matte)),
            None => return Err(JobError::new("cancelled", "another image was opened")),
        }
        let mut value = serde_json::to_value(report).map_err(|e| JobError::new("internal", e.to_string()))?;
        value["serial"] = serde_json::json!(serial);
        Ok(value)
    }))
}

/// Estimate the 3D shape of the open image for lighting, as a background
/// job like [`start_background_separation`]. Returns the job id.
#[tauri::command]
fn start_shape_estimation(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<u64, String> {
    let (serial, source) = {
        let document = state.document.lock().map_err(|_| "internal".to_string())?;
        let document = document.as_ref().ok_or_else(|| "no_document".to_string())?;
        (document.serial, document.source.clone())
    };
    let models_dir = state.models_dir.clone();
    let slot = state.depth_model.clone();
    Ok(state.jobs.start(move |ctx| {
        let registry = Registry::load(&models_dir).map_err(|e| JobError::new(e.code(), e.to_string()))?;
        let (shape, report) = separation::estimate_shape(&registry, &slot, &source, ctx)?;
        let state = app.state::<AppState>();
        let mut document = state.document.lock().map_err(|_| JobError::new("internal", "document lock"))?;
        match document.as_mut().filter(|d| d.serial == serial) {
            Some(document) => document.shape = Some(Arc::new(shape)),
            None => return Err(JobError::new("cancelled", "another image was opened")),
        }
        let mut value = serde_json::to_value(report).map_err(|e| JobError::new("internal", e.to_string()))?;
        value["serial"] = serde_json::json!(serial);
        Ok(value)
    }))
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct BackgroundStatus {
    serial: u64,
    /// Subject and background have been separated.
    ready: bool,
    /// The 3D shape has been estimated.
    shape_ready: bool,
}

#[tauri::command]
fn background_status(state: State<'_, AppState>) -> Result<BackgroundStatus, String> {
    let document = state.document.lock().map_err(|_| "internal".to_string())?;
    let document = document.as_ref().ok_or_else(|| "no_document".to_string())?;
    Ok(BackgroundStatus {
        serial: document.serial,
        ready: document.matte.is_some(),
        shape_ready: document.shape.is_some(),
    })
}

/// Open an image to use as the background; returns its number for the
/// adjustments (`backgroundImage`).
#[tauri::command]
async fn add_background_image(path: String, state: State<'_, AppState>) -> Result<u32, String> {
    let file = PathBuf::from(&path);
    let (source, original) = tauri::async_runtime::spawn_blocking(move || {
        let source = application_core::open_document(&file).map_err(|e| e.code().to_string())?;
        let original = FileRef::of(&file).map_err(|_| "read_failed".to_string())?;
        Ok::<_, String>((source, original))
    })
    .await
    .map_err(|_| "internal".to_string())??;
    let mut document = state.document.lock().map_err(|_| "internal".to_string())?;
    let document = document.as_mut().ok_or_else(|| "no_document".to_string())?;
    document.background_images.push(BackgroundImage {
        file: original,
        source: Some(Arc::new(source)),
    });
    Ok(document.background_images.len() as u32)
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
        let file = PathBuf::from(path);
        if project::is_project_path(&file) {
            // A project's thumbnail is its stored preview.
            let bytes = project::read_preview(&file).map_err(|e| e.code().to_string())?;
            application_core::decode_image(&bytes, THUMBNAIL_MAX_DIMENSION).map_err(|e| e.code().to_string())
        } else {
            application_core::open_image(&file, THUMBNAIL_MAX_DIMENSION).map_err(|e| e.code().to_string())
        }
    })
    .await
    .map_err(|_| "internal".to_string())??;
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
            let recovery = Recovery::new(app.path().app_data_dir()?.join("recovery"));
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
                background_model: Arc::new(ModelSlot::new(separation::BACKGROUND_MODEL)),
                depth_model: Arc::new(ModelSlot::new(separation::DEPTH_MODEL)),
                recovery,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            startup,
            get_settings,
            open_image,
            open_project,
            inspect_project,
            document_status,
            save_project,
            autosave,
            recovery_status,
            restore_recovery,
            discard_recovery,
            export_image,
            render_view,
            render_overview,
            edit_state,
            commit_adjustments,
            undo,
            redo,
            faces_status,
            auto_enhance,
            start_background_separation,
            start_shape_estimation,
            background_status,
            add_background_image,
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
        .build(tauri::generate_context!())
        .expect("error while starting AI Beauty Studio")
        .run(|app, event| {
            // A normal exit keeps no recovery: the user saved or chose to
            // discard their changes.
            if let tauri::RunEvent::Exit = event {
                app.state::<AppState>().recovery.discard();
            }
        });
}
