//! The application log, `omairc.log` in the state folder. Port of
//! `omaircfilelog`. The Go TUI has no twin: it keeps its crash record in
//! `omairc-tui.log` through `internal/crashlog`.
//!
//! Like the Qt log, it records warnings and errors only, one line each:
//! `<UTC time to the millisecond> <warning|error> <text>`, with line breaks in the
//! text flattened to spaces. Lines are appended under an owner-only file mode.
//! It is the `log` crate's logger, so gpui's own warnings land here too.

use std::cell::Cell;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use log::{Level, LevelFilter, Log, Metadata, Record};

/// Serialises appends from any thread, as Qt's `g_writeMutex` does.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

thread_local! {
    /// Set while a write is in progress, so a log call made by the writer itself is
    /// dropped instead of recursing (Qt's `t_inWrite`).
    static WRITING: Cell<bool> = const { Cell::new(false) };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticRecord {
    pub when: DateTime<Utc>,
    pub severity: Severity,
    pub text: String,
}

/// One log line, with its trailing newline.
pub fn format_line(record: &DiagnosticRecord) -> String {
    let severity = match record.severity {
        Severity::Warning => "warning",
        Severity::Error => "error",
    };
    let text: String = record
        .text
        .chars()
        .map(|ch| if ch == '\n' || ch == '\r' { ' ' } else { ch })
        .collect();
    format!(
        "{} {severity} {text}\n",
        record.when.format("%Y-%m-%dT%H:%M:%S%.3fZ")
    )
}

/// Appends `line` to `path`, creating the folder and the file. The file is kept
/// owner read/write only, as Qt sets it on every append.
pub fn append_line(path: &Path, line: &str) -> io::Result<()> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        fs::create_dir_all(dir)?;
    }
    let _guard = WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    set_owner_only(&file)?;
    file.write_all(line.as_bytes())
}

#[cfg(unix)]
fn set_owner_only(file: &fs::File) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_owner_only(_file: &fs::File) -> io::Result<()> {
    Ok(())
}

/// The file logger. [`install`] makes it the process logger.
pub struct FileLog {
    path: PathBuf,
}

impl FileLog {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Appends one record. A failed write is dropped, as the Qt log drops it.
    pub fn write(&self, record: &DiagnosticRecord) -> io::Result<()> {
        append_line(&self.path, &format_line(record))
    }
}

impl Log for FileLog {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Warn
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) || WRITING.with(Cell::get) {
            return;
        }
        let severity = match record.level() {
            Level::Error => Severity::Error,
            _ => Severity::Warning,
        };
        WRITING.with(|writing| writing.set(true));
        let entry = DiagnosticRecord {
            when: Utc::now(),
            severity,
            text: record.args().to_string(),
        };
        let _ = self.write(&entry);
        WRITING.with(|writing| writing.set(false));
    }

    fn flush(&self) {}
}

/// Makes a file logger at `path` the process logger, recording warnings and errors.
/// Call once at startup. A second call fails, and the first logger stays.
pub fn install(path: impl Into<PathBuf>) -> Result<(), log::SetLoggerError> {
    log::set_boxed_logger(Box::new(FileLog::new(path)))?;
    log::set_max_level(LevelFilter::Warn);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testdir::TempDir;
    use chrono::{TimeZone, Timelike};

    fn at(millis: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 21, 12, 34, 56)
            .single()
            .unwrap()
            .with_nanosecond(millis * 1_000_000)
            .unwrap()
    }

    fn emit(log: &FileLog, level: Level, text: &str) {
        log.log(
            &Record::builder()
                .args(format_args!("{text}"))
                .level(level)
                .target("test")
                .build(),
        );
    }

    #[test]
    fn formats_utc_time_severity_and_one_line_text() {
        let record = DiagnosticRecord {
            when: at(7),
            severity: Severity::Warning,
            text: "first\nsecond\r\nthird".into(),
        };
        assert_eq!(
            format_line(&record),
            "2026-10-21T12:34:56.007Z warning first second  third\n"
        );

        let error = DiagnosticRecord {
            severity: Severity::Error,
            ..record
        };
        assert!(format_line(&error).contains(" error first"));
    }

    #[test]
    fn only_warnings_and_errors_are_enabled() {
        let log = FileLog::new("unused.log");
        let meta = |level| Metadata::builder().level(level).target("t").build();
        assert!(log.enabled(&meta(Level::Error)));
        assert!(log.enabled(&meta(Level::Warn)));
        assert!(!log.enabled(&meta(Level::Info)));
        assert!(!log.enabled(&meta(Level::Debug)));
        assert!(!log.enabled(&meta(Level::Trace)));
    }

    #[test]
    fn appends_lines_and_creates_the_folder() {
        let dir = TempDir::new("filelog-append");
        let path = dir.path().join("nested").join("state").join("omairc.log");
        let log = FileLog::new(&path);

        emit(&log, Level::Warn, "first problem");
        emit(&log, Level::Info, "ignored info");
        emit(&log, Level::Error, "second problem");

        let text = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].ends_with(" warning first problem"), "{}", lines[0]);
        assert!(lines[1].ends_with(" error second problem"), "{}", lines[1]);
    }

    #[test]
    fn write_appends_a_record_directly() {
        let dir = TempDir::new("filelog-write");
        let path = dir.path().join("omairc.log");
        let log = FileLog::new(&path);
        log.write(&DiagnosticRecord {
            when: at(0),
            severity: Severity::Error,
            text: "disk full".into(),
        })
        .unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "2026-10-21T12:34:56.000Z error disk full\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_log_file_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new("filelog-mode");
        let path = dir.path().join("omairc.log");
        append_line(&path, "line\n").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }
}
