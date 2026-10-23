//! Port of `tui/internal/irc/storagepath.go` (Go) and `src/irc/ircstoragepath.cpp`
//! (C++). Pure segment encoders and decoders; filesystem helpers live in `storage/`.

use sha2::{Digest, Sha256};

use crate::irc::casemapping::IrcCaseMapping;
use crate::irc::wiretext::wire_text;

fn is_safe_storage_byte(byte_value: u8, is_last_byte: bool) -> bool {
    if (b'a'..=b'z').contains(&byte_value) {
        return true;
    }
    if (b'0'..=b'9').contains(&byte_value) {
        return true;
    }
    match byte_value {
        b'#' | b'&' | b'+' | b'-' | b'_' | b'{' | b'}' | b'~' | b'[' | b']' | b'^' => true,
        b'.' => !is_last_byte,
        _ => false,
    }
}

fn percent_encode_byte(byte_value: u8) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    format!(
        "%{}{}",
        HEX[(byte_value >> 4) as usize] as char,
        HEX[(byte_value & 0x0f) as usize] as char
    )
}

fn encode_storage_bytes(utf8: &[u8]) -> String {
    let mut out = String::with_capacity(utf8.len() * 3);
    for (index, &current) in utf8.iter().enumerate() {
        if is_safe_storage_byte(current, index == utf8.len() - 1) {
            out.push(current as char);
        } else {
            out.push_str(&percent_encode_byte(current));
        }
    }
    out
}

fn encode_with_leading_escape(utf8: &[u8]) -> String {
    if utf8.is_empty() {
        return String::new();
    }
    format!(
        "{}{}",
        percent_encode_byte(utf8[0]),
        encode_storage_bytes(&utf8[1..])
    )
}

fn hash_long_segment(utf8: &[u8]) -> String {
    let sum = Sha256::digest(utf8);
    format!("h{}", hex::encode(sum))
}

fn trim_trailing_spaces_and_dots(text: &str) -> &str {
    let mut end = text.len();
    while end > 0 {
        let ch = text.as_bytes()[end - 1];
        if ch != b' ' && ch != b'.' {
            break;
        }
        end -= 1;
    }
    text.get(..end).unwrap_or("")
}

fn device_base_token(full_name: &str) -> &str {
    let trimmed = trim_trailing_spaces_and_dots(full_name);
    trimmed
        .split_once('.')
        .map_or(trimmed, |(base, _)| base)
}

fn device_base_matches(stem: &str) -> bool {
    let stem = trim_trailing_spaces_and_dots(stem);
    if stem.is_empty() {
        return false;
    }
    let lowered = stem.to_ascii_lowercase();
    if matches!(lowered.as_str(), "con" | "prn" | "aux" | "nul") {
        return true;
    }
    let chars: Vec<char> = lowered.chars().collect();
    if chars.len() != 4 {
        return false;
    }
    let prefix: String = chars[..3].iter().collect();
    if prefix != "com" && prefix != "lpt" {
        return false;
    }
    let suffix = chars[3];
    if suffix.is_ascii_digit() {
        return true;
    }
    matches!(suffix, '\u{00b9}' | '\u{00b2}' | '\u{00b3}')
}

fn is_win32_device_name(text: &str, extension: &str) -> bool {
    device_base_matches(device_base_token(&format!("{text}{extension}")))
}

/// Encodes `text` as an on-disk path segment (`OmaircStorageSegment`).
pub fn storage_segment(text: &str, extension_for_device_check: Option<&str>) -> String {
    let extension = extension_for_device_check.unwrap_or("");
    if text.is_empty() || text == "." || text == ".." {
        return "_".to_owned();
    }
    let utf8 = text.as_bytes();
    let mut segment = if is_win32_device_name(text, extension) {
        encode_with_leading_escape(utf8)
    } else {
        encode_storage_bytes(utf8)
    };
    if segment.len() + extension.len() > 255 {
        return hash_long_segment(utf8);
    }
    if storage_segment_is_hash(&segment) {
        segment = format!("{}{}", percent_encode_byte(b'h'), &segment[1..]);
    }
    segment
}

/// Applies wire-text normalization, then [`storage_segment`].
pub fn wire_storage_segment(text: &str, extension_for_device_check: Option<&str>) -> String {
    storage_segment(&wire_text(text.as_bytes()), extension_for_device_check)
}

/// Normalizes `target` through `mapping`, then encodes with wire semantics.
pub(crate) fn target_segment(
    target: &str,
    mapping: IrcCaseMapping,
    extension_for_device_check: Option<&str>,
) -> String {
    let normalized = mapping.normalize(target);
    wire_storage_segment(&normalized, extension_for_device_check)
}

/// The pre-Omairc segment encoding (`LegacyStorageSegment`).
pub fn legacy_storage_segment(text: &str) -> String {
    let text = text
        .replace('%', "%25")
        .replace('/', "%2f")
        .replace('\\', "%5c")
        .replace('\0', "%00");
    if text.is_empty() || text == "." || text == ".." {
        return "_".to_owned();
    }
    text
}

/// Reverses [`legacy_storage_segment`].
pub(crate) fn decode_legacy_segment(text: &str) -> String {
    text.replace("%00", "\0")
        .replace("%5c", "\\")
        .replace("%2f", "/")
        .replace("%25", "%")
}

/// True when `segment` is a hashed long segment (`h` + 64 lowercase hex chars).
pub fn storage_segment_is_hash(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    if bytes.len() != 65 || bytes[0] != b'h' {
        return false;
    }
    bytes[1..].iter().all(|&character| {
        character.is_ascii_digit() || (b'a'..=b'f').contains(&character)
    })
}

fn hex_byte_value(high: u8, low: u8) -> Option<u8> {
    let high_value = hex_digit_value(high)?;
    let low_value = hex_digit_value(low)?;
    Some(high_value << 4 | low_value)
}

fn hex_digit_value(character: u8) -> Option<u8> {
    match character {
        b'0'..=b'9' => Some(character - b'0'),
        b'a'..=b'f' => Some(character - b'a' + 10),
        b'A'..=b'F' => Some(character - b'A' + 10),
        _ => None,
    }
}

fn latin1_bytes_to_string(raw: &[u8]) -> String {
    raw.iter().map(|&value| char::from(value)).collect()
}

/// Reverses [`storage_segment`] (hashes decode to themselves).
pub(crate) fn decode_storage_segment(segment: &str) -> String {
    if storage_segment_is_hash(segment) {
        return segment.to_owned();
    }
    let bytes = segment.as_bytes();
    let mut decoded_bytes = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Some(value) = hex_byte_value(bytes[index + 1], bytes[index + 2]) {
                decoded_bytes.push(value);
                index += 3;
                continue;
            }
        }
        decoded_bytes.push(bytes[index]);
        index += 1;
    }
    let decoded = String::from_utf8_lossy(&decoded_bytes).into_owned();
    if decoded.is_empty() && !decoded_bytes.is_empty() {
        return latin1_bytes_to_string(&decoded_bytes);
    }
    decoded
}

mod hex {
    pub fn encode(bytes: impl AsRef<[u8]>) -> String {
        bytes
            .as_ref()
            .iter()
            .map(|byte| format!("{:02x}", byte))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::casemapping::{IrcCaseMapping, IrcCaseMappingKind};

    #[test]
    fn encodes_reserved_characters() {
        let cases = [
            ("#omarchy", "#omarchy"),
            ("alice", "alice"),
            ("freenode", "freenode"),
            ("net-1", "net-1"),
            ("#a|b", "#a%7cb"),
        ];
        for (text, want) in cases {
            assert_eq!(
                storage_segment(text, None),
                want,
                "storage_segment({text})"
            );
        }
    }

    #[test]
    fn percent_encoding_is_injective() {
        let slash = storage_segment("a/b", None);
        let literal = storage_segment("a%2fb", None);
        assert_ne!(slash, literal);
    }

    #[test]
    fn encodes_trailing_dot_and_space() {
        assert_eq!(storage_segment("tail.", None), "tail%2e");
        assert_eq!(storage_segment("tail ", None), "tail%20");
    }

    #[test]
    fn encodes_empty_and_dot_segments() {
        for (text, want) in [("", "_"), (".", "_"), ("..", "_")] {
            assert_eq!(storage_segment(text, None), want);
        }
    }

    #[test]
    fn encodes_device_names() {
        let cases = [
            ("con", "", "%63on"),
            ("con", ".json", "%63on"),
            ("aux", "", "%61ux"),
            ("nul", "", "%6eul"),
            ("com1", "", "%63om1"),
            ("lpt9", "", "%6cpt9"),
            ("prn", "", "%70rn"),
            ("con.", "", "%63on%2e"),
        ];
        for (text, extension, want) in cases {
            assert_eq!(
                storage_segment(text, Some(extension)),
                want,
                "storage_segment({text}, {extension})"
            );
        }
    }

    #[test]
    fn encodes_dotted_device_names() {
        let cases = [
            ("nul.tar.gz", "%6eul.tar.gz"),
            ("com1.tar.gz", "%63om1.tar.gz"),
            ("aux.foo.bar", "%61ux.foo.bar"),
            ("con.txt.", "%63on.txt%2e"),
            ("com\u{00b9}", "%63om%c2%b9"),
            ("lpt\u{00b3}", "%6cpt%c2%b3"),
        ];
        for (text, want) in cases {
            assert_eq!(storage_segment(text, None), want);
        }
    }

    #[test]
    fn caps_segment_length_with_extension() {
        let extension = ".json";
        let within_limit = "a".repeat(250);
        let segment_within = storage_segment(&within_limit, Some(extension));
        assert!(segment_within.starts_with('a'));
        assert_eq!(segment_within.len() + extension.len(), 255);

        let over_limit = "a".repeat(251);
        let segment_over = storage_segment(&over_limit, Some(extension));
        assert!(segment_over.starts_with('h'));
        assert!(segment_over.len() + extension.len() <= 255);

        let mapping = IrcCaseMapping::new(IrcCaseMappingKind::Rfc1459);
        let target_segment = target_segment(&over_limit, mapping, Some(extension));
        assert_eq!(target_segment, segment_over);
    }

    #[test]
    fn preserves_network_case_and_lowercase_hex() {
        assert_ne!(storage_segment("Ab", None), storage_segment("ab", None));
        assert_eq!(storage_segment("#a|b", None), "#a%7cb");
    }

    #[test]
    fn accented_characters_encode_distinctly() {
        let lower = storage_segment("\u{e9}", None);
        let upper = storage_segment("\u{c9}", None);
        assert_ne!(lower, upper);
        assert_eq!(lower, "%c3%a9");
        assert_eq!(upper, "%c3%89");
    }

    #[test]
    fn target_segment_follows_case_mapping() {
        let rfc1459 = IrcCaseMapping::new(IrcCaseMappingKind::Rfc1459);
        let ascii = IrcCaseMapping::new(IrcCaseMappingKind::Ascii);
        let strict = IrcCaseMapping::new(IrcCaseMappingKind::Rfc1459Strict);

        assert_eq!(
            target_segment("#Chan", rfc1459, None),
            target_segment("#chan", rfc1459, None)
        );
        assert_eq!(
            target_segment("#Chan", ascii, None),
            target_segment("#chan", ascii, None)
        );
        assert_eq!(
            target_segment("#foo[", rfc1459, None),
            target_segment("#foo{", rfc1459, None)
        );
        assert_ne!(
            target_segment("#foo[", ascii, None),
            target_segment("#foo{", ascii, None)
        );
        assert_eq!(
            target_segment("#foo^", rfc1459, None),
            target_segment("#foo~", rfc1459, None)
        );
        assert_ne!(
            target_segment("#foo^", strict, None),
            target_segment("#foo~", strict, None)
        );
    }

    #[test]
    fn decodes_storage_segments() {
        let cases = [
            ("#%4fmarchy", "#Omarchy"),
            ("#omarchy", "#omarchy"),
            ("#foo%7cbar", "#foo|bar"),
        ];
        for (segment, want) in cases {
            assert_eq!(decode_storage_segment(segment), want);
        }
    }

    #[test]
    fn decodes_h_segments_distinctly_from_hashes() {
        assert_eq!(storage_segment("hEllo", None), "h%45llo");
        assert_eq!(storage_segment("hello", None), "hello");
        assert_eq!(decode_storage_segment("h%45llo"), "hEllo");
        assert_eq!(decode_storage_segment("hello"), "hello");
        assert_eq!(storage_segment("homeUser", None), "home%55ser");
        assert_eq!(storage_segment("homeuser", None), "homeuser");
        assert_eq!(decode_storage_segment("home%55ser"), "homeUser");
        assert_eq!(decode_storage_segment("homeuser"), "homeuser");

        let over_limit = "a".repeat(251);
        let hashed = storage_segment(&over_limit, Some(".json"));
        assert!(storage_segment_is_hash(&hashed));
        assert_eq!(decode_storage_segment(&hashed), hashed);
    }

    #[test]
    fn reserves_hash_shaped_encoded_segments() {
        let hash_like = format!("h{}", "a".repeat(64));
        let encoded = storage_segment(&hash_like, None);
        let want = format!("%68{}", "a".repeat(64));
        assert_eq!(encoded, want);
        assert!(!storage_segment_is_hash(&encoded));
        assert_eq!(decode_storage_segment(&encoded), hash_like);

        let over_limit = "a".repeat(251);
        let hashed = storage_segment(&over_limit, Some(".json"));
        assert!(storage_segment_is_hash(&hashed));
    }

    #[test]
    fn nul_targets_do_not_share_paths() {
        let mapping = IrcCaseMapping::new(IrcCaseMappingKind::Rfc1459);
        let left = "a\0b";
        let right = "a\0\0b";
        assert_ne!(
            target_segment(left, mapping, None),
            target_segment(right, mapping, None)
        );
    }

    #[test]
    fn legacy_storage_segment_cases() {
        let cases = [
            ("#omarchy", "#omarchy"),
            ("a/b", "a%2fb"),
            ("a\\b", "a%5cb"),
            ("%", "%25"),
            ("a%2fb", "a%252fb"),
            ("", "_"),
            (".", "_"),
            ("..", "_"),
        ];
        for (text, want) in cases {
            assert_eq!(legacy_storage_segment(text), want);
        }
    }

    #[test]
    fn decode_legacy_segment_cases() {
        let cases = [
            ("a%2fb", "a/b"),
            ("a%5cb", "a\\b"),
            ("%25", "%"),
            ("a%252fb", "a%2fb"),
            ("a%00b", "a\0b"),
            ("#omarchy", "#omarchy"),
        ];
        for (text, want) in cases {
            assert_eq!(decode_legacy_segment(text), want);
        }
    }
}
