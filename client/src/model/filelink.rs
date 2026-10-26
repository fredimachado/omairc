//! Pasted file paths and composer routing for FILEHOST links.
//! `tui/internal/ui/filepick.go` and `filepick_test.go`.

use std::path::{Path, PathBuf};

use super::composer::Edit;
use super::format::is_allowed_http_url;

/// Every line in `text` names an existing regular file. Any other paste returns `None`.
pub fn local_file_lines(text: &str, stat: &dyn Fn(&Path) -> Option<PathStat>) -> Option<Vec<PathBuf>> {
    let lines = paste_lines(text);
    if lines.is_empty() {
        return None;
    }
    let mut paths = Vec::new();
    for line in lines {
        let path = local_path_token(&line)?;
        let info = stat(&path)?;
        if !info.regular {
            return None;
        }
        paths.push(path);
    }
    Some(paths)
}

/// Non-regular paths when every line is an existing absolute path and at least one
/// is not a regular file. Any other paste returns `None`.
pub fn non_regular_paste_paths(text: &str, stat: &dyn Fn(&Path) -> Option<PathStat>) -> Option<Vec<PathBuf>> {
    let lines = paste_lines(text);
    if lines.is_empty() {
        return None;
    }
    let mut bad = Vec::new();
    for line in lines {
        let path = local_path_token(&line)?;
        let info = stat(&path)?;
        if !info.regular {
            bad.push(path);
        }
    }
    if bad.is_empty() {
        None
    } else {
        Some(bad)
    }
}

/// Whether a clipboard text paste is file paths the upload takes: every line is an
/// existing absolute path, regular or not. A consumed paste never lands as text, so
/// a directory counts too; the upload refuses it with `NOT_A_FILE`.
pub fn paste_consumes(text: &str, stat: &dyn Fn(&Path) -> Option<PathStat>) -> bool {
    local_file_lines(text, stat).is_some() || non_regular_paste_paths(text, stat).is_some()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathStat {
    pub regular: bool,
}

/// Inserts an allowlisted link at the caret with space padding.
pub fn insert_file_link_at_caret(text: &str, cursor: usize, raw: &str) -> Option<Edit> {
    if !is_allowed_http_url(raw) {
        return None;
    }
    let bytes = text.as_bytes();
    let cursor = cursor.min(bytes.len());
    let mut piece = raw.to_owned();
    if cursor > 0 && bytes[cursor - 1] != b' ' {
        piece.insert(0, ' ');
    }
    if cursor < bytes.len() && bytes[cursor] != b' ' {
        piece.push(' ');
    }
    let mut result = String::new();
    result.push_str(&text[..cursor]);
    result.push_str(&piece);
    result.push_str(&text[cursor..]);
    Some(Edit {
        text: result,
        cursor: cursor + piece.len(),
    })
}

pub fn append_stored(stored: &mut String, raw: &str) {
    if stored.is_empty() {
        *stored = raw.to_owned();
    } else if stored.ends_with(' ') {
        stored.push_str(raw);
    } else {
        stored.push(' ');
        stored.push_str(raw);
    }
}

fn paste_lines(text: &str) -> Vec<String> {
    let normalized = text.replace("\r\n", "\n");
    let text = normalized.trim();
    if text.is_empty() {
        return Vec::new();
    }
    text.split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn local_path_token(token: &str) -> Option<PathBuf> {
    let path = convert_file_token(token)?;
    if cfg!(windows) {
        return Some(path);
    }
    if path.is_absolute() {
        Some(path)
    } else {
        None
    }
}

pub fn convert_file_token(token: &str) -> Option<PathBuf> {
    convert_file_token_on(token, cfg!(windows))
}

pub fn convert_file_token_on(token: &str, windows: bool) -> Option<PathBuf> {
    let path = token;
    if token.to_ascii_lowercase().starts_with("file://") {
        let normalized = token.replace('\\', "/");
        let parsed = url::Url::parse(&normalized).ok()?;
        if !parsed.scheme().eq_ignore_ascii_case("file") {
            return None;
        }
        let path = decode_percent(parsed.path());
        if windows {
            return windows_path_from_file_url(parsed.host_str().unwrap_or(""), &path);
        }
        let host = parsed.host_str().unwrap_or("");
        if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
            return None;
        }
        return (!path.is_empty()).then(|| PathBuf::from(path));
    }
    if windows {
        return windows_absolute(path).then(|| PathBuf::from(path));
    }
    Some(PathBuf::from(path))
}

fn windows_path_from_file_url(host: &str, path: &str) -> Option<PathBuf> {
    let mut combined = path.to_owned();
    if let Some(drive) = windows_drive_host(host) {
        combined = format!("{drive}{path}");
    } else if !host.is_empty() && !host.eq_ignore_ascii_case("localhost") {
        return None;
    }
    combined = combined.trim_start_matches('/').replace('/', "\\");
    windows_absolute(&combined).then(|| PathBuf::from(combined))
}

fn windows_drive_host(host: &str) -> Option<String> {
    let trimmed = host.trim_end_matches(':');
    if trimmed.len() == 1 && trimmed.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        Some(format!("{}:", trimmed))
    } else {
        None
    }
}

fn decode_percent(path: &str) -> String {
    let mut out = Vec::new();
    let bytes = path.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&path[index + 1..index + 3], 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| path.to_owned())
}

fn windows_absolute(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use crate::util::testdir::TempDir;

    fn stat_of(path: &Path) -> Option<PathStat> {
        fs::metadata(path).ok().map(|info| PathStat {
            regular: info.is_file(),
        })
    }

    #[test]
    fn insert_file_link_at_caret_spaces() {
        let edit = insert_file_link_at_caret("hello world", 5, "https://example.org/a.png")
            .expect("edit");
        assert_eq!(edit.text, "hello https://example.org/a.png world");
        assert!(insert_file_link_at_caret("hello", 5, "javascript:alert(1)").is_none());
    }

    #[test]
    fn local_file_lines_require_every_line() {
        let dir = TempDir::new("filelink");
        let path = dir.path().join("note.txt");
        fs::write(&path, b"x").expect("write");
        let stat = |path: &Path| stat_of(path);
        let line = path.display().to_string();
        assert_eq!(
            local_file_lines(&line, &stat).expect("one"),
            vec![path.clone()]
        );
        let url = format!("file:///{}", path.display().to_string().replace('\\', "/"));
        assert_eq!(
            local_file_lines(&url, &stat).expect("url"),
            vec![path.clone()]
        );
        assert!(local_file_lines("hello", &stat).is_none());
        assert!(local_file_lines("note.txt", &stat).is_none());
        let mixed = format!("{}\nhello", path.display());
        assert!(local_file_lines(&mixed, &stat).is_none());
    }

    #[test]
    fn paste_consumes_only_existing_paths() {
        let dir = TempDir::new("paste");
        let file = dir.path().join("note.txt");
        fs::write(&file, b"x").expect("write");
        let folder = dir.path().join("folder");
        fs::create_dir(&folder).expect("mkdir");
        let stat = |path: &Path| stat_of(path);
        assert!(paste_consumes(&file.display().to_string(), &stat));
        assert!(paste_consumes(&folder.display().to_string(), &stat));
        assert!(!paste_consumes("hello world", &stat));
        let mixed = format!("{}\nhello", file.display());
        assert!(!paste_consumes(&mixed, &stat));
    }

    #[test]
    fn convert_file_token_windows_drive() {
        let cases = [
            (r"file://C:\Users\note.txt", r"C:\Users\note.txt"),
            ("file:///C:/Users/note.txt", r"C:\Users\note.txt"),
            ("file://C:/Users/note.txt", r"C:\Users\note.txt"),
            (
                "file://localhost/C:/Users/My%20Notes/a.txt",
                r"C:\Users\My Notes\a.txt",
            ),
            (r"C:\Users\note.txt", r"C:\Users\note.txt"),
        ];
        for (token, want) in cases {
            let got = convert_file_token_on(token, true).expect("ok");
            assert_eq!(got, PathBuf::from(want), "token {token}");
        }
        assert!(convert_file_token_on("file://evil.com/etc/passwd", true).is_none());
        assert!(convert_file_token_on("note.txt", true).is_none());
        assert_eq!(
            convert_file_token_on("file:///tmp/note.txt", false).expect("unix"),
            PathBuf::from("/tmp/note.txt")
        );
    }
}
