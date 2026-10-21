//! Port of `tui/internal/irc/prefixnick.go` (Go) and `src/irc/ircprefixnick.{h,cpp}` (C++).
//!
//! The source nick of a message. A server name (a dotted host with no
//! `!user@host`) is not a nick, so it yields nothing.

use super::message::IrcMessage;
use super::wiretext::{contains_dot, wire_text};

/// The nick that sent `message`, or empty when the message has no source or the
/// source is a server name. Parsed `nick` wins; otherwise the bare source is the
/// nick, unless it contains a dot.
#[allow(dead_code)] // The reducer (Phase 3) is the first consumer.
pub fn prefix_nick(message: &IrcMessage) -> String {
    let Some(prefix) = message.prefix.as_ref() else {
        return String::new();
    };
    if !prefix.nick.is_empty() {
        return wire_text(&prefix.nick);
    }
    let raw = wire_text(&prefix.raw);
    if contains_dot(&raw) {
        return String::new();
    }
    raw
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::parser::parse;

    fn message(line: &str) -> IrcMessage {
        parse(line.as_bytes()).unwrap_or_else(|error| panic!("parse({line:?}) failed: {error}"))
    }

    #[test]
    fn a_message_without_a_source_has_no_nick() {
        assert_eq!(prefix_nick(&message("PING :token")), "");
    }

    #[test]
    fn a_bare_source_without_a_dot_is_the_nick() {
        assert_eq!(
            prefix_nick(&message(":alice PRIVMSG #room :hello")),
            "alice"
        );
    }

    #[test]
    fn a_dotted_bare_source_is_a_server_name() {
        assert_eq!(
            prefix_nick(&message(":irc.example.net PRIVMSG #room :hello")),
            ""
        );
    }

    #[test]
    fn a_full_source_yields_its_nick_even_with_a_dot_in_the_host() {
        assert_eq!(prefix_nick(&message(":n!u@h PRIVMSG #room :hello")), "n");
        assert_eq!(
            prefix_nick(&message(":n!u@irc.example.net PRIVMSG #room :hello")),
            "n"
        );
    }

    #[test]
    fn a_nick_with_non_ascii_bytes_decodes_as_wire_text() {
        // Latin-1 0xE9 in the nick, not valid UTF-8, decodes byte for byte.
        let line: &[u8] = b":caf\xe9!u@h PRIVMSG #room :hello";
        let parsed = parse(line).expect("parse");
        assert_eq!(prefix_nick(&parsed), "caf\u{e9}");
    }
}
