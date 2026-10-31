//! The file-size ratchet. A production file under `src/` stays at or under `LIMIT`
//! lines. A file already over it is recorded in `ALLOWLIST` at its current count
//! and the count can only fall:
//! - a listed file that grows past its recorded count fails, so growth is a split
//! - a listed file that drops to `LIMIT` or below fails until its entry is removed
//! - a listed file that drops but stays over `LIMIT` fails until its count is lowered
//! so the slack cannot be spent later on growth
//! - an unlisted file over `LIMIT` fails, and so does a listed file that no longer exists.
//! The count is production code only: non-blank lines with comments blanked, up to the
//! first `#[cfg(test)]` that introduces an inline test module (`#[cfg(test)] mod name {`)
//! so an inline test module belongs at the end of its file. An out-of-line
//! `#[cfg(test)] mod name;` keeps its body in a file of its own, so it is not a cut and
//! the lines after it still count. A test-only seam inside an `impl` (a `#[cfg(test)]` on
//! a `fn` or a field) is not a cut either, so its lines stay in the count.

// This gate reports no line numbers, so `common::line_of` is unused in this crate.
#[allow(dead_code)]
mod common;

use std::path::Path;

use common::{code_only, crate_root, read_code, rust_files_under};

/// Production lines a file may hold without an entry in `ALLOWLIST`.
const LIMIT: usize = 800;

/// Files over `LIMIT`, at their production line counts when the ratchet landed. The
/// count may only fall, and an entry leaves this list once its file is within `LIMIT`.
const ALLOWLIST: &[(&str, usize)] = &[
    ("controller/commanddispatcher.rs", 921),
    ("controller/replyrouter.rs", 912),
];

/// Non-blank production lines in `code` (comments already blanked): every line before
/// the first `#[cfg(test)]` that introduces an inline test module, or the whole file.
fn production_lines(code: &str) -> usize {
    let lines: Vec<&str> = code.lines().collect();
    let cut = (0..lines.len())
        .find(|&index| opens_test_module(&lines, index))
        .unwrap_or(lines.len());
    lines[..cut]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .count()
}

/// Whether `lines[index]` is a `#[cfg(test)]` that introduces an inline test module: the
/// next item, past blank lines and other attributes, is `mod name {`. The item may span
/// lines, so the text after the attribute is read to the end of the file.
fn opens_test_module(lines: &[&str], index: usize) -> bool {
    let Some(rest) = lines[index].trim_start().strip_prefix("#[cfg(test)]") else {
        return false;
    };
    let mut text = String::from(rest);
    for line in &lines[index + 1..] {
        text.push('\n');
        text.push_str(line);
    }
    is_inline_mod(skip_attributes(&text))
}

/// `text` past its leading whitespace and `#[...]` attributes, which may span lines. An
/// attribute that never closes leaves nothing, so the caller sees no item.
fn skip_attributes(mut text: &str) -> &str {
    loop {
        text = text.trim_start();
        let Some(body) = text.strip_prefix("#[") else {
            return text;
        };
        let mut depth = 1usize;
        let mut close = None;
        for (at, c) in body.char_indices() {
            match c {
                '[' => depth += 1,
                ']' => {
                    depth -= 1;
                    if depth == 0 {
                        close = Some(at + 1);
                        break;
                    }
                }
                _ => {}
            }
        }
        match close {
            Some(end) => text = &body[end..],
            None => return "",
        }
    }
}

/// Whether `item` begins an inline module, `mod name {`, optionally with a visibility.
/// `mod name;` is an out-of-line declaration, so it is not one.
fn is_inline_mod(item: &str) -> bool {
    let after_vis = match item.strip_prefix("pub") {
        Some(rest) => {
            let rest = rest.trim_start();
            match rest.strip_prefix('(') {
                Some(inner) => inner
                    .split_once(')')
                    .map_or("", |(_, tail)| tail)
                    .trim_start(),
                None => rest,
            }
        }
        None => item,
    };
    let Some(after_mod) = after_vis.strip_prefix("mod") else {
        return false;
    };
    if !after_mod.starts_with(char::is_whitespace) {
        return false;
    }
    let after_mod = after_mod.trim_start();
    let name_len = after_mod
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(after_mod.len());
    name_len > 0 && after_mod[name_len..].trim_start().starts_with('{')
}

/// A file's path under `src/`, with `/` separators, as `ALLOWLIST` spells it.
fn key_under_src(path: &Path) -> String {
    let src = crate_root().join("src");
    path.strip_prefix(&src)
        .expect("file under src")
        .iter()
        .map(|part| part.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Every way `sizes` (path under `src/`, production line count) breaks the ratchet
/// against `allowlist`. Empty when the tree matches the record.
fn ratchet_violations(sizes: &[(String, usize)], allowlist: &[(&str, usize)]) -> Vec<String> {
    let mut problems = Vec::new();
    for (key, lines) in sizes {
        match allowlist.iter().find(|(listed, _)| listed == key) {
            Some((_, recorded)) if lines > recorded => problems.push(format!(
                "{key}: {lines} lines, recorded {recorded}. It grew; split it instead of adding to it."
            )),
            Some(_) if *lines <= LIMIT => problems.push(format!(
                "{key}: {lines} lines is within the {LIMIT}-line limit. Remove its ALLOWLIST entry."
            )),
            Some((_, recorded)) if lines < recorded => problems.push(format!(
                "{key}: {lines} lines, recorded {recorded}. It shrank; lower its ALLOWLIST count to {lines}."
            )),
            Some(_) => {}
            None if *lines > LIMIT => problems.push(format!(
                "{key}: {lines} lines is over the {LIMIT}-line limit. Split it, or it fails the ratchet."
            )),
            None => {}
        }
    }
    for (listed, _) in allowlist {
        if !sizes.iter().any(|(key, _)| key == listed) {
            problems.push(format!("{listed}: listed but no such file. Remove its ALLOWLIST entry."));
        }
    }
    problems
}

#[test]
fn src_files_stay_within_the_size_ratchet() {
    let src = crate_root().join("src");
    let files = rust_files_under(&src);
    assert!(!files.is_empty(), "no .rs files under {}", src.display());

    let sizes: Vec<(String, usize)> = files
        .iter()
        .map(|file| (key_under_src(file), production_lines(&read_code(file))))
        .collect();
    let problems = ratchet_violations(&sizes, ALLOWLIST);
    assert!(
        problems.is_empty(),
        "src/ files break the {LIMIT}-line ratchet:\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_counter_counts_production_lines_only() {
    let counted = [
        ("", 0),
        ("fn a() {}\n", 1),
        ("\n\n  \t\nfn a() {}\n\n", 1),
        ("// only a comment\nfn a() {}\n", 1),
        ("/* block\n   comment */\nfn a() {}\n", 1),
        ("let x = 1; // a trailing comment keeps the line\n", 1),
        ("fn a() {}\n#[cfg(test)]\nmod tests { fn b() {} }\n", 1),
        ("fn a() {}\n    #[cfg(test)]\nmod tests {}\n", 1),
        ("fn a() {}\n#[cfg(test)]\n#[allow(dead_code)]\n\nmod tests {}\n", 1),
        ("fn a() {}\n#[cfg(test)]\npub(crate) mod tests {}\n", 1),
        ("fn a() {}\n#[cfg(test)]\nfn b() {}\n#[cfg(test)]\nmod tests {}\n", 3),
        ("fn a() {}\n#[cfg(test)]\n\n// later\nmod tests {}\n", 1),
        ("impl A {\n    #[cfg(test)]\n    fn seam() {}\n}\n", 4),
        ("fn a() {}\nfn b() {}\n#[cfg(test)]\n\n// later\nfn c() {}\n", 4),
        ("fn a() {}\n#[cfg(test)]\n", 2),
        ("let s = \"#[cfg(test)]\";\nfn a() {}\n", 2),
        ("let s = \"#[cfg(test)]\nmod tests {}\";\nfn a() {}\n", 3),
        ("#[cfg(test)] mod tests { fn b() {} }\nfn a() {}\n", 0),
        ("fn a() {}\n#[cfg(test)]\nmod tests\n{}\nfn b() {}\n", 1),
        ("#[cfg(test)]\nfn b() {}\nmod tests {}\n", 3),
        ("#[cfg(test)]\nmod tests;\nfn a() {}\n", 3),
        ("mod a;\n#[cfg(test)]\nmod tests;\nfn b() {}\n", 4),
        ("fn a() {}\n#[cfg(test)]\npub(crate) mod tests;\nfn b() {}\n", 4),
        ("fn a() {}\n#[cfg(test)]\n#[path = \"x.rs\"]\nmod tests;\nfn b() {}\n", 5),
        ("fn a() {}\n#[cfg(test)]\nmod tests\n;\nfn b() {}\n", 5),
        ("fn a() {}\n#[cfg(test)]\nmod tests;\n#[cfg(test)]\nmod more {}\nfn c() {}\n", 3),
    ];
    for (source, expected) in counted {
        assert_eq!(production_lines(&code_only(source)), expected, "counting {source:?}");
    }
}

#[test]
fn the_ratchet_flags_each_way_to_break_it() {
    let allowlist = [("big.rs", 900)];
    let broken: [(&str, Vec<(String, usize)>, &[(&str, usize)]); 5] = [
        ("a new file over the limit", sized(&[("new.rs", 801)]), &[]),
        ("a listed file that grew", sized(&[("big.rs", 901)]), &allowlist),
        ("a listed file that fell under the limit", sized(&[("big.rs", 700)]), &allowlist),
        ("a listed file that shrank but stayed over", sized(&[("big.rs", 850)]), &allowlist),
        ("a listed file that no longer exists", sized(&[]), &allowlist),
    ];
    for (case, sizes, list) in broken {
        assert!(!ratchet_violations(&sizes, list).is_empty(), "not flagged: {case}");
    }
}

#[test]
fn the_ratchet_allows_the_recorded_state() {
    let allowlist = [("big.rs", 900)];
    let allowed: [(&str, Vec<(String, usize)>, &[(&str, usize)]); 3] = [
        ("a listed file at its count", sized(&[("big.rs", 900)]), &allowlist),
        ("an unlisted file within the limit", sized(&[("small.rs", 10)]), &[]),
        ("a new file exactly at the limit", sized(&[("edge.rs", 800)]), &[]),
    ];
    for (case, sizes, list) in allowed {
        assert!(ratchet_violations(&sizes, list).is_empty(), "wrongly flagged: {case}");
    }
}

fn sized(files: &[(&str, usize)]) -> Vec<(String, usize)> {
    files.iter().map(|(key, lines)| ((*key).to_string(), *lines)).collect()
}
