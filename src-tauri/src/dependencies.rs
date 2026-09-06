use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

const YTDLP_RELEASES_API: &str = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";
const FFMPEG_RELEASE_BASE: &str = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest";
const FFMPEG_ASSET: &str = "ffmpeg-master-latest-win64-gpl.zip";

/// Ensures yt-dlp.exe / ffmpeg.exe / ffprobe.exe exist in the app data dir,
/// downloading and verifying them on first launch. No CLI/config is ever
/// surfaced to the user for this.
pub async fn ensure_dependencies(app: &AppHandle) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .context("resolving app data dir")?
        .join("bin");
    std::fs::create_dir_all(&dir).context("creating bin dir")?;

    let ytdlp_path = dir.join("yt-dlp.exe");
    let ffmpeg_path = dir.join("ffmpeg.exe");
    let ffprobe_path = dir.join("ffprobe.exe");

    if !ytdlp_path.exists() {
        fetch_ytdlp(&ytdlp_path).await?;
    }
    if !ffmpeg_path.exists() || !ffprobe_path.exists() {
        fetch_ffmpeg(&dir, &ffmpeg_path, &ffprobe_path).await?;
    }

    Ok(dir)
}

async fn fetch_ytdlp(dest: &Path) -> Result<()> {
    let client = reqwest::Client::builder().user_agent("crate-app").build()?;

    let release: serde_json::Value = client
        .get(YTDLP_RELEASES_API)
        .send()
        .await
        .context("querying yt-dlp latest release")?
        .json()
        .await
        .context("parsing yt-dlp release json")?;

    let assets = release["assets"]
        .as_array()
        .context("yt-dlp release has no assets")?;

    let exe_url = find_asset_url(assets, "yt-dlp.exe")
        .context("yt-dlp.exe asset not found in latest release")?;
    let sums_url = find_asset_url(assets, "SHA2-256SUMS");

    let bytes = client.get(&exe_url).send().await?.bytes().await?;

    if let Some(sums_url) = sums_url {
        let sums_text = client.get(&sums_url).send().await?.text().await?;
        let expected = sums_text
            .lines()
            .find(|l| l.ends_with("yt-dlp.exe"))
            .and_then(|l| l.split_whitespace().next())
            .context("yt-dlp.exe checksum not present in SHA2-256SUMS")?;

        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let actual = format!("{:x}", hasher.finalize());

        if actual != expected {
            return Err(anyhow!(
                "yt-dlp.exe checksum mismatch: expected {expected}, got {actual}"
            ));
        }
    }

    write_file(dest, &bytes)
}

async fn fetch_ffmpeg(dir: &Path, ffmpeg_dest: &Path, ffprobe_dest: &Path) -> Result<()> {
    let client = reqwest::Client::builder().user_agent("crate-app").build()?;

    let url = format!("{FFMPEG_RELEASE_BASE}/{FFMPEG_ASSET}");
    let bytes = client.get(&url).send().await?.bytes().await?;

    let zip_path = dir.join(FFMPEG_ASSET);
    write_file(&zip_path, &bytes)?;

    let file = std::fs::File::open(&zip_path)?;
    let mut archive = zip::ZipArchive::new(file).context("opening ffmpeg zip")?;

    let want = ["ffmpeg.exe", "ffprobe.exe"];
    let mut found: HashMap<&str, usize> = HashMap::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let entry_name = entry.name().to_string();
        for w in want {
            if entry_name.ends_with(&format!("bin/{w}")) {
                found.insert(w, i);
            }
        }
    }

    for name in want {
        let idx = *found
            .get(name)
            .with_context(|| format!("{name} not found inside ffmpeg archive"))?;
        let mut entry = archive.by_index(idx)?;
        let dest = if name == "ffmpeg.exe" { ffmpeg_dest } else { ffprobe_dest };
        let mut out = std::fs::File::create(dest)?;
        std::io::copy(&mut entry, &mut out)?;
    }

    std::fs::remove_file(&zip_path).ok();

    // BtbN doesn't consistently publish a per-asset checksum, so verification
    // falls back to a functional smoke test rather than a checksum we don't have.
    let output = std::process::Command::new(ffmpeg_dest)
        .arg("-version")
        .output()
        .context("running ffmpeg -version smoke test")?;
    if !output.status.success() {
        return Err(anyhow!("ffmpeg -version smoke test failed"));
    }

    Ok(())
}

fn find_asset_url(assets: &[serde_json::Value], name: &str) -> Option<String> {
    assets
        .iter()
        .find(|a| a["name"].as_str() == Some(name))
        .and_then(|a| a["browser_download_url"].as_str())
        .map(String::from)
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut f = std::fs::File::create(path)?;
    f.write_all(bytes)?;
    Ok(())
}
