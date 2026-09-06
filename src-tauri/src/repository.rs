use crate::playlist::{MediaItem, PlaylistInfo};
use crate::status;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashSet;

pub struct DownloadTask {
    pub download_id: i64,
    pub media_item_id: i64,
    pub source_id: String,
    pub title: String,
    pub url: String,
    pub index: i64,
}

pub fn upsert_playlist(conn: &Connection, source: &str, source_playlist_id: &str, title: &str, url: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO playlists (source, source_playlist_id, title, url)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(source, source_playlist_id)
         DO UPDATE SET title = excluded.title, updated_at = datetime('now')",
        params![source, source_playlist_id, title, url],
    )
    .context("upserting playlist")?;

    conn.query_row(
        "SELECT id FROM playlists WHERE source = ?1 AND source_playlist_id = ?2",
        params![source, source_playlist_id],
        |row| row.get(0),
    )
    .context("fetching playlist id")
}

/// Inserts/refreshes each item's row and returns their DB ids in the same
/// order as `items`. The UNIQUE(source, source_id) constraint is what makes
/// this a real dedupe point rather than just an in-memory list.
pub fn upsert_media_items(conn: &mut Connection, playlist_id: i64, items: &[MediaItem]) -> Result<Vec<i64>> {
    let tx = conn.transaction()?;
    let mut ids = Vec::with_capacity(items.len());

    for item in items {
        tx.execute(
            "INSERT INTO media_items (playlist_id, source, source_id, title, url, duration, position, thumbnail_url, status)
             VALUES (?1, 'youtube', ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(source, source_id)
             DO UPDATE SET title = excluded.title, url = excluded.url, position = excluded.position",
            params![
                playlist_id,
                item.id,
                item.title,
                item.url,
                item.duration.map(|d| d as i64),
                item.index as i64,
                item.thumbnail,
                status::READY,
            ],
        )
        .context("upserting media item")?;

        let id: i64 = tx.query_row(
            "SELECT id FROM media_items WHERE source = 'youtube' AND source_id = ?1",
            params![item.id],
            |row| row.get(0),
        )?;
        ids.push(id);
    }

    tx.commit()?;
    Ok(ids)
}

/// Which of these source ids already have a completed download — the basic
/// dedupe check Phase 2 needs so re-running a job never re-downloads a
/// finished item. Phase 3 builds the full "diff & report" sync UI on top of
/// this same query.
pub fn already_completed_source_ids(conn: &Connection, source_ids: &[String]) -> Result<HashSet<String>> {
    let mut stmt = conn.prepare(
        "SELECT mi.source_id FROM media_items mi
         JOIN downloads d ON d.media_item_id = mi.id
         WHERE mi.source_id = ?1 AND d.status = ?2",
    )?;

    let mut done = HashSet::new();
    for id in source_ids {
        let found: Option<String> = stmt
            .query_row(params![id, status::COMPLETED], |row| row.get(0))
            .optional()?;
        if let Some(id) = found {
            done.insert(id);
        }
    }
    Ok(done)
}

pub fn create_job(conn: &Connection, playlist_id: i64) -> Result<i64> {
    conn.execute(
        "INSERT INTO jobs (playlist_id, status) VALUES (?1, ?2)",
        params![playlist_id, status::READY],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Creates one `downloads` row per media item (skipping any already
/// COMPLETED) and returns the tasks ready to hand to the worker pool.
pub fn queue_downloads(
    conn: &mut Connection,
    media_item_ids: &[i64],
    playlist: &PlaylistInfo,
    already_done: &HashSet<String>,
) -> Result<Vec<DownloadTask>> {
    let tx = conn.transaction()?;
    let mut tasks = Vec::new();

    for (item, &media_item_id) in playlist.items.iter().zip(media_item_ids) {
        if already_done.contains(&item.id) {
            continue;
        }

        tx.execute(
            "UPDATE media_items SET status = ?1 WHERE id = ?2",
            params![status::QUEUED, media_item_id],
        )?;
        tx.execute(
            "INSERT INTO downloads (media_item_id, format, quality, status)
             VALUES (?1, 'mp3', 'best', ?2)",
            params![media_item_id, status::QUEUED],
        )?;
        let download_id = tx.last_insert_rowid();

        tasks.push(DownloadTask {
            download_id,
            media_item_id,
            source_id: item.id.clone(),
            title: item.title.clone(),
            url: item.url.clone(),
            index: item.index as i64,
        });
    }

    tx.commit()?;
    Ok(tasks)
}

/// Single place that moves a download (and its parent media_item) between
/// states together, so the two can never drift apart.
pub fn transition(conn: &Connection, download_id: i64, media_item_id: i64, new_status: &str) -> Result<()> {
    let tx_time_col = match new_status {
        s if s == status::DOWNLOADING => Some("started_at"),
        s if s == status::COMPLETED => Some("completed_at"),
        _ => None,
    };

    match tx_time_col {
        Some(col) => {
            conn.execute(
                &format!("UPDATE downloads SET status = ?1, {col} = datetime('now') WHERE id = ?2"),
                params![new_status, download_id],
            )?;
        }
        None => {
            conn.execute(
                "UPDATE downloads SET status = ?1 WHERE id = ?2",
                params![new_status, download_id],
            )?;
        }
    }

    conn.execute(
        "UPDATE media_items SET status = ?1 WHERE id = ?2",
        params![new_status, media_item_id],
    )?;
    Ok(())
}

pub fn mark_completed(conn: &Connection, download_id: i64, media_item_id: i64, file_path: &str, file_size: u64) -> Result<()> {
    conn.execute(
        "UPDATE downloads SET status = ?1, file_path = ?2, file_size = ?3, completed_at = datetime('now') WHERE id = ?4",
        params![status::COMPLETED, file_path, file_size as i64, download_id],
    )?;
    conn.execute(
        "UPDATE media_items SET status = ?1 WHERE id = ?2",
        params![status::COMPLETED, media_item_id],
    )?;
    Ok(())
}

/// Records a failed attempt and returns the resulting status (RETRYING with
/// a next_retry_at, or terminal FAILED once attempts are exhausted) plus the
/// new attempt_count.
pub fn record_attempt_failure(conn: &Connection, download_id: i64, media_item_id: i64, error: &str) -> Result<(String, u32)> {
    let attempt_count: u32 = conn.query_row(
        "UPDATE downloads SET attempt_count = attempt_count + 1, last_error = ?1 WHERE id = ?2 RETURNING attempt_count",
        params![error, download_id],
        |row| row.get(0),
    )?;

    let new_status = if attempt_count >= status::MAX_ATTEMPTS {
        status::FAILED
    } else {
        status::RETRYING
    };

    if new_status == status::RETRYING {
        let backoff = status::backoff_seconds(attempt_count);
        conn.execute(
            "UPDATE downloads SET status = ?1, next_retry_at = datetime('now', ?2) WHERE id = ?3",
            params![new_status, format!("+{backoff} seconds"), download_id],
        )?;
    } else {
        conn.execute(
            "UPDATE downloads SET status = ?1 WHERE id = ?2",
            params![new_status, download_id],
        )?;
    }

    conn.execute(
        "UPDATE media_items SET status = ?1 WHERE id = ?2",
        params![new_status, media_item_id],
    )?;

    Ok((new_status.to_string(), attempt_count))
}
