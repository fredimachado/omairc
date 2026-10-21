//! The session layer's portability gate. `irc/` is gated by
//! `tests/irc_portability.rs`; `session/` is the seam where sockets and threads
//! would land, so it is held to a narrower rule: no socket, no thread, no
//! process, no GUI, and no wall-clock read outside `clock.rs`, the one place the
//! Clock seam reads time. The check reads this crate's own sources, so it runs
//! with the rest of `cargo test`.

use std::fs;
use std::path::{Path, PathBuf};

/// Tokens that would tie `session/` to a platform or toolkit.
const FORBIDDEN: &[&str] = &[
    "std::net",
    "std::thread",
    "std::process",
    "std::fs",
    "std::env",
    "tokio",
    "gpui",
];

/// Wall-clock reads, allowed only inside the Clock seam itself.
const CLOCK_READS: &[&str] = &["Utc::now", "Local::now", "SystemTime::now", "Instant::now"];

fn session_sources() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("session");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("read src/session")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
        .filter(|path| !path.components().any(|part| part.as_os_str() == "tests"))
        .collect();
    files.sort();
    files
}

/// The non-comment lines of a file, paired with their line numbers.
fn code_lines(path: &Path) -> Vec<(usize, String)> {
    let text = fs::read_to_string(path).expect("read session source");
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
        .map(|(index, line)| (index + 1, line.to_owned()))
        .collect()
}

#[test]
fn session_stays_portable() {
    let mut violations = Vec::new();
    for path in session_sources() {
        for (line_no, line) in code_lines(&path) {
            for token in FORBIDDEN {
                if line.contains(token) {
                    violations.push(format!("{}:{line_no}: {token}", path.display()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "session/ must stay portable:\n{}",
        violations.join("\n")
    );
}

#[test]
fn only_the_clock_seam_reads_the_wall_clock() {
    let mut violations = Vec::new();
    for path in session_sources() {
        if path.file_name().is_some_and(|name| name == "clock.rs") {
            continue;
        }
        for (line_no, line) in code_lines(&path) {
            for token in CLOCK_READS {
                if line.contains(token) {
                    violations.push(format!("{}:{line_no}: {token}", path.display()));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "only clock.rs may read the clock:\n{}",
        violations.join("\n")
    );
}
