mod db;
mod dependencies;
mod playlist;
mod repository;
mod sanitize;
mod status;
mod worker;

use playlist::{analyze, PlaylistInfo};
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{Manager, State};
use worker::JobManager;

const DEFAULT_CONCURRENCY: usize = 2;

pub struct AppState {
    pub bin_dir: PathBuf,
    pub db: db::DbPool,
    pub jobs: Arc<JobManager>,
}

#[tauri::command]
fn ping() -> String {
    "pong".into()
}

/// Analyzes the playlist, then marks which items are already in the
/// archive (a completed download exists) so the frontend can show a
/// "X downloaded / Y new" summary and default those items unselected —
/// the actual re-queue decision is still re-checked server-side in
/// `start_download_job`, this is purely informational.
#[tauri::command]
async fn analyze_playlist(state: State<'_, AppState>, url: String) -> Result<PlaylistInfo, String> {
    let mut playlist = analyze(&state.bin_dir, &url).await.map_err(|e| e.to_string())?;

    let db = state.db.clone();
    let source_ids: Vec<String> = playlist.items.iter().map(|i| i.id.clone()).collect();
    let already_done = tokio::task::spawn_blocking(move || {
        let conn = db.get()?;
        repository::already_completed_source_ids(&conn, &source_ids)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    for item in &mut playlist.items {
        item.already_downloaded = already_done.contains(&item.id);
    }

    Ok(playlist)
}

/// Persists the (possibly re-analyzed) playlist and its items, queues
/// downloads for `selected_ids` only (skipping anything already completed),
/// and starts the worker pool on them in the background. Returns the new
/// job id immediately — progress arrives via `download-progress` events.
#[tauri::command]
async fn start_download_job(
    state: State<'_, AppState>,
    playlist: PlaylistInfo,
    selected_ids: Vec<String>,
) -> Result<i64, String> {
    let jobs = state.jobs.clone();
    let db = state.db.clone();

    tokio::task::spawn_blocking(move || -> anyhow::Result<(i64, Vec<repository::DownloadTask>)> {
        let mut conn = db.get()?;

        let playlist_db_id = repository::upsert_playlist(
            &conn,
            "youtube",
            &playlist.source_playlist_id,
            &playlist.title,
            &playlist.url,
        )?;
        let media_item_ids = repository::upsert_media_items(&mut conn, playlist_db_id, &playlist.items)?;

        let selected: HashSet<String> = selected_ids.into_iter().collect();
        let already_done = repository::already_completed_source_ids(
            &conn,
            &playlist.items.iter().map(|i| i.id.clone()).collect::<Vec<_>>(),
        )?;

        let job_id = repository::create_job(&conn, playlist_db_id)?;

        // Only queue items the caller actually selected.
        let filtered_items: Vec<_> = playlist
            .items
            .iter()
            .zip(&media_item_ids)
            .filter(|(item, _)| selected.contains(&item.id))
            .collect();
        let filtered_playlist = PlaylistInfo {
            source_playlist_id: playlist.source_playlist_id.clone(),
            url: playlist.url.clone(),
            title: playlist.title.clone(),
            items: filtered_items.iter().map(|(item, _)| (*item).clone()).collect(),
        };
        let filtered_ids: Vec<i64> = filtered_items.iter().map(|(_, id)| **id).collect();

        let tasks = repository::queue_downloads(&mut conn, &filtered_ids, &filtered_playlist, &already_done)?;

        Ok((job_id, tasks))
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
    .map(|(job_id, tasks)| {
        tauri::async_runtime::spawn(jobs.start_job(job_id, tasks));
        job_id
    })
}

#[tauri::command]
fn pause_job(state: State<'_, AppState>, job_id: i64) {
    state.jobs.pause_job(job_id);
}

#[tauri::command]
fn resume_job(state: State<'_, AppState>, job_id: i64) {
    state.jobs.resume_job(job_id);
}

#[tauri::command]
fn cancel_job(state: State<'_, AppState>, job_id: i64) {
    state.jobs.cancel_job(job_id);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            ping,
            analyze_playlist,
            start_download_job,
            pause_job,
            resume_job,
            cancel_job
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let bin_dir = tauri::async_runtime::block_on(dependencies::ensure_dependencies(&handle))
                .map_err(|e| {
                    eprintln!("dependency setup failed: {e:#}");
                    e
                })?;

            let app_data_dir = app.path().app_data_dir()?;
            let db = db::init_pool(&app_data_dir)?;

            let jobs = JobManager::new(db.clone(), handle, bin_dir.clone(), DEFAULT_CONCURRENCY);

            app.manage(AppState { bin_dir, db, jobs });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
