//! Rule 3 of the GPUI plan: `src/irc/` stays portable. It holds no socket, no
//! filesystem, no thread, no process, no storage and no gpui or tokio, so the same
//! protocol core can run under any transport, store and toolkit.
//!
//! This mirrors the Go gate in `bin/check-conventions`
//! (`check_go_irc_platform_neutrality`), which keeps `tui/internal/irc` free of
//! `net`, `os` and `crypto/tls`. Sockets, threads and the filesystem belong in
//! `session/`, `storage/` and `desktop/`.

mod common;
#[path = "common/forbidden.rs"]
mod forbidden;

use std::path::Path;

use common::{code_only, crate_root, line_of, read_code, rust_files_under};
use forbidden::{group_heads, segment_offsets};

/// Paths whose use is forbidden anywhere in `src/irc/`. `crate::session` and
/// `crate::storage` are layers above the protocol core, so the core never imports
/// from them. The reducer takes its closed list through a trait the storage layer
/// implements.
const FORBIDDEN_PATHS: &[&str] = &[
    "std::net",
    "std::fs",
    "std::thread",
    "std::process",
    "crate::session",
    "crate::storage",
];
/// Crate roots that are forbidden anywhere in `src/irc/`.
const FORBIDDEN_CRATES: &[&str] = &["gpui", "tokio"];
/// The `std` modules above, by name, for `use std::{io, fs}` style groups.
const FORBIDDEN_STD_MODULES: &[&str] = &["net", "fs", "thread", "process"];

/// Every forbidden use in `code` (comments already blanked), with its line.
fn violations(code: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for token in FORBIDDEN_PATHS.iter().chain(FORBIDDEN_CRATES) {
        for offset in segment_offsets(code, token) {
            found.push((line_of(code, offset), (*token).to_string()));
        }
    }
    found.extend(std_group_violations(code));
    found.sort();
    found
}

/// `use std::{io, net::TcpStream};` names `net` inside the braces. Each element's
/// first path segment is checked against the forbidden `std` modules.
fn std_group_violations(code: &str) -> Vec<(usize, String)> {
    group_heads(code, "std")
        .into_iter()
        .filter(|(_, head)| FORBIDDEN_STD_MODULES.contains(&head.as_str()))
        .map(|(line, head)| (line, format!("std::{{{head}}}")))
        .collect()
}

fn relative(path: &Path) -> String {
    path.strip_prefix(crate_root()).unwrap_or(path).display().to_string()
}

#[test]
fn irc_sources_stay_portable() {
    let root = crate_root().join("src").join("irc");
    let files = rust_files_under(&root);
    assert!(!files.is_empty(), "no .rs files under {}", root.display());

    let mut problems = Vec::new();
    for file in &files {
        for (line, token) in violations(&read_code(file)) {
            problems.push(format!("{}:{line}: {token}", relative(file)));
        }
    }
    assert!(
        problems.is_empty(),
        "src/irc/ must stay portable (no std::net, std::fs, std::thread, std::process, gpui, tokio, crate::session, crate::storage):\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_gate_flags_each_forbidden_form() {
    let flagged = [
        "use std::net::TcpStream;",
        "use std::fs;",
        "let f = std::fs::read(path);",
        "use std::thread::spawn;",
        "use std::process::Command;",
        "use gpui::App;",
        "use tokio::spawn;",
        "use crate::session::typing::TypingPhase;",
        "use crate::storage::lists::CaseListStore;",
        "use std::{io, fs};",
        "use std::{io::{self, Read}, thread};",
        "let url = \"https://x\"; use std::process;",
        "let c = '\"'; use std::fs;",
        "use std :: net :: TcpStream;",
        "let x = r#\"ok\"#; use std::net;",
    ];
    for source in flagged {
        assert!(!violations(&code_only(source)).is_empty(), "not flagged: {source}");
    }
}

#[test]
fn the_gate_allows_comments_and_lookalike_names() {
    let allowed = [
        "// std::net is not used here",
        "/// Sockets live in session, never std::net.",
        "/* gpui and tokio */ fn f() {}",
        "/* outer /* nested std::fs */ still comment */ fn g() {}",
        "let mygpui = 1; let tokio_rate = 2;",
        "use std::network::Thing;",
        "use std::io::{self, Write};",
        "let lifetime: &'static str = \"ok\";",
    ];
    for source in allowed {
        assert!(violations(&code_only(source)).is_empty(), "wrongly flagged: {source}");
    }
}

#[test]
fn the_gate_reports_line_numbers() {
    let source = "fn a() {}\n// std::net in a comment\nuse std::fs;\n";
    assert_eq!(violations(&code_only(source)), vec![(3, "std::fs".to_string())]);
}
