//! What the server sent: the registration burst, CAP and lusers numerics, nick
//! refusals and the default line, and the WHOIS and notice shapes routed to their own
//! files. `incoming_all` is the one entry point a caller reads.

use super::notice::{parse_incoming_notice, present_invite, present_notice};
use super::whois::format_whois;
use super::{
    CtcpReplyLine, LogSeverity, LogSource, StatusEntry, WhoisLine, CTCP_LABEL, NOTICE_LABEL,
};
use crate::irc::ctcp::{format_ctcp_reply_text, parse_ctcp_request};
use crate::irc::message::IrcMessage;
use crate::irc::secretpolicy::{allows_transcript, redact_message, redact_preview_line};
use crate::irc::wiretext::wire_text;
use chrono::{DateTime, Utc};

/// The command of a message, upper-cased (`statusCommandOf`).
pub(super) fn command_of(message: &IrcMessage) -> String {
    message.command.to_uppercase()
}

/// Every parameter as display text (`statusMessageParameters`).
pub(super) fn parameters_text(message: &IrcMessage) -> Vec<String> {
    message.params.iter().map(|p| wire_text(p)).collect()
}

/// The last parameter as display text, or empty.
pub(super) fn trailing_text(message: &IrcMessage) -> String {
    message
        .params
        .last()
        .map_or_else(String::new, |p| wire_text(p))
}

/// A parameter by index, or empty when out of range (`statusParameterAt`).
pub(super) fn parameter_at(parameters: &[String], index: usize) -> &str {
    parameters.get(index).map_or("", String::as_str)
}

/// Splits on spaces and drops empty fields (`statusSplitFields`, Qt
/// `SkipEmptyParts`).
pub(super) fn split_fields(text: &str) -> Vec<String> {
    text.split(' ')
        .filter(|field| !field.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Whether `text` starts with `prefix`, ignoring ASCII case. It compares bytes, so
/// it never splits a character (`hasPrefixFold`).
pub(super) fn has_prefix_fold(text: &str, prefix: &str) -> bool {
    text.as_bytes()
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix.as_bytes()))
}

/// The first token of a line, upper-cased, split on space or tab
/// (`statusFirstToken`).
pub(super) fn first_token(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let end = trimmed.find([' ', '\t']).unwrap_or(trimmed.len());
    trimmed[..end].to_uppercase()
}

/// Whether a command is exactly three ASCII digits.
pub(super) fn is_numeric_command(command: &str) -> bool {
    command.len() == 3 && command.bytes().all(|byte| byte.is_ascii_digit())
}

/// The standard-reply verbs (`statusStandardReplies`).
fn is_standard_reply(command: &str) -> bool {
    matches!(command, "FAIL" | "WARN" | "NOTE")
}

/// The severity a command's line carries (`statusSeverityFor`).
pub fn severity_for(command: &str) -> LogSeverity {
    match command {
        "PING" | "PONG" | "AUTHENTICATE" => LogSeverity::Trace,
        "FAIL" | "ERROR" => LogSeverity::Alert,
        "WARN" | "NOTE" => LogSeverity::Info,
        _ if is_numeric_command(command) && matches!(command.as_bytes()[0], b'4' | b'5') => {
            LogSeverity::Alert
        }
        _ => LogSeverity::Info,
    }
}

/// Whether a command's Status text is only its trailing parameter
/// (`statusTrailingBodyOnly`).
fn trailing_body_only(command: &str) -> bool {
    if is_standard_reply(command) {
        return true;
    }
    if matches!(command, "PRIVMSG" | "PING" | "PONG" | "ERROR") {
        return true;
    }
    if !is_numeric_command(command) {
        return false;
    }
    let code: u32 = command.parse().unwrap_or(0);
    matches!(code, 1..=3 | 372 | 375 | 376 | 422)
}

/// The tokens of the trailing parameter of a CAP line (`statusCapTokens`).
fn cap_tokens(message: &IrcMessage, subcommand_index: usize) -> Vec<String> {
    if message.params.len() <= subcommand_index + 1 {
        return Vec::new();
    }
    split_fields(&trailing_text(message))
}

/// The Status text of an incoming CAP line (`statusFormatIncomingCap`): LS, ACK,
/// NAK, NEW and DEL each name what the server said, then its tokens.
fn format_incoming_cap(message: &IrcMessage) -> Option<String> {
    if message.params.len() < 2 {
        return None;
    }
    let subcommand = wire_text(&message.params[1]);
    let (_, prefix) = [
        ("LS", "Server supports"),
        ("ACK", "Acknowledged"),
        ("NAK", "Rejected"),
        ("NEW", "Server added"),
        ("DEL", "Server removed"),
    ]
    .into_iter()
    .find(|(verb, _)| subcommand.eq_ignore_ascii_case(verb))?;
    let tokens = cap_tokens(message, 1);
    Some(if tokens.is_empty() {
        format!("{prefix}:")
    } else {
        format!("{prefix}: {}", tokens.join(" | "))
    })
}

/// The Status text of a lusers-style numeric (`statusFormatLusersNumeric`).
fn format_lusers(code: u32, parameters: &[String]) -> Option<String> {
    let rest = if parameters.is_empty() {
        parameters
    } else {
        &parameters[1..]
    };
    let (first, last) = (rest.first()?, rest.last()?);
    match code {
        252..=254 => Some(if rest.len() >= 2 {
            format!("{first} {last}")
        } else {
            first.clone()
        }),
        20 | 42 | 221 | 250 | 251 | 255 | 265 | 266 | 396 => Some(last.clone()),
        _ => None,
    }
}

/// The numerics and verbs whose Status text names a refused nick
/// (`nickRefusalStatusCommand`).
fn is_nick_refusal(command: &str) -> bool {
    matches!(
        command,
        "ERROR" | "432" | "433" | "436" | "437" | "451" | "462" | "465"
    )
}

/// Whether a refusal carries its sentence (`nickRefusalSentencePresent`).
/// 432, 433, 436 and 437 carry `<client> <nick> :<reason>`; 451, 462 and 465
/// carry `<client> :<reason>`. Fewer parameters means no sentence.
fn nick_refusal_sentence_present(command: &str, count: usize) -> bool {
    match command {
        "432" | "433" | "436" | "437" => count >= 3,
        "451" | "462" | "465" => count >= 2,
        _ => count >= 1,
    }
}

/// The reason of a refusal as Status keeps it, and whether to omit the reason
/// (`nickRefusalStatusTail`). A preview replacement is what Status stores. A
/// transcript rejection with no preview is omitted.
fn nick_refusal_tail(raw: &str, channel_types: &str) -> (String, bool) {
    let reason = raw.trim();
    if reason.is_empty() {
        return (String::new(), true);
    }
    if let Some(masked) = redact_preview_line(reason, channel_types) {
        return (masked, false);
    }
    if !allows_transcript(reason, channel_types) {
        return (String::new(), true);
    }
    (reason.to_owned(), false)
}

/// The Status text of an incoming line (`statusIncomingText`). `redacted` says
/// the line went through the command mask, so a lone `***` is shown as the verb
/// and the mask.
fn incoming_text(
    message: &IrcMessage,
    command: &str,
    redacted: bool,
    channel_types: &str,
) -> String {
    if message.params.is_empty() {
        return command.to_owned();
    }
    if redacted
        && message.params.len() == 1
        && wire_text(&message.params[0]) == "***"
        && !trailing_body_only(command)
    {
        return format!("{command} ***");
    }
    let trailing = trailing_text(message);
    if trailing_body_only(command) {
        if !is_nick_refusal(command) {
            return trailing;
        }
        let (tail, omit) = nick_refusal_tail(&trailing, channel_types);
        return if omit { command.to_owned() } else { tail };
    }
    let mut parts = parameters_text(message);
    if command.as_bytes().first().is_some_and(u8::is_ascii_digit) && parts.len() >= 2 {
        parts.remove(0);
    }
    if is_nick_refusal(command) {
        // No usable sentence: the numeric itself, the way ERROR falls back to the
        // command. A leftover nick is not the reason.
        if !nick_refusal_sentence_present(command, message.params.len()) {
            return command.to_owned();
        }
        let (tail, omit) = nick_refusal_tail(&trailing, channel_types);
        if omit {
            return command.to_owned();
        }
        if let Some(last) = parts.last_mut() {
            *last = tail;
        }
    }
    parts.join(" ")
}

/// The sender a CTCP or notice line names, or `unknown` (`sender` in the Go
/// builder).
fn sender_of(message: &IrcMessage) -> String {
    match &message.prefix {
        Some(prefix) if !prefix.nick.is_empty() => wire_text(&prefix.nick),
        _ => "unknown".to_owned(),
    }
}

/// Classifies one incoming message into every Status entry it produces
/// (`IrcStatusEntry::incomingAll`). A 004 yields one entry per advertised field.
/// Most messages yield one.
pub fn incoming_all(
    network_id: &str,
    message: &IrcMessage,
    channel_types: &str,
    now: DateTime<Utc>,
) -> Vec<StatusEntry> {
    let command = command_of(message);

    if command == "004" {
        const FIELDS: [(&str, usize); 5] = [
            ("Host", 1),
            ("IRCd", 2),
            ("User modes", 3),
            ("Channel modes", 4),
            ("Parametric channel modes", 5),
        ];
        let parameters = parameters_text(message);
        let mut entries = Vec::new();
        for (label, index) in FIELDS {
            if index >= parameters.len() || (index == 5 && parameters.len() <= 5) {
                break;
            }
            entries.push(StatusEntry::new(
                network_id,
                now,
                LogSource::Server,
                severity_for(&command),
                "004",
                format!("{label}: {}", parameters[index]),
            ));
        }
        if !entries.is_empty() {
            return entries;
        }
    }

    if command == "CAP" {
        if let Some(text) = format_incoming_cap(message) {
            return vec![StatusEntry::new(
                network_id,
                now,
                LogSource::Server,
                severity_for(&command),
                "CAP",
                text,
            )];
        }
    }

    if is_numeric_command(&command) {
        let code: u32 = command.parse().unwrap_or(0);
        if let Some(text) = format_lusers(code, &parameters_text(message)) {
            return vec![StatusEntry::new(
                network_id,
                now,
                LogSource::Server,
                severity_for(&command),
                &command,
                text,
            )];
        }
    }

    vec![build_default_incoming(
        network_id,
        message,
        channel_types,
        now,
    )]
}

/// Classifies one incoming message into its first Status entry
/// (`IrcStatusEntry::incoming`).
#[cfg(test)]
pub fn incoming(
    network_id: &str,
    message: &IrcMessage,
    channel_types: &str,
    now: DateTime<Utc>,
) -> StatusEntry {
    incoming_all(network_id, message, channel_types, now)
        .into_iter()
        .next()
        .unwrap_or_else(|| build_default_incoming(network_id, message, channel_types, now))
}

/// The default incoming classification (`statusBuildDefaultIncoming`): WHOIS,
/// CTCP requests and replies, notices, invites, then the plain line.
fn build_default_incoming(
    network_id: &str,
    message: &IrcMessage,
    channel_types: &str,
    now: DateTime<Utc>,
) -> StatusEntry {
    let command = command_of(message);

    if let Some(formatted) = format_whois(message) {
        let whois = (!formatted.nick.is_empty() && !formatted.text.is_empty()).then(|| WhoisLine {
            nick: formatted.nick.clone(),
            text: formatted.text.clone(),
            progress: formatted.progress,
        });
        let mut entry = StatusEntry::new(
            network_id,
            now,
            LogSource::Server,
            severity_for(&command),
            &formatted.label,
            formatted.text,
        );
        entry.whois_line = whois;
        return entry;
    }

    // Redaction happens at classification time: the stored text is the masked
    // form, never the raw secret-bearing line.
    let parameters = parameters_text(message);
    let masked = redact_message(&message.command, parameters.as_slice(), channel_types);
    let overlaid = masked.is_some();
    let display = match masked {
        Some(masked) => IrcMessage {
            command: masked.verb,
            params: masked
                .parameters
                .into_iter()
                .map(String::into_bytes)
                .collect(),
            ..message.clone()
        },
        None => message.clone(),
    };
    let display_command = command_of(&display);

    if display_command == "PRIVMSG" && display.params.len() >= 2 {
        if let Some(request) = parse_ctcp_request(&trailing_text(&display)) {
            let sender = sender_of(&display);
            let text = if request.argument.is_empty() {
                request.command
            } else {
                format!("{} {}", request.command, request.argument)
            };
            return StatusEntry::new(
                network_id,
                now,
                LogSource::Server,
                LogSeverity::Info,
                CTCP_LABEL,
                format!("{text} from {sender}"),
            );
        }
    }

    if display_command == "NOTICE" && display.params.len() >= 2 {
        if let Some(request) = parse_ctcp_request(&trailing_text(&display)) {
            if request.command != "ACTION" {
                let sender = sender_of(&display);
                let text = format_ctcp_reply_text(
                    &request.command,
                    &sender,
                    &request.argument,
                    now.timestamp_millis(),
                );
                let mut entry = StatusEntry::new(
                    network_id,
                    now,
                    LogSource::Server,
                    LogSeverity::Info,
                    CTCP_LABEL,
                    text,
                );
                entry.ctcp_reply = Some(CtcpReplyLine {
                    nick: sender,
                    command: request.command,
                });
                return entry;
            }
        }
    }

    if let Some(parts) = parse_incoming_notice(&display) {
        return StatusEntry::new(
            network_id,
            now,
            LogSource::Server,
            severity_for(&command),
            NOTICE_LABEL,
            present_notice(&parts).text(),
        );
    }

    if let Some(invite) = present_invite(&display) {
        return StatusEntry::new(
            network_id,
            now,
            LogSource::Server,
            severity_for(&command),
            "INVITE",
            invite,
        );
    }

    let text = incoming_text(&display, &display_command, overlaid, channel_types);
    StatusEntry::new(
        network_id,
        now,
        LogSource::Server,
        severity_for(&command),
        &command,
        text,
    )
}
