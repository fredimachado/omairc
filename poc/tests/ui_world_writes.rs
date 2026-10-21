//! Rule 2 of the GPUI plan: the UI never writes state. The controller owns every
//! write to the world, and `src/ui/` only reads it and calls controller intents.
//!
//! This gate counts world writes per file under `src/ui/` and compares them with
//! `ALLOWLIST`, the writes that exist today. A write is one of:
//!
//! - a mutable borrow of the world, `&mut self.world` or `&mut world`, which is
//!   how the UI hands the world to a model function such as `open_direct`;
//! - a call to a `&mut self` method on the world (`MockWorld`, `Network` and
//!   `Conversation` in `src/model/mock.rs`) or a collection mutator on a world
//!   field, such as `self.world.push_status(..)` or `.networks.retain(..)`;
//! - an assignment into the world, `self.world.x = ..` or `world.x += ..`.
//!
//! Phase 0 records today's writes. Phase 3 routes each one through a controller
//! intent and the allowlist shrinks to zero; this test then asserts an empty list.
//!
//! The ratchet runs both ways. A file may not exceed its count, and a new file
//! may not appear. A count may not sit above what the file has either, so each
//! removed write must lower its entry. The allowlist only goes down.

mod common;

use std::collections::BTreeMap;
use std::path::Path;

use common::{code_only, crate_root, line_of, read_code, rust_files_under};
use regex::Regex;

/// Writes per file, keyed by path under `src/`. Recorded in Phase 0.
const ALLOWLIST: &[(&str, usize)] = &[
    ("ui/shell/commands.rs", 6),
    ("ui/shell/connect.rs", 2),
    ("ui/shell/inbox.rs", 1),
    ("ui/shell/mod.rs", 1),
    ("ui/shell/slash.rs", 6),
    ("ui/shell/unread.rs", 2),
];

/// Methods that take `&mut self` on the world types, plus collection mutators.
const MUTATORS: &str = "find_mut|record_outgoing|clear_conversation|push_status|clear_status|\
    set_last_error|mark_read|network_mut|push|push_back|push_front|pop|pop_back|pop_front|\
    insert|remove|retain|retain_mut|clear|truncate|extend|append|drain|sort|sort_by|\
    sort_by_key|sort_unstable|dedup|swap|reverse|iter_mut|get_mut|last_mut|first_mut|\
    as_mut|entry|resize|split_off";

/// One step of a field, index or call chain after `world`. Parentheses may nest once.
const ELEM: &str = r"(?:\s*\.\s*\w+|\s*\((?:[^()]|\([^()]*\))*\)|\s*\[[^\[\]]*\])";

/// The three write shapes as one alternation, so each occurrence counts once.
fn write_pattern() -> String {
    let borrow = r"&\s*mut\s+(?:self\s*\.\s*)?world\b";
    let mutator = format!(r"\b(?:self\s*\.\s*)?world\b{ELEM}*\s*\.\s*(?:{MUTATORS})\s*\(");
    let assign = format!(
        r"(?:\bself\s*\.\s*world\b{ELEM}*|\bworld\b{ELEM}+)\s*(?:[-+*/%&|^]|<<|>>)?=(?:[^=>]|$)"
    );
    format!("(?:{borrow})|(?:{mutator})|(?:{assign})")
}

/// Every world write in `code` (comments already blanked): its line and the matched text.
fn world_writes(code: &str) -> Vec<(usize, String)> {
    let pattern = Regex::new(&write_pattern()).expect("write pattern compiles");
    pattern
        .find_iter(code)
        .map(|found| {
            let text = found.as_str().split_whitespace().collect::<Vec<_>>().join(" ");
            (line_of(code, found.start()), text)
        })
        .collect()
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

#[test]
fn allowlist_has_unique_keys() {
    let mut seen = std::collections::BTreeSet::new();
    for (file, _) in ALLOWLIST {
        assert!(seen.insert(*file), "duplicate allowlist entry: {file}");
    }
}

#[test]
fn ui_world_writes_do_not_grow() {
    let ui = crate_root().join("src").join("ui");
    let files = rust_files_under(&ui);
    assert!(!files.is_empty(), "no .rs files under {}", ui.display());

    let mut actual: BTreeMap<String, Vec<(usize, String)>> = BTreeMap::new();
    for file in &files {
        let hits = world_writes(&read_code(file));
        if !hits.is_empty() {
            actual.insert(key_under_src(file), hits);
        }
    }

    let allowed: BTreeMap<&str, usize> = ALLOWLIST.iter().copied().collect();
    let mut problems = Vec::new();

    for (file, hits) in &actual {
        let allow = allowed.get(file.as_str()).copied().unwrap_or(0);
        if hits.len() > allow {
            let lines: Vec<String> = hits.iter().map(|(line, text)| format!("    {file}:{line}: {text}")).collect();
            problems.push(format!(
                "{file}: {} world writes, the allowlist allows {allow}. Route the new write through a controller intent.\n{}",
                hits.len(),
                lines.join("\n")
            ));
        }
    }

    for (file, allow) in &allowed {
        let count = actual.get(*file).map_or(0, Vec::len);
        if count < *allow {
            problems.push(format!(
                "{file}: {count} world writes left, the allowlist still says {allow}. Lower the entry to {count}."
            ));
        }
    }

    assert!(
        problems.is_empty(),
        "src/ui/ must not write the world (Phase 3 drives this to zero):\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_scanner_counts_each_write_shape_once() {
    let counts = [
        ("self.world.push_status(network, line);", 1),
        ("open_direct(&mut self.world, &network_id, nick)", 1),
        ("for network in &mut world.networks {}", 1),
        ("self.world.networks[0].unread += 1;", 1),
        ("self.world.networks[0].name = name;", 1),
        ("self.world = load_world();", 1),
        ("self.world\n    .find_mut(&target)", 1),
        ("self.world.networks.retain(|c| c.name != \"#bench\");", 1),
        ("let mut world = load_world();", 0),
        ("let x = self.world.find(&target);", 0),
        ("let c = self.world.clone();", 0),
        ("if self.world.unread == 1 {}", 0),
        ("if self.world.unread <= 1 {}", 0),
        ("if self.world.unread >= 1 {}", 0),
        ("if self.world.unread != 1 {}", 0),
        ("self.sidebar.world_focus = 3;", 0),
        ("let mark = |w: &mut MockWorld| w.x = 1;", 0),
        ("// self.world.push_status(network, line);", 0),
        ("/* open_direct(&mut self.world, id, nick) */", 0),
    ];
    for (source, expected) in counts {
        let hits = world_writes(&code_only(source));
        assert_eq!(hits.len(), expected, "writes in {source:?}: {hits:?}");
    }
}
