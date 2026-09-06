const ILLEGAL: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

const RESERVED_DEVICE_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Windows-path-safe cleanup for a single path component we build ourselves
/// (currently just the playlist-title folder segment). `%(title)s` in
/// yt-dlp's own `-o` template is sanitized by yt-dlp itself; this only
/// covers literal segments we hand it directly.
///
/// Handles: illegal/control characters, path traversal (a lone `..` has
/// nothing but dots, which get trimmed away — see the tests), trailing
/// dots/spaces (Windows silently strips these, which can make two
/// different-looking names collide), reserved device names (`CON`, `NUL`,
/// `COM1`, ... — Windows treats these as special files regardless of
/// extension), and length.
pub fn sanitize_component(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .map(|c| if ILLEGAL.contains(&c) || c.is_control() { '_' } else { c })
        .collect();

    let trimmed = cleaned.trim().trim_end_matches('.').trim();

    let base = if trimmed.is_empty() {
        "Untitled"
    } else {
        trimmed
    };

    let truncated: String = base.chars().take(150).collect();

    if is_reserved_device_name(&truncated) {
        format!("_{truncated}")
    } else {
        truncated
    }
}

fn is_reserved_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name);
    RESERVED_DEVICE_NAMES
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(stem))
}

#[cfg(test)]
mod tests {
    use super::sanitize_component;

    #[test]
    fn passes_through_a_normal_title() {
        assert_eq!(sanitize_component("Late Night Drives"), "Late Night Drives");
    }

    #[test]
    fn replaces_illegal_characters() {
        assert_eq!(sanitize_component("Songs / Mixes: 2024?"), "Songs _ Mixes_ 2024_");
    }

    #[test]
    fn strips_control_characters() {
        assert_eq!(sanitize_component("Mix\u{0}\u{7}Tape"), "Mix__Tape");
    }

    #[test]
    fn trims_trailing_dots_and_spaces() {
        assert_eq!(sanitize_component("Playlist.. "), "Playlist");
    }

    #[test]
    fn a_lone_dot_dot_does_not_escape_the_directory() {
        // Only dots (and nothing else) after cleanup -> trimmed to empty -> falls back.
        assert_eq!(sanitize_component(".."), "Untitled");
        assert_eq!(sanitize_component("..."), "Untitled");
    }

    #[test]
    fn embedded_slashes_cannot_add_path_segments() {
        assert_eq!(sanitize_component("../../etc"), ".._.._etc");
        assert!(!sanitize_component("../../etc").contains('/'));
    }

    #[test]
    fn empty_and_whitespace_only_fall_back() {
        assert_eq!(sanitize_component(""), "Untitled");
        assert_eq!(sanitize_component("   "), "Untitled");
    }

    #[test]
    fn reserved_device_names_are_escaped_case_insensitively() {
        assert_eq!(sanitize_component("CON"), "_CON");
        assert_eq!(sanitize_component("con"), "_con");
        assert_eq!(sanitize_component("nul"), "_nul");
        assert_eq!(sanitize_component("COM1"), "_COM1");
        // A reserved name with an extension is still reserved on Windows.
        assert_eq!(sanitize_component("NUL.txt"), "_NUL.txt");
    }

    #[test]
    fn names_that_merely_contain_a_reserved_word_are_left_alone() {
        assert_eq!(sanitize_component("Console Sessions"), "Console Sessions");
        assert_eq!(sanitize_component("NULLABLE"), "NULLABLE");
    }

    #[test]
    fn truncates_very_long_titles() {
        let long = "a".repeat(500);
        assert_eq!(sanitize_component(&long).chars().count(), 150);
    }
}
