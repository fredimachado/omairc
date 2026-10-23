//! Port of `tui/internal/irc/parser.go` (Go) and `src/irc/ircparser.{h,cpp}` (C++).
//!
//! `parse` turns one line, without its CRLF, into an `IrcMessage`. `serialize`
//! is the inverse, used by the in-process demo peer and the round-trip tests;
//! neither the Go nor the C++ package has one.

use super::message::{
    IrcError, IrcMessage, IrcPrefix, IrcTag, MAX_PARAMETERS, MAX_TAG_SECTION_BYTES,
};

/// Parses one IRC line (without CRLF). Mirrors `IrcParser::parse`.
pub fn parse(line: &[u8]) -> Result<IrcMessage, IrcError> {
    if line.is_empty() {
        return Err(IrcError::EmptyInput);
    }
    if contains_forbidden(line) {
        return Err(IrcError::InvalidCharacter);
    }

    let mut message = IrcMessage::default();
    let mut position = 0;

    if line[0] == b'@' {
        let space = line
            .iter()
            .position(|&byte| byte == b' ')
            .ok_or(IrcError::InvalidTags)?;
        if space == 1 || space - 1 > MAX_TAG_SECTION_BYTES {
            return Err(IrcError::InvalidTags);
        }
        message.tags = parse_tags(&line[1..space]).ok_or(IrcError::InvalidTags)?;
        position = skip_spaces(line, space + 1);
    }

    if line.get(position) == Some(&b':') {
        let space = match line[position..].iter().position(|&byte| byte == b' ') {
            None | Some(1) => return Err(IrcError::InvalidPrefix),
            Some(relative) => position + relative,
        };
        message.prefix = Some(parse_prefix(&line[position + 1..space]).ok_or(IrcError::InvalidPrefix)?);
        position = skip_spaces(line, space + 1);
    }

    if position == line.len() {
        return Err(IrcError::InvalidCommand);
    }

    let relative = line[position..].iter().position(|&byte| byte == b' ');
    let command_end = relative.map_or(line.len(), |offset| position + offset);
    let command = &line[position..command_end];
    if !is_valid_command(command) {
        return Err(IrcError::InvalidCommand);
    }
    message.command = String::from_utf8(command.to_ascii_uppercase()).map_err(|_| IrcError::InvalidCommand)?;

    if let Some(offset) = relative {
        message.params = parse_parameters(&line[position + offset + 1..]);
    }
    Ok(message)
}

/// Builds the line for `message`, without CRLF. The source is written from
/// `prefix.raw`, and every field is checked against the grammar `parse` reads, so
/// `parse(serialize(m))` returns `m` for any message this accepts.
#[cfg(test)]
pub fn serialize(message: &IrcMessage) -> Result<Vec<u8>, IrcError> {
    if message.params.len() > MAX_PARAMETERS {
        return Err(IrcError::InvalidField);
    }
    let mut line = Vec::new();

    if !message.tags.is_empty() {
        let mut section = Vec::new();
        for (index, tag) in message.tags.iter().enumerate() {
            if index > 0 {
                section.push(b';');
            }
            if !is_valid_tag_name(tag.name.as_bytes()) {
                return Err(IrcError::InvalidTags);
            }
            section.extend_from_slice(tag.name.as_bytes());
            if let Some(value) = &tag.value {
                section.push(b'=');
                section.extend_from_slice(&escape_tag_value(value));
            }
        }
        if section.len() > MAX_TAG_SECTION_BYTES {
            return Err(IrcError::InvalidTags);
        }
        line.push(b'@');
        line.extend_from_slice(&section);
        line.push(b' ');
    }

    if let Some(prefix) = &message.prefix {
        // A space would end the source early on re-parse, so it is refused too.
        if prefix.raw.contains(&b' ') || parse_prefix(&prefix.raw).is_none() {
            return Err(IrcError::InvalidPrefix);
        }
        line.push(b':');
        line.extend_from_slice(&prefix.raw);
        line.push(b' ');
    }

    if !is_valid_command(message.command.as_bytes()) {
        return Err(IrcError::InvalidCommand);
    }
    line.extend_from_slice(message.command.as_bytes());

    let count = message.params.len();
    for (index, param) in message.params.iter().enumerate() {
        let is_last = index + 1 == count;
        let needs_colon = param.is_empty() || param[0] == b':' || param.contains(&b' ');
        if needs_colon && !is_last {
            return Err(IrcError::InvalidField);
        }
        line.push(b' ');
        if needs_colon {
            line.push(b':');
        }
        line.extend_from_slice(param);
    }

    if contains_forbidden(&line) {
        return Err(IrcError::InvalidCharacter);
    }
    Ok(line)
}

/// True for CR, LF or NUL anywhere in `bytes`. A line may hold none of them.
pub(crate) fn contains_forbidden(bytes: &[u8]) -> bool {
    bytes.iter().any(|&byte| byte == b'\r' || byte == b'\n' || byte == 0)
}

fn is_valid_command(command: &[u8]) -> bool {
    if command.len() == 3 && command.iter().all(u8::is_ascii_digit) {
        return true;
    }
    !command.is_empty() && command.iter().all(u8::is_ascii_alphabetic)
}

fn skip_spaces(line: &[u8], mut position: usize) -> usize {
    while position < line.len() && line[position] == b' ' {
        position += 1;
    }
    position
}

fn unescape_tag_value(value: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(value.len());
    let mut index = 0;
    while index < value.len() {
        if value[index] != b'\\' {
            result.push(value[index]);
            index += 1;
            continue;
        }
        index += 1;
        let Some(&escaped) = value.get(index) else {
            break;
        };
        result.push(match escaped {
            b':' => b';',
            b's' => b' ',
            b'\\' => b'\\',
            b'r' => b'\r',
            b'n' => b'\n',
            other => other,
        });
        index += 1;
    }
    result
}

#[cfg(test)]
fn escape_tag_value(value: &[u8]) -> Vec<u8> {
    let mut escaped = Vec::with_capacity(value.len());
    for &byte in value {
        match byte {
            b';' => escaped.extend_from_slice(b"\\:"),
            b' ' => escaped.extend_from_slice(b"\\s"),
            b'\\' => escaped.extend_from_slice(b"\\\\"),
            b'\r' => escaped.extend_from_slice(b"\\r"),
            b'\n' => escaped.extend_from_slice(b"\\n"),
            other => escaped.push(other),
        }
    }
    escaped
}

fn is_valid_tag_name(name: &[u8]) -> bool {
    let name = if name.first() == Some(&b'+') { &name[1..] } else { name };
    if name.is_empty() {
        return false;
    }

    let (vendor, key) = match name.iter().position(|&byte| byte == b'/') {
        Some(slash) => (Some(&name[..slash]), &name[slash + 1..]),
        None => (None, name),
    };
    if key.is_empty() || (vendor.is_some() && key.contains(&b'/')) {
        return false;
    }
    if !key.iter().all(|&byte| byte.is_ascii_alphanumeric() || byte == b'-') {
        return false;
    }
    match vendor {
        None => true,
        Some(vendor) => {
            !vendor.is_empty()
                && vendor
                    .iter()
                    .all(|&byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'.')
        }
    }
}

/// Splits the tag section on ';'. `None` when any tag name is invalid.
fn parse_tags(section: &[u8]) -> Option<Vec<IrcTag>> {
    let mut tags = Vec::new();
    for tag in section.split(|&byte| byte == b';') {
        let (name, value) = match tag.iter().position(|&byte| byte == b'=') {
            Some(equals) => (&tag[..equals], Some(unescape_tag_value(&tag[equals + 1..]))),
            None => (tag, None),
        };
        if !is_valid_tag_name(name) {
            return None;
        }
        let name = std::str::from_utf8(name).ok()?.to_owned();
        tags.push(IrcTag { name, value });
    }
    Some(tags)
}

fn parse_prefix(raw: &[u8]) -> Option<IrcPrefix> {
    if raw.is_empty() || contains_forbidden(raw) {
        return None;
    }

    let mut prefix = IrcPrefix {
        raw: raw.to_vec(),
        nick: Vec::new(),
        user: Vec::new(),
        host: Vec::new(),
    };

    let at = raw.iter().position(|&byte| byte == b'@');
    let nick_part = match at {
        Some(at) => {
            if at == 0 || at + 1 == raw.len() || raw[at + 1..].contains(&b'@') {
                return None;
            }
            prefix.host = raw[at + 1..].to_vec();
            &raw[..at]
        }
        None => raw,
    };

    match nick_part.iter().position(|&byte| byte == b'!') {
        Some(bang) => {
            if bang == 0 || bang + 1 == nick_part.len() || nick_part[bang + 1..].contains(&b'!') {
                return None;
            }
            prefix.nick = nick_part[..bang].to_vec();
            prefix.user = nick_part[bang + 1..].to_vec();
        }
        None => {
            if at.is_some() {
                prefix.nick = nick_part.to_vec();
            }
        }
    }
    Some(prefix)
}

fn parse_parameters(input: &[u8]) -> Vec<Vec<u8>> {
    let mut parameters = Vec::new();
    let mut position = 0;

    while position < input.len() {
        position = skip_spaces(input, position);
        if position == input.len() {
            break;
        }

        if parameters.len() == MAX_PARAMETERS - 1 {
            if input[position] == b':' {
                position += 1;
            }
            parameters.push(input[position..].to_vec());
            break;
        }

        if input[position] == b':' {
            parameters.push(input[position + 1..].to_vec());
            break;
        }

        match input[position..].iter().position(|&byte| byte == b' ') {
            None => {
                parameters.push(input[position..].to_vec());
                break;
            }
            Some(relative) => {
                parameters.push(input[position..position + relative].to_vec());
                position += relative + 1;
            }
        }
    }
    parameters
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::prefixnick::prefix_nick;

    fn parse_ok(line: &str) -> IrcMessage {
        parse(line.as_bytes()).unwrap_or_else(|error| panic!("parse({line:?}) failed: {error}"))
    }

    fn bytes(text: &str) -> Vec<u8> {
        text.as_bytes().to_vec()
    }

    #[test]
    fn parses_trailing_parameters() {
        let message = parse_ok(":n!u@h PRIVMSG #c :hello world");
        assert_eq!(message.params, vec![bytes("#c"), bytes("hello world")]);

        let message = parse_ok(":n!u@h PRIVMSG #c :");
        assert_eq!(message.params.len(), 2);
        assert_eq!(message.params[1], Vec::<u8>::new());
    }

    #[test]
    fn preserves_middle_parameters() {
        let message = parse_ok("PRIVMSG #channel hello");
        assert_eq!(message.params, vec![bytes("#channel"), bytes("hello")]);
    }

    #[test]
    fn limits_parameters() {
        let message = parse_ok("TEST 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17");
        assert_eq!(message.params.len(), 15);
        assert_eq!(message.params[13], bytes("14"));
        assert_eq!(message.params[14], bytes("15 16 17"));
    }

    #[test]
    fn rejects_malformed_messages() {
        let malformed = [
            ":nick! PRIVMSG #c :hi",
            ":nick@ PRIVMSG #c :hi",
            ":@host PRIVMSG #c :hi",
            ":nospacePRIVMSG",
            ": ",
            "!!!",
            "22",
            "2222",
            "@tag=x",
            "@=value PRIVMSG #c :hi",
            "PR1VMSG #c :hi",
        ];
        for line in malformed {
            assert!(parse(line.as_bytes()).is_err(), "parse({line:?}) unexpectedly succeeded");
        }
    }

    #[test]
    fn classifies_prefixes() {
        let message = parse_ok(":alice PRIVMSG #room :hello");
        let prefix = message.prefix.expect("server prefix");
        assert_eq!(prefix.raw, bytes("alice"));
        assert_eq!(prefix.nick, Vec::<u8>::new());

        let message = parse_ok(":irc.example.net PRIVMSG #room :hello");
        let prefix = message.prefix.expect("server name prefix");
        assert_eq!(prefix.raw, bytes("irc.example.net"));
        assert_eq!(prefix.nick, Vec::<u8>::new());

        let message = parse_ok(":n!u@h PRIVMSG #room :hello");
        let prefix = message.prefix.expect("user prefix");
        assert_eq!(prefix.nick, bytes("n"));
        assert_eq!(prefix.user, bytes("u"));
        assert_eq!(prefix.host, bytes("h"));
    }

    // Ported from tui/internal/irc/parser_test.go (TestRecoversPrefixNick).
    #[test]
    fn recovers_prefix_nick() {
        let message = parse_ok(":alice PRIVMSG #room :hello");
        assert_eq!(prefix_nick(&message), "alice");

        let message = parse_ok(":irc.example.net PRIVMSG #room :hello");
        assert_eq!(prefix_nick(&message), "", "a server name has no nick");

        let message = parse_ok(":n!u@h PRIVMSG #room :hello");
        assert_eq!(prefix_nick(&message), "n");
    }

    #[test]
    fn parses_tags() {
        let message = parse_ok(
            "@aaa=hello\\sworld;semi=one\\:two;slash=one\\\\two;\
             lines=one\\rtwo\\nthree;flag :n!u@h PRIVMSG #c :tagged",
        );
        assert_eq!(message.tags.len(), 5);
        assert_eq!(message.tags[0].name, "aaa");
        assert_eq!(message.tags[0].value, Some(bytes("hello world")));
        assert_eq!(message.tags[1].value, Some(bytes("one;two")));
        assert_eq!(message.tags[2].value, Some(bytes("one\\two")));
        assert_eq!(message.tags[3].value, Some(bytes("one\rtwo\nthree")));
        assert_eq!(message.tags[4].value, None);
        assert_eq!(message.command, "PRIVMSG");
    }

    #[test]
    fn parses_client_only_typing_tag() {
        let message = parse_ok("@+typing=active :n!u@h TAGMSG #c");
        assert_eq!(message.tags.len(), 1);
        assert_eq!(message.tags[0].name, "+typing");
        assert_eq!(message.tags[0].value, Some(bytes("active")));
        assert_eq!(message.command, "TAGMSG");
    }

    #[test]
    fn preserves_utf8() {
        let payload = "PRIVMSG #c :h\u{e9}llo \u{1F954}";
        let message = parse_ok(payload);
        assert_eq!(message.params.len(), 2);
        let want = &payload.as_bytes()[payload.find(':').expect("trailing colon") + 1..];
        assert_eq!(message.params[1], want);
    }

    #[test]
    fn uppercases_the_command() {
        assert_eq!(parse_ok("privmsg #c :hi").command, "PRIVMSG");
    }

    #[test]
    fn serializes_and_reparses_to_the_same_message() {
        let lines = [
            "PRIVMSG #c :hello world",
            "@msgid=abc;+typing=active :n!u@h PRIVMSG #c :tagged",
            "@aaa=hello\\sworld;semi=one\\:two;slash=one\\\\two;flag :n!u@h PRIVMSG #c :x",
            ":irc.example.net 001 fred :Welcome",
            "PRIVMSG #c :",
            "PRIVMSG #c :::leading colon",
            "TEST 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17",
            "PING :token",
        ];
        for line in lines {
            let message = parse_ok(line);
            let wire = serialize(&message).unwrap_or_else(|error| panic!("serialize({line:?}): {error}"));
            assert_eq!(parse(&wire), Ok(message), "round trip of {line:?}");
        }
    }

    #[test]
    fn serialize_writes_the_compact_forms() {
        let message = IrcMessage {
            command: "PRIVMSG".into(),
            params: vec![bytes("#c"), bytes("hello world")],
            ..IrcMessage::default()
        };
        assert_eq!(serialize(&message).as_deref(), Ok(&b"PRIVMSG #c :hello world"[..]));

        let plain = IrcMessage {
            command: "PING".into(),
            params: vec![bytes("token")],
            ..IrcMessage::default()
        };
        assert_eq!(serialize(&plain).as_deref(), Ok(&b"PING token"[..]));
    }

    #[test]
    fn serialize_rejects_what_parse_cannot_read_back() {
        let command = |text: &str| IrcMessage {
            command: text.into(),
            ..IrcMessage::default()
        };
        assert_eq!(serialize(&command("")), Err(IrcError::InvalidCommand));
        assert_eq!(serialize(&command("PR1VMSG")), Err(IrcError::InvalidCommand));

        let spaced_middle = IrcMessage {
            command: "PRIVMSG".into(),
            params: vec![bytes("a b"), bytes("c")],
            ..IrcMessage::default()
        };
        assert_eq!(serialize(&spaced_middle), Err(IrcError::InvalidField));

        let empty_middle = IrcMessage {
            command: "PRIVMSG".into(),
            params: vec![Vec::new(), bytes("c")],
            ..IrcMessage::default()
        };
        assert_eq!(serialize(&empty_middle), Err(IrcError::InvalidField));

        let too_many = IrcMessage {
            command: "TEST".into(),
            params: vec![bytes("1"); MAX_PARAMETERS + 1],
            ..IrcMessage::default()
        };
        assert_eq!(serialize(&too_many), Err(IrcError::InvalidField));

        let injected = IrcMessage {
            command: "PRIVMSG".into(),
            params: vec![bytes("#c"), bytes("hi\r\nQUIT")],
            ..IrcMessage::default()
        };
        assert_eq!(serialize(&injected), Err(IrcError::InvalidCharacter));

        let bad_tag = IrcMessage {
            tags: vec![IrcTag { name: "bad tag".into(), value: None }],
            command: "PING".into(),
            ..IrcMessage::default()
        };
        assert_eq!(serialize(&bad_tag), Err(IrcError::InvalidTags));

        let prefix_with = |raw: &str| IrcMessage {
            prefix: Some(IrcPrefix {
                raw: bytes(raw),
                nick: Vec::new(),
                user: Vec::new(),
                host: Vec::new(),
            }),
            command: "PING".into(),
            ..IrcMessage::default()
        };
        assert_eq!(serialize(&prefix_with("nick!")), Err(IrcError::InvalidPrefix));
        assert_eq!(serialize(&prefix_with("nick extra")), Err(IrcError::InvalidPrefix));
    }
}
