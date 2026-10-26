//! On-disk and IPC identity strings live only in `src/identity.rs`. A hardcoded
//! `omairc-poc` elsewhere under `src/` means a second home for the same name.
//!
//! `omairc-poc` may remain in `identity.rs` (and this test) when the `dev`
//! feature is enabled. The cargo binary is `omairc` and is not an exception.

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

/// Hits of the banned name in `code` (comments already blanked).
fn banned_hits(code: &str) -> Vec<(usize, String)> {
    banned_pattern()
        .find_iter(code)
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
        "{BANNED} must live only in src/{HOME}:\n{}",
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
