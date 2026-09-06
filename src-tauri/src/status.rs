//! Status vocabulary shared by `media_items`, `downloads`, and `jobs` (§5 of
//! the architecture doc). Kept as constants so a typo doesn't silently create
//! a new, unmatched status string.

pub const DISCOVERING: &str = "DISCOVERING";
pub const READY: &str = "READY";
pub const QUEUED: &str = "QUEUED";
pub const DOWNLOADING: &str = "DOWNLOADING";
pub const PROCESSING: &str = "PROCESSING";
pub const COMPLETED: &str = "COMPLETED";
pub const PAUSED: &str = "PAUSED";
pub const RETRYING: &str = "RETRYING";
pub const FAILED: &str = "FAILED";
pub const CANCELLED: &str = "CANCELLED";

pub const MAX_ATTEMPTS: u32 = 3;

/// Exponential backoff for retry scheduling: 30s, 2m, 8m.
pub fn backoff_seconds(attempt_count: u32) -> i64 {
    30 * 4i64.pow(attempt_count.saturating_sub(1))
}
