use crate::playlist::MediaItem;
use crate::sanitize::sanitize_component;
use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;
use std::process::Stdio;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;

#[derive(Serialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum DownloadStatus {
    Downloading,
    Processing,
    Done,
    Failed,
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub item_id: String,
    pub status: DownloadStatus,
    pub percent: Option<f64>,
    pub error: Option<String>,
}

pub async fn download_sequential(
    app: &AppHandle,
    bin_dir: &Path,
    playlist_title: &str,
    items: Vec<MediaItem>,
) -> Result<()> {
    let music_dir = app
        .path()
        .audio_dir()
        .context("resolving Music directory")?;
    let target_dir = music_dir.join(sanitize_component(playlist_title));
    std::fs::create_dir_all(&target_dir).context("creating playlist output dir")?;

    let ytdlp = bin_dir.join("yt-dlp.exe");

    for item in items {
        emit(app, &item.id, DownloadStatus::Downloading, Some(0.0), None);

        let output_template = target_dir.join(format!("{} - %(title)s.%(ext)s", item.index));

        let spawn_result = Command::new(&ytdlp)
            .arg("-x")
            .arg("--audio-format")
            .arg("mp3")
            .arg("--audio-quality")
            .arg("0")
            .arg("--ffmpeg-location")
            .arg(bin_dir)
            .arg("--newline")
            .arg("--progress-template")
            .arg("download:%(progress)j")
            .arg("--progress-template")
            .arg("postprocess:%(progress)j")
            .arg("--no-playlist")
            .arg("-o")
            .arg(&output_template)
            .arg(&item.url)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();

        let mut child = match spawn_result {
            Ok(c) => c,
            Err(e) => {
                emit(app, &item.id, DownloadStatus::Failed, None, Some(e.to_string()));
                continue;
            }
        };

        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");

        // Drain stderr concurrently so a chatty process can't deadlock us by
        // filling its stderr pipe while we're only reading stdout.
        let stderr_handle = tokio::spawn(async move {
            let mut buf = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut buf).await;
            buf
        });

        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let (prefix, json_str) = match line.split_once(':') {
                Some((p, rest)) if p == "download" || p == "postprocess" => (p, rest),
                _ => continue,
            };
            let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) else {
                continue;
            };

            let percent = match (
                v["downloaded_bytes"].as_f64(),
                v["total_bytes"].as_f64().or(v["total_bytes_estimate"].as_f64()),
            ) {
                (Some(d), Some(t)) if t > 0.0 => Some((d / t * 100.0).min(100.0)),
                _ => None,
            };

            let status = if prefix == "postprocess" {
                DownloadStatus::Processing
            } else {
                DownloadStatus::Downloading
            };
            emit(app, &item.id, status, percent, None);
        }

        let wait_result = child.wait().await;
        let stderr_text = stderr_handle.await.unwrap_or_default();

        match wait_result {
            Ok(status) if status.success() => {
                emit(app, &item.id, DownloadStatus::Done, Some(100.0), None);
            }
            Ok(_) => {
                emit(app, &item.id, DownloadStatus::Failed, None, Some(stderr_text));
            }
            Err(e) => {
                emit(app, &item.id, DownloadStatus::Failed, None, Some(e.to_string()));
            }
        }
    }

    Ok(())
}

fn emit(
    app: &AppHandle,
    item_id: &str,
    status: DownloadStatus,
    percent: Option<f64>,
    error: Option<String>,
) {
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            item_id: item_id.to_string(),
            status,
            percent,
            error,
        },
    );
}
