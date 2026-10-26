//! Every `std::os::unix` / `std::os::windows` use under `src/` must sit under a
//! matching `cfg`, so a Unix-only call cannot break the Windows build the way the
//! theme symlink test once did. Platform module files (`windows.rs`, `linux.rs`
//! `macos.rs`) count as that cfg for the whole file. `vendor/` is not scanned.

mod common;
#[path = "common/forbidden.rs"]
mod forbidden;

use std::path::{Path, PathBuf};

use common::{code_only, crate_root, line_of, read_code, rust_files_under};
use forbidden::segment_offsets;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Os {
    Unix,
    Windows,
}

impl Os {
    fn path(self) -> &'static str {
        match self {
            Os::Unix => "std::os::unix",
            Os::Windows => "std::os::windows",
        }
    }

    fn from_platform_file(path: &Path) -> Option<Self> {
        match path.file_name().and_then(|name| name.to_str()) {
            Some("windows.rs") => Some(Os::Windows),
            Some("linux.rs") | Some("macos.rs") => Some(Os::Unix),
            _ => None,
        }
    }
}

/// One unguarded use, for the failure message.
#[derive(Debug, PartialEq, Eq)]
struct Hit {
    line: usize,
    path: &'static str,
}

/// Every `std::os::{unix,windows}` use in `code` that is not under a matching cfg.
/// `file_os` is set for platform module files that are only compiled on that OS.
/// Uses are found after blanking string literals (so a path named in a string is
/// not a use); cfg coverage is read from the original text so `target_os = "..."`
/// stays visible.
fn unguarded_uses(code: &str, file_os: Option<Os>) -> Vec<Hit> {
    let covered = cfg_coverage(code);
    let scanned = blank_literals(code);
    let mut hits = Vec::new();
    for os in [Os::Unix, Os::Windows] {
        if file_os == Some(os) {
            continue;
        }
        for offset in segment_offsets(&scanned, os.path()) {
            let line = line_of(&scanned, offset);
            if !covered.iter().any(|span| span.covers(offset, os)) {
                hits.push(Hit {
                    line,
                    path: os.path(),
                });
            }
        }
    }
    hits.sort_by_key(|hit| hit.line);
    hits
}

/// `code` with every string, raw string and char literal replaced by spaces of the
/// same UTF-8 width. Line breaks stay, so byte offsets and line numbers still match.
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

fn literal_end(chars: &[char], at: usize) -> usize {
    let starts_word = at == 0 || !is_ident_char(chars[at - 1]);
    match chars[at] {
        '"' => quoted_end(chars, at + 1),
        'b' if starts_word && chars.get(at + 1) == Some(&'"') => quoted_end(chars, at + 2),
        'r' | 'b' if starts_word => raw_end(chars, at).unwrap_or(at),
        '\'' => char_end(chars, at),
        _ => at,
    }
}

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

fn is_ident_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// A region of `code` where `os` is allowed, as a half-open byte range.
#[derive(Debug)]
struct Span {
    start: usize,
    end: usize,
    os: Os,
}

impl Span {
    fn covers(&self, offset: usize, os: Os) -> bool {
        self.os == os && offset >= self.start && offset < self.end
    }
}

#[derive(Clone, Copy)]
struct CfgAttr {
    /// Byte index after the closing `]`.
    after: usize,
    /// True for `#![cfg(...)]`.
    inner: bool,
    platforms: u8,
}

const PLAT_UNIX: u8 = 1;
const PLAT_WINDOWS: u8 = 2;

/// Regions under `#[cfg(...)]` / `#![cfg(...)]` that name unix or windows.
fn cfg_coverage(code: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut i = 0;
    while i < code.len() {
        if let Some(attr) = parse_cfg_attr(code, i) {
            if attr.platforms == 0 {
                i = attr.after;
                continue;
            }
            let (start, end) = cfg_region(code, attr);
            for os in [Os::Unix, Os::Windows] {
                let bit = match os {
                    Os::Unix => PLAT_UNIX,
                    Os::Windows => PLAT_WINDOWS,
                };
                if attr.platforms & bit != 0 {
                    spans.push(Span { start, end, os });
                }
            }
            i = attr.after;
            continue;
        }
        i = next_char(code, i);
    }
    spans
}

/// `#[cfg(...)]` or `#![cfg(...)]` at `at`.
fn parse_cfg_attr(code: &str, at: usize) -> Option<CfgAttr> {
    if !code[at..].starts_with('#') {
        return None;
    }
    let mut j = at + 1;
    let inner = code[j..].starts_with('!');
    if inner {
        j += 1;
    }
    if !code[j..].starts_with('[') {
        return None;
    }
    j += 1;
    j = skip_ws(code, j);
    if !code[j..].starts_with("cfg") {
        return None;
    }
    j += 3;
    j = skip_ws(code, j);
    if !code[j..].starts_with('(') {
        return None;
    }
    j += 1;
    let body_start = j;
    let mut depth = 1usize;
    while j < code.len() {
        match code.as_bytes()[j] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    let inner_cfg = &code[body_start..j];
                    j += 1;
                    j = skip_ws(code, j);
                    if !code[j..].starts_with(']') {
                        return None;
                    }
                    return Some(CfgAttr {
                        after: j + 1,
                        inner,
                        platforms: cfg_platforms(inner_cfg),
                    });
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

/// Byte range the cfg applies to.
fn cfg_region(code: &str, attr: CfgAttr) -> (usize, usize) {
    if attr.inner {
        return (0, code.len());
    }
    let mut j = attr.after;
    loop {
        j = skip_ws_and_newlines(code, j);
        if code[j..].starts_with('#') {
            if let Some(next) = skip_attr(code, j) {
                j = next;
                continue;
            }
        }
        break;
    }
    let start = j;
    while j < code.len() {
        match code.as_bytes()[j] {
            b'{' => {
                let end = matching_brace(code, j);
                return (start, end);
            }
            b';' => return (start, j + 1),
            b'"' | b'\'' => j = skip_stringish(code, j),
            _ => j = next_char(code, j),
        }
    }
    (start, code.len())
}

fn skip_attr(code: &str, at: usize) -> Option<usize> {
    if !code[at..].starts_with('#') {
        return None;
    }
    let mut j = at + 1;
    if code[j..].starts_with('!') {
        j += 1;
    }
    if !code[j..].starts_with('[') {
        return None;
    }
    j += 1;
    let mut depth = 1usize;
    while j < code.len() {
        match code.as_bytes()[j] {
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(j + 1);
                }
            }
            _ => {}
        }
        j += 1;
    }
    None
}

fn matching_brace(code: &str, open: usize) -> usize {
    let mut depth = 0usize;
    let mut j = open;
    while j < code.len() {
        match code.as_bytes()[j] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return j + 1;
                }
            }
            b'"' | b'\'' => {
                j = skip_stringish(code, j);
                continue;
            }
            _ => {}
        }
        j = next_char(code, j);
    }
    code.len()
}

fn skip_stringish(code: &str, at: usize) -> usize {
    let bytes = code.as_bytes();
    let quote = bytes[at];
    let mut j = at + 1;
    while j < bytes.len() {
        if bytes[j] == b'\\' {
            j += 2;
            continue;
        }
        if bytes[j] == quote {
            return j + 1;
        }
        j += 1;
    }
    code.len()
}

fn skip_ws(code: &str, mut j: usize) -> usize {
    let bytes = code.as_bytes();
    while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
        j += 1;
    }
    j
}

fn skip_ws_and_newlines(code: &str, mut j: usize) -> usize {
    while j < code.len() && code.as_bytes()[j].is_ascii_whitespace() {
        j += 1;
    }
    j
}

fn next_char(code: &str, j: usize) -> usize {
    code[j..]
        .chars()
        .next()
        .map(|c| j + c.len_utf8())
        .unwrap_or(code.len())
}

/// Bitmask of platforms a `cfg(...)` predicate allows for `std::os::*` uses.
fn cfg_platforms(inner: &str) -> u8 {
    let compact: String = inner.chars().filter(|c| !c.is_whitespace()).collect();
    let mut out = 0u8;
    if cfg_allows_unix(&compact) {
        out |= PLAT_UNIX;
    }
    if cfg_allows_windows(&compact) {
        out |= PLAT_WINDOWS;
    }
    out
}

fn cfg_allows_unix(compact: &str) -> bool {
    if compact.contains("not(windows)") || compact.contains("not(target_os=\"windows\")") {
        return true;
    }
    contains_pred(compact, "unix")
        || compact.contains("target_os=\"linux\"")
        || compact.contains("target_os=\"macos\"")
        || compact.contains("target_os=\"freebsd\"")
        || compact.contains("target_os=\"openbsd\"")
        || compact.contains("target_os=\"netbsd\"")
        || compact.contains("target_os=\"dragonfly\"")
}

fn cfg_allows_windows(compact: &str) -> bool {
    if compact.contains("not(unix)") {
        return true;
    }
    contains_pred(compact, "windows") || compact.contains("target_os=\"windows\"")
}

/// Whole-predicate match. A `windows` inside `target_os="windows"` is skipped so
/// the dedicated `target_os=` check owns that form.
fn contains_pred(compact: &str, pred: &str) -> bool {
    let bytes = compact.as_bytes();
    let needle = pred.as_bytes();
    let mut start = 0;
    while start + needle.len() <= bytes.len() {
        if &bytes[start..start + needle.len()] == needle {
            let before = start
                .checked_sub(1)
                .map(|i| is_ident_byte(bytes[i]))
                .unwrap_or(false);
            let after = bytes
                .get(start + needle.len())
                .copied()
                .map(is_ident_byte)
                .unwrap_or(false);
            let quote_before = start
                .checked_sub(1)
                .map(|i| bytes[i] == b'"')
                .unwrap_or(false);
            if !before && !after && !quote_before {
                return true;
            }
        }
        start += 1;
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn relative(path: &Path) -> String {
    path.strip_prefix(crate_root())
        .unwrap_or(path)
        .display()
        .to_string()
}

fn src_files() -> Vec<PathBuf> {
    rust_files_under(&crate_root().join("src"))
}

#[test]
fn os_specific_std_uses_sit_under_matching_cfg() {
    let files = src_files();
    assert!(!files.is_empty(), "no .rs files under src/");

    let mut problems = Vec::new();
    for file in &files {
        let file_os = Os::from_platform_file(file);
        for hit in unguarded_uses(&read_code(file), file_os) {
            problems.push(format!(
                "{}:{}: {} without a matching cfg",
                relative(file),
                hit.line,
                hit.path
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "std::os::unix / std::os::windows must sit under a matching cfg \
         (or in windows.rs / linux.rs / macos.rs):\n{}",
        problems.join("\n")
    );
}

#[test]
fn the_gate_flags_an_unguarded_unix_use() {
    let code = code_only("fn a() {\n    let _ = std::os::unix::fs::PermissionsExt;\n}\n");
    let hits = unguarded_uses(&code, None);
    assert_eq!(
        hits,
        [Hit {
            line: 2,
            path: "std::os::unix",
        }]
    );
}

#[test]
fn the_gate_accepts_cfg_unix_on_the_item() {
    let code = code_only(
        "#[cfg(unix)]\nfn a() {\n    use std::os::unix::fs::PermissionsExt;\n}\n",
    );
    assert!(unguarded_uses(&code, None).is_empty());
}

#[test]
fn the_gate_accepts_cfg_block_and_all_windows() {
    let unix_block = code_only("#[cfg(unix)] {\n    use std::os::unix::fs::PermissionsExt;\n}\n");
    assert!(unguarded_uses(&unix_block, None).is_empty());

    let win = code_only(
        "#[cfg(all(windows, feature = \"product\"))]\nmod m {\n    use std::os::windows::ffi::OsStrExt;\n}\n",
    );
    assert!(unguarded_uses(&win, None).is_empty());
}

#[test]
fn the_gate_accepts_platform_module_files_without_inline_cfg() {
    let code = code_only("use std::os::windows::ffi::OsStrExt;\n");
    assert!(unguarded_uses(&code, Some(Os::Windows)).is_empty());
    assert_eq!(unguarded_uses(&code, None).len(), 1);

    let unix = code_only("use std::os::unix::fs::PermissionsExt;\n");
    assert!(unguarded_uses(&unix, Some(Os::Unix)).is_empty());
}

#[test]
fn the_gate_accepts_target_os_linux_for_unix() {
    let code = code_only(
        "#[cfg(target_os = \"linux\")]\nfn a() {\n    use std::os::unix::net::UnixStream;\n}\n",
    );
    assert!(unguarded_uses(&code, None).is_empty());
}

#[test]
fn the_gate_skips_comments_and_strings() {
    let code = code_only("// std::os::unix\nlet s = \"std::os::windows\";\n");
    assert!(unguarded_uses(&code, None).is_empty());
}

#[test]
fn the_gate_flags_unix_use_under_windows_only_cfg() {
    let code = code_only(
        "#[cfg(target_os = \"windows\")]\nfn a() {\n    use std::os::unix::fs::PermissionsExt;\n}\n",
    );
    assert_eq!(unguarded_uses(&code, None).len(), 1);
}
