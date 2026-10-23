//! Whole-segment matching for the architecture gates. A gate names the paths and crates
//! a source tree must not use. This module finds them in comment-free code (see
//! `common::code_only`), including the `prefix::{a, b::c}` group form that
//! `use std::{io, net::TcpStream};` writes.
//!
//! Included by `irc_portability.rs` and `net_boundary.rs` with `#[path]`, not from
//! `common/mod.rs`, so the gates that do not need it do not compile it.

use crate::common::line_of;

/// True when the match at `offset` is a whole path segment, not part of a longer name.
fn bounded(code: &str, offset: usize, len: usize) -> bool {
    let before = code[..offset].chars().next_back();
    let after = code[offset + len..].chars().next();
    !before.is_some_and(is_ident) && !after.is_some_and(is_ident)
}

fn is_ident(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The byte offset of every whole-segment occurrence of `token` in `code`.
pub fn segment_offsets(code: &str, token: &str) -> Vec<usize> {
    code.match_indices(token)
        .map(|(offset, _)| offset)
        .filter(|&offset| bounded(code, offset, token.len()))
        .collect()
}

/// For each `prefix::{...}` group in `code`, the leading segment of every element, with
/// the group's line. `use std::{io, net::TcpStream};` gives `io` and `net` for `std`.
pub fn group_heads(code: &str, prefix: &str) -> Vec<(usize, String)> {
    let opener = format!("{prefix}::{{");
    let mut found = Vec::new();
    for (offset, _) in code.match_indices(&opener) {
        if !bounded(code, offset, prefix.len()) {
            continue;
        }
        let body = &code[offset + opener.len()..];
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
            if !head.is_empty() {
                found.push((line_of(code, offset), head));
            }
        }
    }
    found
}
