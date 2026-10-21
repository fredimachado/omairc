//! Source scanning shared by the architecture gates in `tests/`. The gates read
//! the PoC's own `.rs` files, so they need comments blanked out and nothing
//! more: string literals stay, so a forbidden name inside one still counts.

use std::fs;
use std::path::{Path, PathBuf};

/// The PoC crate root, `poc/`.
pub fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Every `.rs` file under `dir`, sorted.
pub fn rust_files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect(dir, &mut files);
    files.sort();
    files
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|err| panic!("read {}: {err}", dir.display()));
    for entry in entries {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            collect(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

/// Reads a source file and blanks its comments.
pub fn read_code(path: &Path) -> String {
    let source = fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
    code_only(&source)
}

/// The 1-based line number of a byte offset in `text`.
pub fn line_of(text: &str, offset: usize) -> usize {
    text[..offset].matches('\n').count() + 1
}

/// `source` with every comment replaced by a space. Line breaks survive, so
/// line numbers still match the file. String, raw string and char literals are
/// copied verbatim. Blanks around `::` are dropped, so `std :: net` reads as
/// `std::net`.
pub fn code_only(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            out.push(' ');
        } else if c == '/' && next == Some('*') {
            i = skip_block_comment(&chars, i, &mut out);
        } else if let Some((hashes, quote)) = raw_string_open(&chars, i) {
            i = copy_raw_string(&chars, i, hashes, quote, &mut out);
        } else if c == '"' {
            i = copy_quoted(&chars, i, &mut out);
        } else if c == '\'' {
            i = copy_char_or_lifetime(&chars, i, &mut out);
        } else if c == ' ' || c == '\t' {
            let mut end = i;
            while end < chars.len() && (chars[end] == ' ' || chars[end] == '\t') {
                end += 1;
            }
            let joins_path = out.ends_with(':')
                || (chars.get(end) == Some(&':') && chars.get(end + 1) == Some(&':'));
            if !joins_path {
                out.extend(&chars[i..end]);
            }
            i = end;
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn skip_block_comment(chars: &[char], start: usize, out: &mut String) -> usize {
    let mut depth = 0usize;
    let mut j = start;
    while j < chars.len() {
        if chars[j] == '/' && chars.get(j + 1) == Some(&'*') {
            depth += 1;
            j += 2;
            continue;
        }
        if chars[j] == '*' && chars.get(j + 1) == Some(&'/') {
            depth -= 1;
            j += 2;
            if depth == 0 {
                out.push(' ');
                return j;
            }
            continue;
        }
        if chars[j] == '\n' {
            out.push('\n');
        }
        j += 1;
    }
    out.push(' ');
    j
}

/// `r"`, `r#"`, `br"` and so on: the hash count and the index of the opening quote.
fn raw_string_open(chars: &[char], i: usize) -> Option<(usize, usize)> {
    if i > 0 && is_ident(chars[i - 1]) {
        return None;
    }
    let mut j = i;
    if chars.get(j) == Some(&'b') {
        j += 1;
    }
    if chars.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let mut hashes = 0;
    while chars.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    (chars.get(j) == Some(&'"')).then_some((hashes, j))
}

fn copy_raw_string(chars: &[char], start: usize, hashes: usize, quote: usize, out: &mut String) -> usize {
    let mut j = quote + 1;
    while j < chars.len() {
        if chars[j] == '"' && (0..hashes).all(|k| chars.get(j + 1 + k) == Some(&'#')) {
            let end = j + 1 + hashes;
            out.extend(&chars[start..end]);
            return end;
        }
        j += 1;
    }
    out.extend(&chars[start..]);
    chars.len()
}

fn copy_quoted(chars: &[char], start: usize, out: &mut String) -> usize {
    out.push('"');
    let mut j = start + 1;
    while j < chars.len() {
        match chars[j] {
            '\\' => {
                out.push('\\');
                if let Some(&escaped) = chars.get(j + 1) {
                    out.push(escaped);
                }
                j += 2;
            }
            '"' => {
                out.push('"');
                return j + 1;
            }
            c => {
                out.push(c);
                j += 1;
            }
        }
    }
    chars.len()
}

/// A char literal (`'x'`, `'\''`, `'\u{1F600}'`) is copied whole. Anything else
/// starting with `'` is a lifetime, and only the quote is copied.
fn copy_char_or_lifetime(chars: &[char], start: usize, out: &mut String) -> usize {
    if chars.get(start + 1) == Some(&'\\') {
        let mut j = start + 3;
        while j < chars.len() && chars[j] != '\'' && chars[j] != '\n' {
            j += 1;
        }
        let end = (j + 1).min(chars.len());
        out.extend(&chars[start..end]);
        return end;
    }
    if chars.get(start + 2) == Some(&'\'') {
        out.extend(&chars[start..start + 3]);
        return start + 3;
    }
    out.push('\'');
    start + 1
}
