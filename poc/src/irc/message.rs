//! Port of `tui/internal/irc/message.go` (Go) and `src/irc/ircmessage.h` (C++).
//!
//! Frame limits, the error taxonomy and the parsed line. Wire fields are bytes
//! (`Vec<u8>`): the wire carries any octets, and `WireText` decodes them for
//! display. The command and tag names are validated ASCII, so they are `String`.
//! The C++ `IrcResult<T>` becomes `Result<T, IrcError>`, which is why `IrcError`
//! has no `None` variant.

use std::fmt;

use crate::irc::wiretext::wire_text;

/// RFC 1459 line ceiling, CRLF included.
pub const MAX_CLASSIC_FRAME_BYTES: usize = 512;
/// Receive ceiling: servers prepend prefixes and emit long 005 lines.
pub const MAX_INBOUND_CLASSIC_FRAME_BYTES: usize = 4096;
/// IRCv3 message-tags ceiling, excluding the leading '@' and the trailing space.
pub const MAX_TAG_SECTION_BYTES: usize = 8191;
/// Parameters one line may carry. The last one swallows the rest of the line.
pub const MAX_PARAMETERS: usize = 15;

/// Why a line or a frame was rejected. Mirrors `IrcError` (without `None`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IrcError {
    EmptyInput,
    InvalidCharacter,
    InvalidTags,
    InvalidPrefix,
    InvalidCommand,
    TooManyBytes,
    InvalidField,
}

impl IrcError {
    /// The name `ircErrorName` gives in the C++ core.
    pub fn name(self) -> &'static str {
        match self {
            Self::EmptyInput => "empty input",
            Self::InvalidCharacter => "invalid character",
            Self::InvalidTags => "invalid tags",
            Self::InvalidPrefix => "invalid prefix",
            Self::InvalidCommand => "invalid command",
            Self::TooManyBytes => "too many bytes",
            Self::InvalidField => "invalid field",
        }
    }
}

impl fmt::Display for IrcError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

impl std::error::Error for IrcError {}

/// One IRCv3 message tag. `value` is `None` for a valueless tag (`IrcTag`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrcTag {
    pub name: String,
    pub value: Option<Vec<u8>>,
}

/// A parsed message source. `raw` always holds the whole source; the other
/// fields are filled only when present (`IrcPrefix`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrcPrefix {
    pub raw: Vec<u8>,
    pub nick: Vec<u8>,
    pub user: Vec<u8>,
    pub host: Vec<u8>,
}

impl IrcPrefix {
    /// Whether the source names a nick and a user. A bare server name does not.
    pub fn is_user(&self) -> bool {
        !self.nick.is_empty() && !self.user.is_empty()
    }
}

/// A parsed IRC line (`IrcMessage`). `prefix` is `None` when the line carried no
/// source, as on client-to-server lines.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IrcMessage {
    pub tags: Vec<IrcTag>,
    pub prefix: Option<IrcPrefix>,
    pub command: String,
    pub params: Vec<Vec<u8>>,
}

impl IrcMessage {
    /// The parameter at `index`, as raw bytes.
    pub fn param(&self, index: usize) -> Option<&[u8]> {
        self.params.get(index).map(Vec::as_slice)
    }

    /// The first tag named `name`, matched exactly (`+typing` keeps its plus).
    #[cfg(test)]
    pub fn tag(&self, name: &str) -> Option<&IrcTag> {
        self.tags.iter().find(|tag| tag.name == name)
    }

    /// The parameter at `index` as display text, or "" past the end.
    pub fn text_param(&self, index: usize) -> String {
        self.params
            .get(index)
            .map(|raw| wire_text(raw))
            .unwrap_or_default()
    }

    /// The index of the first parameter equal to `value` under ASCII case folding.
    pub fn param_index(&self, value: &str) -> Option<usize> {
        (0..self.params.len()).find(|&index| self.text_param(index).eq_ignore_ascii_case(value))
    }

    /// The value of the first tag named `name` that carries a value, as display
    /// text. `None` when the tag is absent or valueless.
    pub fn tag_text(&self, name: &str) -> Option<String> {
        self.tags
            .iter()
            .filter(|tag| tag.name == name)
            .find_map(|tag| tag.value.as_ref())
            .map(|raw| wire_text(raw))
    }

    /// Whether a tag named `name` is present, with or without a value.
    /// `draft/chathistory-end` carries no value, so `tag_text` would miss it.
    pub fn has_tag(&self, name: &str) -> bool {
        self.tags.iter().any(|tag| tag.name == name)
    }

    /// The `msgid` tag value, or empty.
    pub fn msgid(&self) -> String {
        self.tag_text("msgid").unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_names_match_the_cpp_core() {
        assert_eq!(IrcError::EmptyInput.name(), "empty input");
        assert_eq!(IrcError::InvalidCharacter.name(), "invalid character");
        assert_eq!(IrcError::InvalidTags.name(), "invalid tags");
        assert_eq!(IrcError::InvalidPrefix.name(), "invalid prefix");
        assert_eq!(IrcError::InvalidCommand.name(), "invalid command");
        assert_eq!(IrcError::TooManyBytes.name(), "too many bytes");
        assert_eq!(IrcError::InvalidField.name(), "invalid field");
        assert_eq!(IrcError::TooManyBytes.to_string(), "too many bytes");
    }

    #[test]
    fn tag_lookup_is_exact() {
        let message = IrcMessage {
            tags: vec![IrcTag { name: "+typing".into(), value: Some(b"active".to_vec()) }],
            ..IrcMessage::default()
        };
        assert!(message.tag("+typing").is_some());
        assert!(message.tag("typing").is_none());
        assert_eq!(message.param(0), None);
    }

    #[test]
    fn text_helpers_decode_wire_text_and_default_to_empty() {
        let message = IrcMessage {
            tags: vec![
                IrcTag {
                    name: "msgid".into(),
                    value: Some(b"m1".to_vec()),
                },
                IrcTag {
                    name: "draft/chathistory-end".into(),
                    value: None,
                },
                IrcTag {
                    name: "label".into(),
                    value: Some(vec![0xe9]),
                },
            ],
            params: vec![b"bob".to_vec(), b"LS".to_vec(), vec![0xe9]],
            ..IrcMessage::default()
        };
        assert_eq!(message.text_param(0), "bob");
        assert_eq!(message.text_param(9), "");
        assert_eq!(message.param_index("ls"), Some(1));
        assert_eq!(message.param_index("CAP"), None);
        assert_eq!(message.tag_text("label").as_deref(), Some("\u{e9}"));
        assert_eq!(message.tag_text("draft/chathistory-end"), None);
        assert_eq!(message.tag_text("missing"), None);
        assert!(message.has_tag("draft/chathistory-end"));
        assert!(!message.has_tag("missing"));
        assert_eq!(message.msgid(), "m1");
        assert_eq!(IrcMessage::default().msgid(), "");
    }

    #[test]
    fn a_prefix_is_a_user_only_with_a_nick_and_a_user() {
        let user = IrcPrefix {
            raw: b"bob!b@host".to_vec(),
            nick: b"bob".to_vec(),
            user: b"b".to_vec(),
            host: b"host".to_vec(),
        };
        assert!(user.is_user());
        let server = IrcPrefix {
            raw: b"irc.example.net".to_vec(),
            nick: Vec::new(),
            user: Vec::new(),
            host: Vec::new(),
        };
        assert!(!server.is_user());
        let no_user = IrcPrefix {
            user: Vec::new(),
            ..user
        };
        assert!(!no_user.is_user());
    }
}
