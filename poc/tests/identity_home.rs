//! On-disk and IPC identity strings live only in `src/identity.rs`. A hardcoded
//! `omairc-poc` elsewhere under `src/` means a second home for the same name.
//!
//! The binary / package name `omairc-poc-gpui` is Phase 3 and is allowed until then.
//! This gate matches `omairc-poc` when it is not that binary stem.

mod common;

use std::path::Path;

use common::{crate_root, line_of, read_code, rust_files_under};
use regex::Regex;

/// The folder / endpoint leaf that must not be copied outside `identity.rs`.
const BANNED: &str = "omairc-poc";

/// The one file that may define the banned name.
const HOME: &str = "identity.rs";

fn banned_pattern() -> Regex {
    Regex::new(BANNED).expect("banned pattern compiles")
}

fn key_under_src(path: &Path) -> String {
    let src = crate_root().join("src");
    path.strip_prefix(&src)
        .expect("file under src")
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// True when this match is the cargo binary stem `omairc-poc-gpui` (Phase 3).
fn is_binary_stem(code: &str, start: usize) -> bool {
    code[start..].starts_with("omairc-poc-gpui")
}

/// Hits of the banned name in `code` (comments already blanked), excluding the
/// Phase 3 binary stem.
fn banned_hits(code: &str) -> Vec<(usize, String)> {
    banned_pattern()
        .find_iter(code)
        .filter(|found| !is_binary_stem(code, found.start()))
        .map(|found| {
            let line = line_of(code, found.start());
            let text = found.as_str().to_owned();
            (line, text)
        })
        .collect()
}

#[test]
fn omairc_poc_lives_only_in_identity() {
    let src = crate_root().join("src");
    let files = rust_files_under(&src);
    assert!(!files.is_empty(), "no .rs files under {}", src.display());

    let mut problems = Vec::new();
    for file in &files {
        let key = key_under_src(file);
        if key == HOME || key.ends_with(&format!("/{HOME}")) {
            continue;
        }
        for (line, text) in banned_hits(&read_code(file)) {
            problems.push(format!("{key}:{line}: {text}"));
        }
    }

    assert!(
        problems.is_empty(),
        "{BANNED} must live only in src/{HOME} (binary stem omairc-poc-gpui is Phase 3):\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_gate_flags_a_hardcoded_folder() {
    let code = r#"pub const APP_FOLDER: &str = "omairc-poc";"#;
    let hits = banned_hits(code);
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].1, BANNED);
}

#[test]
fn the_gate_allows_the_binary_stem() {
    let code = r#"name = "omairc-poc-gpui""#;
    assert!(banned_hits(code).is_empty(), "binary stem must stay until Phase 3");
}
