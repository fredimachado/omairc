//! Port of `tui/internal/irc/channelname.go` (Go) and `src/irc/ircchannelname.{h,cpp}` (C++).
//!
//! Finds channel names inside message text, so a transcript can link them. The
//! prefix set comes from the server's CHANTYPES and the rank glyphs from PREFIX,
//! never from a hard-coded `#` or `@`.

#[cfg(test)]
use super::serverfeatures::IrcServerFeatures;

/// One channel name inside a message or a topic (`IrcChannelNameSpan`). `start`
/// and `end` are byte offsets, with `end` exclusive. `name` is the trimmed token,
/// so trailing sentence punctuation sits outside the span.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
pub struct IrcChannelNameSpan {
    pub start: usize,
    pub end: usize,
    pub name: String,
}

/// Every channel name in `text`. Returns nothing when CHANTYPES is empty.
///
/// A token starts with an advertised channel type, and only at the start of the
/// text, after whitespace or an opening delimiter, or right after a PREFIX rank
/// glyph (the span does not include that glyph). It runs to a space, comma, colon
/// or control character. Trailing `.,;:!?` and a trailing `"` or `'` are stripped
/// one mark at a time. An unmatched trailing `)`, `]`, `}` or `>` is stripped too,
/// and `>` pairs with `<`. The result is kept only when `is_channel` accepts it
/// and it is at least two characters.
#[cfg(test)]
pub fn channel_name_spans(text: &str, features: &IrcServerFeatures) -> Vec<IrcChannelNameSpan> {
    let types = features.channel_types();
    if text.is_empty() || types.is_empty() {
        return Vec::new();
    }
    let symbols = features.prefix_symbols();
    let mut spans = Vec::new();
    let mut index = 0;
    while index < text.len() {
        let Some(character) = text[index..].chars().next() else {
            break;
        };
        let size = character.len_utf8();
        let boundary = index == 0
            || rank_before(text, index, symbols)
            || channel_boundary_before(text, index);
        if !boundary || !is_channel_prefix(character, types) {
            index += size;
            continue;
        }

        let body_start = index + size;
        let body_length = text[body_start..].find(stops_channel_name).unwrap_or(text.len() - body_start);
        let raw_end = body_start + body_length;
        let name = trim_channel_name(&text[index..raw_end]);
        if name.chars().count() >= 2 && features.is_channel(name) {
            spans.push(IrcChannelNameSpan {
                start: index,
                end: index + name.len(),
                name: name.to_owned(),
            });
        }
        index = raw_end;
    }
    spans
}

/// The channel name whose span contains byte offset `index`, if any.
#[cfg(test)]
pub fn channel_name_at(text: &str, index: usize, features: &IrcServerFeatures) -> Option<String> {
    if text.is_empty() || index >= text.len() {
        return None;
    }
    channel_name_spans(text, features)
        .into_iter()
        .find(|span| index >= span.start && index < span.end)
        .map(|span| span.name)
}

#[cfg(test)]
fn rank_before(text: &str, index: usize, symbols: &str) -> bool {
    if index == 0 || symbols.is_empty() {
        return false;
    }
    text[..index]
        .chars()
        .next_back()
        .is_some_and(|character| is_channel_prefix(character, symbols))
}

/// True when `character` is one of the ASCII channel-type or rank bytes in `types`.
#[cfg(test)]
fn is_channel_prefix(character: char, types: &str) -> bool {
    character.is_ascii() && types.as_bytes().contains(&(character as u8))
}

/// A space, a control character, or one of `,` and `:`. Such a character ends a name.
#[cfg(test)]
fn stops_channel_name(character: char) -> bool {
    character < ' ' || character == '\u{7F}' || character.is_whitespace() || character == ',' || character == ':'
}

#[cfg(test)]
fn channel_boundary_before(text: &str, index: usize) -> bool {
    text[..index].chars().next_back().is_some_and(is_channel_boundary)
}

#[cfg(test)]
fn is_channel_boundary(character: char) -> bool {
    character.is_whitespace() || matches!(character, '(' | '[' | '{' | '<' | '"' | '\'' | ',')
}

/// Strips trailing sentence punctuation and unmatched closing brackets from `raw`.
#[cfg(test)]
fn trim_channel_name(raw: &str) -> &str {
    let mut end = raw.len();
    while let Some(character) = raw[..end].chars().next_back() {
        if matches!(character, '.' | ',' | ';' | ':' | '!' | '?' | '"' | '\'') {
            end -= character.len_utf8();
            continue;
        }
        let open = match character {
            ')' => '(',
            ']' => '[',
            '}' => '{',
            '>' => '<',
            _ => break,
        };
        let opens = raw[..end].chars().filter(|&current| current == open).count();
        let closes = raw[..end].chars().filter(|&current| current == character).count();
        if closes > opens {
            end -= character.len_utf8();
            continue;
        }
        break;
    }
    &raw[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The byte offset of the first occurrence of `needle` in `text`.
    fn index_of(text: &str, needle: &str) -> usize {
        text.find(needle).unwrap_or_else(|| panic!("{text:?} does not contain {needle:?}"))
    }

    fn features_with_types(types: &str) -> IrcServerFeatures {
        let mut features = IrcServerFeatures::new();
        features.apply_token(&format!("CHANTYPES={types}"));
        features
    }

    #[test]
    fn spans_use_advertised_types() {
        let hash = features_with_types("#");
        assert_eq!(channel_name_at("see #desktop now", index_of("see #desktop now", "#desktop"), &hash).as_deref(), Some("#desktop"));
        assert_eq!(channel_name_at("see &local now", index_of("see &local now", "&local"), &hash), None);

        let amp = features_with_types("&");
        assert_eq!(channel_name_at("see #desktop now", index_of("see #desktop now", "#"), &amp), None);
        assert_eq!(channel_name_at("see &local now", index_of("see &local now", "&local"), &amp).as_deref(), Some("&local"));

        let dollar = features_with_types("$");
        let text = "try $secret and #nope";
        assert_eq!(channel_name_at(text, index_of(text, "$secret"), &dollar).as_deref(), Some("$secret"));
        assert_eq!(channel_name_at(text, index_of(text, "#nope"), &dollar), None);

        let empty = features_with_types("");
        assert_eq!(channel_name_at("see #desktop", index_of("see #desktop", "#"), &empty), None, "empty CHANTYPES matches nothing");
    }

    #[test]
    fn boundaries_and_trim() {
        let features = features_with_types("#");
        let at = |text: &str, needle: &str| channel_name_at(text, index_of(text, needle), &features);

        assert_eq!(at("foo#desktop", "#"), None, "a mid-word channel is not a link");
        assert_eq!(at("https://example.com/#desktop", "#desktop"), None, "a URL fragment is not a channel");
        assert_eq!(at("just # here", "#"), None, "a lone hash is not a channel");

        let paren = "(#desktop).";
        assert_eq!(at(paren, "#desktop").as_deref(), Some("#desktop"));
        assert_eq!(channel_name_at(paren, paren.len() - 1, &features), None, "a trailing period is outside");

        let bang = "join #foo!";
        assert_eq!(at(bang, "#foo").as_deref(), Some("#foo"));
        assert_eq!(channel_name_at(bang, bang.len() - 1, &features), None, "the bang itself is not a name");

        assert_eq!(at("see #c++ now", "#c++").as_deref(), Some("#c++"));

        let spans = channel_name_spans("try #foo,#bar please", &features);
        let names: Vec<&str> = spans.iter().map(|span| span.name.as_str()).collect();
        assert_eq!(names, ["#foo", "#bar"]);

        assert_eq!(channel_name_at("#foo_(bar)", 0, &features).as_deref(), Some("#foo_(bar)"), "balanced parens stay");

        let quoted = "try \"#desktop\" today";
        assert_eq!(at(quoted, "#desktop").as_deref(), Some("#desktop"));
        assert_eq!(at(quoted, "\" today"), None, "the closing quote is not a name");

        let apostrophe = "try '#ops' today";
        assert_eq!(at(apostrophe, "#ops").as_deref(), Some("#ops"));
        assert_eq!(at(apostrophe, "' today"), None);

        let angle = "see <#desktop> now";
        assert_eq!(at(angle, "#desktop").as_deref(), Some("#desktop"));
        assert_eq!(at(angle, ">"), None);

        let kept_angle = "#foo<bar>";
        assert_eq!(channel_name_at(kept_angle, 0, &features).as_deref(), Some("#foo<bar>"));
        assert_eq!(channel_name_at(kept_angle, kept_angle.len() - 1, &features).as_deref(), Some("#foo<bar>"));

        assert_eq!(channel_name_at("#foo's", 0, &features).as_deref(), Some("#foo's"), "an interior apostrophe stays");

        let ranked = "on @#chan today";
        assert_eq!(at(ranked, "#chan").as_deref(), Some("#chan"));
        assert_eq!(at(ranked, "@"), None, "a rank glyph is not part of the name");
    }

    #[test]
    fn rank_glyphs_come_from_prefix() {
        let mut voice = IrcServerFeatures::new();
        voice.apply_token("CHANTYPES=#");
        voice.apply_token("PREFIX=(v)+");

        let at_rank = "on @#chan";
        assert_eq!(channel_name_at(at_rank, index_of(at_rank, "#"), &voice), None, "@ is not a rank here");

        let plus_rank = "on +#chan";
        assert_eq!(channel_name_at(plus_rank, index_of(plus_rank, "#chan"), &voice).as_deref(), Some("#chan"));
        assert_eq!(channel_name_at(plus_rank, index_of(plus_rank, "+"), &voice), None, "the voice glyph is not a name");
    }

    #[test]
    fn short_and_non_ascii_names() {
        let features = features_with_types("#");
        assert_eq!(channel_name_at("#", 0, &features), None, "one character is too short");
        assert_eq!(channel_name_at("#\u{e9}t\u{e9} ok", 0, &features).as_deref(), Some("#\u{e9}t\u{e9}"));
    }
}
