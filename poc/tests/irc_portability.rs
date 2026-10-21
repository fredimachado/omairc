//! Rule 3 of the GPUI plan: `src/irc/` stays portable. It holds no socket, no
//! filesystem, no thread, no process and no gpui or tokio, so the same protocol
//! core can run under any transport and any toolkit.
//!
//! This mirrors the Go gate in `bin/check-conventions`
//! (`check_go_irc_platform_neutrality`), which keeps `tui/internal/irc` free of
//! `net`, `os` and `crypto/tls`. Sockets, threads and the filesystem belong in
//! `session/`, `storage/` and `desktop/`.

mod common;

use std::path::Path;

use common::{code_only, crate_root, line_of, read_code, rust_files_under};

/// Paths whose use is forbidden anywhere in `src/irc/`. `crate::session` is the
/// layer above the protocol core, so the core never imports from it.
const FORBIDDEN_PATHS: &[&str] = &[
    "std::net",
    "std::fs",
    "std::thread",
    "std::process",
    "crate::session",
];
/// Crate roots that are forbidden anywhere in `src/irc/`.
const FORBIDDEN_CRATES: &[&str] = &["gpui", "tokio"];
/// The `std` modules above, by name, for `use std::{io, fs}` style groups.
const FORBIDDEN_STD_MODULES: &[&str] = &["net", "fs", "thread", "process"];

/// Every forbidden use in `code` (comments already blanked), with its line.
fn violations(code: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for token in FORBIDDEN_PATHS.iter().chain(FORBIDDEN_CRATES) {
        for (offset, _) in code.match_indices(token) {
            if bounded(code, offset, token.len()) {
                found.push((line_of(code, offset), (*token).to_string()));
            }
        }
    }
    found.extend(std_group_violations(code));
    found.sort();
    found
}

/// True when the match at `offset` is a whole path segment, not part of a longer name.
fn bounded(code: &str, offset: usize, len: usize) -> bool {
    let before = code[..offset].chars().next_back();
    let after = code[offset + len..].chars().next();
    !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `use std::{io, net::TcpStream};` names `net` inside the braces. Each element's
/// first path segment is checked against the forbidden `std` modules.
fn std_group_violations(code: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for (offset, _) in code.match_indices("std::{") {
        if !bounded(code, offset, "std".len()) {
            continue;
        }
        let body = &code[offset + "std::{".len()..];
        let mut depth = 1usize;
        let mut elements = vec![String::new()];
        for c in body.chars() {
            match c {
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                ',' if depth == 1 => {
                    elements.push(String::new());
                    continue;
                }
                _ => {}
            }
            elements.last_mut().expect("one element").push(c);
        }
        for element in elements {
            let head: String = element.trim().chars().take_while(|c| is_ident(*c)).collect();
            if FORBIDDEN_STD_MODULES.contains(&head.as_str()) {
                found.push((line_of(code, offset), format!("std::{{{head}}}")));
            }
        }
    }
    found
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
        "src/irc/ must stay portable (no std::net, std::fs, std::thread, std::process, gpui, tokio, crate::session):\n{}",
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
