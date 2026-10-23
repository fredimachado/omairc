//! On-disk conversation transcripts (`tui/internal/storage/conversationlog.go`).

use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::irc::casemapping::IrcCaseMapping;
use crate::irc::reducer::{ConversationLog, TranscriptLine};
use crate::irc::secretpolicy::allows_transcript;
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::storagepath::{
    legacy_storage_segment, storage_segment, wire_storage_segment,
};

/// JSONL transcript store under `root/<network>/<target>`.
pub struct FileConversationLog {
    root: PathBuf,
}

impl FileConversationLog {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    #[cfg(test)]
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn path_for(&self, network_id: &str, target: &str, _mapping: IrcCaseMapping) -> PathBuf {
        let new_network_dir = storage_segment(network_id, None);
        let new_target = wire_storage_segment(target, None);
        let new_path = self.root.join(new_network_dir).join(new_target);
        migrate_legacy_transcript_path(&self.root, network_id, target, new_path)
    }
}

impl ConversationLog for FileConversationLog {
    fn append(
        &self,
        network_id: &str,
        target: &str,
        mapping: IrcCaseMapping,
        line: &TranscriptLine,
    ) -> bool {
        if line.kind.is_empty() || line.body.is_empty() {
            return true;
        }
        let channel_types = channel_types();
        if !allows_transcript(&line.body, &channel_types)
            || !allows_transcript(&line.author, &channel_types)
        {
            return true;
        }
        append_bytes(&self.path_for(network_id, target, mapping), &format_line(line))
    }

    fn prepend(
        &self,
        network_id: &str,
        target: &str,
        mapping: IrcCaseMapping,
        lines: &[TranscriptLine],
    ) -> bool {
        let channel_types = channel_types();
        let encoded: Vec<Vec<u8>> = lines
            .iter()
            .filter(|line| {
                !line.kind.is_empty()
                    && !line.body.is_empty()
                    && allows_transcript(&line.body, &channel_types)
                    && allows_transcript(&line.author, &channel_types)
            })
            .map(format_line)
            .collect();
        if encoded.is_empty() {
            return true;
        }
        prepend_bytes(&self.path_for(network_id, target, mapping), &encoded)
    }

    fn read_tail(
        &self,
        network_id: &str,
        target: &str,
        mapping: IrcCaseMapping,
        max_lines: usize,
    ) -> Vec<TranscriptLine> {
        if max_lines == 0 {
            return Vec::new();
        }
        let path = self.path_for(network_id, target, mapping);
        let file = match File::open(&path) {
            Ok(file) => file,
            Err(_) => return Vec::new(),
        };
        read_tail_file(file, max_lines)
    }
}

fn channel_types() -> String {
    IrcServerFeatures::new().channel_types().to_owned()
}

#[derive(Serialize, Deserialize)]
struct ConversationLogRecord {
    timestamp: String,
    author: String,
    kind: String,
    body: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    msgid: String,
    #[serde(
        default,
        rename = "onlyVisibleToYou",
        skip_serializing_if = "is_false"
    )]
    only_visible_to_you: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn trim_bytes(raw: &[u8]) -> &[u8] {
    let start = raw.iter().position(|byte| !byte.is_ascii_whitespace()).unwrap_or(raw.len());
    let end = raw
        .iter()
        .rposition(|byte| !byte.is_ascii_whitespace())
        .map(|index| index + 1)
        .unwrap_or(start);
    raw.get(start..end).unwrap_or(&[])
}

fn flatten_text(text: &str) -> String {
    text.chars()
        .map(|ch| if ch == '\n' || ch == '\r' { ' ' } else { ch })
        .collect()
}

fn iso_date_with_ms(when: DateTime<Utc>) -> String {
    when.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

fn format_line(line: &TranscriptLine) -> Vec<u8> {
    let when = if line.timestamp.timestamp() == 0 && line.timestamp.timestamp_subsec_nanos() == 0 {
        Utc::now()
    } else {
        line.timestamp
    };
    let record = ConversationLogRecord {
        timestamp: iso_date_with_ms(when),
        author: flatten_text(&line.author),
        kind: line.kind.clone(),
        body: flatten_text(&line.body),
        msgid: line.msgid.clone(),
        only_visible_to_you: line.only_visible_to_you,
    };
    let mut buffer = serde_json::to_vec(&record).expect("json encode");
    buffer.push(b'\n');
    buffer
}

fn parse_line(raw: &[u8]) -> Option<TranscriptLine> {
    let record: ConversationLogRecord = serde_json::from_slice(raw).ok()?;
    if record.kind.is_empty() {
        return None;
    }
    let timestamp = parse_timestamp(&record.timestamp);
    Some(TranscriptLine {
        timestamp,
        author: sanitize(&record.author),
        kind: record.kind,
        body: sanitize(&record.body),
        msgid: record.msgid,
        only_visible_to_you: record.only_visible_to_you,
    })
}

fn parse_timestamp(text: &str) -> DateTime<Utc> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(text) {
        return parsed.with_timezone(&Utc);
    }
    if let Ok(naive) = NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.3fZ") {
        return naive.and_utc();
    }
    DateTime::UNIX_EPOCH
}

fn kept_control(code: char) -> bool {
    matches!(
        code,
        '\u{0001}'
            | '\u{0002}'
            | '\u{0003}'
            | '\u{0004}'
            | '\u{000f}'
            | '\u{0011}'
            | '\u{0016}'
            | '\u{001d}'
            | '\u{001e}'
            | '\u{001f}'
            | '\t'
            | '\r'
            | '\n'
    )
}

fn dropped_terminal(code: char) -> bool {
    if code < '\u{0020}' {
        return !kept_control(code);
    }
    if code == '\u{007f}' {
        return true;
    }
    ('\u{0080}'..='\u{009f}').contains(&code)
}

/// Strips terminal control bytes from disk text (`termtext.Sanitize`).
fn sanitize(text: &str) -> String {
    if text.is_ascii() {
        if !text.chars().any(dropped_terminal) {
            return text.to_owned();
        }
    } else if !text.chars().any(dropped_terminal) {
        return text.to_owned();
    }
    text.chars().filter(|&ch| !dropped_terminal(ch)).collect()
}

fn prepare_tree(file_path: &Path) -> bool {
    let Some(dir) = file_path.parent() else {
        return false;
    };
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut current = dir.to_path_buf();
        for _ in 0..3 {
            if current.as_os_str().is_empty() || current == Path::new(".") {
                break;
            }
            let _ = std::fs::set_permissions(&current, std::fs::Permissions::from_mode(0o700));
            if !current.pop() {
                break;
            }
        }
    }
    true
}

fn append_bytes(path: &Path, line: &[u8]) -> bool {
    if !prepare_tree(path) {
        return false;
    }
    let mut file = match OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        Ok(file) => file,
        Err(_) => return false,
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    file.write_all(line).is_ok()
}

fn prepend_bytes(path: &Path, lines: &[Vec<u8>]) -> bool {
    if !prepare_tree(path) {
        return false;
    }
    let existing = std::fs::read(path).unwrap_or_default();
    let mut buffer = Vec::new();
    for line in lines {
        buffer.extend_from_slice(line);
    }
    buffer.extend_from_slice(&existing);
    if std::fs::write(path, &buffer).is_err() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
    }
    true
}

fn read_tail_file(mut file: File, max_lines: usize) -> Vec<TranscriptLine> {
    let size = file.metadata().map(|meta| meta.len()).unwrap_or(0);
    let channel_types = channel_types();
    let mut position = size;
    let mut pending = Vec::new();
    let mut lines = Vec::new();

    const CHUNK_SIZE: u64 = 4096;
    while position > 0 && lines.len() < max_lines {
        let bytes_to_read = position.min(CHUNK_SIZE);
        position -= bytes_to_read;
        if file.seek(SeekFrom::Start(position)).is_err() {
            break;
        }
        let mut chunk = vec![0; bytes_to_read as usize];
        if file.read_exact(&mut chunk).is_err() {
            break;
        }
        let mut combined = chunk;
        combined.extend_from_slice(&pending);
        pending = combined;

        while let Some(newline) = pending.iter().rposition(|&byte| byte == b'\n') {
            let raw = pending[newline + 1..].to_vec();
            pending.truncate(newline);
            if accept_tail_line(&raw, &channel_types, &mut lines, max_lines) {
                break;
            }
        }
    }

    if position == 0 && lines.len() < max_lines {
        accept_tail_line(&pending, &channel_types, &mut lines, max_lines);
    }

    lines.reverse();
    lines
}

fn accept_tail_line(
    raw: &[u8],
    channel_types: &str,
    lines: &mut Vec<TranscriptLine>,
    max_lines: usize,
) -> bool {
    let raw = trim_bytes(raw);
    if raw.is_empty() {
        return false;
    }
    let Some(parsed) = parse_line(raw) else {
        return false;
    };
    if !allows_transcript(&parsed.body, channel_types)
        || !allows_transcript(&parsed.author, channel_types)
    {
        return false;
    }
    lines.push(parsed);
    lines.len() == max_lines
}

fn file_exists(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|meta| meta.is_file())
        .unwrap_or(false)
}

fn dir_exists(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|meta| meta.is_dir())
        .unwrap_or(false)
}

fn canonical_path(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn paths_same_file(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        if let (Some(left_id), Some(right_id)) = (windows_file_id(left), windows_file_id(right)) {
            return left_id == right_id;
        }
    }
    #[cfg(unix)]
    {
        if let (Ok(left_meta), Ok(right_meta)) = (std::fs::metadata(left), std::fs::metadata(right)) {
            return same_file_metadata(&left_meta, &right_meta);
        }
    }
    let left_canon = canonical_path(left);
    let right_canon = canonical_path(right);
    !left_canon.as_os_str().is_empty() && left_canon == right_canon
}

#[cfg(unix)]
fn same_file_metadata(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

/// Volume serial + file index (`os.SameFile` on Windows).
#[cfg(windows)]
fn windows_file_id(path: &Path) -> Option<(u32, u64)> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_READ, FILE_SHARE_DELETE,
        FILE_SHARE_READ, FILE_SHARE_WRITE, GetFileInformationByHandle, OPEN_EXISTING,
    };

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            FILE_GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };
    if handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
        return None;
    }
    let _guard = HandleGuard(handle);
    let mut info = windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION {
        dwFileAttributes: 0,
        ftCreationTime: windows_sys::Win32::Foundation::FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        },
        ftLastAccessTime: windows_sys::Win32::Foundation::FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        },
        ftLastWriteTime: windows_sys::Win32::Foundation::FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        },
        dwVolumeSerialNumber: 0,
        nFileSizeHigh: 0,
        nFileSizeLow: 0,
        nNumberOfLinks: 0,
        nFileIndexHigh: 0,
        nFileIndexLow: 0,
    };
    if unsafe { GetFileInformationByHandle(handle, &mut info) } == 0 {
        return None;
    }
    let index = (info.nFileIndexHigh as u64) << 32 | info.nFileIndexLow as u64;
    Some((info.dwVolumeSerialNumber, index))
}

#[cfg(windows)]
struct HandleGuard(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl Drop for HandleGuard {
    fn drop(&mut self) {
        use windows_sys::Win32::Foundation::CloseHandle;
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

fn dir_segments_share_location(root: &Path, left_segment: &str, right_segment: &str) -> bool {
    if left_segment == right_segment {
        return true;
    }
    paths_same_file(&root.join(left_segment), &root.join(right_segment))
}

fn migrate_legacy_transcript_path(
    root: &Path,
    network_id: &str,
    target: &str,
    new_path: PathBuf,
) -> PathBuf {
    let legacy_network_dir = legacy_storage_segment(network_id);
    let new_network_dir = storage_segment(network_id, None);
    let mut network_dir_path = root.join(&legacy_network_dir);
    let legacy_network_dir_exists = dir_exists(&network_dir_path);
    let new_network_dir_exists = dir_exists(&root.join(&new_network_dir));

    if legacy_network_dir != new_network_dir {
        let shared = dir_segments_share_location(root, &legacy_network_dir, &new_network_dir);
        if !shared && legacy_network_dir_exists && !new_network_dir_exists {
            let legacy = root.join(&legacy_network_dir);
            let updated = root.join(&new_network_dir);
            if std::fs::rename(&legacy, &updated).is_ok() {
                network_dir_path = updated;
            }
        } else if new_network_dir_exists {
            network_dir_path = root.join(&new_network_dir);
        }
    } else if new_network_dir_exists {
        network_dir_path = root.join(&new_network_dir);
    }

    let mut resolved_path = new_path.clone();
    let mut try_migrate_file = |legacy_path: &Path| {
        if !file_exists(legacy_path) {
            return;
        }
        if file_exists(&new_path) || paths_same_file(legacy_path, &new_path) {
            return;
        }
        prepare_tree(&new_path);
        if std::fs::rename(legacy_path, &new_path).is_ok() {
            return;
        }
        if file_exists(legacy_path) {
            resolved_path = legacy_path.to_path_buf();
        }
    };

    let legacy_path = network_dir_path.join(legacy_storage_segment(target));
    try_migrate_file(&legacy_path);

    if legacy_network_dir != new_network_dir && legacy_network_dir_exists {
        let legacy_path_in_legacy_dir = root
            .join(&legacy_network_dir)
            .join(legacy_storage_segment(target));
        try_migrate_file(&legacy_path_in_legacy_dir);
    }

    resolved_path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::secretpolicy::allows_transcript;
    use crate::irc::serverfeatures::IrcServerFeatures;
    use crate::irc::storagepath::target_segment;
    use crate::util::testdir::TempDir;
    use chrono::TimeZone;

    fn test_line(body: &str) -> TranscriptLine {
        TranscriptLine {
            timestamp: Utc.with_ymd_and_hms(2026, 9, 4, 0, 0, 0).unwrap(),
            author: "alice".to_owned(),
            kind: "chat".to_owned(),
            body: body.to_owned(),
            msgid: String::new(),
            only_visible_to_you: false,
        }
    }

    fn write_file(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("mkdir");
        }
        std::fs::write(path, contents).expect("write");
    }

    fn exists(path: &Path) -> bool {
        file_exists(path)
    }

    #[test]
    fn root_matches_the_constructor_path() {
        let dir = TempDir::new("log-root");
        let log = FileConversationLog::new(dir.path().to_path_buf());
        assert_eq!(log.root(), dir.path());
    }

    #[test]
    fn formatted_line_parses_back() {
        let bytes = format_line(&test_line("x"));
        assert!(
            parse_line(&bytes[..bytes.len().saturating_sub(1)]).is_some(),
            "json {:?}",
            String::from_utf8_lossy(&bytes)
        );
    }

    #[test]
    fn prepend_preserves_order_on_disk() {
        let dir = TempDir::new("prepend-order");
        let log = FileConversationLog::new(dir.path().to_path_buf());
        let mapping = IrcCaseMapping::default();
        log.append(
            "net",
            "#room",
            mapping,
            &TranscriptLine {
                kind: "message".to_owned(),
                body: "live".to_owned(),
                author: "a".to_owned(),
                ..test_line("")
            },
        );
        log.prepend(
            "net",
            "#room",
            mapping,
            &[TranscriptLine {
                kind: "message".to_owned(),
                body: "older".to_owned(),
                author: "b".to_owned(),
                ..test_line("")
            }],
        );
        let lines = log.read_tail("net", "#room", mapping, 10);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].body, "older");
        assert_eq!(lines[1].body, "live");
    }

    #[test]
    fn read_tail_drops_terminal_escapes() {
        let dir = TempDir::new("terminal-escapes");
        let log = FileConversationLog::new(dir.path().to_path_buf());
        let mapping = IrcCaseMapping::default();
        let path = log.path_for("net", "#room", mapping);
        write_file(
            &path,
            r#"{"author":"ev\u001b[1mil","body":"\u001b]8;;https://evil.example\u0007click\u001b]8;;\u0007 \u009b2J \u0002bold\u0002","kind":"message","timestamp":"2026-09-04T00:00:00.000Z"}
"#,
        );
        let lines = log.read_tail("net", "#room", mapping, 10);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].author, "ev[1mil");
        assert_eq!(
            lines[0].body,
            "]8;;https://evil.exampleclick]8;; 2J \u{0002}bold\u{0002}"
        );
    }

    #[test]
    fn append_read_tail_round_trip() {
        let dir = TempDir::new("round-trip");
        let log = FileConversationLog::new(dir.path().to_path_buf());
        let mapping = IrcCaseMapping::default();
        let base = Utc.with_ymd_and_hms(2026, 9, 4, 0, 0, 0).unwrap();
        for (index, body) in ["one", "two", "three"].into_iter().enumerate() {
            let line = TranscriptLine {
                timestamp: base + chrono::Duration::minutes(index as i64),
                ..test_line(body)
            };
            assert!(log.append("net", "#room", mapping, &line));
        }
        let all = log.read_tail("net", "#room", mapping, 10);
        assert_eq!(all.len(), 3);
        for (index, body) in ["one", "two", "three"].into_iter().enumerate() {
            assert_eq!(all[index].body, body);
        }
        assert_eq!(all[0].timestamp, base);
        assert_eq!(all[0].author, "alice");
        assert_eq!(all[0].kind, "chat");
        let tail = log.read_tail("net", "#room", mapping, 2);
        assert_eq!(tail.len(), 2);
        assert_eq!(tail[0].body, "two");
        assert_eq!(tail[1].body, "three");
        assert!(log.read_tail("net", "#room", mapping, 0).is_empty());
        assert!(log.read_tail("net", "missing", mapping, 5).is_empty());
    }

    #[test]
    fn read_tail_spans_chunk_boundary() {
        let dir = TempDir::new("chunk-boundary");
        let log = FileConversationLog::new(dir.path().to_path_buf());
        let mapping = IrcCaseMapping::default();
        let big = "x".repeat(5000);
        assert!(log.append("net", "#room", mapping, &test_line("older")));
        assert!(log.append("net", "#room", mapping, &test_line(&big)));
        let lines = log.read_tail("net", "#room", mapping, 2);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].body, "older");
        assert_eq!(lines[1].body, big);
    }

    #[test]
    fn path_for_preserves_target_case() {
        let dir = TempDir::new("target-case");
        let log = FileConversationLog::new(dir.path().to_path_buf());
        let mapping = IrcCaseMapping::default();
        let upper = log.path_for("freenode", "#Omarchy", mapping);
        let want = dir
            .path()
            .join(storage_segment("freenode", None))
            .join(wire_storage_segment("#Omarchy", None));
        assert_eq!(upper, want);
        let lower = log.path_for("freenode", "#omarchy", mapping);
        assert_ne!(upper, lower);
        let mapped = dir
            .path()
            .join(storage_segment("freenode", None))
            .join(target_segment("#Omarchy", mapping, None));
        assert_ne!(upper, mapped);
    }

    #[test]
    fn migrates_legacy_transcript_paths() {
        let dir = TempDir::new("legacy-migrate");
        let root = dir.path();
        let legacy_dir = root.join(legacy_storage_segment("Freenode"));
        let legacy_file = legacy_dir.join(legacy_storage_segment("#a/b"));
        write_file(&legacy_file, "legacy\n");
        let log = FileConversationLog::new(root.to_path_buf());
        let mapping = IrcCaseMapping::default();
        let path = log.path_for("Freenode", "#a/b", mapping);
        let want = root
            .join(storage_segment("Freenode", None))
            .join(wire_storage_segment("#a/b", None));
        assert_eq!(path, want);
        assert!(exists(&path));
        assert!(!exists(&legacy_file));
        assert!(!dir_exists(&legacy_dir));
        assert_eq!(log.path_for("Freenode", "#a/b", mapping), want);
    }

    #[test]
    fn does_not_overwrite_existing_transcript_during_migration() {
        let dir = TempDir::new("legacy-no-overwrite");
        let root = dir.path();
        let legacy_dir = root.join(legacy_storage_segment("Freenode"));
        let legacy_file = legacy_dir.join(legacy_storage_segment("#a/b"));
        write_file(&legacy_file, "legacy\n");
        let new_path = root
            .join(storage_segment("Freenode", None))
            .join(wire_storage_segment("#a/b", None));
        write_file(&new_path, "newer\n");
        let log = FileConversationLog::new(root.to_path_buf());
        let mapping = IrcCaseMapping::default();
        assert_eq!(log.path_for("Freenode", "#a/b", mapping), new_path);
        assert!(exists(&legacy_file));
        let contents = std::fs::read_to_string(&new_path).expect("read");
        assert_eq!(contents, "newer\n");
    }

    #[test]
    fn skips_rejected_transcript_lines() {
        let features = IrcServerFeatures::new();
        let channel_types = features.channel_types();
        let rejected = "PRIVMSG NickServ :IDENTIFY swordfish";
        assert!(!allows_transcript(rejected, &channel_types));
        let dir = TempDir::new("rejected");
        let log = FileConversationLog::new(dir.path().to_path_buf());
        let mapping = IrcCaseMapping::default();
        assert!(log.append("net", "#room", mapping, &test_line(rejected)));
        assert!(log.read_tail("net", "#room", mapping, 10).is_empty());
        assert!(!exists(&log.path_for("net", "#room", mapping)));
        assert!(log.append(
            "net",
            "#room",
            mapping,
            &TranscriptLine {
                body: "no kind".to_owned(),
                ..test_line("")
            },
        ));
        assert!(log.append(
            "net",
            "#room",
            mapping,
            &TranscriptLine {
                kind: "chat".to_owned(),
                body: String::new(),
                ..test_line("")
            },
        ));
    }

    #[test]
    fn append_tightens_permissions() {
        let dir = TempDir::new("permissions");
        let log = FileConversationLog::new(dir.path().to_path_buf());
        let mapping = IrcCaseMapping::default();
        assert!(log.append("net", "#room", mapping, &test_line("hello")));
        #[cfg(unix)]
        assert_transcript_permissions(&log.path_for("net", "#room", mapping));
    }

    #[test]
    fn prepend_tightens_permissions() {
        let dir = TempDir::new("prepend-permissions");
        let log = FileConversationLog::new(dir.path().to_path_buf());
        let mapping = IrcCaseMapping::default();
        assert!(log.prepend("net", "#room", mapping, &[test_line("history")]));
        #[cfg(unix)]
        assert_transcript_permissions(&log.path_for("net", "#room", mapping));
    }

    #[cfg(unix)]
    fn assert_transcript_permissions(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        let file_mode = std::fs::metadata(path).expect("stat").permissions().mode() & 0o777;
        assert_eq!(file_mode, 0o600);
        let dir_mode = std::fs::metadata(path.parent().expect("parent"))
            .expect("stat")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(dir_mode, 0o700);
    }

    #[test]
    fn writes_flattened_jsonl() {
        let dir = TempDir::new("jsonl");
        let log = FileConversationLog::new(dir.path().to_path_buf());
        let mapping = IrcCaseMapping::default();
        let line = TranscriptLine {
            timestamp: Utc.with_ymd_and_hms(2026, 9, 4, 12, 30, 0).unwrap(),
            author: "alice\r\nbob".to_owned(),
            msgid: "m1".to_owned(),
            ..test_line("hello\nworld")
        };
        assert!(log.append("net", "#room", mapping, &line));
        let text = std::fs::read_to_string(log.path_for("net", "#room", mapping)).expect("read");
        assert_eq!(text.matches('\n').count(), 1);
        assert!(text.ends_with('\n'));
        for want in [
            r#""timestamp":"2026-09-04T12:30:00.000Z""#,
            r#""author":"alice  bob""#,
            r#""kind":"chat""#,
            r#""body":"hello world""#,
            r#""msgid":"m1""#,
        ] {
            assert!(text.contains(want), "missing {want} in {text}");
        }
        assert!(log.append("net", "#other", mapping, &test_line("hi")));
        let other = std::fs::read_to_string(log.path_for("net", "#other", mapping)).expect("read");
        assert!(!other.contains("msgid"));
    }

    #[test]
    fn only_visible_to_you_round_trip() {
        let dir = TempDir::new("only-you");
        let log = FileConversationLog::new(dir.path().to_path_buf());
        let mapping = IrcCaseMapping::default();
        assert!(log.append(
            "net",
            "#room",
            mapping,
            &TranscriptLine {
                kind: "message".to_owned(),
                author: "bot".to_owned(),
                body: "just you".to_owned(),
                only_visible_to_you: true,
                ..test_line("")
            },
        ));
        assert!(log.append(
            "net",
            "#room",
            mapping,
            &TranscriptLine {
                kind: "message".to_owned(),
                author: "alice".to_owned(),
                body: "everyone".to_owned(),
                ..test_line("")
            },
        ));
        let lines = log.read_tail("net", "#room", mapping, 10);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].only_visible_to_you);
        assert_eq!(lines[0].body, "just you");
        assert!(!lines[1].only_visible_to_you);
        let raw = std::fs::read_to_string(log.path_for("net", "#room", mapping)).expect("read");
        assert!(raw.contains(r#""onlyVisibleToYou":true"#));
        assert_eq!(raw.matches("onlyVisibleToYou").count(), 1);
    }
}
