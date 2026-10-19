use super::color::Color;
use std::ops::Range;

const BOLD: char = '\x02';
const COLOR: char = '\x03';
const HEX_COLOR: char = '\x04';
const RESET: char = '\x0f';
const MONOSPACE: char = '\x11';
const REVERSE: char = '\x16';
const ITALIC: char = '\x1d';
const STRIKETHROUGH: char = '\x1e';
const UNDERLINE: char = '\x1f';

const MIRC_PALETTE: [&str; 16] = [
    "#FFFFFF", "#000000", "#00007F", "#009300", "#FF0000", "#7F0000", "#9C009C", "#FC7F00",
    "#FFFF00", "#00FC00", "#009393", "#00FFFF", "#0000FC", "#FF00FF", "#7F7F7F", "#D2D2D2",
];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
    pub monospace: bool,
    pub strikethrough: bool,
    pub fg: Option<Color>,
    pub bg: Option<Color>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StyleRun {
    pub range: Range<usize>,
    pub style: TextStyle,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FormattedText {
    pub plain: String,
    pub runs: Vec<StyleRun>,
    pub links: Vec<Range<usize>>,
}

pub fn format_irc(text: &str) -> FormattedText {
    let plain = plain_irc_text(text);
    let mut runs = style_runs(text);
    for run in &mut runs {
        debug_assert!(run.range.end <= plain.len());
    }
    let links = http_link_ranges(&plain);
    FormattedText { plain, runs, links }
}

pub fn plain_irc_text(text: &str) -> String {
    let stripped = strip_irc_colors(text);
    let mut out = String::with_capacity(stripped.len());
    for ch in stripped.chars() {
        match ch {
            BOLD | RESET | MONOSPACE | REVERSE | ITALIC | STRIKETHROUGH | UNDERLINE => {}
            _ => out.push(ch),
        }
    }
    out
}

fn strip_irc_colors(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let code = chars[i];
        if code == COLOR {
            i = consume_mirc_color(&chars, i) + 1;
            continue;
        }
        if code == HEX_COLOR {
            i = consume_hex_color(&chars, i) + 1;
            continue;
        }
        out.push(code);
        i += 1;
    }
    out
}

fn style_runs(text: &str) -> Vec<StyleRun> {
    let chars: Vec<char> = text.chars().collect();
    let mut state = TextStyle::default();
    let mut plain = String::new();
    let mut runs = Vec::new();
    let mut run_start = 0usize;

    let flush = |plain: &str, runs: &mut Vec<StyleRun>, run_start: &mut usize, state: &TextStyle| {
        let end = plain.len();
        if end > *run_start {
            runs.push(StyleRun {
                range: *run_start..end,
                style: state.clone(),
            });
        }
        *run_start = end;
    };

    let mut i = 0;
    while i < chars.len() {
        let code = chars[i];
        if code == COLOR {
            flush(&plain, &mut runs, &mut run_start, &state);
            let next = consume_mirc_color(&chars, i);
            if next == i {
                state.fg = None;
                state.bg = None;
            } else {
                apply_mirc_color(&chars, i, next, &mut state);
            }
            i = next + 1;
            continue;
        }
        if code == HEX_COLOR {
            flush(&plain, &mut runs, &mut run_start, &state);
            let next = consume_hex_color(&chars, i);
            if next > i {
                apply_hex_color(&chars, i, next, &mut state);
            }
            i = next + 1;
            continue;
        }
        if matches!(
            code,
            BOLD | ITALIC | UNDERLINE | REVERSE | MONOSPACE | STRIKETHROUGH | RESET
        ) {
            flush(&plain, &mut runs, &mut run_start, &state);
            match code {
                BOLD => state.bold = !state.bold,
                ITALIC => state.italic = !state.italic,
                UNDERLINE => state.underline = !state.underline,
                REVERSE => state.reverse = !state.reverse,
                MONOSPACE => state.monospace = !state.monospace,
                STRIKETHROUGH => state.strikethrough = !state.strikethrough,
                RESET => state = TextStyle::default(),
                _ => {}
            }
            i += 1;
            continue;
        }
        plain.push(code);
        i += 1;
    }
    flush(&plain, &mut runs, &mut run_start, &state);
    runs
}

fn apply_mirc_color(chars: &[char], start: usize, end: usize, state: &mut TextStyle) {
    let mut i = start + 1;
    if i >= chars.len() || !chars[i].is_ascii_digit() {
        state.fg = None;
        state.bg = None;
        return;
    }
    let mut fg = digit_value(chars[i]);
    i += 1;
    if i < chars.len() && chars[i].is_ascii_digit() {
        fg = fg * 10 + digit_value(chars[i]);
        i += 1;
    }
    if fg <= 15 {
        state.fg = palette_color(fg);
    }
    if i < chars.len() && chars[i] == ',' && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() {
        i += 1;
        let mut bg = digit_value(chars[i]);
        i += 1;
        if i < chars.len() && chars[i].is_ascii_digit() {
            bg = bg * 10 + digit_value(chars[i]);
        }
        if bg <= 15 {
            state.bg = palette_color(bg);
        }
    }
    let _ = end;
}

fn apply_hex_color(chars: &[char], start: usize, end: usize, state: &mut TextStyle) {
    let mut i = start + 1;
    if let Some(fg) = read_hex6(chars, i) {
        state.fg = Some(fg);
        i += 6;
    }
    if i < chars.len() && chars[i] == ',' {
        if let Some(bg) = read_hex6(chars, i + 1) {
            state.bg = Some(bg);
        }
    }
    let _ = end;
}

fn read_hex6(chars: &[char], start: usize) -> Option<Color> {
    if start + 5 >= chars.len() {
        return None;
    }
    let mut hex = String::with_capacity(6);
    for ch in &chars[start..start + 6] {
        if !ch.is_ascii_hexdigit() {
            return None;
        }
        hex.push(ch.to_ascii_lowercase());
    }
    Color::from_hex(&hex)
}

fn palette_color(index: u8) -> Option<Color> {
    MIRC_PALETTE.get(index as usize).and_then(|h| Color::from_hex(h))
}

fn digit_value(ch: char) -> u8 {
    (ch as u8) - b'0'
}

fn ascii_digit(ch: char) -> bool {
    ch.is_ascii_digit()
}

fn consume_mirc_color(chars: &[char], index: usize) -> usize {
    let n = chars.len();
    let mut i = index + 1;
    if i >= n || !ascii_digit(chars[i]) {
        return index;
    }
    i += 1;
    if i < n && ascii_digit(chars[i]) {
        i += 1;
    }
    if i < n
        && chars[i] == ','
        && i + 1 < n
        && ascii_digit(chars[i + 1])
    {
        i += 2;
        if i < n && ascii_digit(chars[i]) {
            i += 1;
        }
    }
    i - 1
}

fn consume_hex_color(chars: &[char], index: usize) -> usize {
    let n = chars.len();
    let mut i = index + 1;
    if !six_hex_at(chars, i) {
        return index;
    }
    i += 6;
    if i < n && chars[i] == ',' && six_hex_at(chars, i + 1) {
        i += 7;
    }
    i - 1
}

fn six_hex_at(chars: &[char], start: usize) -> bool {
    if start + 5 >= chars.len() {
        return false;
    }
    chars[start..start + 6]
        .iter()
        .all(|c| c.is_ascii_hexdigit())
}

fn advance_one_code_unit(text: &str, i: usize) -> usize {
    if i >= text.len() {
        return i;
    }
    if text.as_bytes()[i].is_ascii() {
        i + 1
    } else {
        i + text[i..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(1)
    }
}

pub fn http_link_ranges(text: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 7 < bytes.len() {
        let rest = &text[i..];
        let start_scheme = if rest.starts_with("https://") {
            8
        } else if rest.starts_with("http://") {
            7
        } else {
            i = advance_one_code_unit(text, i);
            continue;
        };
        let start = i;
        let mut j = i + start_scheme;
        while j < bytes.len() {
            let b = bytes[j];
            if b.is_ascii_whitespace() || b == b'\n' || b == b'\r' {
                break;
            }
            if b == b'<' || b == b'>' || b == b'"' || b == b'\'' {
                break;
            }
            j += 1;
        }
        let raw = &text[start..j];
        let trimmed = trim_http_url_match(raw);
        let end = start + trimmed.len();
        if !trimmed.is_empty() && is_allowed_http_url(trimmed) {
            ranges.push(start..end);
        }
        i = if j == start + start_scheme {
            advance_one_code_unit(text, i)
        } else {
            j
        };
    }
    ranges
}

fn is_allowed_http_url(url: &str) -> bool {
    let Some(colon) = url.find(':') else {
        return false;
    };
    if colon == 0 {
        return false;
    }
    let scheme = &url[..colon];
    if scheme != "http" && scheme != "https" {
        return false;
    }
    if !url[colon..].starts_with("://") {
        return false;
    }
    let rest = &url[(colon + 3)..];
    !rest.is_empty() && !rest.contains(['\n', '\r', ' '])
}

fn trim_http_url_match(raw: &str) -> &str {
    let mut end = raw.len();
    while end > 0 {
        let ch = raw[..end].chars().next_back().unwrap();
        if ".,;:!?".contains(ch) {
            end -= ch.len_utf8();
            continue;
        }
        if ch == ')' {
            let open = raw[..end].chars().filter(|&c| c == '(').count();
            let close = raw[..end].chars().filter(|&c| c == ')').count();
            if close > open {
                end -= 1;
                continue;
            }
        }
        if ch == ']' {
            let open = raw[..end].chars().filter(|&c| c == '[').count();
            let close = raw[..end].chars().filter(|&c| c == ']').count();
            if close > open {
                end -= 1;
                continue;
            }
        }
        break;
    }
    &raw[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bold_italic_underline_reset() {
        let input = format!("{BOLD}hi{ITALIC} there{UNDERLINE}!{RESET}plain");
        let plain = plain_irc_text(&input);
        assert_eq!(plain, "hi there!plain");
        let formatted = format_irc(&input);
        assert!(formatted.runs.iter().any(|r| r.style.bold && r.range.start == 0));
    }

    #[test]
    fn mirc_color_plain() {
        let input = format!("{COLOR}04red{COLOR}");
        assert_eq!(plain_irc_text(&input), "red");
        let formatted = format_irc(&input);
        assert_eq!(formatted.plain, "red");
    }

    #[test]
    fn hex_color_plain() {
        let input = format!("{HEX_COLOR}FF0000warm");
        assert_eq!(plain_irc_text(&input), "warm");
    }

    #[test]
    fn invalid_hex_leaves_following_text() {
        let input = format!("{HEX_COLOR}ZZwarm");
        assert_eq!(plain_irc_text(&input), "ZZwarm");
    }

    #[test]
    fn link_range() {
        let text = "see https://ci.example.com/omairc/42 ok";
        let links = http_link_ranges(text);
        assert_eq!(links.len(), 1);
        assert_eq!(&text[links[0].clone()], "https://ci.example.com/omairc/42");
    }

    #[test]
    fn link_range_after_em_dash() {
        let text = format!("day \u{2014} https://example.com/x ok");
        let formatted = format_irc(&text);
        assert_eq!(formatted.links.len(), 1);
        assert_eq!(
            &formatted.plain[formatted.links[0].clone()],
            "https://example.com/x"
        );
    }

    #[test]
    fn strip_controls_like_qt() {
        let input = format!("{BOLD}a{COLOR}04") + "b" + &format!("{RESET}c");
        assert_eq!(plain_irc_text(&input), "abc");
    }
}
