use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tokio::process::Command;

#[derive(Serialize, Deserialize, Clone)]
pub struct MediaItem {
    pub id: String,
    pub title: String,
    pub url: String,
    pub duration: Option<u64>,
    pub thumbnail: Option<String>,
    pub index: usize,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PlaylistInfo {
    pub source_playlist_id: String,
    pub url: String,
    pub title: String,
    pub items: Vec<MediaItem>,
}

pub async fn analyze(bin_dir: &Path, url: &str) -> Result<PlaylistInfo> {
    let ytdlp = bin_dir.join("yt-dlp.exe");

    let output = Command::new(&ytdlp)
        .args(["--flat-playlist", "--dump-json", "--no-warnings", url])
        .output()
        .await
        .context("running yt-dlp --flat-playlist")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("yt-dlp analyze failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut items = Vec::new();
    let mut playlist_title: Option<String> = None;
    let mut source_playlist_id: Option<String> = None;

    for (i, line) in stdout.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let entry: serde_json::Value =
            serde_json::from_str(line).context("parsing yt-dlp json line")?;

        if playlist_title.is_none() {
            playlist_title = entry["playlist_title"].as_str().map(String::from);
        }
        if source_playlist_id.is_none() {
            source_playlist_id = entry["playlist_id"].as_str().map(String::from);
        }

        let id = entry["id"].as_str().unwrap_or_default().to_string();
        items.push(MediaItem {
            index: i + 1,
            title: entry["title"].as_str().unwrap_or("Untitled").to_string(),
            url: format!("https://www.youtube.com/watch?v={id}"),
            duration: entry["duration"].as_f64().map(|d| d as u64),
            thumbnail: entry["thumbnail"].as_str().map(String::from),
            id,
        });
    }

    if items.is_empty() {
        return Err(anyhow!("no items found for this URL"));
    }

    Ok(PlaylistInfo {
        source_playlist_id: source_playlist_id.unwrap_or_else(|| url.to_string()),
        url: url.to_string(),
        title: playlist_title.unwrap_or_else(|| "Playlist".to_string()),
        items,
    })
}
