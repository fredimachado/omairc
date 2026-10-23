//! One home for message helpers. The field readers over a parsed `IrcMessage` live
//! on `IrcMessage` in `src/irc/message.rs` (`text_param`, `param_index`, `tag_text`,
//! `has_tag`, `msgid`), the source test on `IrcPrefix::is_user` lives there too, and
//! the service and notice-target tests live in `src/irc/servicenick.rs`. A second
//! copy in any other file is the drift this gate stops: callers reach the home.
//!
//! The gate matches a definition, not a call. `fn name(` followed by a reference or a
//! named argument is a function over a message or a text, so it is a copy. A method
//! that takes `self` by value, such as the typing phase's `tag_value(self)`, is a
//! different function and is not matched.

mod common;

use std::path::Path;

use common::{code_only, crate_root, line_of, read_code, rust_files_under};
use regex::Regex;

/// Each helper name and the one file that may define it. `None` retires the name:
/// no file may define it, because its meaning now lives under a new name.
const HELPERS: &[(&str, Option<&str>)] = &[
    ("text_param", Some("src/irc/message.rs")),
    ("param", Some("src/irc/message.rs")),
    ("param_index", Some("src/irc/message.rs")),
    ("tag_text", Some("src/irc/message.rs")),
    ("has_tag", Some("src/irc/message.rs")),
    ("msgid", Some("src/irc/message.rs")),
    ("is_service_source", Some("src/irc/servicenick.rs")),
    ("is_network_notice_target", Some("src/irc/servicenick.rs")),
    ("tag_value", None),
    ("tag_present", None),
    ("has_user_prefix", None),
    ("is_service_user", None),
];

/// A definition of `name` whose first argument is a reference or a named argument.
fn definition_pattern(name: &str) -> Regex {
    Regex::new(&format!(r"\bfn\s+{name}\s*\(\s*(?:&|\w+\s*:)"))
        .expect("definition pattern compiles")
}

/// One helper with its home and its compiled definition pattern.
struct Helper {
    name: &'static str,
    home: Option<&'static str>,
    definition: Regex,
}

/// Every entry of `HELPERS`, compiled once for a scan.
fn helpers() -> Vec<Helper> {
    HELPERS
        .iter()
        .map(|&(name, home)| Helper {
            name,
            home,
            definition: definition_pattern(name),
        })
        .collect()
}

/// Helper definitions in `code` (comments already blanked) that sit outside their
/// home. `file` is the crate-relative path with forward slashes. Each hit is its line
/// and the helper name.
fn copies_outside_home(helpers: &[Helper], code: &str, file: &str) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    for helper in helpers {
        if helper.home == Some(file) {
            continue;
        }
        for hit in helper.definition.find_iter(code) {
            found.push((line_of(code, hit.start()), format!("fn {}", helper.name)));
        }
    }
    found.sort();
    found
}

fn key_under_crate(path: &Path) -> String {
    path.strip_prefix(crate_root())
        .expect("file under the crate root")
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

#[test]
fn each_helper_is_defined_only_in_its_home() {
    let src = crate_root().join("src");
    let files = rust_files_under(&src);
    assert!(!files.is_empty(), "no .rs files under {}", src.display());

    let helpers = helpers();
    let mut problems = Vec::new();
    for file in &files {
        let key = key_under_crate(file);
        for (line, text) in copies_outside_home(&helpers, &read_code(file), &key) {
            problems.push(format!("{key}:{line}: {text}"));
        }
    }
    assert!(
        problems.is_empty(),
        "message helpers have one home each (IrcMessage in src/irc/message.rs, servicenick.rs). \
         Call the home instead of defining a copy:\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_homes_define_what_the_gate_names() {
    // A home that lost its definition would make the gate pass for the wrong reason.
    for helper in helpers() {
        let Some(home) = helper.home else { continue };
        let code = read_code(&crate_root().join(home));
        assert!(
            helper.definition.is_match(&code),
            "{home} does not define {}",
            helper.name
        );
    }
}

#[test]
fn the_gate_flags_a_copy_in_any_other_file() {
    let copies = [
        "fn has_user_prefix(message: &IrcMessage) -> bool { true }",
        "fn is_service_user(message: &IrcMessage, features: &IrcServerFeatures) -> bool { true }",
        "fn is_network_notice_target(target: &str) -> bool { true }",
        "fn tag_value(message: &IrcMessage, name: &str) -> Option<String> { None }",
        "fn tag_present(message: &IrcMessage, name: &str) -> bool { true }",
        "fn param(message: &IrcMessage, index: usize) -> String { String::new() }",
        "pub(crate) fn msgid(message: &IrcMessage) -> String { String::new() }",
        "fn param_index(message: &IrcMessage, value: &str) -> Option<usize> { None }",
        "fn text_param(message: &IrcMessage, index: usize) -> String { String::new() }",
        "fn tag_text(message: &IrcMessage, name: &str) -> Option<String> { None }",
        "fn has_tag(message: &IrcMessage, name: &str) -> bool { true }",
        "fn is_service_source(message: &IrcMessage) -> bool { true }",
    ];
    let helpers = helpers();
    for source in copies {
        let hits = copies_outside_home(&helpers, &code_only(source), "src/session/wire.rs");
        assert_eq!(hits.len(), 1, "not flagged: {source}");
    }
}

#[test]
fn the_gate_allows_the_homes_comments_and_the_by_value_method() {
    let allowed = [
        (
            "src/irc/message.rs",
            "    pub fn param(&self, index: usize) -> Option<&[u8]> { None }",
        ),
        (
            "src/irc/message.rs",
            "    pub fn tag_text(&self, name: &str) -> Option<String> { None }",
        ),
        (
            "src/irc/servicenick.rs",
            "pub fn is_network_notice_target(target: &str) -> bool { true }",
        ),
        (
            "src/irc/typing.rs",
            "    pub fn tag_value(self) -> &'static str { \"\" }",
        ),
        (
            "src/session/wire.rs",
            "// fn has_user_prefix(message: &IrcMessage) -> bool",
        ),
        (
            "src/session/wire.rs",
            "let text = message.text_param(0); let id = message.msgid();",
        ),
        (
            "src/session/history.rs",
            "fn msgid_dedup_skips_live_then_replay() {}",
        ),
    ];
    let helpers = helpers();
    for (file, source) in allowed {
        let hits = copies_outside_home(&helpers, &code_only(source), file);
        assert!(
            hits.is_empty(),
            "wrongly flagged in {file}: {source} -> {hits:?}"
        );
    }
}

#[test]
fn the_gate_reports_line_numbers() {
    let source = "fn a() {}\n// fn param(&self) in a comment\nfn tag_present(message: &IrcMessage) -> bool { true }\n";
    assert_eq!(
        copies_outside_home(&helpers(), &code_only(source), "src/session/x.rs"),
        vec![(3, "fn tag_present".to_string())]
    );
}
