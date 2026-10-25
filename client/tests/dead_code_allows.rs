//! The dead-code ratchet. A lint attribute that silences `dead_code` silences the
//! compiler's proof that an item has no caller, so each one under `src/` and `tests/`
//! must say why it stays, and the total may only fall. The counted attributes are
//! `allow` and `expect`, outer or inner (`#[...]`, `#![...]`), and one inside `cfg_attr`,
//! that name `dead_code` or a lint that silences it: the `unused` group and `warnings`.
//! A specific lint such as `unused_imports` does not silence `dead_code`, so it is not
//! counted:
//!
//! - every such attribute (`#[allow(dead_code)]`, `#[expect(unused)]`, `#![allow(unused)]`,
//!   the same with more lints, or one inside `cfg_attr`) needs a plain `//` line comment
//!   directly above it, skipping other attribute lines, or after its `]` on the same
//!   line. A doc comment does not count: it describes the item, not the lint;
//! - `MAX_ALLOWS` is the count the tree holds. A new one pushes the count past it and
//!   fails. A removed one leaves the count under it and fails until `MAX_ALLOWS` is
//!   lowered, so the slack cannot be spent on a later one.
//!
//! The scan reads code with comments blanked (`common::read_code`), then blanks the
//! contents of string, raw string and char literals, so an attribute named in a string
//! or a comment is not an allow. `vendor/` is third-party code and is not scanned.

mod common;

use std::fs;
use std::path::Path;

use common::{code_only, crate_root, line_of, read_code, rust_files_under};
use regex::Regex;

/// The `dead_code` silencers the tree holds. The count may only fall.
const MAX_ALLOWS: usize = 3;

/// The lints whose `allow` or `expect` silences `dead_code`: `dead_code` itself, the
/// `unused` group that contains it, and `warnings`, which silences every warning.
const SILENCERS: &[&str] = &["dead_code", "unused", "warnings"];

/// One `allow(...)` or `expect(...)` attribute that silences `dead_code`.
#[derive(Debug, PartialEq, Eq)]
struct Allow {
    /// The 1-based line where the attribute starts.
    line: usize,
    /// Whether a plain `//` comment gives the reason.
    explained: bool,
}

/// Every tree-wide `dead_code` allow, with the file it sits in.
fn tree_allows() -> Vec<(String, Allow)> {
    let mut found = Vec::new();
    for root in ["src", "tests"] {
        for file in rust_files_under(&crate_root().join(root)) {
            let source = fs::read_to_string(&file)
                .unwrap_or_else(|err| panic!("read {}: {err}", file.display()));
            let code = read_code(&file);
            let name = relative(&file);
            found.extend(
                allows_in(&source, &code)
                    .into_iter()
                    .map(|allow| (name.clone(), allow)),
            );
        }
    }
    found
}

/// The `dead_code` allows in one file. `source` is the file as written, which the reason
/// check reads; `code` is the same text with comments blanked.
fn allows_in(source: &str, code: &str) -> Vec<Allow> {
    let masked = blank_literals(code);
    let raw: Vec<&str> = source.lines().collect();
    attribute_pattern()
        .captures_iter(&masked)
        .filter(|caps| caps[1].split(',').any(silences_dead_code))
        .map(|caps| {
            let whole = caps.get(0).expect("whole match");
            let start_line = line_of(&masked, whole.start());
            let closer = whole.end() - 1;
            let closer_line = line_of(&masked, closer);
            let line_start = masked[..closer].rfind('\n').map_or(0, |at| at + 1);
            let tail = raw
                .get(closer_line - 1)
                .and_then(|text| text.get(closer - line_start + 1..))
                .unwrap_or("");
            Allow {
                line: start_line,
                explained: plain_comment(tail.trim_start()) || comment_above(&raw, start_line),
            }
        })
        .collect()
}

/// An `allow(...)` or `expect(...)` attribute, inner or outer, and its argument list.
/// `[^\]]` stops at the attribute's own `]`, so one match never runs into the next
/// attribute.
fn attribute_pattern() -> Regex {
    Regex::new(r"#!?\[[^\]]*\b(?:allow|expect)\s*\(([^)]*)\)[^\]]*\]")
        .expect("attribute pattern compiles")
}

/// Whether one entry of an attribute's lint list silences `dead_code`. A path counts by
/// its last segment, so `clippy::unused` is still the `unused` group.
fn silences_dead_code(lint: &str) -> bool {
    let name = lint.trim().rsplit("::").next().unwrap_or("");
    SILENCERS.contains(&name)
}

/// Whether `text` is a plain `//` line comment with words in it. A doc comment (`///`,
/// `//!`) and an empty `//` do not give a reason.
fn plain_comment(text: &str) -> bool {
    text.strip_prefix("//").is_some_and(|rest| {
        !(rest.starts_with('/') || rest.starts_with('!')) && !rest.trim().is_empty()
    })
}

/// Whether the nearest line above `line` (1-based), skipping other attribute lines, is a
/// plain comment. A blank line in between breaks the link.
fn comment_above(raw: &[&str], line: usize) -> bool {
    let mut index = line - 1;
    while index > 0 {
        index -= 1;
        let text = raw[index].trim_start();
        if !(text.starts_with("#[") || text.starts_with("#![")) {
            return plain_comment(text);
        }
    }
    false
}

/// `code` with every string, raw string and char literal replaced by spaces of the same
/// UTF-8 width, delimiters included. Line breaks stay, so byte offsets and line numbers
/// still match the source.
fn blank_literals(code: &str) -> String {
    let chars: Vec<char> = code.chars().collect();
    let mut out = String::with_capacity(code.len());
    let mut at = 0;
    while at < chars.len() {
        let end = literal_end(&chars, at);
        if end == at {
            out.push(chars[at]);
            at += 1;
            continue;
        }
        for &c in &chars[at..end] {
            if c == '\n' {
                out.push('\n');
            } else {
                out.push_str(&" ".repeat(c.len_utf8()));
            }
        }
        at = end;
    }
    out
}

/// Where the literal that starts at `at` ends, exclusive, or `at` when no literal starts
/// there.
fn literal_end(chars: &[char], at: usize) -> usize {
    let starts_word = at == 0 || !is_ident(chars[at - 1]);
    match chars[at] {
        '"' => quoted_end(chars, at + 1),
        'b' if starts_word && chars.get(at + 1) == Some(&'"') => quoted_end(chars, at + 2),
        'r' | 'b' if starts_word => raw_end(chars, at).unwrap_or(at),
        '\'' => char_end(chars, at),
        _ => at,
    }
}

/// The end of a quoted string whose body starts at `from`. A backslash skips the next
/// character, so `\"` does not close it.
fn quoted_end(chars: &[char], from: usize) -> usize {
    let mut j = from;
    while j < chars.len() {
        match chars[j] {
            '\\' => j += 2,
            '"' => return j + 1,
            _ => j += 1,
        }
    }
    chars.len()
}

/// The end of a raw string (`r"..."`, `r#"..."#`, `br"..."`) that starts at `at`, or
/// `None` when `at` does not open one.
fn raw_end(chars: &[char], at: usize) -> Option<usize> {
    let mut i = at;
    if chars[i] == 'b' {
        i += 1;
    }
    if chars.get(i) != Some(&'r') {
        return None;
    }
    i += 1;
    let hashes = chars[i..].iter().take_while(|&&c| c == '#').count();
    i += hashes;
    if chars.get(i) != Some(&'"') {
        return None;
    }
    let mut closing = vec!['"'];
    closing.extend(std::iter::repeat_n('#', hashes));
    let mut j = i + 1;
    while j + closing.len() <= chars.len() {
        if chars[j..j + closing.len()] == closing[..] {
            return Some(j + closing.len());
        }
        j += 1;
    }
    Some(chars.len())
}

/// The end of a char literal (`'x'`, `'\''`, `'\u{1F600}'`) that starts at `at`. Anything
/// else after a quote is a lifetime or a label, so it is not a literal.
fn char_end(chars: &[char], at: usize) -> usize {
    if chars.get(at + 1) == Some(&'\\') {
        let mut j = at + 3;
        while j < chars.len() && chars[j] != '\'' && chars[j] != '\n' {
            j += 1;
        }
        return (j + 1).min(chars.len());
    }
    if chars.get(at + 2) == Some(&'\'') {
        return at + 3;
    }
    at
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// A file's path under the crate root, for messages.
fn relative(path: &Path) -> String {
    path.strip_prefix(crate_root())
        .unwrap_or(path)
        .display()
        .to_string()
}

/// Every allow with no reason, as a message naming its file and line.
fn unexplained(found: &[(String, Allow)]) -> Vec<String> {
    found
        .iter()
        .filter(|(_, allow)| !allow.explained)
        .map(|(file, allow)| {
            format!(
                "{file}:{}: this `allow` or `expect` silences `dead_code` but has no reason. Put a `//` comment directly above it, or after its `]`, that says why the item stays.",
                allow.line
            )
        })
        .collect()
}

/// Whether `total` moved off `max`. A rise means a new allow; a fall means the ratchet
/// has to tighten to the new count.
fn count_violations(total: usize, max: usize, found: &[(String, Allow)]) -> Vec<String> {
    if total > max {
        let places: Vec<String> = found
            .iter()
            .map(|(file, allow)| format!("{file}:{}", allow.line))
            .collect();
        vec![format!(
            "{total} `dead_code` silencers, up from MAX_ALLOWS = {max}. Remove the new one, or make the item used or `#[cfg(test)]`. Silencers at: {}",
            places.join(", ")
        )]
    } else if total < max {
        vec![format!(
            "{total} `dead_code` silencers, under MAX_ALLOWS = {max}. Lower MAX_ALLOWS to {total} so the ratchet keeps the gain."
        )]
    } else {
        Vec::new()
    }
}

#[test]
fn every_dead_code_allow_says_why_it_stays() {
    let found = tree_allows();
    let problems = unexplained(&found);
    assert!(
        problems.is_empty(),
        "dead_code allows without a reason:\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_dead_code_allow_count_only_falls() {
    let found = tree_allows();
    let problems = count_violations(found.len(), MAX_ALLOWS, &found);
    assert!(
        problems.is_empty(),
        "the dead_code ratchet moved:\n{}",
        problems.join("\n")
    );
}

/// The scanner's view of `source`, with the comment blanking `read_code` applies.
fn scan(source: &str) -> Vec<Allow> {
    allows_in(source, &code_only(source))
}

#[test]
fn the_scanner_finds_each_form_of_the_allow() {
    let found = |source: &str| {
        scan(source)
            .iter()
            .map(|allow| allow.line)
            .collect::<Vec<_>>()
    };
    assert_eq!(found("#[allow(dead_code)]\nfn a() {}\n"), [1]);
    assert_eq!(found("fn a() {}\n#![allow(dead_code)]\n"), [2]);
    assert_eq!(found("#[allow(unused, dead_code)]\nfn a() {}\n"), [1]);
    assert_eq!(
        found("#[allow(dead_code, unused_imports)]\npub mod m;\n"),
        [1]
    );
    assert_eq!(
        found("#[cfg_attr(test, allow(dead_code))]\nfn a() {}\n"),
        [1]
    );
    assert_eq!(found("#[allow(\n    dead_code,\n)]\nfn a() {}\n"), [1]);
    assert_eq!(
        found("#[allow(unused_variables)]\nfn a() {}\n"),
        Vec::<usize>::new()
    );
    assert_eq!(
        found("#[allow(clippy::too_many_arguments)]\n"),
        Vec::<usize>::new()
    );
    assert_eq!(found("#[cfg(test)]\nfn a() {}\n"), Vec::<usize>::new());
    assert_eq!(found("#[allow(unused)]\nfn a() {}\n"), [1]);
    assert_eq!(found("#![allow(unused)]\n"), [1]);
    assert_eq!(found("#[allow(warnings)]\nfn a() {}\n"), [1]);
    assert_eq!(found("#[expect(dead_code)]\nfn a() {}\n"), [1]);
    assert_eq!(found("#[expect(unused)]\nfn a() {}\n"), [1]);
    assert_eq!(found("fn a() {}\n#![expect(dead_code)]\n"), [2]);
    assert_eq!(
        found("#[expect(dead_code, reason = \"the harness calls it\")]\nfn a() {}\n"),
        [1]
    );
    assert_eq!(
        found("#[cfg_attr(test, expect(unused))]\nfn a() {}\n"),
        [1]
    );
    assert_eq!(
        found("#[allow(clippy::unused)]\nfn a() {}\n"),
        [1],
        "a path counts by its last segment"
    );
    assert_eq!(
        found("#[allow(unused_mut, unused_imports)]\nfn a() {}\n"),
        Vec::<usize>::new(),
        "a specific unused_ lint does not silence dead_code"
    );
    assert_eq!(
        found("#[expect(unused_variables)]\nfn a() {}\n"),
        Vec::<usize>::new()
    );
}

#[test]
fn the_scanner_skips_strings_and_comments_that_name_one() {
    let found = |source: &str| scan(source).len();
    assert_eq!(found("let s = \"#[allow(dead_code)]\";\n"), 0);
    assert_eq!(found("let s = r#\"#[allow(dead_code)]\"#;\n"), 0);
    assert_eq!(found("let s = \"a\\\"#[allow(dead_code)]\";\n"), 0);
    assert_eq!(found("// #[allow(dead_code)]\n"), 0);
    assert_eq!(found("/* #[allow(dead_code)] */\n"), 0);
    assert_eq!(found("//! #[allow(dead_code)]\n"), 0);
    assert_eq!(found("let s = \"#[expect(unused)]\";\n"), 0);
    assert_eq!(found("// #[expect(dead_code)]\n"), 0);
    assert_eq!(found("/// #[allow(unused)]\nfn a() {}\n"), 0);
    assert_eq!(
        found("fn a() {}\n#[cfg(test)]\n#[allow(dead_code)]\n\nmod tests {}\n"),
        1,
        "an attribute line in the source counts, even under a cfg(test)"
    );
    assert_eq!(
        found("let quote = '\"';\n#[allow(dead_code)]\nfn a() {}\n"),
        1,
        "a char literal holding a quote does not open a string"
    );
    assert_eq!(
        found("fn f<'a>(x: &'a str) {}\n#[allow(dead_code)]\nfn g() {}\n"),
        1,
        "a lifetime does not open a char literal"
    );
}

#[test]
fn the_scanner_reads_the_reason_from_plain_comments_only() {
    let reason = |source: &str| scan(source).first().map(|allow| allow.explained);
    assert_eq!(
        reason("// Why it stays.\n#[allow(dead_code)]\nfn a() {}\n"),
        Some(true)
    );
    assert_eq!(
        reason("// Why it stays.\n#[cfg(test)]\n#[allow(dead_code)]\nmod t {}\n"),
        Some(true),
        "other attribute lines between the comment and the allow are skipped"
    );
    assert_eq!(reason("#[allow(dead_code)] // Why it stays.\n"), Some(true));
    assert_eq!(reason("#[allow(dead_code)]\nfn a() {}\n"), Some(false));
    assert_eq!(
        reason("/// Doc for the item.\n#[allow(dead_code)]\nfn a() {}\n"),
        Some(false)
    );
    assert_eq!(
        reason("//! Module doc.\n#![allow(dead_code)]\n"),
        Some(false)
    );
    assert_eq!(reason("//\n#[allow(dead_code)]\nfn a() {}\n"), Some(false));
    assert_eq!(reason("#[allow(dead_code)] //\n"), Some(false));
    assert_eq!(
        reason("/* Why it stays. */\n#[allow(dead_code)]\n"),
        Some(false)
    );
    assert_eq!(
        reason("// Why it stays.\n\n#[allow(dead_code)]\nfn a() {}\n"),
        Some(false),
        "a blank line breaks the link"
    );
    assert_eq!(
        reason("// Why it stays.\n#[expect(unused)]\nfn a() {}\n"),
        Some(true)
    );
    assert_eq!(
        reason("#[expect(dead_code)]\nfn a() {}\n"),
        Some(false),
        "expect needs the same comment as allow"
    );
    assert_eq!(
        reason("//! Module doc.\n#![expect(dead_code)]\n"),
        Some(false)
    );
}

#[test]
fn the_ratchet_flags_an_unexplained_allow_and_each_count_move() {
    let bare = [(
        "src/a.rs".to_string(),
        Allow {
            line: 3,
            explained: false,
        },
    )];
    assert_eq!(unexplained(&bare).len(), 1, "a bare allow is flagged");

    let found: Vec<(String, Allow)> = (0..MAX_ALLOWS + 1)
        .map(|index| {
            (
                "src/a.rs".to_string(),
                Allow {
                    line: index + 1,
                    explained: true,
                },
            )
        })
        .collect();
    assert_eq!(
        count_violations(MAX_ALLOWS + 1, MAX_ALLOWS, &found).len(),
        1,
        "a rise is flagged"
    );
    assert_eq!(
        count_violations(MAX_ALLOWS - 1, MAX_ALLOWS, &found[..MAX_ALLOWS - 1]).len(),
        1,
        "a fall is flagged, so the ratchet tightens"
    );
    assert!(count_violations(MAX_ALLOWS, MAX_ALLOWS, &found[..MAX_ALLOWS]).is_empty());
}
