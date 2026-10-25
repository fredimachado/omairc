//! Port of `tui/internal/irc/textformat.go` (Go) and `emphasizedIrcText` /
//! `hasIrcEmphasis` in `src/irc/irctextformatter.cpp` (C++).
//!
//! One walker produces styled runs with the full attribute set (colors, reverse,
//! monospace, strikethrough) for the GPUI transcript. `emphasized_segments` is the
//! TUI's view: bold, italic and underline only, with colors and the other codes
//! dropped, and adjacent runs that share those three flags merged.

use std::ops::Range;

#[cfg(test)]
use super::plaintext::plain_irc_text;
use super::plaintext::{
    consume_hex_color, consume_mirc_color, BOLD, COLOR, HEX_COLOR, ITALIC, MONOSPACE, RESET,
    REVERSE, STRIKETHROUGH, UNDERLINE,
};

/// A run of TUI-visible text. The TUI never renders mIRC colors, so only the
/// three emphasis flags survive.
#[cfg(test)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub text: String,
}

/// One color from a color code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrcColor {
    /// A mIRC palette entry, 0 to 15.
    Palette(u8),
    /// A 24-bit color from a `\x04` hex code.
    Rgb(u8, u8, u8),
}

/// Every attribute a control code can set. A default style is plain text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IrcStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub reverse: bool,
    pub monospace: bool,
    pub strikethrough: bool,
    pub fg: Option<IrcColor>,
    pub bg: Option<IrcColor>,
}

/// A styled stretch of the plain text. `range` is a byte range in
/// `plain_irc_text(text)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StyledRun {
    pub range: Range<usize>,
    pub style: IrcStyle,
}

/// Whether the raw text carries a bold, italic or underline code. Colors are not
/// stripped first, so a color-only string reports false.
#[cfg(test)]
pub fn has_irc_emphasis(text: &str) -> bool {
    text.chars()
        .any(|code| matches!(code, BOLD | ITALIC | UNDERLINE))
}

/// The TUI's styled runs: bold, italic and underline only, over the plain text.
/// Colors and the reverse, monospace and strikethrough codes do not split a run.
/// Adjacent runs with equal flags are merged, so an unchanged flag set reads as
/// one run. Go would split at a toggle that leaves the flags unchanged; the text
/// and the flags a reader sees are the same either way.
#[cfg(test)]
pub fn emphasized_segments(text: &str) -> Vec<Segment> {
    let plain = plain_irc_text(text);
    let mut segments: Vec<Segment> = Vec::new();
    for run in styled_runs(text) {
        let (bold, italic, underline) = (run.style.bold, run.style.italic, run.style.underline);
        let piece = &plain[run.range];
        match segments.last_mut() {
            Some(last) if (last.bold, last.italic, last.underline) == (bold, italic, underline) => {
                last.text.push_str(piece);
            }
            _ => segments.push(Segment {
                bold,
                italic,
                underline,
                text: piece.to_string(),
            }),
        }
    }
    segments
}

/// Every styled run of `text`, in order, with each attribute change starting a
/// new run. The runs cover the plain text, which equals `plain_irc_text(text)`.
pub fn styled_runs(text: &str) -> Vec<StyledRun> {
    let chars: Vec<char> = text.chars().collect();
    let mut runs = Vec::new();
    let mut style = IrcStyle::default();
    let mut plain_len = 0usize;
    let mut run_start = 0usize;
    let mut index = 0usize;
    while index < chars.len() {
        let start = index;
        index += 1;
        match chars[start] {
            COLOR => {
                close_run(&mut runs, &mut run_start, plain_len, &style);
                index = consume_mirc_color(&chars, start) + 1;
                apply_mirc_color(&chars, start, &mut style);
            }
            HEX_COLOR => {
                close_run(&mut runs, &mut run_start, plain_len, &style);
                let end = consume_hex_color(&chars, start);
                if end > start {
                    apply_hex_color(&chars, start, &mut style);
                }
                index = end + 1;
            }
            BOLD => {
                close_run(&mut runs, &mut run_start, plain_len, &style);
                style.bold = !style.bold;
            }
            ITALIC => {
                close_run(&mut runs, &mut run_start, plain_len, &style);
                style.italic = !style.italic;
            }
            UNDERLINE => {
                close_run(&mut runs, &mut run_start, plain_len, &style);
                style.underline = !style.underline;
            }
            REVERSE => {
                close_run(&mut runs, &mut run_start, plain_len, &style);
                style.reverse = !style.reverse;
            }
            MONOSPACE => {
                close_run(&mut runs, &mut run_start, plain_len, &style);
                style.monospace = !style.monospace;
            }
            STRIKETHROUGH => {
                close_run(&mut runs, &mut run_start, plain_len, &style);
                style.strikethrough = !style.strikethrough;
            }
            RESET => {
                close_run(&mut runs, &mut run_start, plain_len, &style);
                style = IrcStyle::default();
            }
            code => plain_len += code.len_utf8(),
        }
    }
    close_run(&mut runs, &mut run_start, plain_len, &style);
    runs
}

/// Ends the open run at `plain_len` with the style it was written in.
fn close_run(runs: &mut Vec<StyledRun>, run_start: &mut usize, plain_len: usize, style: &IrcStyle) {
    if plain_len > *run_start {
        runs.push(StyledRun {
            range: *run_start..plain_len,
            style: style.clone(),
        });
    }
    *run_start = plain_len;
}

/// Applies the `\x03` at `start`. A bare code resets both colors; digits set the
/// foreground and an optional `,bg` sets the background. Palette indices past 15
/// are ignored and keep the previous color, as in Qt.
fn apply_mirc_color(chars: &[char], start: usize, style: &mut IrcStyle) {
    let mut at = start + 1;
    let Some(fg) = read_code(chars, &mut at) else {
        style.fg = None;
        style.bg = None;
        return;
    };
    if fg <= 15 {
        style.fg = Some(IrcColor::Palette(fg));
    }
    if chars.get(at) == Some(&',') {
        let mut bg_at = at + 1;
        if let Some(bg) = read_code(chars, &mut bg_at) {
            if bg <= 15 {
                style.bg = Some(IrcColor::Palette(bg));
            }
        }
    }
}

/// Applies the `\x04` hex color at `start`: six hex digits set the foreground,
/// and an optional `,` plus six more set the background.
fn apply_hex_color(chars: &[char], start: usize, style: &mut IrcStyle) {
    let Some(fg) = rgb_at(chars, start + 1) else {
        return;
    };
    style.fg = Some(fg);
    let comma = start + 7;
    if chars.get(comma) == Some(&',') {
        if let Some(bg) = rgb_at(chars, comma + 1) {
            style.bg = Some(bg);
        }
    }
}

/// Reads one or two decimal digits at `*at` and moves `*at` past them.
fn read_code(chars: &[char], at: &mut usize) -> Option<u8> {
    let first = chars.get(*at)?.to_digit(10)?;
    *at += 1;
    let mut value = first as u8;
    if let Some(second) = chars.get(*at).and_then(|digit| digit.to_digit(10)) {
        value = value * 10 + second as u8;
        *at += 1;
    }
    Some(value)
}

/// Six hex digits at `start` as a color, or `None`.
fn rgb_at(chars: &[char], start: usize) -> Option<IrcColor> {
    let digits = chars.get(start..start + 6)?;
    if !digits.iter().all(char::is_ascii_hexdigit) {
        return None;
    }
    let value = digits.iter().fold(0u32, |acc, digit| {
        acc * 16 + digit.to_digit(16).unwrap_or(0)
    });
    Some(IrcColor::Rgb(
        (value >> 16) as u8,
        (value >> 8) as u8,
        value as u8,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from tui/internal/irc/textformat_test.go.

    fn segment(bold: bool, italic: bool, underline: bool, text: &str) -> Segment {
        Segment {
            bold,
            italic,
            underline,
            text: text.to_string(),
        }
    }

    fn assert_segments(text: &str, want: Vec<Segment>) {
        assert_eq!(emphasized_segments(text), want, "input {text:?}");
    }

    #[test]
    fn bold_italic_and_underline_each_start_a_run() {
        assert_segments(
            "hello \x02world\x02",
            vec![
                segment(false, false, false, "hello "),
                segment(true, false, false, "world"),
            ],
        );
        assert_segments(
            "\x1ditalic\x1d",
            vec![segment(false, true, false, "italic")],
        );
        assert_segments("\x1funder\x1f", vec![segment(false, false, true, "under")]);
    }

    #[test]
    fn nested_emphasis_closes_inner_first_without_changing_runs() {
        assert_segments(
            "\x02bold\x1ditalic\x1d\x02",
            vec![
                segment(true, false, false, "bold"),
                segment(true, true, false, "italic"),
            ],
        );
    }

    #[test]
    fn reset_closes_every_open_run_and_the_rest_is_plain() {
        assert_segments(
            "\x02a\x0fb",
            vec![
                segment(true, false, false, "a"),
                segment(false, false, false, "b"),
            ],
        );
        assert_segments("\x02\x1d\x1f\x0f", vec![]);
    }

    #[test]
    fn reverse_monospace_and_strikethrough_do_not_split_a_bold_run() {
        assert_segments("\x02a\x16b\x02", vec![segment(true, false, false, "ab")]);
        assert_segments(
            "\x02a\x11b\x1ec\x02",
            vec![segment(true, false, false, "abc")],
        );
    }

    #[test]
    fn a_color_only_string_is_one_plain_segment() {
        assert_segments("\x0304red\x03", vec![segment(false, false, false, "red")]);
    }

    #[test]
    fn literal_markup_stays_literal_text() {
        assert_segments(
            "<b>not html</b>",
            vec![segment(false, false, false, "<b>not html</b>")],
        );
    }

    #[test]
    fn a_hex_color_run_is_dropped_and_never_leaks_into_text() {
        assert_segments("\x04FF0000red", vec![segment(false, false, false, "red")]);
    }

    #[test]
    fn double_spaces_survive() {
        assert_segments(
            "a  \x02b\x02",
            vec![
                segment(false, false, false, "a  "),
                segment(true, false, false, "b"),
            ],
        );
    }

    #[test]
    fn non_ascii_text_survives_byte_for_byte_under_emphasis() {
        assert_segments(
            "\x02héllo → 世界\x0f",
            vec![segment(true, false, false, "héllo → 世界")],
        );
    }

    #[test]
    fn has_irc_emphasis_ignores_codes_that_are_stripped_only() {
        let cases = [
            ("\x16flip\x16", false),
            ("\x11mono\x11", false),
            ("\x0304red\x03", false),
            ("\x04FF0000red", false),
            ("hello \x02world\x02", true),
            ("\x1ditalic\x1d", true),
            ("\x1funder\x1f", true),
        ];
        for (input, want) in cases {
            assert_eq!(has_irc_emphasis(input), want, "input {input:?}");
        }
    }

    // Styled runs: the full attribute set the GPUI transcript renders.

    fn run(start: usize, end: usize, style: IrcStyle) -> StyledRun {
        StyledRun {
            range: start..end,
            style,
        }
    }

    #[test]
    fn a_palette_color_persists_past_a_two_digit_code_it_cannot_use() {
        let runs = styled_runs("\x0304a\x0399b");
        assert_eq!(
            runs,
            vec![
                run(
                    0,
                    1,
                    IrcStyle {
                        fg: Some(IrcColor::Palette(4)),
                        ..Default::default()
                    }
                ),
                run(
                    1,
                    2,
                    IrcStyle {
                        fg: Some(IrcColor::Palette(4)),
                        ..Default::default()
                    }
                ),
            ]
        );
    }

    #[test]
    fn a_comma_pair_sets_the_background_and_a_bare_code_resets_both() {
        let runs = styled_runs("\x0304,12a\x03b");
        assert_eq!(
            runs,
            vec![
                run(
                    0,
                    1,
                    IrcStyle {
                        fg: Some(IrcColor::Palette(4)),
                        bg: Some(IrcColor::Palette(12)),
                        ..Default::default()
                    },
                ),
                run(1, 2, IrcStyle::default()),
            ]
        );
    }

    #[test]
    fn hex_foreground_and_background_parse_to_rgb() {
        let runs = styled_runs("\x04FF0080,00ff00x");
        assert_eq!(
            runs,
            vec![run(
                0,
                1,
                IrcStyle {
                    fg: Some(IrcColor::Rgb(255, 0, 128)),
                    bg: Some(IrcColor::Rgb(0, 255, 0)),
                    ..Default::default()
                },
            )]
        );
    }

    #[test]
    fn reverse_monospace_and_strikethrough_are_styles_in_their_own_right() {
        let runs = styled_runs("\x16r\x11m\x1es");
        let last = runs.last().expect("a run");
        assert!(last.style.reverse && last.style.monospace && last.style.strikethrough);
        assert_eq!(runs.len(), 3);
    }

    #[test]
    fn styled_runs_cover_plain_text_byte_for_byte() {
        let text = "\x02héllo\x0304 → \x0f世界\x04FF0000!";
        let plain = plain_irc_text(text);
        let covered: String = styled_runs(text)
            .iter()
            .map(|run| &plain[run.range.clone()])
            .collect();
        assert_eq!(covered, plain);
    }
}
