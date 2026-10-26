//! Builds the outbound lines the client writes, and the byte budgets the composer
//! uses so one send stays inside one frame. Every frame size is the network's
//! LINELEN (`IrcServerFeatures::line_length`), passed in as `features`. Nothing
//! here assumes 512. Before ISUPPORT arrives, pass `IrcServerFeatures::new`
//! whose LINELEN is the classic 512.
//! Frame sizes count CRLF and never count IRCv3 tags, as in Go and C++. The line
//! builders return the whole line with its CRLF.
//! No redaction happens here, as in Go. A builder error carries only its kind
//! never the rejected text, so a password cannot leak through an error. The
//! Status recorder masks the built line later with `secretpolicy`.

use super::message::IrcError;
use super::parser::contains_forbidden;
use super::serverfeatures::IrcServerFeatures;

/// `\r\n`, which the frame counts and the composer does not hold.
const CRLF_BYTES: usize = 2;
/// `\x01ACTION ` (8 bytes) plus the closing `\x01`.
const ACTION_WRAPPER_BYTES: usize = 9;
/// The `/me ` that the composer holds in front of an action body.
const ME_PREFIX_BYTES: usize = 4;

/// Appends CRLF to `command`, rejecting an empty line, embedded CR, LF or NUL, and
/// any line past the frame. The frame is `features.line_length`, CRLF included.
pub fn line(command: &str, features: &IrcServerFeatures) -> Result<String, IrcError> {
    let frame = features.line_length();
    if command.is_empty() {
        return Err(IrcError::EmptyInput);
    }
    if contains_forbidden(command.as_bytes()) {
        return Err(IrcError::InvalidCharacter);
    }
    if frame < CRLF_BYTES || command.len() + CRLF_BYTES > frame {
        return Err(IrcError::TooManyBytes);
    }
    Ok(format!("{command}\r\n"))
}

/// The bytes the composer may hold so one send stays inside the frame. An empty
/// target is Status: a raw line, excluding CRLF. Any other target is the PRIVMSG
/// trailing body that fits, `PRIVMSG <target>:<body>\r\n`. Tags are not
/// subtracted.
pub fn composer_byte_budget(target: &str, features: &IrcServerFeatures) -> usize {
    let frame = features.line_length();
    if frame < CRLF_BYTES {
        return 0;
    }
    if target.is_empty() {
        return frame - CRLF_BYTES;
    }
    // Same residual as `split_trailing_param` for the prefix and an empty suffix.
    let prefix_len = "PRIVMSG ".len() + target.len() + " :".len();
    if prefix_len + 3 > frame {
        return 0;
    }
    frame - prefix_len - CRLF_BYTES
}

/// The composer cap for a `/me` draft: a CTCP ACTION is 9 bytes beyond a PRIVMSG
/// and the 4-byte `/me ` counts toward the draft.
pub fn action_composer_byte_budget(target: &str, features: &IrcServerFeatures) -> usize {
    let privmsg = composer_byte_budget(target, features);
    if privmsg < ACTION_WRAPPER_BYTES {
        return 0;
    }
    privmsg - ACTION_WRAPPER_BYTES + ME_PREFIX_BYTES
}

/// The composer cap for a draft: the action cap when the draft is `/me` plus a
/// body on a conversation, otherwise the plain cap. Status stays a raw line.
pub fn composer_byte_budget_for_draft(
    target: &str,
    draft: &str,
    features: &IrcServerFeatures,
) -> usize {
    if target.is_empty() || !draft_is_action(draft) {
        return composer_byte_budget(target, features);
    }
    action_composer_byte_budget(target, features)
}

/// Whether `draft` starts, after leading spaces or tabs, with `/me` followed by a
/// space. The command letters match in either case.
fn draft_is_action(draft: &str) -> bool {
    let trimmed = draft.trim_start_matches([' ', '\t']);
    if !trimmed.starts_with('/') {
        return false;
    }
    let Some(space) = trimmed.find(' ') else {
        return false;
    };
    let token = &trimmed.as_bytes()[..space];
    token.len() == 3
        && (token[1] == b'm' || token[1] == b'M')
        && (token[2] == b'e' || token[2] == b'E')
}

/// The longest prefix of `text` whose UTF-8 encoding fits in `max_bytes`. A scalar
/// is kept whole or dropped, so the result is always valid UTF-8.
pub fn clamp_utf8_prefix(text: &str, max_bytes: usize) -> &str {
    let mut end = 0;
    for (index, character) in text.char_indices() {
        let next = index + character.len_utf8();
        if next > max_bytes {
            break;
        }
        end = next;
    }
    &text[..end]
}

/// Splits `body` into chunks that each fit one frame with `prefix` in front and
/// `suffix` behind. It prefers a space boundary and never splits a UTF-8
/// sequence. An empty result means the input cannot be sent: an empty body, a
/// forbidden byte, a frame too small for the wrapper, or a token that cannot be
/// split at a scalar boundary.
pub fn split_trailing_param<'body>(
    prefix: &str,
    body: &'body str,
    suffix: &str,
    features: &IrcServerFeatures,
) -> Vec<&'body str> {
    let frame = features.line_length();
    if body.is_empty()
        || contains_forbidden(prefix.as_bytes())
        || contains_forbidden(body.as_bytes())
        || contains_forbidden(suffix.as_bytes())
    {
        return Vec::new();
    }
    if frame < 3 || prefix.len() + suffix.len() + 3 > frame {
        return Vec::new();
    }
    let max_body = frame - prefix.len() - suffix.len() - CRLF_BYTES;

    let bytes = body.as_bytes();
    let mut chunks = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let remaining = bytes.len() - offset;
        if remaining <= max_body {
            chunks.push(&body[offset..]);
            break;
        }

        let window = &bytes[offset..offset + max_body];
        if let Some(last_space) = window.iter().rposition(|&byte| byte == b' ') {
            if last_space > 0 {
                chunks.push(&body[offset..offset + last_space]);
                offset += last_space + 1;
                continue;
            }
        }

        // Hard split. Back up while the cut would land on a continuation byte, so
        // the lead byte travels with its tail.
        let mut take = max_body;
        while take > 0 && bytes[offset + take] & 0xC0 == 0x80 {
            take -= 1;
        }
        if take == 0 {
            return Vec::new();
        }
        chunks.push(&body[offset..offset + take]);
        offset += take;
    }
    chunks
}

pub fn nick(nickname: &str, features: &IrcServerFeatures) -> Result<String, IrcError> {
    if !is_single_field(nickname) {
        return Err(IrcError::InvalidField);
    }
    line(&format!("NICK {nickname}"), features)
}

/// Builds the USER line with the fixed `<user> 8 *:<realname>` shape.
pub fn user(
    username: &str,
    realname: &str,
    features: &IrcServerFeatures,
) -> Result<String, IrcError> {
    if !is_single_field(username) || realname.is_empty() || contains_forbidden(realname.as_bytes())
    {
        return Err(IrcError::InvalidField);
    }
    line(&format!("USER {username} 8 * :{realname}"), features)
}

pub fn pass(password: &str, features: &IrcServerFeatures) -> Result<String, IrcError> {
    if !is_single_field(password) {
        return Err(IrcError::InvalidField);
    }
    line(&format!("PASS {password}"), features)
}

/// Builds a JOIN line. An empty key omits the key parameter.
pub fn join(channel: &str, key: &str, features: &IrcServerFeatures) -> Result<String, IrcError> {
    if !is_join_field(channel) {
        return Err(IrcError::InvalidField);
    }
    if key.is_empty() {
        return line(&format!("JOIN {channel}"), features);
    }
    if !is_join_field(key) {
        return Err(IrcError::InvalidField);
    }
    line(&format!("JOIN {channel} {key}"), features)
}

/// The registration burst: an optional PASS, then NICK and USER. An empty password
/// sends no PASS.
#[cfg(test)]
pub fn registration(
    nickname: &str,
    username: &str,
    realname: &str,
    password: &str,
    features: &IrcServerFeatures,
) -> Result<String, IrcError> {
    let nick_command = nick(nickname, features)?;
    let user_command = user(username, realname, features)?;

    let mut result = String::new();
    if !password.is_empty() {
        result.push_str(&pass(password, features)?);
    }
    result.push_str(&nick_command);
    result.push_str(&user_command);
    Ok(result)
}

/// A single IRC parameter: non-empty, with no CR, LF, NUL or ASCII space.
fn is_single_field(field: &str) -> bool {
    !field.is_empty() && !contains_forbidden(field.as_bytes()) && !field.contains(' ')
}

/// A JOIN channel or key: non-empty, no leading `:`, no Unicode whitespace, `,`
/// or NUL.
fn is_join_field(field: &str) -> bool {
    !field.is_empty()
        && !field.starts_with(':')
        && !field
            .chars()
            .any(|character| character.is_whitespace() || character == ',' || character == '\0')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::message::MAX_CLASSIC_FRAME_BYTES;

    // tui/internal/irc/commandbuilder_test.go. The Go tests pass a
    // frame size in each call; here each test builds the features that carry that
    // LINELEN, so the same path is exercised as in a live session.

    fn classic() -> IrcServerFeatures {
        IrcServerFeatures::new()
    }

    fn with_linelen(length: usize) -> IrcServerFeatures {
        let mut features = IrcServerFeatures::new();
        features.apply_token(&format!("LINELEN={length}"));
        features
    }

    #[test]
    fn builds_registration() {
        let features = classic();
        let result =
            registration("alice", "aliceuser", "Omairc User", "", &features).expect("registration");
        assert_eq!(result, "NICK alice\r\nUSER aliceuser 8 * :Omairc User\r\n");
        assert!(
            !result.contains("HELLO"),
            "registration leaked a default realname: {result:?}"
        );

        let result = registration("alice", "aliceuser", "Omairc User", "s3cret", &features)
            .expect("registration with password");
        assert_eq!(
            result,
            "PASS s3cret\r\nNICK alice\r\nUSER aliceuser 8 * :Omairc User\r\n"
        );
    }

    #[test]
    fn rejects_invalid_registration() {
        let features = classic();
        let cases = [
            ("", "user", "real", ""),
            ("nick", "", "real", ""),
            ("ni ck", "user", "real", ""),
            ("nick", "us er", "real", ""),
            ("nick", "user", "real", "pass word"),
            ("nick\nnick", "user", "real", ""),
            ("nick", "user", "real", "p\0ass"),
        ];
        for (nickname, username, realname, password) in cases {
            assert!(
                registration(nickname, username, realname, password, &features).is_err(),
                "registration({nickname:?}, {username:?}, {realname:?}, {password:?}) unexpectedly succeeded"
            );
        }
    }

    #[test]
    fn rejects_outbound_injection() {
        let features = classic();
        for command in [
            "PRIVMSG #c :hi\r\nQUIT",
            "PRIVMSG #c :hi\nQUIT",
            "PRIVMSG #c :hi\rQUIT",
            "PRIVMSG #c :hi\0there",
        ] {
            assert!(
                line(command, &features).is_err(),
                "line({command:?}) unexpectedly succeeded"
            );
        }

        assert_eq!(
            line("JOIN #chan", &features).expect("JOIN"),
            "JOIN #chan\r\n"
        );
    }

    #[test]
    fn line_reports_the_kind_of_rejection() {
        assert_eq!(line("", &classic()), Err(IrcError::EmptyInput));
        assert_eq!(line("NICK\rx", &classic()), Err(IrcError::InvalidCharacter));
    }

    #[test]
    fn enforces_outbound_boundary() {
        let features = classic();
        let result = line(&"A".repeat(510), &features).expect("510 bytes");
        assert_eq!(result.len(), MAX_CLASSIC_FRAME_BYTES);

        assert_eq!(
            line(&"A".repeat(511), &features),
            Err(IrcError::TooManyBytes)
        );
    }

    #[test]
    fn splits_outbound_chat_on_word_boundary() {
        let features = classic();
        let prefix = "PRIVMSG #omarchy :";
        let first = "a".repeat(400);
        let second = "b".repeat(200);

        let body = format!("{first} {second}");
        let chunks = split_trailing_param(prefix, &body, "", &features);
        assert_eq!(chunks, [first.as_str(), second.as_str()]);

        let first_line = format!("{prefix}{}", chunks[0]);
        let second_line = format!("{prefix}{}", chunks[1]);
        assert_eq!(first_line, format!("PRIVMSG #omarchy :{}", "a".repeat(400)));
        assert_eq!(
            second_line,
            format!("PRIVMSG #omarchy :{}", "b".repeat(200))
        );
        assert!(first_line.len() + 2 <= MAX_CLASSIC_FRAME_BYTES);
        assert!(second_line.len() + 2 <= MAX_CLASSIC_FRAME_BYTES);
        assert!(line(&first_line, &features).is_ok());
        assert!(line(&second_line, &features).is_ok());
    }

    #[test]
    fn splits_outbound_chat_hard_when_token_exceeds_frame() {
        let features = classic();
        let prefix = "PRIVMSG #omarchy :";
        let max_body = MAX_CLASSIC_FRAME_BYTES - prefix.len() - 2;
        let token = "x".repeat(max_body * 2 + 16);

        let chunks = split_trailing_param(prefix, &token, "", &features);
        assert_eq!(chunks.len(), 3, "chunks = {chunks:?}");
        assert_eq!(chunks[0], &token[..max_body]);
        assert_eq!(chunks[1], &token[max_body..max_body * 2]);
        assert_eq!(chunks[2], &token[max_body * 2..]);

        for chunk in &chunks {
            let built = line(&format!("{prefix}{chunk}"), &features).expect("chunk line");
            assert_eq!(built.len(), prefix.len() + chunk.len() + 2);
        }

        // A two-byte scalar straddles the cut, so the lead byte moves with it.
        let eacute = "\u{e9}";
        let torn = format!("{}{eacute}{}", "x".repeat(max_body - 1), "y".repeat(10));
        let torn_chunks = split_trailing_param(prefix, &torn, "", &features);
        assert_eq!(torn_chunks.len(), 2, "torn chunks = {torn_chunks:?}");
        assert_eq!(torn_chunks[0], "x".repeat(max_body - 1));
        assert_eq!(torn_chunks[1], format!("{eacute}{}", "y".repeat(10)));
    }

    #[test]
    fn split_refuses_inputs_it_cannot_send() {
        let features = classic();
        assert!(split_trailing_param("PRIVMSG #c :", "", "", &features).is_empty());
        assert!(split_trailing_param("PRIVMSG #c :", "a\0b", "", &features).is_empty());
        assert!(split_trailing_param("PRIVMSG #c :", "a", "\r", &features).is_empty());
        let huge_prefix = "P".repeat(MAX_CLASSIC_FRAME_BYTES);
        assert!(split_trailing_param(&huge_prefix, "a", "", &features).is_empty());
    }

    #[test]
    fn builds_join() {
        let features = classic();
        assert_eq!(join("#a", "", &features).expect("#a"), "JOIN #a\r\n");
        assert_eq!(
            join("#a", "pword", &features).expect("keyed"),
            "JOIN #a pword\r\n"
        );
        assert_eq!(
            join("&local", "secret", &features).expect("&local"),
            "JOIN &local secret\r\n"
        );
    }

    #[test]
    fn rejects_invalid_join() {
        let features = classic();
        for channel in ["", "#a b", "#a,b"] {
            assert!(
                join(channel, "", &features).is_err(),
                "join({channel:?}) unexpectedly succeeded"
            );
        }
        for key in ["p word", "x,y"] {
            assert!(
                join("#a", key, &features).is_err(),
                "join(#a, {key:?}) unexpectedly succeeded"
            );
        }
        assert!(join("#a\r", "", &features).is_err(), "join with CR");
        assert!(join("#a\n", "", &features).is_err(), "join with LF");
        assert!(join("#a\0", "", &features).is_err(), "join with NUL");
        assert!(
            join("#a", "k\0ey", &features).is_err(),
            "join with a NUL key"
        );
        assert!(join(":chan", "", &features).is_err(), "join(:chan)");
        assert!(join("#ok", ":key", &features).is_err(), "join(#ok, :key)");
        assert!(
            join(&"A".repeat(508), "", &features).is_err(),
            "overlong join"
        );
    }

    #[test]
    fn join_honours_the_negotiated_linelen() {
        // The same 508-byte channel is too long for the classic frame but fits a
        // network that advertised a larger LINELEN. A fixed 512 would reject it.
        let channel = format!("#{}", "a".repeat(507));
        assert!(join(&channel, "", &classic()).is_err());
        assert_eq!(
            join(&channel, "", &with_linelen(2048)).expect("long channel"),
            format!("JOIN {channel}\r\n")
        );
    }

    #[test]
    fn composer_budget_matches_one_frame() {
        let features = classic();
        assert_eq!(composer_byte_budget("", &features), 510);
        assert_eq!(composer_byte_budget("#place", &features), 494);
        assert_eq!(composer_byte_budget("#omarchy", &features), 492);
        assert_eq!(composer_byte_budget(&"x".repeat(500), &features), 0);

        let target = "#place";
        let budget = composer_byte_budget(target, &features);
        let prefix = format!("PRIVMSG {target} :");
        let body = "a".repeat(budget);
        assert_eq!(
            split_trailing_param(&prefix, &body, "", &features).len(),
            1,
            "one frame"
        );
        assert_eq!(prefix.len() + body.len() + 2, MAX_CLASSIC_FRAME_BYTES);
        assert!(
            split_trailing_param(&prefix, &format!("{body}b"), "", &features).len() >= 2,
            "one extra byte must split"
        );
    }

    #[test]
    fn composer_budget_reads_linelen() {
        let small = with_linelen(300);
        assert_eq!(composer_byte_budget("", &small), 298);
        assert_eq!(composer_byte_budget("#omarchy", &small), 280);
        let prefix = "PRIVMSG #omarchy :";
        assert!(
            split_trailing_param(prefix, &"a".repeat(281), "", &small).len() >= 2,
            "281 bytes at LINELEN 300 must split"
        );
        assert_eq!(
            line(&"A".repeat(510), &small),
            Err(IrcError::TooManyBytes),
            "510 status bytes at LINELEN 300"
        );

        let large = with_linelen(2048);
        assert_eq!(composer_byte_budget("", &large), 2046);
        assert_eq!(composer_byte_budget("#omarchy", &large), 2028);
        assert!(
            line(&"A".repeat(2000), &large).is_ok(),
            "2000 status bytes fit LINELEN 2048"
        );
    }

    #[test]
    fn clamp_utf8_prefix_keeps_whole_scalars() {
        assert_eq!(clamp_utf8_prefix("abcd", 3), "abc");
        let acute = "\u{e9}";
        assert_eq!(
            clamp_utf8_prefix(&format!("a{acute}"), 2),
            "a",
            "2-byte drop"
        );
        assert_eq!(
            clamp_utf8_prefix(&format!("a{acute}"), 3),
            format!("a{acute}"),
            "2-byte keep"
        );
        let euro = "\u{20ac}";
        assert_eq!(
            clamp_utf8_prefix(&format!("a{euro}"), 3),
            "a",
            "3-byte drop"
        );
        assert_eq!(
            clamp_utf8_prefix(&format!("a{euro}"), 4),
            format!("a{euro}"),
            "3-byte keep"
        );
        let grin = "\u{1F600}";
        assert_eq!(
            clamp_utf8_prefix(&format!("a{grin}"), 4),
            "a",
            "4-byte drop"
        );
        assert_eq!(
            clamp_utf8_prefix(&format!("a{grin}"), 5),
            format!("a{grin}"),
            "4-byte keep"
        );
        assert_eq!(clamp_utf8_prefix("abc", 0), "", "zero budget");
    }

    #[test]
    fn filled_action_fits_one_frame() {
        let features = classic();
        let target = "#omarchy";
        let budget = action_composer_byte_budget(target, &features);
        assert_eq!(budget, 487);

        let body = "a".repeat(budget - 4);
        let prefix = format!("PRIVMSG {target} :\u{1}ACTION ");
        assert_eq!(
            split_trailing_param(&prefix, &body, "\u{1}", &features).len(),
            1,
            "filled /me is one chunk"
        );
        assert_eq!(prefix.len() + body.len() + 1 + 2, MAX_CLASSIC_FRAME_BYTES);
        assert!(
            split_trailing_param(&prefix, &format!("{body}b"), "\u{1}", &features).len() >= 2,
            "one byte past a filled /me must split"
        );
    }

    #[test]
    fn draft_budget_selects_the_action_cap_only_for_me_with_a_body() {
        let features = classic();
        let plain = composer_byte_budget("#omarchy", &features);
        let action = action_composer_byte_budget("#omarchy", &features);

        assert_eq!(
            composer_byte_budget_for_draft("#omarchy", "/me waves", &features),
            action
        );
        assert_eq!(
            composer_byte_budget_for_draft("#omarchy", "  \t/ME waves", &features),
            action
        );
        assert_eq!(
            composer_byte_budget_for_draft("#omarchy", "/mex waves", &features),
            plain
        );
        assert_eq!(
            composer_byte_budget_for_draft("#omarchy", "/me", &features),
            plain,
            "no space"
        );
        assert_eq!(
            composer_byte_budget_for_draft("#omarchy", "hi /me", &features),
            plain
        );
        assert_eq!(
            composer_byte_budget_for_draft("", "/me waves", &features),
            composer_byte_budget("", &features)
        );
    }

    #[test]
    fn nick_and_user_reject_unsafe_fields() {
        let features = classic();
        assert_eq!(nick("alice", &features).expect("nick"), "NICK alice\r\n");
        assert_eq!(nick("", &features), Err(IrcError::InvalidField));
        assert_eq!(nick("a b", &features), Err(IrcError::InvalidField));
        assert_eq!(user("u", "", &features), Err(IrcError::InvalidField));
        assert_eq!(
            user("u", "real\nname", &features),
            Err(IrcError::InvalidField)
        );
        assert_eq!(pass("", &features), Err(IrcError::InvalidField));
    }
}
