// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use application_core::info::{self, FileInfo};
use application_core::settings::{self, LanguagePreference, Settings, ThemePreference};
use tauri::ipc::Response;
use tauri::path::BaseDirectory;
use tauri::{Manager, State};

/// Longer side of the image sent to the UI for display. The full-resolution
/// pipeline arrives with the canvas work in Phase 3.
const DISPLAY_MAX_DIMENSION: u32 = 2048;

/// Longer side of recent-file thumbnails on the Home screen.
const THUMBNAIL_MAX_DIMENSION: u32 = 320;

const NOTICES_FILE: &str = "THIRD_PARTY_NOTICES.txt";

struct AppState {
    settings_path: PathBuf,
    settings: Mutex<Settings>,
    /// True when the settings file was damaged at startup and was reset.
    settings_recovered: bool,
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

/// Decode an image on a worker thread and return a display frame
/// (see `application_core::encode_frame`).
async fn decode_frame(path: String, max_dimension: u32) -> Result<Vec<u8>, String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        application_core::open_image(&PathBuf::from(path), max_dimension)
    })
    .await
    .map_err(|_| "internal".to_string())?;
    result
        .map(|image| application_core::encode_frame(&image))
        .map_err(|error| error.code().to_string())
}

/// Open an image for editing and record it in the recent-files list.
/// Errors are stable codes that the UI maps to localized messages.
#[tauri::command]
async fn open_image(path: String, state: State<'_, AppState>) -> Result<Response, String> {
    let frame = decode_frame(path.clone(), DISPLAY_MAX_DIMENSION).await?;
    // The image opened; failing to save the recent list must not block it.
    let _ = state.update(|s| s.add_recent(&path, now_seconds()));
    Ok(Response::new(frame))
}

#[tauri::command]
async fn thumbnail(path: String) -> Result<Response, String> {
    decode_frame(path, THUMBNAIL_MAX_DIMENSION).await.map(Response::new)
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
            app.manage(AppState {
                settings_path,
                settings: Mutex::new(loaded.settings),
                settings_recovered: loaded.recovered_from_damage,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            startup,
            get_settings,
            open_image,
            thumbnail,
            file_info,
            set_theme,
            set_language,
            remove_recent,
            clear_recent,
            third_party_notices,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AI Beauty Studio");
}
