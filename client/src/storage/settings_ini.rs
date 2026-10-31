//! QSettings-compatible INI line parsing and value escaping.

use std::collections::BTreeMap;

use super::IniValue;
/// Byte positions of one logical line. `equals` is the absolute offset of the first `=` outside quotes, if any.
#[derive(Clone, Copy, Debug)]
struct IniLine {
    start: usize,
    len: usize,
    equals: Option<usize>,
}

/// Leading `;` starts a Qt comment; leading `#` is an omairc extension. `=` inside
/// quotes is not a separator, and a backslash consumes the next byte, newline included.
struct IniLines<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Iterator for IniLines<'_> {
    type Item = IniLine;

    fn next(&mut self) -> Option<IniLine> {
        let data = self.data;
        let data_len = data.len();
        let mut in_quotes = false;
        let mut equals: Option<usize> = None;

        let mut line_start = self.pos;
        while line_start < data_len && ini_space(data[line_start]) {
            line_start += 1;
        }

        let mut i = line_start;
        let mut ended = false;
        while i < data_len && !ended {
            let ch = data[i];
            if !ini_special(ch) {
                if i == line_start && ch == b'#' {
                    i += 1;
                    while i < data_len && !matches!(data[i], b'\n' | b'\r') {
                        i += 1;
                    }
                    while i < data_len && ini_space(data[i]) {
                        i += 1;
                    }
                    line_start = i;
                    continue;
                }
                i += 1;
                continue;
            }
            i += 1;
            match ch {
                b'=' => {
                    if !in_quotes && equals.is_none() {
                        equals = Some(i - 1);
                    }
                }
                b'\n' | b'\r' => {
                    if i == line_start + 1 {
                        line_start += 1;
                    } else if !in_quotes {
                        i -= 1;
                        ended = true;
                    }
                }
                b'\\' => {
                    if i < data_len {
                        let escaped = data[i];
                        i += 1;
                        if i < data_len {
                            let next = data[i];
                            if (escaped == b'\n' && next == b'\r')
                                || (escaped == b'\r' && next == b'\n')
                            {
                                i += 1;
                            }
                        }
                    }
                }
                b'"' => in_quotes = !in_quotes,
                _ if i == line_start + 1 => {
                    // A ';' at the start of a line: consume the whole comment.
                    while i < data_len && data[i] != b'\n' && data[i] != b'\r' {
                        i += 1;
                    }
                    while i < data_len && ini_space(data[i]) {
                        i += 1;
                    }
                    line_start = i;
                }
                _ if !in_quotes => {
                    // A ';' or other special byte mid-line ends the line.
                    i -= 1;
                    ended = true;
                }
                _ => {}
            }
        }

        self.pos = i;
        let len = i - line_start;
        (len > 0).then_some(IniLine {
            start: line_start,
            len,
            equals,
        })
    }
}

fn ini_space(ch: u8) -> bool {
    matches!(ch, b' ' | b'\t' | b'\n' | b'\r')
}

fn ini_special(ch: u8) -> bool {
    matches!(ch, b'\n' | b'\r' | b'"' | b';' | b'=' | b'\\')
}

fn strip_bom(data: &[u8]) -> &[u8] {
    data.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(data)
}

/// Decodes INI bytes into full key paths. A `[networks]` section followed by
/// `abc\host=x` yields `networks/abc/host`. A hand-written `[networks/abc]` section
/// yields the same path.
pub(super) fn parse_settings_ini(data: &[u8]) -> BTreeMap<String, IniValue> {
    let raw = strip_bom(data);
    let mut values = BTreeMap::new();
    let mut section = String::new();
    for line in (IniLines { data: raw, pos: 0 }) {
        let text = &raw[line.start..line.start + line.len];
        if text[0] == b'[' {
            section = section_prefix(text);
            continue;
        }
        // A comment or a value with no '=' carries no key. Probing has already
        // refused a malformed file, so skipping here never invents a key.
        let Some(equals_at) = line.equals else {
            continue;
        };
        let Some(equals) = equals_at.checked_sub(line.start) else {
            continue;
        };
        if equals >= text.len() {
            continue;
        }
        let key = ini_unescaped_key(text[..equals].trim_ascii());
        let value = ini_unescaped_string_list(&text[equals + 1..]);
        values.insert(format!("{section}{key}"), value);
    }
    values
}

/// STS load path: like [`parse_settings_ini`], but after a bad non-header line a
/// later key is ignored only when that key is already set in the current group,
/// so a garbage line cannot drop the rest of an intact policy and leaked keys
/// cannot overwrite real ones. An unclosed `[` header still starts a new section
/// (as on main) and clears the after-bad-line flag, so the next host's keys never
/// land in the previous group.
pub(super) fn parse_sts_ini(data: &[u8]) -> BTreeMap<String, IniValue> {
    let raw = strip_bom(data);
    let mut values = BTreeMap::new();
    let mut section = String::new();
    let mut after_bad = false;
    for line in (IniLines { data: raw, pos: 0 }) {
        let text = &raw[line.start..line.start + line.len];
        if text[0] == b'[' {
            // Match main: unclosed `[` still opens a new section via section_prefix.
            section = section_prefix(text);
            after_bad = false;
            continue;
        }
        let Some(equals_at) = line.equals else {
            if text[0] != b';' {
                after_bad = true;
            }
            continue;
        };
        let Some(equals) = equals_at.checked_sub(line.start) else {
            continue;
        };
        if equals >= text.len() {
            continue;
        }
        let key = ini_unescaped_key(text[..equals].trim_ascii());
        let path = format!("{section}{key}");
        if after_bad && values.contains_key(&path) {
            continue;
        }
        let value = ini_unescaped_string_list(&text[equals + 1..]);
        values.insert(path, value);
    }
    values
}

/// The group prefix a `[...]` line gives its keys: empty for `[General]`, a decoded
/// segment plus `/` otherwise, and the literal name for `[%General]`.
fn section_prefix(line: &[u8]) -> String {
    let mut inner = &line[1..];
    if let Some(close) = inner.iter().position(|&byte| byte == b']') {
        inner = &inner[..close];
    }
    let inner = inner.trim_ascii();
    if inner.eq_ignore_ascii_case(b"general") {
        String::new()
    } else if inner.eq_ignore_ascii_case(b"%general") {
        format!("{}/", String::from_utf8_lossy(&inner[1..]))
    } else {
        format!("{}/", ini_unescaped_key(inner))
    }
}

/// A `[section` line with no `]` on its line, or a non-comment line with no `=`, is malformed.
pub(crate) fn ini_bytes_are_malformed(data: &[u8]) -> bool {
    let raw = strip_bom(data);
    for line in (IniLines { data: raw, pos: 0 }) {
        if raw[line.start] == b'[' {
            let closed = raw[line.start..]
                .iter()
                .position(|&byte| byte == b']')
                .is_some_and(|close| close < line.len);
            if !closed {
                return true;
            }
            continue;
        }
        if line.equals.is_none() && raw[line.start] != b';' {
            return true;
        }
    }
    false
}

/// `QSettingsPrivate::iniUnescapedKey`. A backslash is `/`, and `%XX` or
/// `%UXXXX` are hex code units. Anything else is literal.
pub(crate) fn ini_unescaped_key(key: &[u8]) -> String {
    let text = String::from_utf8_lossy(key);
    let units: Vec<u16> = text.encode_utf16().collect();
    let percent = u16::from(b'%');
    let mut out: Vec<u16> = Vec::with_capacity(units.len());
    let mut i = 0usize;
    while i < units.len() {
        let ch = units[i];
        if ch == u16::from(b'\\') {
            out.push(u16::from(b'/'));
            i += 1;
            continue;
        }
        if ch != percent || i == units.len() - 1 {
            out.push(ch);
            i += 1;
            continue;
        }

        let (mut first, mut digits) = (i + 1, 2);
        if units[i + 1] == u16::from(b'U') {
            first += 1;
            digits = 4;
        }
        if first + digits > units.len() {
            out.push(percent);
            i += 1;
            continue;
        }
        let mut value: u32 = 0;
        let mut valid = true;
        for offset in 0..digits {
            match u8::try_from(units[first + offset]).ok().and_then(hex_value) {
                Some(digit) => value = value * 16 + digit,
                None => {
                    valid = false;
                    break;
                }
            }
        }
        if !valid {
            out.push(percent);
            i += 1;
            continue;
        }
        out.push(value as u16);
        i = first + digits;
    }
    String::from_utf16_lossy(&out)
}

/// `QSettingsPrivate::iniEscapedKey`. Letters, digits, `_`, `-` and `.` stay
/// literal, `/` becomes `\`, and other code units become `%XX` or `%UXXXX`.
pub(crate) fn ini_escaped_key(key: &str) -> String {
    let mut out = String::new();
    for unit in key.encode_utf16() {
        match unit {
            0x2F => out.push('\\'),
            0x30..=0x39 | 0x41..=0x5A | 0x61..=0x7A | 0x2D | 0x2E | 0x5F => {
                out.push(char::from(unit as u8));
            }
            0..=0xFF => {
                out.push('%');
                out.push(hex_upper((unit >> 4) as u8));
                out.push(hex_upper((unit & 0xF) as u8));
            }
            _ => {
                out.push_str("%U");
                for shift in [12, 8, 4, 0] {
                    out.push(hex_upper(((unit >> shift) & 0xF) as u8));
                }
            }
        }
    }
    out
}

/// `=` forces quotes. A leading or trailing space forces quotes. `\` and `"` are
/// escaped, and control bytes use named or `\x` escapes. A hex digit after a
/// `\0` or `\x` escape is itself escaped, so it cannot extend the escape.
pub(crate) fn ini_escaped_string(text: &str) -> String {
    let mut out = String::new();
    let mut needs_quotes = false;
    let mut escape_next_if_digit = false;

    for ch in text.chars() {
        if matches!(ch, ';' | ',' | '=') {
            needs_quotes = true;
        }
        if escape_next_if_digit && ch.is_ascii_hexdigit() {
            out.push_str("\\x");
            out.push_str(&format!("{:x}", ch as u32));
            continue;
        }
        escape_next_if_digit = false;

        match ch {
            '\0' => {
                out.push_str("\\0");
                escape_next_if_digit = true;
            }
            '\u{7}' => out.push_str("\\a"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{b}' => out.push_str("\\v"),
            '"' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            c if (c as u32) <= 0x1F => {
                out.push_str("\\x");
                out.push_str(&format!("{:x}", c as u32));
                escape_next_if_digit = true;
            }
            c => out.push(c),
        }
    }

    if needs_quotes || out.starts_with(' ') || out.ends_with(' ') {
        format!("\"{out}\"")
    } else {
        out
    }
}

/// an empty list is `@Invalid`
/// and items are escaped and joined with `, `.
pub(crate) fn ini_escaped_string_list(items: &[String]) -> String {
    if items.is_empty() {
        return "@Invalid()".to_string();
    }
    items
        .iter()
        .map(|item| ini_escaped_string(item))
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    SkipSpaces,
    Normal,
    HexEscape,
    OctEscape,
    End,
}

/// `QSettingsPrivate::iniUnescapedStringList`. It decodes one value, splits
/// top-level commas into a list, and applies the `@Invalid` and `@@` sentinels. An
/// escape is decoded, and a quoted comma is literal.
fn ini_unescaped_string_list(value: &[u8]) -> IniValue {
    let length = value.len();
    let mut buf: Vec<u8> = Vec::new();
    let mut items: Vec<String> = Vec::new();
    let mut is_list = false;
    let mut in_quotes = false;
    let mut current_quoted = false;
    // Set on entry to each Normal step, before any read of it.
    let mut chop_limit: usize;
    let mut escape_val: u32 = 0;
    let mut i = 0usize;
    let mut step = Step::SkipSpaces;

    while step != Step::End {
        step = match step {
            Step::SkipSpaces => {
                while i < length && (value[i] == b' ' || value[i] == b'\t') {
                    i += 1;
                }
                Step::Normal
            }
            Step::Normal => {
                chop_limit = buf.len();
                let mut next = Step::End;
                let mut natural_end = true;
                while i < length {
                    match value[i] {
                        b'\\' => {
                            i += 1;
                            if i >= length {
                                natural_end = false;
                                break;
                            }
                            let escaped = value[i];
                            i += 1;
                            if let Some(decoded) = escape_code(escaped) {
                                push_code_point(&mut buf, decoded);
                                next = Step::Normal;
                                natural_end = false;
                                break;
                            }
                            if escaped == b'x' {
                                escape_val = 0;
                                if i >= length {
                                    natural_end = false;
                                    break;
                                }
                                if hex_value(value[i]).is_some() {
                                    next = Step::HexEscape;
                                    natural_end = false;
                                    break;
                                }
                            } else if let Some(octal) = oct_value(escaped) {
                                escape_val = octal;
                                next = Step::OctEscape;
                                natural_end = false;
                                break;
                            } else if escaped == b'\n' || escaped == b'\r' {
                                if i < length {
                                    let following = value[i];
                                    if (following == b'\n' || following == b'\r')
                                        && following != escaped
                                    {
                                        i += 1;
                                    }
                                }
                            }
                            // Any other escape is skipped, as in Qt.
                            chop_limit = buf.len();
                        }
                        b'"' => {
                            i += 1;
                            current_quoted = true;
                            in_quotes = !in_quotes;
                            if !in_quotes {
                                next = Step::SkipSpaces;
                                natural_end = false;
                                break;
                            }
                        }
                        b',' if !in_quotes => {
                            if !current_quoted {
                                chop(&mut buf, chop_limit);
                            }
                            if !is_list {
                                is_list = true;
                                items.clear();
                            }
                            items.push(String::from_utf8_lossy(&buf).into_owned());
                            buf.clear();
                            current_quoted = false;
                            i += 1;
                            next = Step::SkipSpaces;
                            natural_end = false;
                            break;
                        }
                        _ => {
                            let mut j = i + 1;
                            while j < length && !matches!(value[j], b'\\' | b'"' | b',') {
                                j += 1;
                            }
                            buf.extend_from_slice(&value[i..j]);
                            i = j;
                        }
                    }
                }
                if natural_end && !current_quoted {
                    chop(&mut buf, chop_limit);
                }
                next
            }
            Step::HexEscape => {
                let mut resume = Step::End;
                while i < length {
                    match hex_value(value[i]) {
                        Some(digit) => {
                            escape_val = escape_val.wrapping_mul(16).wrapping_add(digit);
                            i += 1;
                        }
                        None => {
                            resume = Step::Normal;
                            break;
                        }
                    }
                }
                push_code_point(&mut buf, escape_val);
                resume
            }
            Step::OctEscape => {
                let mut resume = Step::End;
                while i < length {
                    match oct_value(value[i]) {
                        Some(digit) => {
                            escape_val = escape_val.wrapping_mul(8).wrapping_add(digit);
                            i += 1;
                        }
                        None => {
                            resume = Step::Normal;
                            break;
                        }
                    }
                }
                push_code_point(&mut buf, escape_val);
                resume
            }
            Step::End => Step::End,
        };
    }

    let text = String::from_utf8_lossy(&buf).into_owned();
    if is_list {
        items.push(text);
        return IniValue::List(items);
    }
    if text == "@Invalid()" {
        IniValue::Invalid
    } else if let Some(rest) = text.strip_prefix("@@") {
        IniValue::Text(format!("@{rest}"))
    } else if text.starts_with('@') {
        IniValue::Variant(text)
    } else {
        IniValue::Text(text)
    }
}

fn chop(buf: &mut Vec<u8>, limit: usize) {
    while buf.len() > limit {
        match buf.last() {
            Some(b' ' | b'\t') => {
                buf.pop();
            }
            _ => break,
        }
    }
}

fn push_code_point(buf: &mut Vec<u8>, code: u32) {
    let ch = char::from_u32(code).unwrap_or('\u{FFFD}');
    let mut tmp = [0u8; 4];
    buf.extend_from_slice(ch.encode_utf8(&mut tmp).as_bytes());
}

/// The single-letter escapes of `iniUnescapedStringList`.
fn escape_code(ch: u8) -> Option<u32> {
    Some(match ch {
        b'a' => 0x07,
        b'b' => 0x08,
        b'f' => 0x0C,
        b'n' => 0x0A,
        b'r' => 0x0D,
        b't' => 0x09,
        b'v' => 0x0B,
        b'"' => 0x22,
        b'?' => 0x3F,
        b'\'' => 0x27,
        b'\\' => 0x5C,
        _ => return None,
    })
}

fn hex_value(ch: u8) -> Option<u32> {
    match ch {
        b'0'..=b'9' => Some(u32::from(ch - b'0')),
        b'a'..=b'f' => Some(u32::from(ch - b'a') + 10),
        b'A'..=b'F' => Some(u32::from(ch - b'A') + 10),
        _ => None,
    }
}

fn oct_value(ch: u8) -> Option<u32> {
    (b'0'..=b'7').contains(&ch).then(|| u32::from(ch - b'0'))
}

fn hex_upper(nibble: u8) -> char {
    char::from(b"0123456789ABCDEF"[usize::from(nibble & 0xF)])
}

