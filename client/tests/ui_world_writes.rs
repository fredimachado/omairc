//! Rule 2 of the GPUI plan: the UI never writes state. The controller owns every write
//! to the IRC state, and `src/ui/` reads its snapshots and calls its intents.
//! This gate scans `src/ui/` for writes to a world value. A write is one of:
//! - a mutable borrow of the world, `&mut self.world` or `&mut world`, which is how a
//! model function would hand the state over to be changed
//! - a call to a `&mut self` method on the world or a collection mutator on one of its
//! fields, such as `self.world.push_status(..)` or `.networks.retain(..)`
//! - an assignment into the world, `self.world.x =..` or `world.x +=..`.
//! The UI holds no world at all, so the gate asserts that the scan finds
//! nothing. A write that appears again is a new failure: route it through a
//! `Controller` intent instead of an allowlist entry.

mod common;

use std::path::Path;

use common::{code_only, crate_root, line_of, read_code, rust_files_under};
use regex::Regex;

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
fn ui_never_writes_a_world() {
    let ui = crate_root().join("src").join("ui");
    let files = rust_files_under(&ui);
    assert!(!files.is_empty(), "no .rs files under {}", ui.display());

    let mut problems = Vec::new();
    for file in &files {
        for (line, text) in world_writes(&read_code(file)) {
            problems.push(format!("    {}:{line}: {text}", key_under_src(file)));
        }
    }

    assert!(
        problems.is_empty(),
        "src/ui/ must not write state. Route the write through a Controller intent:\n{}",
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

#[test]
fn a_controller_intent_is_not_a_world_write() {
    // The shell calls intents on its controller. Those are method calls on a value the
    // controller owns, not writes to a world, so the gate lets them through.
    let intents = [
        "self.controller.select_conversation(&net, &key);",
        "self.controller.clear_status(&network_id);",
        "let ok = self.controller.join_channel(network_id, channel);",
        "self.controller.set_network_collapsed(&id, true);",
    ];
    for source in intents {
        assert!(world_writes(&code_only(source)).is_empty(), "{source}");
    }
}
