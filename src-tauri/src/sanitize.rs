const ILLEGAL: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Minimal Windows-path-safe cleanup for a single path component we build
/// ourselves (e.g. the playlist-title folder). `%(title)s` in yt-dlp's own
/// `-o` template is sanitized by yt-dlp itself; this only covers literal
/// segments we hand it. Phase 3 hardens this further (reserved device
/// names, full test coverage).
pub fn sanitize_component(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .map(|c| if ILLEGAL.contains(&c) || c.is_control() { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').trim();
    if trimmed.is_empty() {
        "Untitled".to_string()
    } else {
        trimmed.chars().take(150).collect()
    }
}
