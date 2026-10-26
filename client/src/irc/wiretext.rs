//! Port of `tui/internal/irc/wiretext.go`.
//!
//! Valid UTF-8 passes through; anything else is decoded as Latin-1, one scalar
//! per byte, so no U+FFFD is injected.

/// Converts raw wire bytes to display text.
pub fn wire_text(raw: &[u8]) -> String {
    match std::str::from_utf8(raw) {
        Ok(valid) => valid.to_string(),
        Err(_) => raw.iter().map(|&byte| char::from(byte)).collect(),
    }
}

/// Whether `text` holds an ASCII period. `prefix_nick` uses it to recognize server
/// names.
pub fn contains_dot(text: &str) -> bool {
    text.contains('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    // Ported from tui/internal/irc/parser_test.go (TestDecodesValidUtf8WireText),
    // plus the Latin-1 body of the corpus, verbatim from testdata/corpus/.

    #[test]
    fn empty_and_ascii_bytes_pass_through() {
        assert_eq!(wire_text(&[]), "");
        assert_eq!(wire_text(b"hello"), "hello");
    }

    #[test]
    fn valid_utf8_decodes_as_utf8() {
        assert_eq!(wire_text("\u{e9}".as_bytes()), "\u{e9}");
        assert_eq!(
            wire_text("h\u{e9}llo \u{1F954}".as_bytes()),
            "h\u{e9}llo \u{1F954}"
        );
    }

    #[test]
    fn a_lone_latin1_byte_becomes_its_scalar_without_replacement() {
        let latin1 = wire_text(&[0xe9]);
        assert_eq!(latin1, "\u{e9}");
        assert!(!latin1.contains('\u{FFFD}'));
    }

    #[test]
    fn a_lone_utf8_lead_byte_is_latin1_not_replacement() {
        let lone_lead = wire_text(&[0xc3]);
        assert_eq!(lone_lead, "\u{c3}");
        assert!(!lone_lead.contains('\u{FFFD}'));
    }

    #[test]
    fn a_latin1_word_decodes_byte_for_byte() {
        assert_eq!(wire_text(&[b'c', b'a', b'f', 0xe9]), "caf\u{e9}");
    }

    #[test]
    fn the_corpus_latin1_body_decodes_to_utf8() {
        let line = include_bytes!("testdata/corpus/03-latin1-e9-body.txt");
        assert_eq!(wire_text(line), ":n!u@h PRIVMSG #c :caf\u{e9}\r\n");
    }

    #[test]
    fn dots_are_found_anywhere() {
        assert!(contains_dot("irc.example.net"));
        assert!(!contains_dot("NickServ"));
        assert!(!contains_dot(""));
    }
}
