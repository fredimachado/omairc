//! Port of `tui/internal/irc/secretpolicy.go`
//! (C++).
//!
//! Redaction fails closed. A wire line that would reveal a password, a channel key
//! or a service credential comes back masked. A preview with no mask to apply
//! comes back as `None`. That is not a safety verdict, so each caller decides what
//! to show (see `redact_preview_line`). When a case is unclear, drop the line or
//! keep the masked form; do not add a well-formed bypass for one fixture.
//!
//! The Go port has no tests of its own. The tests below carry the policy-level
//! expectations of `SessionTest::keyedJoinIsRedactedInStatusEntries`,
//! `serviceIdentifyIsRedactedInStatusEntries`, `selfEchoToServiceIsRedacted`,
//! `channelTalkAboutServicesStaysReadable` and the preview checks of
//! `malformedInputSurfacesProtocolError` in `tests/session/tst_session.cpp`.
//! Those tests drive `IrcStatusEntry`, which is a later phase; these drive the
//! policy directly with the same lines.

use super::servicenick::looks_like_channel;

/// A redacted command: the upper-cased verb and the parameters after masking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MaskedCommand {
    pub verb: String,
    pub parameters: Vec<String>,
}

/// How a verb's secret is laid out in its parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SecretShape {
    /// Everything after the first `visible_parameters` is the secret.
    TailAfterParameters,
    /// A mode string names a `k` (key); the parameters it consumes are masked.
    KeyedModeTail,
    /// A PRIVMSG or NOTICE to a person whose body is a service password command.
    ServiceRequestBody,
}

/// One secret-bearing verb. Verbs are upper-case and unique.
struct SecretRule {
    verb: &'static str,
    shape: SecretShape,
    visible_parameters: usize,
}

/// Mirrors `kSecretMap` in `tui/internal/irc/secretpolicy.go`.
const SECRET_MAP: &[SecretRule] = &[
    SecretRule {
        verb: "PASS",
        shape: SecretShape::TailAfterParameters,
        visible_parameters: 0,
    },
    SecretRule {
        verb: "AUTHENTICATE",
        shape: SecretShape::TailAfterParameters,
        visible_parameters: 0,
    },
    SecretRule {
        verb: "OPER",
        shape: SecretShape::TailAfterParameters,
        visible_parameters: 1,
    },
    SecretRule {
        verb: "JOIN",
        shape: SecretShape::TailAfterParameters,
        visible_parameters: 1,
    },
    SecretRule {
        verb: "MODE",
        shape: SecretShape::KeyedModeTail,
        visible_parameters: 2,
    },
    SecretRule {
        verb: "324",
        shape: SecretShape::KeyedModeTail,
        visible_parameters: 3,
    },
    SecretRule {
        verb: "PRIVMSG",
        shape: SecretShape::ServiceRequestBody,
        visible_parameters: 1,
    },
    SecretRule {
        verb: "NOTICE",
        shape: SecretShape::ServiceRequestBody,
        visible_parameters: 1,
    },
];

/// Mirrors `kServicePasswordCommands`.
const SERVICE_PASSWORD_COMMANDS: &[&str] = &[
    "IDENTIFY", "REGISTER", "GHOST", "RECOVER", "RELEASE", "REGAIN", "SETPASS",
];

const fn bytes_equal(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut at = 0;
    while at < left.len() {
        if left[at] != right[at] {
            return false;
        }
        at += 1;
    }
    true
}

/// The rows must be upper-case, unique and match their shape. The C++ core checks
/// this with a `static_assert`; the const assertion below does the same.
const fn secret_map_is_well_formed() -> bool {
    let mut row = 0;
    while row < SECRET_MAP.len() {
        let rule = &SECRET_MAP[row];
        let verb = rule.verb.as_bytes();
        if verb.is_empty() {
            return false;
        }
        let mut at = 0;
        while at < verb.len() {
            if verb[at] >= b'a' && verb[at] <= b'z' {
                return false;
            }
            at += 1;
        }
        match rule.shape {
            SecretShape::ServiceRequestBody => {
                if rule.visible_parameters != 1 {
                    return false;
                }
            }
            SecretShape::KeyedModeTail => {
                if rule.visible_parameters < 2 {
                    return false;
                }
            }
            SecretShape::TailAfterParameters => {}
        }
        let mut earlier = 0;
        while earlier < row {
            if bytes_equal(SECRET_MAP[earlier].verb.as_bytes(), verb) {
                return false;
            }
            earlier += 1;
        }
        row += 1;
    }
    true
}

const _: () = assert!(
    secret_map_is_well_formed(),
    "secret map rows must be unique and well-formed"
);

/// Upper-cases one scalar at a time, like Go's `strings.ToUpper`. A scalar whose
/// uppercase is more than one character (`ß`) stays as it is. Expanding it would
/// let `IDENTIFß` match `IDENTIFY` where Go does not.
fn simple_upper(text: &str) -> String {
    text.chars()
        .map(|mark| {
            let mut upper = mark.to_uppercase();
            match (upper.next(), upper.next()) {
                (Some(single), None) => single,
                _ => mark,
            }
        })
        .collect()
}

/// Keeps ASCII letters and digits, upper-cased. Every other byte is dropped.
fn alnum_only_upper(token: &str) -> String {
    token
        .bytes()
        .filter(u8::is_ascii_alphanumeric)
        .map(|byte| char::from(byte.to_ascii_uppercase()))
        .collect()
}

/// Levenshtein distance over bytes. Callers feed it `alnum_only_upper` output, so
/// bytes and characters agree.
fn letter_distance(left: &str, right: &str) -> usize {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }
    let mut previous: Vec<usize> = (0..=right.len()).collect();
    let mut current = vec![0usize; right.len() + 1];
    for (row, &left_byte) in left.iter().enumerate() {
        current[0] = row + 1;
        for column in 1..=right.len() {
            let cost = usize::from(left_byte != right[column - 1]);
            current[column] = (current[column - 1] + 1)
                .min(previous[column] + 1)
                .min(previous[column - 1] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[right.len()]
}

fn secret_rule_for(verb: &str) -> Option<&'static SecretRule> {
    SECRET_MAP.iter().find(|rule| rule.verb == verb)
}

/// The one service password command within distance 1 of `word`, when that match
/// is unambiguous and the word has at least four letters.
fn nearest_service_password(word: &str) -> Option<&'static str> {
    let alnum = alnum_only_upper(word);
    if alnum.len() < 4 {
        return None;
    }
    let mut best: Option<&'static str> = None;
    let mut best_distance = 2;
    for &command in SERVICE_PASSWORD_COMMANDS {
        let distance = letter_distance(&alnum, command);
        if distance < best_distance {
            best_distance = distance;
            best = Some(command);
        } else if distance == best_distance {
            best = None;
        }
    }
    best.filter(|_| best_distance <= 1)
}

/// The secret verb within distance 1 of `verb`, when that match is unambiguous and
/// the verb has at least three letters.
fn nearest_secret_rule(verb: &str) -> Option<&'static SecretRule> {
    let alnum = alnum_only_upper(verb);
    if alnum.len() < 3 {
        return None;
    }
    let mut best: Option<&'static SecretRule> = None;
    let mut best_distance = 2;
    for rule in SECRET_MAP {
        let distance = letter_distance(&alnum, rule.verb);
        if distance < best_distance {
            best_distance = distance;
            best = Some(rule);
        } else if distance == best_distance {
            best = None;
        }
    }
    best.filter(|_| best_distance <= 1)
}

/// A parsed line: the upper-cased verb and its parameters, the last of which may
/// have been a `:` trailing parameter.
#[derive(Clone, Debug, PartialEq, Eq)]
struct WireCommand {
    verb: String,
    parameters: Vec<String>,
}

fn skip_field_spaces(text: &str) -> &str {
    text.trim_start_matches([' ', '\t'])
}

fn field_break(text: &str) -> Option<usize> {
    text.find([' ', '\t'])
}

/// Drops one leading word (a tag block or a source) and the spaces after it.
/// `None` when nothing follows.
fn drop_leading_word(text: &str) -> Option<&str> {
    let space = field_break(text)?;
    let rest = skip_field_spaces(&text[space + 1..]);
    (!rest.is_empty()).then_some(rest)
}

/// Splits a line the way `secretTokenize` does: skip the tags and the source,
/// upper-case the verb, and split the rest into parameters with the trailing
/// parameter kept whole.
fn tokenize(line: &str) -> Option<WireCommand> {
    let mut rest = skip_field_spaces(line);
    if rest.is_empty() {
        return None;
    }
    if rest.starts_with('@') {
        rest = drop_leading_word(rest)?;
    }
    if rest.starts_with(':') {
        rest = drop_leading_word(rest)?;
    }

    let (verb_text, mut rest) = match field_break(rest) {
        Some(end) => (&rest[..end], skip_field_spaces(&rest[end + 1..])),
        None => (rest, ""),
    };
    let verb = simple_upper(verb_text);
    if verb.is_empty() {
        return None;
    }

    let mut parameters = Vec::new();
    while !rest.is_empty() {
        if let Some(trailing) = rest.strip_prefix(':') {
            parameters.push(trailing.to_string());
            break;
        }
        match field_break(rest) {
            Some(end) => {
                parameters.push(rest[..end].to_string());
                rest = skip_field_spaces(&rest[end + 1..]);
            }
            None => {
                parameters.push(rest.to_string());
                break;
            }
        }
    }
    Some(WireCommand { verb, parameters })
}

/// A trailing parameter needs `:` when it is empty, starts with `:`, or has a space.
fn trailing_separator(trailing: &str) -> &'static str {
    if trailing.is_empty() || trailing.starts_with(':') || trailing.contains(' ') {
        " :"
    } else {
        " "
    }
}

fn format_line(command: &WireCommand) -> String {
    let Some((trailing, head)) = command.parameters.split_last() else {
        return command.verb.clone();
    };
    let mut line = command.verb.clone();
    if !head.is_empty() {
        line.push(' ');
        line.push_str(&head.join(" "));
    }
    line.push_str(trailing_separator(trailing));
    line.push_str(trailing);
    line
}

fn mask_tail_after_parameters(command: &WireCommand, visible: usize) -> Option<WireCommand> {
    if command.parameters.len() <= visible {
        return None;
    }
    let mut parameters = command.parameters[..visible].to_vec();
    parameters.push("***".to_string());
    Some(WireCommand {
        verb: command.verb.clone(),
        parameters,
    })
}

/// Whether a mode letter takes a parameter when set (`adding`) or unset.
fn mode_letter_consumes_parameter(letter: u8, adding: bool) -> bool {
    matches!(letter, b'k' | b'o' | b'v' | b'b') || (letter == b'l' && adding)
}

/// Mode letters the RFC defines without a parameter. Anything else is ambiguous.
fn is_rfc_default_mode_letter(letter: u8, adding: bool) -> bool {
    mode_letter_consumes_parameter(letter, adding)
        || matches!(letter, b'i' | b'm' | b'n' | b'p' | b's' | b't')
}

fn mask_keyed_mode_tail(command: &WireCommand, visible: usize) -> Option<WireCommand> {
    if visible < 2 || command.parameters.len() <= visible {
        return None;
    }
    let modes = command.parameters[visible - 1].as_bytes();
    let mut adding = true;
    let mut has_key = false;
    let mut ambiguous = false;
    for &letter in modes {
        match letter {
            b'+' => adding = true,
            b'-' => adding = false,
            b'k' => has_key = true,
            _ if !is_rfc_default_mode_letter(letter, adding) => ambiguous = true,
            _ => {}
        }
    }
    if !has_key {
        return None;
    }
    // An unknown mode letter may take a parameter of its own, so the positions
    // cannot be trusted. Mask everything after the mode string.
    if ambiguous {
        return mask_tail_after_parameters(command, visible);
    }

    let mut parameters = command.parameters.clone();
    let mut masked_key = false;
    let mut argument = visible;
    adding = true;
    for &letter in modes {
        match letter {
            b'+' => adding = true,
            b'-' => adding = false,
            _ if !mode_letter_consumes_parameter(letter, adding) => {}
            _ => {
                if argument >= parameters.len() {
                    break;
                }
                if letter == b'k' {
                    parameters[argument] = "***".to_string();
                    masked_key = true;
                }
                argument += 1;
            }
        }
    }
    if !masked_key {
        return None;
    }
    Some(WireCommand {
        verb: command.verb.clone(),
        parameters,
    })
}

/// Whether any comma-separated target is a person rather than a channel.
fn has_private_recipient(targets: &str, channel_types: &str) -> bool {
    targets
        .split(',')
        .any(|piece| !looks_like_channel(piece, channel_types))
}

/// The words of a service request body: outer CTCP `\x01` framing removed, and
/// split on spaces and tabs.
fn service_body_tokens(body: &str) -> Vec<&str> {
    let mut normalized = body.trim();
    normalized = normalized.strip_prefix('\u{1}').unwrap_or(normalized);
    normalized = normalized.strip_suffix('\u{1}').unwrap_or(normalized);
    normalized = normalized.trim();
    normalized
        .split([' ', '\t'])
        .filter(|token| !token.is_empty())
        .collect()
}

/// The service password command a body opens with, or `None`. A near miss of a
/// command counts (`identif` is `IDENTIFY`), and `SET` followed by `PASSWORD` is
/// `SET PASSWORD`.
fn matched_service_command(tokens: &[&str]) -> Option<&'static str> {
    if tokens.len() < 2 {
        return None;
    }
    let first = simple_upper(tokens[0]);
    if let Some(command) = SERVICE_PASSWORD_COMMANDS
        .iter()
        .copied()
        .find(|command| *command == first)
    {
        return Some(command);
    }
    if let Some(near) = nearest_service_password(&first) {
        return Some(near);
    }
    if tokens.len() >= 3 && first == "SET" {
        let second = alnum_only_upper(tokens[1]);
        if second == "PASSWORD" || letter_distance(&second, "PASSWORD") == 1 {
            return Some("SET PASSWORD");
        }
    }
    None
}

fn mask_service_request_body(command: &WireCommand, channel_types: &str) -> Option<WireCommand> {
    let (target, body) = match command.parameters.split_first() {
        Some((target, body)) if !body.is_empty() => (target, body),
        _ => return None,
    };
    if !has_private_recipient(target, channel_types) {
        return None;
    }
    let body = body.join(" ");
    let word = matched_service_command(&service_body_tokens(&body))?;
    Some(WireCommand {
        verb: command.verb.clone(),
        parameters: vec![target.clone(), format!("{word} ***")],
    })
}

fn secret_mask(command: &WireCommand, channel_types: &str) -> Option<WireCommand> {
    let rule = secret_rule_for(&command.verb)?;
    match rule.shape {
        SecretShape::TailAfterParameters => {
            mask_tail_after_parameters(command, rule.visible_parameters)
        }
        SecretShape::KeyedModeTail => mask_keyed_mode_tail(command, rule.visible_parameters),
        SecretShape::ServiceRequestBody => mask_service_request_body(command, channel_types),
    }
}

/// The mask for a command whose verb is a near miss of a secret verb (`PSS` for
/// `PASS`), masked as that verb would be.
fn near_miss_mask(parsed: &WireCommand, channel_types: &str) -> Option<String> {
    if parsed.parameters.is_empty() {
        return None;
    }
    let rule = nearest_secret_rule(&parsed.verb)?;
    let canonical = WireCommand {
        verb: rule.verb.to_string(),
        parameters: parsed.parameters.clone(),
    };
    secret_mask(&canonical, channel_types).map(|masked| format_line(&masked))
}

/// The conservative mask: a line that starts with a secret verb, however its
/// words are spelled, comes back as `VERB ***`. `None` when the verb does not lead
/// or nothing follows it.
fn conservative_preview_mask(line: &str) -> Option<String> {
    let alnum = alnum_only_upper(line);
    let mut best: Option<&'static SecretRule> = None;
    let mut best_length = 0;
    for rule in SECRET_MAP {
        if rule.verb.len() >= 3 && alnum.starts_with(rule.verb) && rule.verb.len() > best_length {
            best = Some(rule);
            best_length = rule.verb.len();
        }
    }
    match best {
        Some(rule) if alnum.len() > best_length => Some(format!("{} ***", rule.verb)),
        _ => None,
    }
}

/// The preview mask for one fragment: the fragment as a command, then as a near
/// miss, then the conservative leading-verb mask.
fn preview_mask_at(fragment: &str, channel_types: &str) -> Option<String> {
    if let Some(parsed) = tokenize(fragment) {
        if let Some(masked) = secret_mask(&parsed, channel_types) {
            return Some(format_line(&masked));
        }
        if let Some(near) = near_miss_mask(&parsed, channel_types) {
            return Some(near);
        }
    }
    conservative_preview_mask(fragment)
}

/// A later command inside a fragment: a line that holds a second command after a
/// space, as when a frame was split and joined.
fn later_command_preview_mask(line: &str, channel_types: &str) -> Option<String> {
    let mut rest = skip_field_spaces(line);
    while !rest.is_empty() {
        let Some(break_at) = field_break(rest) else {
            break;
        };
        rest = skip_field_spaces(&rest[break_at + 1..]);
        if rest.is_empty() {
            break;
        }
        if let Some(safe) = preview_mask_at(rest, channel_types) {
            return Some(safe);
        }
    }
    None
}

/// Masks a wire line that carries a secret. `None` when the line is not redacted
/// (it did not tokenize, or it matched no mask), and the caller keeps the line.
/// `channel_types` is the network's CHANTYPES, and the empty string means none.
pub fn redact_wire_line(line: &str, channel_types: &str) -> Option<String> {
    let parsed = tokenize(line)?;
    let masked = secret_mask(&parsed, channel_types)?;
    Some(format_line(&masked))
}

/// The fail-closed preview redactor for text that is not known to be a command:
/// the wire mask, a near-miss verb, a later command in the fragment, then the
/// conservative leading-verb mask.
///
/// `Some` is the masked text to show. `None` means no mask applied. It is not a
/// verdict that the text is safe, and it does not tell the caller to drop it.
/// Each caller decides what `None` leaves on screen:
/// - the protocol-error preview (`session::malformed`'s `preview_wire`) shows the
///   printable text unmasked;
/// - a refusal reason on Status (`statusentry`'s `nick_refusal_tail`) and the
///   server sentence (`session::dispatch::Core::server_sentence_or`) show the
///   unmasked reason only when `allows_transcript` also passes, and omit it
///   otherwise.
///
/// Mirrors `RedactPreviewLine` in `tui/internal/irc/secretpolicy.go` and
/// `IrcSecretPolicy::redactPreviewLine` in `tui/internal/irc/secretpolicy.go`.
pub fn redact_preview_line(line: &str, channel_types: &str) -> Option<String> {
    if let Some(safe) = redact_wire_line(line, channel_types) {
        return Some(safe);
    }
    if let Some(parsed) = tokenize(line) {
        if let Some(near) = near_miss_mask(&parsed, channel_types) {
            return Some(near);
        }
    }
    if let Some(later) = later_command_preview_mask(line, channel_types) {
        return Some(later);
    }
    conservative_preview_mask(line)
}

/// Masks a parsed message's command and parameters. `None` when no mask applies,
/// which leaves the message untouched. A PRIVMSG or NOTICE is masked only when its
/// target is a person; the command is upper-cased before the rule lookup.
pub fn redact_message<C, P>(
    command: C,
    parameters: &[P],
    channel_types: &str,
) -> Option<MaskedCommand>
where
    C: AsRef<str>,
    P: AsRef<str>,
{
    let verb = simple_upper(command.as_ref());
    let rule = secret_rule_for(&verb)?;
    match rule.shape {
        SecretShape::ServiceRequestBody => {
            if parameters.len() < 2 || !has_private_recipient(parameters[0].as_ref(), channel_types)
            {
                return None;
            }
        }
        _ if verb == "JOIN" => return None,
        _ if parameters.len() <= rule.visible_parameters => return None,
        _ => {}
    }
    let view = WireCommand {
        verb,
        parameters: parameters.iter().map(|p| p.as_ref().to_string()).collect(),
    };
    let masked = secret_mask(&view, channel_types)?;
    Some(MaskedCommand {
        verb: masked.verb,
        parameters: masked.parameters,
    })
}

/// Whether `text` is safe to keep in a transcript. It fails closed: a wire command
/// that redaction would mask, or a service request body that `redact_message`
/// would mask, is not allowed.
pub fn allows_transcript(text: &str, channel_types: &str) -> bool {
    if text.is_empty() {
        return true;
    }
    if redact_wire_line(text, channel_types).is_some() {
        return false;
    }
    redact_message("PRIVMSG", &["NickServ", text], channel_types).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The outgoing status line for a wire line, with the CRLF already removed.
    fn outgoing(line: &str) -> Option<String> {
        redact_wire_line(line, "")
    }

    fn outgoing_in(line: &str, channel_types: &str) -> Option<String> {
        redact_wire_line(line, channel_types)
    }

    fn masked(verb: &str, parameters: &[&str]) -> Option<MaskedCommand> {
        redact_message(verb, parameters, "")
    }

    fn masked_parameters(verb: &str, parameters: &[&str]) -> Vec<String> {
        masked(verb, parameters)
            .expect("the message is masked")
            .parameters
    }

    #[test]
    fn the_secret_map_is_well_formed() {
        assert!(secret_map_is_well_formed());
    }

    // Outgoing lines (`IrcStatusEntry::outgoing` calls redactWireLine).

    #[test]
    fn a_keyed_join_masks_its_key_and_an_unkeyed_join_is_untouched() {
        assert_eq!(
            outgoing("JOIN #secret hunter2").as_deref(),
            Some("JOIN #secret ***")
        );
        assert_eq!(outgoing("JOIN #omarchy"), None);
    }

    #[test]
    fn identify_is_masked_in_every_spelling() {
        let cases = [
            (
                "PRIVMSG nickserv :identify my_nick s3cret",
                "PRIVMSG nickserv :IDENTIFY ***",
            ),
            (
                "PRIVMSG NS :IDENTIFY account hunter2",
                "PRIVMSG NS :IDENTIFY ***",
            ),
            (
                "PRIVMSG nickserv :identify\tmy_nick\ts3cret",
                "PRIVMSG nickserv :IDENTIFY ***",
            ),
            (
                "PRIVMSG nickserv IDENTIFY my_nick s3cret",
                "PRIVMSG nickserv :IDENTIFY ***",
            ),
            (
                "PRIVMSG nickserv :\u{1}identify my_nick s3cret\u{1}",
                "PRIVMSG nickserv :IDENTIFY ***",
            ),
            (
                "PRIVMSG nickserv : \u{1}identify my_nick s3cret\u{1} ",
                "PRIVMSG nickserv :IDENTIFY ***",
            ),
            (
                "PRIVMSG nickserv :identif my_nick s3cret",
                "PRIVMSG nickserv :IDENTIFY ***",
            ),
            (
                "PRIVMSG -serv :identify my_nick s3cret",
                "PRIVMSG -serv :IDENTIFY ***",
            ),
            (
                "PRIVMSG [serv :identify my_nick s3cret",
                "PRIVMSG [serv :IDENTIFY ***",
            ),
            (
                "PRIVMSG nickserv,#discuss :identify my_nick s3cret",
                "PRIVMSG nickserv,#discuss :IDENTIFY ***",
            ),
            (
                "PRIVMSG #discuss,nickserv :identify my_nick s3cret",
                "PRIVMSG #discuss,nickserv :IDENTIFY ***",
            ),
            (
                "PRIVMSG NickServ@services :identify my_nick s3cret",
                "PRIVMSG NickServ@services :IDENTIFY ***",
            ),
            (
                "PRIVMSG NickServ!ns@services :identify my_nick s3cret",
                "PRIVMSG NickServ!ns@services :IDENTIFY ***",
            ),
            (
                "PRIVMSG helper!u@services :identify my_nick s3cret",
                "PRIVMSG helper!u@services :IDENTIFY ***",
            ),
            (
                "NOTICE NickServ :ghost old_nick s3cret",
                "NOTICE NickServ :GHOST ***",
            ),
        ];
        for (line, want) in cases {
            assert_eq!(outgoing(line).as_deref(), Some(want), "input {line:?}");
        }
    }

    #[test]
    fn a_service_password_command_with_a_known_word_masks_the_body() {
        assert_eq!(
            outgoing("PRIVMSG nickserv :set password s3cret").as_deref(),
            Some("PRIVMSG nickserv :SET PASSWORD ***")
        );
        assert_eq!(
            outgoing("PRIVMSG nickserv :set email user@example.net"),
            None
        );
    }

    #[test]
    fn a_pass_line_masks_its_tab_separated_secret() {
        assert_eq!(outgoing("PASS\thunter2").as_deref(), Some("PASS ***"));
    }

    #[test]
    fn oper_keeps_its_name_and_masks_the_password() {
        assert_eq!(
            outgoing("OPER admin s3cret").as_deref(),
            Some("OPER admin ***")
        );
    }

    #[test]
    fn mode_keys_are_masked_wherever_the_key_argument_falls() {
        let cases = [
            ("MODE #omarchy +k s3cret", "MODE #omarchy +k ***"),
            (
                "MODE #omarchy +ok alice s3cret",
                "MODE #omarchy +ok alice ***",
            ),
            (
                "MODE #omarchy +ko s3cret alice",
                "MODE #omarchy +ko *** alice",
            ),
            // An unknown letter (f, a) makes the argument positions unknowable,
            // so everything after the mode string is masked.
            ("MODE #omarchy +fk 10 s3cret", "MODE #omarchy +fk ***"),
            ("MODE #omarchy +ak s3cret", "MODE #omarchy +ak ***"),
        ];
        for (line, want) in cases {
            assert_eq!(outgoing(line).as_deref(), Some(want), "input {line:?}");
        }
        assert_eq!(outgoing("MODE #omarchy +o alice"), None);
    }

    #[test]
    fn a_channel_target_is_talk_and_stays_readable() {
        assert_eq!(outgoing("PRIVMSG #serv :identify my_nick s3cret"), None);
        assert_eq!(
            outgoing("PRIVMSG #discuss :I told NickServ to IDENTIFY later"),
            None
        );
        assert_eq!(
            outgoing("PRIVMSG #help@services :IDENTIFY examples stay visible"),
            None
        );
        assert_eq!(outgoing("PRIVMSG ~serv :identify my_nick s3cret"), None);
        assert_eq!(outgoing("PRIVMSG $serv :identify my_nick s3cret"), None);
    }

    #[test]
    fn the_channel_types_decide_whether_a_dollar_target_is_a_person() {
        let line = "PRIVMSG $serv :identify my_nick s3cret";
        assert_eq!(outgoing_in(line, "$"), None);
        assert_eq!(
            outgoing_in(line, "#"),
            Some("PRIVMSG $serv :IDENTIFY ***".to_string())
        );
        assert_eq!(
            outgoing_in("PRIVMSG NS :IDENTIFY account hunter2", "#&").as_deref(),
            Some("PRIVMSG NS :IDENTIFY ***")
        );
    }

    #[test]
    fn a_line_that_does_not_tokenize_is_not_rewritten() {
        assert_eq!(outgoing(""), None);
        assert_eq!(outgoing("   "), None);
        assert_eq!(outgoing("@tagonly"), None);
    }

    // Incoming messages (`IrcStatusEntry::incoming` calls redactMessage).

    #[test]
    fn a_keyed_listed_mode_masks_its_key_argument() {
        assert_eq!(
            masked_parameters("324", &["omairc", "#omarchy", "+k", "s3cret"]),
            ["omairc", "#omarchy", "+k", "***"]
        );
        assert_eq!(
            masked_parameters("324", &["omairc", "#omarchy", "+kl", "s3cret", "40"]),
            ["omairc", "#omarchy", "+kl", "***", "40"]
        );
        assert_eq!(
            masked_parameters("324", &["omairc", "#omarchy", "+lk", "40", "s3cret"]),
            ["omairc", "#omarchy", "+lk", "40", "***"]
        );
        assert_eq!(masked("324", &["omairc", "#omarchy", "+nt"]), None);
    }

    #[test]
    fn an_incoming_pass_is_masked_and_a_literal_star_is_not() {
        assert_eq!(masked_parameters("PASS", &["hunter2"]), ["***"]);
        assert_eq!(masked("MODE", &["***"]), None);
    }

    #[test]
    fn a_self_echo_or_a_service_source_is_masked_by_its_target_alone() {
        // The source never matters: only the command and its target decide.
        assert_eq!(
            masked_parameters("PRIVMSG", &["nickserv", "identify my_nick s3cret"]),
            ["nickserv", "IDENTIFY ***"]
        );
        assert_eq!(
            masked_parameters(
                "PRIVMSG",
                &["nickserv", "\u{1}IDENTIFY my_nick s3cret\u{1}"]
            ),
            ["nickserv", "IDENTIFY ***"]
        );
        assert_eq!(
            masked_parameters("PRIVMSG", &["nickserv", "IDENTIFY", "my_nick", "s3cret"]),
            ["nickserv", "IDENTIFY ***"]
        );
    }

    #[test]
    fn a_message_with_too_few_parameters_or_a_channel_target_is_not_masked() {
        assert_eq!(masked("PRIVMSG", &["nickserv"]), None);
        assert_eq!(
            masked("PRIVMSG", &["#serv", "identify my_nick s3cret"]),
            None
        );
        assert_eq!(masked("PRIVMSG", &["#discuss", "hello there"]), None);
    }

    #[test]
    fn a_non_secret_verb_is_never_masked() {
        assert_eq!(masked("PING", &["token"]), None);
        assert_eq!(masked("JOIN", &["#omarchy", "alice"]), None);
    }

    // Transcript gate and preview redaction.

    #[test]
    fn allows_transcript_keeps_talk_and_drops_credentials() {
        assert!(allows_transcript("", ""));
        assert!(allows_transcript("hello world", ""));
        assert!(allows_transcript(
            "PRIVMSG #discuss :I told NickServ to IDENTIFY later",
            ""
        ));
        assert!(allows_transcript(
            "PRIVMSG #serv :identify my_nick s3cret",
            ""
        ));
        assert!(!allows_transcript(
            "PRIVMSG nickserv :identify my_nick s3cret",
            ""
        ));
        assert!(!allows_transcript("PASS hunter2", ""));
        assert!(!allows_transcript("JOIN #secret hunter2", ""));
    }

    #[test]
    fn previews_keep_the_verb_and_drop_the_secret() {
        let cases = [
            ("PASS hunter2 xxxxxxxxxxxx", "PASS ***"),
            ("JOIN #secret hunter2 xxxxxxxxxxxx", "JOIN #secret ***"),
            (
                "PRIVMSG nickserv :identify my_nick s3cret xxxxxxxx",
                "PRIVMSG nickserv :IDENTIFY ***",
            ),
            // A NUL inside the verb word still names IDENTIFY.
            (
                "PRIVMSG nickserv :identif\u{0} my_nick s3cret",
                "PRIVMSG nickserv :IDENTIFY ***",
            ),
            // A tagged line still masks its body.
            (
                "@bad tag=foo PRIVMSG nickserv :identify my_nick s3cret",
                "PRIVMSG nickserv :IDENTIFY ***",
            ),
            // A verb split by NULs is still a PASS.
            ("P\u{0}ASS\u{0}hunter2", "PASS ***"),
            // A misspelled verb is masked as the verb it nearly is.
            ("PSS hunter2", "PASS ***"),
        ];
        for (line, want) in cases {
            let preview = redact_preview_line(line, "");
            assert_eq!(preview.as_deref(), Some(want), "input {line:?}");
            assert!(!preview.unwrap_or_default().contains("hunter2"));
        }
    }

    #[test]
    fn a_line_with_no_mask_returns_none() {
        // IDENTIFY is a NickServ body, not a wire command, so no mask applies here
        // and the password stays in the text. `None` is not a verdict: the
        // transcript check is what rejects this line.
        assert_eq!(redact_preview_line("IDENTIFY hunter2", ""), None);
        assert!(!allows_transcript("IDENTIFY hunter2", ""));
        assert_eq!(redact_preview_line("hello hunter2", ""), None);
    }

    #[test]
    fn a_later_command_in_a_fragment_is_masked_too() {
        assert_eq!(
            redact_preview_line("garbage PASS hunter2", "").as_deref(),
            Some("PASS ***")
        );
    }
}
