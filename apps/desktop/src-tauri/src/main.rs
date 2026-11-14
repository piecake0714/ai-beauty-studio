// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use tauri::ipc::Response;

/// Longer side of the image sent to the UI for display. The full-resolution
/// pipeline arrives with the canvas work in Phase 3.
const DISPLAY_MAX_DIMENSION: u32 = 2048;

/// Open an image file and return a display frame (see
/// `application_core::encode_frame`). Decoding runs on a worker thread so the
/// UI thread is never blocked. Errors are returned as stable codes that the
/// UI maps to localized messages.
#[tauri::command]
async fn open_image(path: String) -> Result<Response, String> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        application_core::open_image(&PathBuf::from(path), DISPLAY_MAX_DIMENSION)
    })
    .await
    .map_err(|_| "internal".to_string())?;

    match result {
        Ok(image) => Ok(Response::new(application_core::encode_frame(&image))),
        Err(error) => Err(error.code().to_string()),
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![open_image])
        .run(tauri::generate_context!())
        .expect("error while running AI Beauty Studio");
}
