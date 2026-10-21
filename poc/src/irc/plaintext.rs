//! Port of `tui/internal/irc/plaintext.go` (Go) and `plainIrcText` /
//! `stripIrcColors` in `src/irc/irctextformatter.cpp` (C++).
//!
//! The mIRC control codes live here, with the color walkers that both the plain
//! text and the styled runs in `textformat` share. Walking `char`s instead of
//! bytes is equivalent: every consumed sequence is ASCII, and a UTF-8 continuation
//! scalar can never equal a control code.

pub const BOLD: char = '\x02';
pub const COLOR: char = '\x03';
pub const HEX_COLOR: char = '\x04';
pub const RESET: char = '\x0f';
pub const MONOSPACE: char = '\x11';
pub const REVERSE: char = '\x16';
pub const ITALIC: char = '\x1d';
pub const STRIKETHROUGH: char = '\x1e';
pub const UNDERLINE: char = '\x1f';

/// The text a reader sees: colors and every formatting code removed.
pub fn plain_irc_text(text: &str) -> String {
    strip_irc_colors(text)
        .chars()
        .filter(|&code| {
            !matches!(
                code,
                BOLD | RESET | MONOSPACE | REVERSE | ITALIC | STRIKETHROUGH | UNDERLINE
            )
        })
        .collect()
}

/// Removes mIRC color and hex-color codes, mirroring `stripIrcColors`.
fn strip_irc_colors(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        match chars[index] {
            COLOR => index = consume_mirc_color(&chars, index),
            HEX_COLOR => index = consume_hex_color(&chars, index),
            code => out.push(code),
        }
        index += 1;
    }
    out
}

/// Returns the last index of `\x03(\d{1,2}(,\d{1,2})?)?` starting at `index`,
/// or `index` itself when no digit follows the code.
pub(super) fn consume_mirc_color(chars: &[char], index: usize) -> usize {
    let size = chars.len();
    let mut at = index + 1;
    if at >= size || !chars[at].is_ascii_digit() {
        return index;
    }
    at += 1;
    if at < size && chars[at].is_ascii_digit() {
        at += 1;
    }
    if at + 1 < size && chars[at] == ',' && chars[at + 1].is_ascii_digit() {
        at += 2;
        if at < size && chars[at].is_ascii_digit() {
            at += 1;
        }
    }
    at - 1
}

/// Returns the last index of `\x04([0-9A-Fa-f]{6}(,[0-9A-Fa-f]{6})?)?` starting
/// at `index`, or `index` itself when no six hex digits follow the code.
pub(super) fn consume_hex_color(chars: &[char], index: usize) -> usize {
    let mut at = index + 1;
    if !six_hex_at(chars, at) {
        return index;
    }
    at += 6;
    if at < chars.len() && chars[at] == ',' && six_hex_at(chars, at + 1) {
        at += 7;
    }
    at - 1
}

fn six_hex_at(chars: &[char], start: usize) -> bool {
    chars
        .get(start..start + 6)
        .is_some_and(|digits| digits.iter().all(char::is_ascii_hexdigit))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from tui/internal/irc/plaintext_test.go.

    #[test]
    fn bold_is_stripped() {
        assert_eq!(plain_irc_text("hey \x02fred"), "hey fred");
    }

    #[test]
    fn mirc_colors_cover_runs_resets_single_digits_and_bare_codes() {
        let cases = [
            ("\x0304red\x0fplain", "redplain"),
            ("\x0304,05red", "red"),
            ("\x034red", "red"),
            ("\x03", ""),
            ("\x0304,", ","),
        ];
        for (input, want) in cases {
            assert_eq!(plain_irc_text(input), want, "input {input:?}");
        }
    }

    #[test]
    fn hex_colors_with_and_without_a_background_pair() {
        let cases = [
            ("\x04000000blue", "blue"),
            ("\x04000000,ffffffblue", "blue"),
        ];
        for (input, want) in cases {
            assert_eq!(plain_irc_text(input), want, "input {input:?}");
        }
    }

    #[test]
    fn all_formatting_control_codes_are_dropped() {
        assert_eq!(plain_irc_text("\x02b\x0f\x11m\x16r\x1d\x1e\x1f"), "bmr");
    }

    #[test]
    fn multibyte_text_survives_stripping() {
        assert_eq!(plain_irc_text("\x02héllo → 世界\x0f"), "héllo → 世界");
    }

    #[test]
    fn an_invalid_hex_code_drops_only_itself() {
        assert_eq!(plain_irc_text("\x04ZZwarm"), "ZZwarm");
    }

    #[test]
    fn a_color_code_with_two_digit_fg_and_bg_is_fully_consumed() {
        assert_eq!(plain_irc_text("\x0312,13x"), "x");
        // Only two digits belong to the fg; the third digit is text.
        assert_eq!(plain_irc_text("\x03123,4x"), "3,4x");
    }
}
