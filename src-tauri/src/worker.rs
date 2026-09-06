use crate::db::DbPool;
use crate::repository::{self, DownloadTask};
use crate::status;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Semaphore;

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub media_item_id: i64,
    pub source_id: String,
    pub status: String,
    pub percent: Option<f64>,
    pub error: Option<String>,
}

struct JobControl {
    paused: AtomicBool,
    cancelled: AtomicBool,
}

pub struct JobManager {
    db: DbPool,
    app: AppHandle,
    bin_dir: PathBuf,
    semaphore: Arc<Semaphore>,
    jobs: Mutex<HashMap<i64, Arc<JobControl>>>,
}

impl JobManager {
    pub fn new(db: DbPool, app: AppHandle, bin_dir: PathBuf, concurrency: usize) -> Arc<Self> {
        Arc::new(Self {
            db,
            app,
            bin_dir,
            semaphore: Arc::new(Semaphore::new(concurrency)),
            jobs: Mutex::new(HashMap::new()),
        })
    }

    pub fn pause_job(&self, job_id: i64) {
        if let Some(c) = self.jobs.lock().unwrap().get(&job_id) {
            c.paused.store(true, Ordering::SeqCst);
        }
    }

    pub fn resume_job(&self, job_id: i64) {
        if let Some(c) = self.jobs.lock().unwrap().get(&job_id) {
            c.paused.store(false, Ordering::SeqCst);
        }
    }

    pub fn cancel_job(&self, job_id: i64) {
        if let Some(c) = self.jobs.lock().unwrap().get(&job_id) {
            c.cancelled.store(true, Ordering::SeqCst);
        }
    }

    /// Dispatches every task in the job onto the shared, semaphore-bounded
    /// worker pool (§6 of the architecture doc) and returns once all of them
    /// have finished (or the job was cancelled). Meant to be run inside its
    /// own `tokio::spawn` by the caller — this does not return early.
    ///
    /// Pause takes effect at item boundaries (a running download finishes
    /// its current attempt rather than being frozen mid-stream — yt-dlp has
    /// no live-pause hook, so pausing an in-flight process would mean
    /// killing it, which is what Cancel is for). Cancel does kill in-flight
    /// yt-dlp processes.
    pub async fn start_job(self: Arc<Self>, job_id: i64, tasks: Vec<DownloadTask>) {
        let control = Arc::new(JobControl {
            paused: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
        });
        self.jobs.lock().unwrap().insert(job_id, control.clone());

        let mut handles = Vec::new();

        for task in tasks {
            loop {
                if control.cancelled.load(Ordering::SeqCst) {
                    break;
                }
                if !control.paused.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            if control.cancelled.load(Ordering::SeqCst) {
                break;
            }

            let permit = match self.semaphore.clone().acquire_owned().await {
                Ok(p) => p,
                Err(_) => break,
            };
            let db = self.db.clone();
            let app = self.app.clone();
            let bin_dir = self.bin_dir.clone();
            let control = control.clone();

            handles.push(tokio::spawn(async move {
                let _permit = permit;
                run_item(&app, &db, &bin_dir, &control, task).await;
            }));
        }

        for h in handles {
            let _ = h.await;
        }

        self.jobs.lock().unwrap().remove(&job_id);
    }
}

async fn run_item(
    app: &AppHandle,
    db: &DbPool,
    bin_dir: &std::path::Path,
    control: &JobControl,
    task: DownloadTask,
) {
    loop {
        if control.cancelled.load(Ordering::SeqCst) {
            set_status(db, &task, status::CANCELLED);
            return;
        }

        set_status(db, &task, status::DOWNLOADING);
        emit(app, &task, status::DOWNLOADING, Some(0.0), None);

        match run_ytdlp(app, bin_dir, &task, control).await {
            Ok(RunOutcome::Completed { file_path, file_size }) => {
                let conn = db.get().expect("db pool");
                let _ = repository::mark_completed(
                    &conn,
                    task.download_id,
                    task.media_item_id,
                    &file_path,
                    file_size,
                );
                emit(app, &task, status::COMPLETED, Some(100.0), None);
                return;
            }
            Ok(RunOutcome::Cancelled) => {
                set_status(db, &task, status::CANCELLED);
                emit(app, &task, status::CANCELLED, None, None);
                return;
            }
            Err(error) => {
                let conn = db.get().expect("db pool");
                let (new_status, attempts) = repository::record_attempt_failure(
                    &conn,
                    task.download_id,
                    task.media_item_id,
                    &error,
                )
                .unwrap_or((status::FAILED.to_string(), status::MAX_ATTEMPTS));
                drop(conn);

                if new_status == status::RETRYING {
                    emit(app, &task, status::RETRYING, None, Some(error.clone()));
                    let backoff = status::backoff_seconds(attempts);
                    tokio::time::sleep(Duration::from_secs(backoff.max(0) as u64)).await;
                    continue;
                }

                emit(app, &task, status::FAILED, None, Some(error));
                return;
            }
        }
    }
}

enum RunOutcome {
    Completed { file_path: String, file_size: u64 },
    Cancelled,
}

async fn run_ytdlp(
    app: &AppHandle,
    bin_dir: &std::path::Path,
    task: &DownloadTask,
    control: &JobControl,
) -> Result<RunOutcome, String> {
    let ytdlp = bin_dir.join("yt-dlp.exe");
    let output_template = format!("{} - %(title)s.%(ext)s", task.index);

    let mut child = Command::new(&ytdlp)
        .arg("-x")
        .arg("--audio-format")
        .arg("mp3")
        .arg("--audio-quality")
        .arg("0")
        .arg("--ffmpeg-location")
        .arg(bin_dir)
        .arg("--newline")
        .arg("--quiet")
        .arg("--no-warnings")
        .arg("--progress-template")
        .arg("download:%(progress)j")
        .arg("--progress-template")
        .arg("postprocess:%(progress)j")
        .arg("--print")
        .arg("after_move:%(filepath)s")
        .arg("--no-playlist")
        .arg("-o")
        .arg(&output_template)
        .arg(&task.url)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");

    let stderr_handle = tokio::spawn(async move {
        let mut buf = String::new();
        let _ = BufReader::new(stderr).read_to_string(&mut buf).await;
        buf
    });

    let mut final_path: Option<String> = None;
    let mut lines = BufReader::new(stdout).lines();

    loop {
        if control.cancelled.load(Ordering::SeqCst) {
            let _ = child.kill().await;
            return Ok(RunOutcome::Cancelled);
        }

        let next = tokio::time::timeout(Duration::from_millis(500), lines.next_line()).await;
        let line = match next {
            Ok(Ok(Some(l))) => l,
            Ok(Ok(None)) => break,
            Ok(Err(_)) => break,
            Err(_) => continue, // timeout: loop back around to re-check `cancelled`
        };

        if let Some((prefix, json_str)) = line.split_once(':') {
            if prefix == "download" || prefix == "postprocess" {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(json_str) {
                    let percent = match (
                        v["downloaded_bytes"].as_f64(),
                        v["total_bytes"].as_f64().or(v["total_bytes_estimate"].as_f64()),
                    ) {
                        (Some(d), Some(t)) if t > 0.0 => Some((d / t * 100.0).min(100.0)),
                        _ => None,
                    };
                    let status = if prefix == "postprocess" { status::PROCESSING } else { status::DOWNLOADING };
                    emit(app, task, status, percent, None);
                }
                continue;
            }
        }
        // Not a progress line: this is our `--print after_move:...` output.
        final_path = Some(line);
    }

    let wait_result = child.wait().await;
    let stderr_text = stderr_handle.await.unwrap_or_default();

    match wait_result {
        Ok(status) if status.success() => {
            let file_path = final_path.ok_or_else(|| "yt-dlp succeeded but produced no output path".to_string())?;
            let file_size = std::fs::metadata(&file_path).map(|m| m.len()).unwrap_or(0);
            Ok(RunOutcome::Completed { file_path, file_size })
        }
        Ok(_) => Err(if stderr_text.trim().is_empty() {
            "yt-dlp exited with a non-zero status".to_string()
        } else {
            stderr_text
        }),
        Err(e) => Err(e.to_string()),
    }
}

fn set_status(db: &DbPool, task: &DownloadTask, new_status: &str) {
    if let Ok(conn) = db.get() {
        let _ = repository::transition(&conn, task.download_id, task.media_item_id, new_status);
    }
}

fn emit(app: &AppHandle, task: &DownloadTask, status: &str, percent: Option<f64>, error: Option<String>) {
    let _ = app.emit(
        "download-progress",
        DownloadProgress {
            media_item_id: task.media_item_id,
            source_id: task.source_id.clone(),
            status: status.to_string(),
            percent,
            error,
        },
    );
}
