//! A full backward scan of one conversation log, for the history search. `read_tail` stops
//! after a fixed count of lines; a search has to look at every line until it has enough hits,
//! so this reads the file from the end in chunks and hands each admitted line to the caller
//! newest first. It owns no UI types and no query: the caller decides what a line is worth.

use std::fs::File;
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use super::conversationlog::{admitted_line, channel_types, reverse_lines};
use crate::irc::reducer::TranscriptLine;

/// Bytes read per chunk. Larger than the tail reader's: a scan reads far more of the file.
const SCAN_CHUNK_SIZE: u64 = 64 * 1024;

/// Hands each line of the log at `path` to `visit`, newest first, until `visit` breaks, the
/// file runs out or `cancel` is set. A missing or unreadable file has no lines. A line the
/// transcript policy would not have stored (a credential) is skipped here, so no caller can
/// forget the filter and a search never shows what the transcript would drop. A line that
/// does not parse, such as one cut short by a crash, is skipped too.
pub fn scan_log(
    path: &Path,
    cancel: &AtomicBool,
    visit: &mut dyn FnMut(TranscriptLine) -> ControlFlow<()>,
) {
    scan_file(path, SCAN_CHUNK_SIZE, cancel, visit);
}

fn scan_file(
    path: &Path,
    chunk_size: u64,
    cancel: &AtomicBool,
    visit: &mut dyn FnMut(TranscriptLine) -> ControlFlow<()>,
) {
    let Ok(file) = File::open(path) else {
        return;
    };
    let channel_types = channel_types();
    reverse_lines(file, chunk_size, cancel, &mut |raw| match admitted_line(raw, &channel_types) {
        Some(line) => visit(line),
        None => ControlFlow::Continue(()),
    });
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use serde_json::json;

    use super::*;
    use crate::util::testdir::TempDir;

    fn record(n: usize, author: &str, body: &str) -> String {
        json!({
            "timestamp": format!("2026-09-04T00:{:02}:{:02}.000Z", n / 60, n % 60),
            "author": author,
            "kind": "message",
            "body": body,
        })
        .to_string()
    }

    fn write_log(dir: &TempDir, lines: &[String], trailing_newline: bool) -> std::path::PathBuf {
        let path = dir.path().join("net").join("#room");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut text = lines.join("\n");
        if trailing_newline {
            text.push('\n');
        }
        std::fs::write(&path, text).unwrap();
        path
    }

    fn bodies(path: &Path, chunk: u64) -> Vec<String> {
        let never = AtomicBool::new(false);
        let mut seen = Vec::new();
        scan_file(path, chunk, &never, &mut |line| {
            seen.push(line.body);
            ControlFlow::Continue(())
        });
        seen
    }

    #[test]
    fn lines_come_newest_first() {
        let dir = TempDir::new("scan-order");
        let path = write_log(
            &dir,
            &(0..5).map(|n| record(n, "anna", &format!("line {n}"))).collect::<Vec<_>>(),
            true,
        );
        assert_eq!(
            bodies(&path, 4096),
            ["line 4", "line 3", "line 2", "line 1", "line 0"]
        );
    }

    #[test]
    fn every_line_is_seen_whatever_the_chunk_size() {
        let dir = TempDir::new("scan-chunks");
        let expected: Vec<String> = (0..40).rev().map(|n| format!("line {n} é")).collect();
        let path = write_log(
            &dir,
            &(0..40).map(|n| record(n, "anna", &format!("line {n} é"))).collect::<Vec<_>>(),
            true,
        );
        // Chunks that fall inside a line, inside a multi-byte char, and exactly on a newline.
        for chunk in [1, 2, 3, 7, 64, 101, 4096] {
            assert_eq!(bodies(&path, chunk), expected, "chunk {chunk}");
        }
    }

    #[test]
    fn a_log_without_a_final_newline_still_gives_its_first_and_last_lines() {
        let dir = TempDir::new("scan-no-newline");
        let path = write_log(
            &dir,
            &[record(0, "anna", "first"), record(1, "anna", "last")],
            false,
        );
        assert_eq!(bodies(&path, 16), ["last", "first"]);
    }

    #[test]
    fn a_line_cut_short_by_a_crash_is_skipped() {
        let dir = TempDir::new("scan-truncated");
        let whole = record(1, "anna", "whole");
        let cut = record(2, "anna", "cut off mid-write");
        let path = write_log(
            &dir,
            &[record(0, "anna", "old"), whole, cut[..cut.len() - 9].to_owned()],
            false,
        );
        assert_eq!(bodies(&path, 32), ["whole", "old"]);
    }

    #[test]
    fn blank_and_garbage_lines_are_skipped() {
        let dir = TempDir::new("scan-garbage");
        let path = write_log(
            &dir,
            &[
                record(0, "anna", "one"),
                String::new(),
                "not json".to_owned(),
                "   ".to_owned(),
                record(1, "anna", "two"),
            ],
            true,
        );
        assert_eq!(bodies(&path, 4096), ["two", "one"]);
    }

    #[test]
    fn a_missing_file_has_no_lines() {
        let dir = TempDir::new("scan-missing");
        assert!(bodies(&dir.path().join("nothing"), 4096).is_empty());
    }

    #[test]
    fn an_empty_file_has_no_lines() {
        let dir = TempDir::new("scan-empty");
        let path = write_log(&dir, &[], false);
        assert!(bodies(&path, 4096).is_empty());
    }

    #[test]
    fn a_credential_line_is_never_handed_over() {
        let dir = TempDir::new("scan-secret");
        let path = write_log(
            &dir,
            &[
                record(0, "anna", "nothing secret"),
                record(1, "fred", "IDENTIFY hunter2"),
                // A wire line that reached a body.
                record(2, "fred", "PRIVMSG NickServ :IDENTIFY hunter2"),
                record(4, "anna", "still nothing secret"),
            ],
            true,
        );
        let seen = bodies(&path, 4096);
        assert!(
            !seen.iter().any(|body| body.contains("hunter2")),
            "a credential came back: {seen:?}"
        );
        assert!(seen.contains(&"nothing secret".to_owned()));
        assert!(seen.contains(&"still nothing secret".to_owned()));
    }

    #[test]
    fn visiting_stops_when_the_caller_breaks() {
        let dir = TempDir::new("scan-break");
        let path = write_log(
            &dir,
            &(0..10).map(|n| record(n, "anna", &format!("line {n}"))).collect::<Vec<_>>(),
            true,
        );
        let never = AtomicBool::new(false);
        let mut count = 0;
        scan_file(&path, 16, &never, &mut |_| {
            count += 1;
            if count == 3 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        });
        assert_eq!(count, 3);
    }

    #[test]
    fn a_cancel_stops_the_scan_at_once() {
        let dir = TempDir::new("scan-cancel");
        let path = write_log(
            &dir,
            &(0..50).map(|n| record(n % 60, "anna", &format!("line {n}"))).collect::<Vec<_>>(),
            true,
        );
        let cancel = AtomicBool::new(false);
        let mut count = 0;
        scan_file(&path, 64, &cancel, &mut |_| {
            count += 1;
            if count == 4 {
                cancel.store(true, Ordering::Relaxed);
            }
            ControlFlow::Continue(())
        });
        assert_eq!(count, 4, "no line is visited after the cancel");

        let already = AtomicBool::new(true);
        let mut none = 0;
        scan_file(&path, 64, &already, &mut |_| {
            none += 1;
            ControlFlow::Continue(())
        });
        assert_eq!(none, 0);
    }
}
