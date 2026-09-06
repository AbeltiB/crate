mod dependencies;
mod downloader;
mod playlist;
mod sanitize;

use downloader::download_sequential;
use playlist::{analyze, MediaItem, PlaylistInfo};
use std::path::PathBuf;
use tauri::{AppHandle, Manager, State};

pub struct AppState {
    pub bin_dir: PathBuf,
}

#[tauri::command]
fn ping() -> String {
    "pong".into()
}

#[tauri::command]
async fn analyze_playlist(state: State<'_, AppState>, url: String) -> Result<PlaylistInfo, String> {
    analyze(&state.bin_dir, &url).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn download_items(
    app: AppHandle,
    state: State<'_, AppState>,
    playlist_title: String,
    items: Vec<MediaItem>,
) -> Result<(), String> {
    download_sequential(&app, &state.bin_dir, &playlist_title, items)
        .await
        .map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ping, analyze_playlist, download_items])
        .setup(|app| {
            let handle = app.handle().clone();
            let bin_dir = tauri::async_runtime::block_on(dependencies::ensure_dependencies(&handle))
                .map_err(|e| {
                    eprintln!("dependency setup failed: {e:#}");
                    e
                })?;
            app.manage(AppState { bin_dir });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
