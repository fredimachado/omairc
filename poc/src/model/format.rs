use super::color::Color;
use super::mock::PREFIX_RANK;
use crate::irc::plaintext::plain_irc_text;
use crate::irc::textformat::{styled_runs, IrcColor, IrcStyle};
use std::ops::Range;

/// The sixteen mIRC palette colors. Presentation: the protocol only carries the index.
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

/// Formats one IRC line for the transcript. The control-code walk lives in
/// `irc::textformat`; this only maps its colors onto the presentation palette.
pub fn format_irc(text: &str) -> FormattedText {
    let plain = plain_irc_text(text);
    let runs = styled_runs(text)
        .into_iter()
        .map(|run| StyleRun {
            range: run.range,
            style: text_style(&run.style),
        })
        .collect();
    let links = http_link_ranges(&plain);
    FormattedText { plain, runs, links }
}

fn text_style(style: &IrcStyle) -> TextStyle {
    TextStyle {
        bold: style.bold,
        italic: style.italic,
        underline: style.underline,
        reverse: style.reverse,
        monospace: style.monospace,
        strikethrough: style.strikethrough,
        fg: style.fg.and_then(presentation_color),
        bg: style.bg.and_then(presentation_color),
    }
}

fn presentation_color(color: IrcColor) -> Option<Color> {
    match color {
        IrcColor::Palette(index) => MIRC_PALETTE
            .get(index as usize)
            .and_then(|hex| Color::from_hex(hex)),
        IrcColor::Rgb(red, green, blue) => Some(Color::from_rgb8(red, green, blue)),
    }
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

/// The http(s) URL policy shared by the transcript and the Link sheet.
pub fn is_allowed_http_url(url: &str) -> bool {
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

/// A channel name found in a line: its byte range in the line, and the name
/// the range spells. The range is the trimmed token, so sentence punctuation,
/// a trailing quote and an unmatched closing bracket are not part of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelNameSpan {
    pub range: Range<usize>,
    pub name: String,
}

/// What a click on conversation or topic text does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextHit {
    /// An http(s) URL: it opens in the browser.
    Url(String),
    /// A channel name: it switches to that row, or asks before joining.
    Channel(String),
}

/// Channel names in `text`, left to right (`ircChannelNameSpans`). `chantypes`
/// is the network's CHANTYPES. A name starts with one of those characters, and
/// only at the start of the text, after whitespace or an opening delimiter, or
/// straight after a rank symbol. The rank is not part of the name. The token
/// runs to a space, comma or colon, and the name needs two characters. Trailing
/// `.,;:!?` and a trailing `"` or `'` come off one at a time, and so does a `)`,
/// `]`, `}` or `>` with no opener. `#foo<bar>` keeps its `>`.
///
/// This does not skip names inside URLs. `channel_hits` does that.
pub fn channel_name_ranges(text: &str, chantypes: &str) -> Vec<ChannelNameSpan> {
    let mut spans = Vec::new();
    if chantypes.is_empty() {
        return spans;
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut index = 0;
    while index < chars.len() {
        let (start, ch) = chars[index];
        let boundary = match index.checked_sub(1).map(|prev| chars[prev].1) {
            None => true,
            Some(prev) => is_rank_symbol(prev) || is_channel_boundary(prev),
        };
        if !boundary || !is_channel_type(ch, chantypes) {
            index += 1;
            continue;
        }
        let mut end = index + 1;
        while end < chars.len() && !stops_channel_name(chars[end].1) {
            end += 1;
        }
        let stop = chars.get(end).map_or(text.len(), |&(at, _)| at);
        let name = trim_channel_name(&text[start..stop]);
        let is_channel = name
            .chars()
            .next()
            .is_some_and(|first| is_channel_type(first, chantypes));
        if is_channel && name.chars().count() >= 2 {
            spans.push(ChannelNameSpan {
                range: start..start + name.len(),
                name: name.to_string(),
            });
        }
        index = end;
    }
    spans
}

/// The channel names in a formatted line that a click can reach: those that do
/// not start inside a URL (`ircChannelNameAt` and `appendChannelNamesFromText`
/// both skip them). The ranges are in `text.plain`.
pub fn channel_hits(text: &FormattedText, chantypes: &str) -> Vec<ChannelNameSpan> {
    channel_name_ranges(&text.plain, chantypes)
        .into_iter()
        .filter(|span| {
            !text
                .links
                .iter()
                .any(|link| link.contains(&span.range.start))
        })
        .collect()
}

/// Every clickable piece of a formatted line, with its range in `text.plain`:
/// the URLs first, then the channel names. Topic text uses this.
pub fn text_hits(text: &FormattedText, chantypes: &str) -> Vec<(Range<usize>, TextHit)> {
    let urls = text.links.iter().map(|range| {
        (
            range.clone(),
            TextHit::Url(text.plain[range.clone()].to_string()),
        )
    });
    let channels = channel_hits(text, chantypes)
        .into_iter()
        .map(|span| (span.range, TextHit::Channel(span.name)));
    urls.chain(channels).collect()
}

/// Qt's `isChannelBoundary`: what may come just before a channel name.
fn is_channel_boundary(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '(' | '[' | '{' | '<' | '"' | '\'' | ',')
}

/// Qt's `stopsChannelName`: what ends a channel token.
fn stops_channel_name(ch: char) -> bool {
    (ch as u32) < 0x20 || ch == '\u{7f}' || ch.is_whitespace() || ch == ',' || ch == ':'
}

fn is_channel_type(ch: char, chantypes: &str) -> bool {
    ch.is_ascii() && chantypes.contains(ch)
}

fn is_rank_symbol(ch: char) -> bool {
    ch.is_ascii() && PREFIX_RANK.contains(&ch)
}

/// Qt's `trimChannelName`: drops trailing sentence marks and quotes one at a
/// time, and a closing bracket that has no opener inside the token.
fn trim_channel_name(raw: &str) -> &str {
    let mut end = raw.len();
    while let Some(last) = raw[..end].chars().next_back() {
        if ".,;:!?\"'".contains(last) {
            end -= last.len_utf8();
            continue;
        }
        let opener = match last {
            ')' => '(',
            ']' => '[',
            '}' => '{',
            '>' => '<',
            _ => break,
        };
        let opens = raw[..end].chars().filter(|&c| c == opener).count();
        let closes = raw[..end].chars().filter(|&c| c == last).count();
        if closes <= opens {
            break;
        }
        end -= last.len_utf8();
    }
    &raw[..end]
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
    use crate::irc::plaintext::{BOLD, COLOR, HEX_COLOR, ITALIC, RESET, UNDERLINE};

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
    fn palette_and_hex_colors_map_to_presentation_colors() {
        let palette = format_irc("\x0304red");
        assert_eq!(palette.runs[0].style.fg, Color::from_hex("#FF0000"));
        let hex = format_irc("\x04FF0080x");
        assert_eq!(hex.runs[0].style.fg, Some(Color::from_rgb8(255, 0, 128)));
    }

    #[test]
    fn a_palette_index_past_fifteen_leaves_the_color_alone() {
        let text = format_irc("\x0399x");
        assert_eq!(text.plain, "x");
        assert_eq!(text.runs[0].style.fg, None);
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
        let text = "day \u{2014} https://example.com/x ok";
        let formatted = format_irc(text);
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

    fn names(text: &str) -> Vec<String> {
        channel_name_ranges(text, "#&")
            .into_iter()
            .map(|span| span.name)
            .collect()
    }

    #[test]
    fn a_channel_name_starts_after_a_boundary() {
        assert_eq!(names("join #desktop now"), ["#desktop"]);
        assert_eq!(names("#desktop"), ["#desktop"]);
        assert_eq!(names("x,#desktop"), ["#desktop"]);
        assert_eq!(names("(#desktop)"), ["#desktop"]);
        assert_eq!(names("[#desktop]"), ["#desktop"]);
        assert_eq!(names("#ab #cd"), ["#ab", "#cd"]);
    }

    #[test]
    fn a_name_glued_to_a_word_is_not_a_channel() {
        assert!(names("glued#desktop").is_empty());
        assert!(names("a#b").is_empty());
        assert!(names("nick:#desktop").is_empty(), "a colon glues the name");
    }

    #[test]
    fn a_name_needs_two_characters_and_a_channel_type() {
        assert!(names("#").is_empty());
        assert!(names("# alone").is_empty());
        assert!(names("#.").is_empty(), "trimming leaves only the type");
        assert!(names("nick hello").is_empty());
        assert!(channel_name_ranges("#desktop", "").is_empty());
        assert!(channel_name_ranges("&local", "#").is_empty());
        assert_eq!(
            channel_name_ranges("&local #x", "#")
                .into_iter()
                .map(|span| span.name)
                .collect::<Vec<_>>(),
            ["#x"]
        );
    }

    #[test]
    fn sentence_marks_and_quotes_come_off_the_end() {
        assert_eq!(names("see #desktop."), ["#desktop"]);
        assert_eq!(names("#desktop?!"), ["#desktop"]);
        assert_eq!(names("say \"#desktop\""), ["#desktop"]);
        assert_eq!(names("#foo's"), ["#foo's"]);
    }

    #[test]
    fn an_unmatched_closer_is_trimmed_but_a_matched_one_stays() {
        assert_eq!(names("<#desktop>"), ["#desktop"]);
        assert_eq!(names("#foo<bar>"), ["#foo<bar>"]);
        let text = "<#desktop>";
        let span = &channel_name_ranges(text, "#&")[0];
        assert_eq!(&text[span.range.clone()], "#desktop");
    }

    #[test]
    fn a_rank_in_front_is_outside_the_name() {
        let text = "@#desktop";
        let spans = channel_name_ranges(text, "#&");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].name, "#desktop");
        assert_eq!(&text[spans[0].range.clone()], "#desktop");
        assert_eq!(names("+#ab"), ["#ab"]);
    }

    #[test]
    fn multibyte_text_keeps_byte_ranges() {
        let text = "héllo #café ok";
        let spans = channel_name_ranges(text, "#&");
        assert_eq!(spans.len(), 1);
        assert_eq!(&text[spans[0].range.clone()], "#café");
    }

    #[test]
    fn channel_hits_skip_a_name_that_starts_inside_a_url() {
        let text = format_irc("see https://x.example/(#chan) and #lab");
        let hits = channel_hits(&text, "#&");
        let found: Vec<&str> = hits.iter().map(|span| span.name.as_str()).collect();
        assert_eq!(found, ["#lab"]);
        assert_eq!(&text.plain[hits[0].range.clone()], "#lab");
    }

    #[test]
    fn text_hits_list_urls_then_channel_names() {
        let text = format_irc("docs https://d.example/ in #lab");
        let hits = text_hits(&text, "#&");
        assert_eq!(hits.len(), 2);
        assert_eq!(&text.plain[hits[0].0.clone()], "https://d.example/");
        assert_eq!(hits[0].1, TextHit::Url("https://d.example/".to_string()));
        assert_eq!(&text.plain[hits[1].0.clone()], "#lab");
        assert_eq!(hits[1].1, TextHit::Channel("#lab".to_string()));
    }

    #[test]
    fn colour_codes_do_not_shift_the_channel_range() {
        let text = format_irc(&format!("{BOLD}#lab{RESET} ok"));
        let hits = channel_hits(&text, "#&");
        assert_eq!(&text.plain[hits[0].range.clone()], "#lab");
    }
}
