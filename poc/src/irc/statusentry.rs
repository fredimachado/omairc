//! Port of `tui/internal/irc/statusentry.go` (Go), `src/irc/ircstatusentry.{h,cpp}`
//! (C++, `IrcStatusEntry`).
//!
//! Classification turns one wire line into the Status entries it produces: the
//! server's registration burst, the CAP and lusers numerics, the WHOIS and notice
//! shapes, CTCP replies, and the outgoing line the client sent. Which lines reach
//! Status at all is the keep table (`keeps_incoming`, `keeps_outgoing`).
//!
//! Secrets are masked here, at classification time. A stored entry text is the
//! redacted form, never the raw line, and every redaction call fails closed: a
//! line the policy cannot read is dropped or masked, not kept.
//!
//! Pure: `now` is passed in, and no entry reads the IRCv3 `time` tag. Qt stamps
//! each entry with the wall clock when it classifies the line; the Rust core
//! receives that clock reading from its caller.

use chrono::{DateTime, Utc};

use super::ctcp::{format_ctcp_reply_text, parse_ctcp_request};
use super::message::IrcMessage;
use super::prefixnick::prefix_nick;
use super::secretpolicy::{
    allows_transcript, redact_message, redact_preview_line, redact_wire_line,
};
use super::serverfeatures::IrcServerFeatures;
use super::servicenick::{is_service_identity, nick_is_routable};
use super::wiretext::wire_text;

/// The label a command-outcome line carries (`IrcStatusEntry::outcome`).
pub const OUTCOME_LABEL: &str = "command";
/// The label a CTCP reply or request line carries.
pub const CTCP_LABEL: &str = "CTCP";
/// The label a notice carries.
pub const NOTICE_LABEL: &str = "NOTICE";

/// Who produced a Status line (`LogSource`, `IrcLogSource`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogSource {
    /// A line the server sent.
    Server,
    /// A line this client sent.
    Client,
    /// A locally generated lifecycle or outcome line.
    Local,
}

/// How a Status line is coloured (`LogSeverity`, `IrcLogSeverity`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogSeverity {
    /// PING, PONG and AUTHENTICATE chatter.
    Trace,
    /// The default.
    Info,
    /// Errors and standard-reply failures.
    Alert,
}

/// Where a WHOIS sub-line sits in its reply sequence (`WhoisProgress`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhoisProgress {
    /// A mid-sequence detail line.
    Detail,
    /// The 318 end-of-WHOIS line.
    Terminal,
    /// A 401 or 402 no-such-nick or no-such-server line.
    Failed,
}

/// The WHOIS detail a server Status entry carries (`WhoisLine`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhoisLine {
    pub nick: String,
    pub text: String,
    pub progress: WhoisProgress,
}

impl WhoisLine {
    /// Whether the line ends the WHOIS exchange (`Terminal`).
    pub fn is_terminal(&self) -> bool {
        self.progress != WhoisProgress::Detail
    }
}

/// The CTCP reply metadata a server Status entry carries (`CtcpReplyLine`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtcpReplyLine {
    pub nick: String,
    /// The uppercased CTCP command.
    pub command: String,
}

/// One classified Status line (`StatusEntry`, `IrcStatusEntry`). The text is the
/// redacted form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusEntry {
    pub network_id: String,
    pub timestamp: DateTime<Utc>,
    pub source: LogSource,
    pub severity: LogSeverity,
    pub label: String,
    /// The correlation label of the request this line answers, or empty.
    pub request_label: String,
    pub text: String,
    pub whois_line: Option<WhoisLine>,
    pub ctcp_reply: Option<CtcpReplyLine>,
}

impl StatusEntry {
    fn new(
        network_id: &str,
        timestamp: DateTime<Utc>,
        source: LogSource,
        severity: LogSeverity,
        label: &str,
        text: String,
    ) -> Self {
        Self {
            network_id: network_id.to_owned(),
            timestamp,
            source,
            severity,
            label: label.to_owned(),
            request_label: String::new(),
            text,
            whois_line: None,
            ctcp_reply: None,
        }
    }

    /// Records the correlation label of the request this line answers
    /// (`SetRequestLabel`).
    pub fn set_request_label(&mut self, request_label: &str) {
        self.request_label = request_label.to_owned();
    }
}

/// A locally generated lifecycle line (`Lifecycle`).
pub fn lifecycle(
    network_id: &str,
    severity: LogSeverity,
    label: &str,
    text: &str,
    now: DateTime<Utc>,
) -> StatusEntry {
    StatusEntry::new(
        network_id,
        now,
        LogSource::Local,
        severity,
        label,
        text.to_owned(),
    )
}

/// A locally generated command-outcome line (`Outcome`).
pub fn outcome(network_id: &str, text: &str, now: DateTime<Utc>) -> StatusEntry {
    StatusEntry::new(
        network_id,
        now,
        LogSource::Local,
        LogSeverity::Info,
        OUTCOME_LABEL,
        text.to_owned(),
    )
}

/// A CTCP reply line written on Status, labelled `CTCP`, such as the `/version`
/// answer `VERSION reply from anna: https://omairc.app`. It reads as a server
/// line, as the reply branch of a NOTICE does.
pub fn ctcp_reply(network_id: &str, text: &str, now: DateTime<Utc>) -> StatusEntry {
    StatusEntry::new(
        network_id,
        now,
        LogSource::Server,
        LogSeverity::Info,
        CTCP_LABEL,
        text.to_owned(),
    )
}

/// The command of a message, upper-cased (`statusCommandOf`).
fn command_of(message: &IrcMessage) -> String {
    message.command.to_uppercase()
}

/// Every parameter as display text (`statusMessageParameters`).
fn parameters_text(message: &IrcMessage) -> Vec<String> {
    message.params.iter().map(|p| wire_text(p)).collect()
}

/// The last parameter as display text, or empty.
fn trailing_text(message: &IrcMessage) -> String {
    message
        .params
        .last()
        .map_or_else(String::new, |p| wire_text(p))
}

/// A parameter by index, or empty when out of range (`statusParameterAt`).
fn parameter_at(parameters: &[String], index: usize) -> &str {
    parameters.get(index).map_or("", String::as_str)
}

/// Splits on spaces and drops empty fields (`statusSplitFields`, Qt
/// `SkipEmptyParts`).
fn split_fields(text: &str) -> Vec<String> {
    text.split(' ')
        .filter(|field| !field.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Whether `text` starts with `prefix`, ignoring ASCII case. It compares bytes, so
/// it never splits a character (`hasPrefixFold`).
fn has_prefix_fold(text: &str, prefix: &str) -> bool {
    text.as_bytes()
        .get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix.as_bytes()))
}

/// The first token of a line, upper-cased, split on space or tab
/// (`statusFirstToken`).
fn first_token(line: &str) -> String {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let end = trimmed.find([' ', '\t']).unwrap_or(trimmed.len());
    trimmed[..end].to_uppercase()
}

/// Whether a command is exactly three ASCII digits.
fn is_numeric_command(command: &str) -> bool {
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

/// The Status text of an outgoing `CAP REQ` (`statusFormatOutgoingCapReq`).
fn format_outgoing_cap_req(display: &str) -> Option<String> {
    let trimmed = display.trim();
    if !has_prefix_fold(trimmed, "CAP") {
        return None;
    }
    let rest = trimmed.get(3..)?.trim();
    if !has_prefix_fold(rest, "REQ") {
        return None;
    }
    let mut rest = rest.get(3..)?.trim();
    if let Some(stripped) = rest.strip_prefix(':') {
        rest = stripped;
    }
    let tokens = split_fields(rest);
    Some(if tokens.is_empty() {
        "Requesting:".to_owned()
    } else {
        format!("Requesting: {}", tokens.join(" | "))
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

/// How a WHOIS numeric lays out its parameters (`statusWhoisLayout`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WhoisLayout {
    User,
    Channels,
    Server,
    Away,
    Idle,
    End,
    NickRest,
    Account,
    NoSuchNick,
    NoSuchServer,
}

/// One WHOIS numeric the Status pane formats (`statusWhoisSpec`). An empty label
/// means the command itself.
struct WhoisSpec {
    code: u32,
    label: &'static str,
    layout: WhoisLayout,
}

/// The WHOIS numerics Status formats (`statusWhoisNumerics`).
static WHOIS_NUMERICS: [WhoisSpec; 17] = [
    WhoisSpec {
        code: 311,
        label: "whois",
        layout: WhoisLayout::User,
    },
    WhoisSpec {
        code: 319,
        label: "whois",
        layout: WhoisLayout::Channels,
    },
    WhoisSpec {
        code: 312,
        label: "whois",
        layout: WhoisLayout::Server,
    },
    WhoisSpec {
        code: 301,
        label: "whois",
        layout: WhoisLayout::Away,
    },
    WhoisSpec {
        code: 317,
        label: "whois",
        layout: WhoisLayout::Idle,
    },
    WhoisSpec {
        code: 318,
        label: "whois",
        layout: WhoisLayout::End,
    },
    WhoisSpec {
        code: 276,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 307,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 313,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 320,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 330,
        label: "whois",
        layout: WhoisLayout::Account,
    },
    WhoisSpec {
        code: 338,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 378,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 379,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 671,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 401,
        label: "",
        layout: WhoisLayout::NoSuchNick,
    },
    WhoisSpec {
        code: 402,
        label: "",
        layout: WhoisLayout::NoSuchServer,
    },
];

/// The WHOIS spec for a numeric, or `None` (`statusWhoisSpecFor`).
fn whois_spec(code: u32) -> Option<&'static WhoisSpec> {
    WHOIS_NUMERICS.iter().find(|spec| spec.code == code)
}

/// A WHOIS line, formatted (`statusFormattedWhois`).
struct FormattedWhois {
    label: String,
    text: String,
    nick: String,
    progress: WhoisProgress,
}

/// Formats a WHOIS numeric (`statusFormatWhois`). `None` when the numeric is not
/// one Status formats, or its parameters are too few for its layout.
fn format_whois(message: &IrcMessage) -> Option<FormattedWhois> {
    let command = command_of(message);
    if command.len() != 3 {
        return None;
    }
    let code: u32 = command.parse().ok()?;
    let spec = whois_spec(code)?;
    let parameters = parameters_text(message);
    let at = |index: usize| parameter_at(&parameters, index).to_owned();
    let short = |needed: usize| parameters.len() < needed;

    let text = match spec.layout {
        WhoisLayout::User => {
            if short(6) {
                return None;
            }
            format!("{} is {}@{} ({})", at(1), at(2), at(3), at(5))
        }
        WhoisLayout::Channels => {
            if short(3) {
                return None;
            }
            format!("{} is on {}", at(1), at(2))
        }
        WhoisLayout::Server => {
            if short(4) {
                return None;
            }
            format!("{} using {} ({})", at(1), at(2), at(3))
        }
        WhoisLayout::Away => {
            if short(3) {
                return None;
            }
            format!("{} is away: {}", at(1), at(2))
        }
        WhoisLayout::Idle => {
            if short(3) {
                return None;
            }
            let mut text = format!("{} idle {}s", at(1), at(2));
            if parameters.len() >= 5 && !at(3).is_empty() {
                text.push_str(&format!(", signon {}", at(3)));
            }
            text
        }
        WhoisLayout::End => {
            if short(2) {
                return None;
            }
            format!("End of WHOIS for {}", at(1))
        }
        WhoisLayout::NickRest => {
            if short(3) {
                return None;
            }
            format!("{} {}", at(1), parameters[2..].join(" "))
        }
        WhoisLayout::Account => {
            if short(3) {
                return None;
            }
            format!("{} is logged in as {}", at(1), at(2))
        }
        WhoisLayout::NoSuchNick => {
            if short(2) {
                return None;
            }
            format!("No such nick: {}", at(1))
        }
        WhoisLayout::NoSuchServer => {
            if short(2) {
                return None;
            }
            format!("No such server: {}", at(1))
        }
    };

    let progress = match code {
        318 => WhoisProgress::Terminal,
        401 | 402 => WhoisProgress::Failed,
        _ => WhoisProgress::Detail,
    };
    let label = if spec.label.is_empty() {
        command
    } else {
        spec.label.to_owned()
    };
    Some(FormattedWhois {
        label,
        text,
        nick: at(1),
        progress,
    })
}

/// The Status copy of a notice: wrapped with its speaker, or bare
/// (`statusNoticeStatusCopy`).
enum NoticeCopy {
    Wrapped { speaker: String, body: String },
    Bare { body: String },
}

impl NoticeCopy {
    /// The Status text: `-<speaker>- <body>`, or the body alone.
    fn text(&self) -> String {
        match self {
            NoticeCopy::Wrapped { speaker, body } => format!("-{speaker}- {body}"),
            NoticeCopy::Bare { body } => body.clone(),
        }
    }
}

/// The speaker token of a notice, trimmed. `*` and empty are not speakers
/// (`statusTryMakeNoticeSpeaker`).
fn notice_speaker(token: &str) -> Option<String> {
    let trimmed = token.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("*") {
        return None;
    }
    Some(trimmed.to_owned())
}

/// The wire pieces of an incoming NOTICE (`statusNoticeWireParts`).
struct NoticeWire {
    prefix_nick: Option<String>,
    prefix_raw: Option<String>,
    target: String,
    body: String,
}

/// Reads the pieces of an incoming NOTICE (`statusParseIncomingNotice`).
fn parse_incoming_notice(message: &IrcMessage) -> Option<NoticeWire> {
    if command_of(message) != "NOTICE" || message.params.is_empty() {
        return None;
    }
    let (prefix_nick, prefix_raw) = match &message.prefix {
        Some(prefix) => (
            (!prefix.nick.is_empty()).then(|| wire_text(&prefix.nick)),
            (!prefix.raw.is_empty()).then(|| wire_text(&prefix.raw)),
        ),
        None => (None, None),
    };
    let body = if message.params.len() >= 2 {
        trailing_text(message)
    } else {
        String::new()
    };
    Some(NoticeWire {
        prefix_nick,
        prefix_raw,
        target: wire_text(&message.params[0]),
        body,
    })
}

/// Chooses the notice speaker: the nick, then the raw source, then a target that
/// names a speaker (`statusResolveIncomingNoticeSpeaker`). Then presents the copy.
fn present_notice(parts: &NoticeWire) -> NoticeCopy {
    let speaker = parts
        .prefix_nick
        .as_deref()
        .and_then(notice_speaker)
        .or_else(|| parts.prefix_raw.as_deref().and_then(notice_speaker))
        .or_else(|| notice_speaker(&parts.target));
    match speaker {
        Some(speaker) => NoticeCopy::Wrapped {
            speaker,
            body: parts.body.clone(),
        },
        None => NoticeCopy::Bare {
            body: parts.body.clone(),
        },
    }
}

/// The Status text of an incoming INVITE (`statusPresentIncomingInvite`).
fn present_invite(message: &IrcMessage) -> Option<String> {
    if command_of(message) != "INVITE" || message.params.len() < 2 {
        return None;
    }
    let nick = prefix_nick(message);
    let channel = trailing_text(message);
    if nick.is_empty() || channel.is_empty() {
        return None;
    }
    Some(format!("{nick} invited you to {channel}"))
}

/// What the keep table does with a line (`statusKeep`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Keep {
    Drop,
    Keep,
    IncomingPrivmsg,
}

/// One keep-table row (`statusKeepRow`).
struct KeepRow {
    command: &'static str,
    incoming: Keep,
    outgoing: Keep,
}

/// The keep table, mirroring `kStatusKeep` in `src/irc/ircstatusentry.cpp`. Each
/// command is uppercase and appears once; `keep_rows_are_well_formed` checks it.
static KEEP_ROWS: [KeepRow; 25] = [
    KeepRow {
        command: "PING",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "PONG",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "CAP",
        incoming: Keep::Keep,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "AUTHENTICATE",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "JOIN",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "PART",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "QUIT",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "NICK",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "KICK",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "TOPIC",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "MODE",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "BATCH",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "TAGMSG",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "AWAY",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "CHATHISTORY",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "MARKREAD",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "READ",
        incoming: Keep::Drop,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "PRIVMSG",
        incoming: Keep::IncomingPrivmsg,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "NOTICE",
        incoming: Keep::Keep,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "FAIL",
        incoming: Keep::Keep,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "WARN",
        incoming: Keep::Keep,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "NOTE",
        incoming: Keep::Keep,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "ERROR",
        incoming: Keep::Keep,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "INVITE",
        incoming: Keep::Keep,
        outgoing: Keep::Drop,
    },
    KeepRow {
        command: "PASS",
        incoming: Keep::Drop,
        outgoing: Keep::Keep,
    },
];

/// The keep-table row for a command (`statusKeepRowFor`).
fn keep_row(command: &str) -> Option<&'static KeepRow> {
    KEEP_ROWS.iter().find(|row| row.command == command)
}

/// The feature set the classifier uses when only CHANTYPES is on hand
/// (`statusFeaturesForChannelTypes`). The other fields keep their defaults.
fn features_for_channel_types(channel_types: &str) -> IrcServerFeatures {
    let mut features = IrcServerFeatures::new();
    if !channel_types.is_empty() {
        features.apply_token(&format!("CHANTYPES={channel_types}"));
    }
    features
}

/// Targets that name the network rather than a conversation.
fn is_network_notice_target(target: &str) -> bool {
    target.is_empty() || target == "*" || target.eq_ignore_ascii_case("AUTH")
}

/// Whether the source names a nick and a user (`statusHasUserPrefix`).
fn has_user_prefix(message: &IrcMessage) -> bool {
    message
        .prefix
        .as_ref()
        .is_some_and(|prefix| !prefix.nick.is_empty() && !prefix.user.is_empty())
}

/// Whether the source is a services identity (`statusIsServiceUser`).
fn is_service_user(message: &IrcMessage, features: &IrcServerFeatures) -> bool {
    message.prefix.as_ref().is_some_and(|prefix| {
        is_service_identity(
            &wire_text(&prefix.nick),
            &wire_text(&prefix.host),
            features.channel_types(),
        )
    })
}

/// Whether a direct message belongs to a conversation the user is in
/// (`statusConversationEngaged`, the part of `ircConversationFor` that Status
/// consults).
fn conversation_engaged(
    message: &IrcMessage,
    target: &str,
    current_nick: &str,
    features: &IrcServerFeatures,
) -> bool {
    if let Some(prefix) = &message.prefix {
        if !prefix.nick.is_empty() && !nick_is_routable(&wire_text(&prefix.nick)) {
            return false;
        }
    }
    if features.is_channel(target) {
        return true;
    }
    if is_network_notice_target(target)
        || !has_user_prefix(message)
        || is_service_user(message, features)
    {
        return false;
    }
    let sender = prefix_nick(message);
    if sender.is_empty() {
        return false;
    }
    let mapping = features.case_mapping();
    mapping.equals(target, current_nick) || mapping.equals(&sender, current_nick)
}

/// Whether a numeric is one Status keeps (`statusKeepsIncomingNumeric`).
fn keeps_numeric(command: &str) -> bool {
    if !is_numeric_command(command) {
        return false;
    }
    let code: u32 = command.parse().unwrap_or(0);
    if matches!(code, 1..=4) {
        return true;
    }
    if matches!(
        code,
        20 | 42
            | 221
            | 250
            | 251
            | 252
            | 253
            | 254
            | 255
            | 263
            | 265
            | 266
            | 396
            | 372
            | 375
            | 376
            | 422
            | 764
            | 767
            | 769
            | 903
            | 904
            | 905
    ) {
        return true;
    }
    if whois_spec(code).is_some() {
        return true;
    }
    matches!(command.as_bytes()[0], b'4' | b'5')
}

/// Whether a PRIVMSG belongs in Status (`statusKeepsIncomingPrivmsg`). Channel
/// chat stays in its channel. A services line, a CTCP other than ACTION, and a
/// direct message the user is not engaged in do go to Status.
fn keeps_privmsg(message: &IrcMessage, current_nick: &str, channel_types: &str) -> bool {
    if message.params.len() >= 2 {
        if let Some(request) = parse_ctcp_request(&trailing_text(message)) {
            if request.command != "ACTION" {
                return true;
            }
        }
    }

    let features = features_for_channel_types(channel_types);
    let types = features.channel_types();
    let target = message
        .params
        .first()
        .map_or_else(String::new, |p| wire_text(p));
    // Channel chat belongs in the channel transcript. Ergo replays join and quit
    // history as HistServ PRIVMSG to the channel; the service-nick rule below
    // would otherwise flood Status with that replay.
    if features.is_channel(&target) {
        return false;
    }
    let (sender, host) = match &message.prefix {
        Some(prefix) => (wire_text(&prefix.nick), wire_text(&prefix.host)),
        None => (String::new(), String::new()),
    };
    if is_service_identity(&sender, &host, types) || is_service_identity(&target, "", types) {
        return true;
    }
    !conversation_engaged(message, &target, current_nick, &features)
}

/// Whether an incoming message belongs in Status (`ircStatusKeepsIncoming`).
pub fn keeps_incoming(message: &IrcMessage, current_nick: &str, channel_types: &str) -> bool {
    let command = command_of(message);
    match keep_row(&command) {
        Some(row) => match row.incoming {
            Keep::Keep => true,
            Keep::Drop => false,
            Keep::IncomingPrivmsg => keeps_privmsg(message, current_nick, channel_types),
        },
        None => keeps_numeric(&command),
    }
}

/// The wire line without its CRLF or LF.
fn strip_line_end(line: &[u8]) -> &[u8] {
    if let Some(stripped) = line.strip_suffix(b"\r\n") {
        stripped
    } else if let Some(stripped) = line.strip_suffix(b"\n") {
        stripped
    } else {
        line
    }
}

/// Whether an outgoing line belongs in Status (`ircStatusKeepsOutgoing`). The
/// CAP REQ the client sent is kept; other CAP lines are not.
pub fn keeps_outgoing(line: &[u8]) -> bool {
    let display = wire_text(strip_line_end(line));
    let verb = first_token(&display);
    if verb.is_empty() {
        return false;
    }
    let Some(row) = keep_row(&verb) else {
        return false;
    };
    if row.outgoing == Keep::Keep {
        return true;
    }
    if verb == "CAP" {
        let trimmed = display.trim();
        if trimmed.len() > 3 {
            if let Some(rest) = trimmed.get(3..) {
                return has_prefix_fold(rest.trim(), "REQ");
            }
        }
    }
    false
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

/// Classifies one outgoing wire line (`IrcStatusEntry::outgoing`). The text is
/// redacted with `redact_wire_line`, so a PASS, a keyed JOIN or MODE, or a service
/// request never carries its secret into Status.
pub fn outgoing(
    network_id: &str,
    line: &[u8],
    channel_types: &str,
    now: DateTime<Utc>,
) -> StatusEntry {
    let display = wire_text(strip_line_end(line));
    let verb = first_token(&display);
    let text = if let Some(cap_req) = format_outgoing_cap_req(&display) {
        cap_req
    } else if let Some(safe) = redact_wire_line(&display, channel_types) {
        safe
    } else {
        display
    };
    StatusEntry::new(
        network_id,
        now,
        LogSource::Client,
        severity_for(&verb),
        &verb,
        text,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::parser::parse;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap()
    }

    fn msg(line: &str) -> IrcMessage {
        parse(line.as_bytes()).expect("parses")
    }

    /// The Status text of each entry `incoming_all` yields.
    fn texts(line: &str) -> Vec<String> {
        incoming_all("net", &msg(line), "#&", now())
            .into_iter()
            .map(|entry| entry.text)
            .collect()
    }

    fn one(line: &str) -> StatusEntry {
        incoming("net", &msg(line), "#&", now())
    }

    #[test]
    fn severities_follow_the_command() {
        assert_eq!(severity_for("PING"), LogSeverity::Trace);
        assert_eq!(severity_for("AUTHENTICATE"), LogSeverity::Trace);
        assert_eq!(severity_for("FAIL"), LogSeverity::Alert);
        assert_eq!(severity_for("WARN"), LogSeverity::Info);
        assert_eq!(severity_for("ERROR"), LogSeverity::Alert);
        assert_eq!(severity_for("433"), LogSeverity::Alert);
        assert_eq!(severity_for("512"), LogSeverity::Alert);
        assert_eq!(severity_for("001"), LogSeverity::Info);
        assert_eq!(severity_for("NOTICE"), LogSeverity::Info);
    }

    #[test]
    fn a_004_splits_into_one_entry_per_advertised_field() {
        let line = ":srv 004 me demo.omairc OmaircDemo iw abc def";
        assert_eq!(
            texts(line),
            [
                "Host: demo.omairc",
                "IRCd: OmaircDemo",
                "User modes: iw",
                "Channel modes: abc",
                "Parametric channel modes: def",
            ]
        );
        assert!(incoming_all("net", &msg(line), "#&", now())
            .iter()
            .all(|entry| entry.label == "004"));
    }

    #[test]
    fn a_short_004_keeps_only_the_fields_it_has() {
        assert_eq!(
            texts(":srv 004 me demo.omairc OmaircDemo iw"),
            ["Host: demo.omairc", "IRCd: OmaircDemo", "User modes: iw"]
        );
    }

    #[test]
    fn cap_lines_name_the_subcommand_and_its_tokens() {
        assert_eq!(
            texts(":srv CAP me LS :echo-message message-tags batch"),
            ["Server supports: echo-message | message-tags | batch"]
        );
        assert_eq!(texts(":srv CAP me ACK :batch"), ["Acknowledged: batch"]);
        assert_eq!(texts(":srv CAP me NAK :"), ["Rejected:"]);
        assert_eq!(texts(":srv CAP me NEW :sasl"), ["Server added: sasl"]);
        assert_eq!(texts(":srv CAP me DEL :sasl"), ["Server removed: sasl"]);
    }

    #[test]
    fn lusers_numerics_keep_their_text_and_nothing_else() {
        assert_eq!(
            texts(":srv 251 me :There are 9 users"),
            ["There are 9 users"]
        );
        assert_eq!(
            texts(":srv 252 me 3 :operators online"),
            ["3 operators online"]
        );
        assert_eq!(
            texts(":srv 265 me 9 12 :Current local users 9, max 12"),
            ["Current local users 9, max 12"]
        );
    }

    #[test]
    fn whois_lines_format_their_known_numerics_and_mark_the_end() {
        let user = one(":srv 311 me kai ~kai host * :Kai Tanaka");
        assert_eq!(user.label, "whois");
        assert_eq!(user.text, "kai is ~kai@host (Kai Tanaka)");
        let line = user.whois_line.expect("whois line");
        assert_eq!(line.nick, "kai");
        assert!(!line.is_terminal());

        let channels = one(":srv 319 me kai :@#omarchy +#ricing");
        assert_eq!(channels.text, "kai is on @#omarchy +#ricing");

        let idle = one(":srv 317 me kai 42 1700000000 :seconds idle, signon time");
        assert_eq!(idle.text, "kai idle 42s, signon 1700000000");

        let end = one(":srv 318 me kai :End of /WHOIS list.");
        assert_eq!(end.text, "End of WHOIS for kai");
        assert!(end.whois_line.expect("line").is_terminal());

        let missing = one(":srv 401 me ghost :No such nick/channel");
        assert_eq!(missing.label, "401", "401 has no label, so the command");
        assert_eq!(missing.text, "No such nick: ghost");
        assert_eq!(
            missing.whois_line.expect("line").progress,
            WhoisProgress::Failed
        );
    }

    #[test]
    fn a_whois_with_too_few_parameters_falls_back_to_the_plain_line() {
        let short = one(":srv 311 me kai ~kai");
        assert_eq!(short.label, "311");
        assert!(short.whois_line.is_none());
    }

    #[test]
    fn a_notice_wraps_its_speaker_and_a_bare_notice_does_not() {
        let wrapped = one(":anna!a@host NOTICE me :hello there");
        assert_eq!(wrapped.label, "NOTICE");
        assert_eq!(wrapped.text, "-anna- hello there");

        let bare = one("NOTICE * :server says hi");
        assert_eq!(bare.text, "server says hi");
    }

    #[test]
    fn a_ctcp_reply_notice_is_a_ctcp_line_that_names_its_reply() {
        let entry = one(":anna!a@host NOTICE me :\u{1}VERSION omairc-test\u{1}");
        assert_eq!(entry.label, "CTCP");
        assert_eq!(entry.text, "VERSION reply from anna: omairc-test");
        let reply = entry.ctcp_reply.expect("ctcp metadata");
        assert_eq!(reply.nick, "anna");
        assert_eq!(reply.command, "VERSION");
    }

    #[test]
    fn a_ctcp_request_in_a_privmsg_reads_as_its_command_from_the_sender() {
        let entry = one(":anna!a@host PRIVMSG me :\u{1}PING 1234\u{1}");
        assert_eq!(entry.label, "CTCP");
        assert_eq!(entry.text, "PING 1234 from anna");
    }

    #[test]
    fn an_invite_names_the_nick_and_the_channel() {
        let entry = one(":rio!r@host INVITE me #debian");
        assert_eq!(entry.label, "INVITE");
        assert_eq!(entry.text, "rio invited you to #debian");
    }

    #[test]
    fn a_standard_reply_shows_its_description_on_status() {
        assert_eq!(
            texts(":srv FAIL AUTH BAD_ACCOUNT :Account is not set up"),
            ["Account is not set up"]
        );
    }

    #[test]
    fn a_nick_refusal_names_the_reason_or_the_numeric() {
        assert_eq!(
            texts(":srv 433 * taken :Nickname is already in use"),
            ["taken Nickname is already in use"]
        );
        assert_eq!(
            texts(":srv 433 *"),
            ["433"],
            "a refusal with no sentence keeps the numeric"
        );
    }

    #[test]
    fn a_chat_line_to_a_channel_stays_in_the_channel_not_status() {
        let line = msg(":anna!a@host PRIVMSG #omarchy :hi all");
        assert!(!keeps_incoming(&line, "fred", "#&"));
    }

    #[test]
    fn a_direct_message_to_the_user_is_engaged_and_stays_out_of_status() {
        let line = msg(":anna!a@host PRIVMSG fred :hi");
        assert!(!keeps_incoming(&line, "fred", "#&"));
    }

    #[test]
    fn a_services_privmsg_goes_to_status() {
        let line = msg(":NickServ!services@services.example PRIVMSG fred :You are identified");
        assert!(keeps_incoming(&line, "fred", "#&"));
    }

    #[test]
    fn the_keep_table_drops_protocol_chatter_and_keeps_replies() {
        assert!(!keeps_incoming(&msg(":a!b@c JOIN #omarchy"), "fred", "#&"));
        assert!(!keeps_incoming(&msg("PING :srv"), "fred", "#&"));
        assert!(keeps_incoming(&msg(":srv 001 fred :Welcome"), "fred", "#&"));
        assert!(keeps_incoming(
            &msg(":srv ERROR :Closing link"),
            "fred",
            "#&"
        ));
        assert!(
            !keeps_incoming(&msg(":srv 303 fred :online"), "fred", "#&"),
            "303 is not a Status numeric"
        );
    }

    #[test]
    fn the_keep_table_is_uppercase_and_unique() {
        for (index, row) in KEEP_ROWS.iter().enumerate() {
            assert!(!row.command.is_empty());
            assert!(
                row.command.bytes().all(|byte| !byte.is_ascii_lowercase()),
                "{} must be uppercase",
                row.command
            );
            assert!(
                KEEP_ROWS[..index]
                    .iter()
                    .all(|earlier| earlier.command != row.command),
                "{} appears twice",
                row.command
            );
        }
    }

    #[test]
    fn an_outgoing_cap_req_is_kept_and_reads_as_requesting() {
        assert!(keeps_outgoing(b"CAP REQ :message-tags batch\r\n"));
        assert!(!keeps_outgoing(b"CAP END\r\n"));
        let entry = outgoing("net", b"CAP REQ :message-tags batch\r\n", "#&", now());
        assert_eq!(entry.text, "Requesting: message-tags | batch");
        assert_eq!(entry.source, LogSource::Client);
    }

    #[test]
    fn an_outgoing_pass_is_kept_with_its_secret_masked() {
        assert!(keeps_outgoing(b"PASS hunter2\r\n"));
        let entry = outgoing("net", b"PASS hunter2\r\n", "#&", now());
        assert!(
            !entry.text.contains("hunter2"),
            "a password never reaches Status"
        );
        assert_eq!(entry.label, "PASS");
    }

    #[test]
    fn an_outgoing_line_strips_either_line_ending() {
        let crlf = outgoing("net", b"PRIVMSG #omarchy :hi\r\n", "#&", now());
        let lf = outgoing("net", b"PRIVMSG #omarchy :hi\n", "#&", now());
        assert_eq!(crlf.text, "PRIVMSG #omarchy :hi");
        assert_eq!(crlf.text, lf.text);
        assert_eq!(crlf.label, "PRIVMSG");
    }

    #[test]
    fn local_lines_carry_the_local_source_and_their_label() {
        let outcome_line = outcome("net", "Command was refused", now());
        assert_eq!(outcome_line.label, "command");
        assert_eq!(outcome_line.source, LogSource::Local);
        assert_eq!(outcome_line.severity, LogSeverity::Info);

        let life = lifecycle(
            "net",
            LogSeverity::Alert,
            "connect",
            "Connection lost",
            now(),
        );
        assert_eq!(life.source, LogSource::Local);
        assert_eq!(life.severity, LogSeverity::Alert);
        assert_eq!(life.timestamp, now());
    }

    #[test]
    fn the_request_label_is_recorded_on_the_entry() {
        let mut entry = outcome("net", "ok", now());
        assert!(entry.request_label.is_empty());
        entry.set_request_label("req-7");
        assert_eq!(entry.request_label, "req-7");
    }

    /// Ported from `TestSessionConfiguredPasswordNeverAppearsInStatusEntries`: the
    /// server password reaches Status only as `PASS ***`.
    #[test]
    fn a_configured_password_reads_as_pass_stars() {
        let entry = outgoing("net", b"PASS hunter2\r\n", "", now());
        assert_eq!(entry.label, "PASS");
        assert_eq!(entry.text, "PASS ***");
    }

    /// Ported from `TestSessionKeyedJoinIsRedactedInStatusEntries`.
    #[test]
    fn a_keyed_join_is_masked_and_an_unkeyed_one_is_not() {
        let keyed = outgoing("net", b"JOIN #secret hunter2\r\n", "", now());
        assert_eq!(keyed.text, "JOIN #secret ***");
        assert!(!keyed.text.contains("hunter2"));
        assert_eq!(
            outgoing("net", b"JOIN #omarchy\r\n", "", now()).text,
            "JOIN #omarchy"
        );
    }

    /// Ported from `TestSessionSelfEchoToServiceIsRedacted`: a self-echoed
    /// services command never shows its secret, and a harmless `set` line does.
    #[test]
    fn a_self_echo_to_a_service_masks_its_secret() {
        let cases = [
            (":me!u@h PRIVMSG nickserv :identify my_nick s3cret", "IDENTIFY ***"),
            (":myserv!u@h PRIVMSG nickserv :identify my_nick s3cret", "IDENTIFY ***"),
            (":me!u@h PRIVMSG nickserv IDENTIFY my_nick s3cret", "IDENTIFY ***"),
            (":me!u@h PRIVMSG nickserv :\u{1}IDENTIFY my_nick s3cret\u{1}", "IDENTIFY ***"),
            (":me!u@h PRIVMSG nickserv :set password s3cret", "SET PASSWORD ***"),
            (
                ":me!u@h PRIVMSG nickserv :set email user@example.net",
                "set email user@example.net",
            ),
        ];
        for (line, want) in cases {
            let entry = incoming("freenode", &msg(line), "", now());
            assert_eq!(entry.text, want, "{line:?}");
            assert!(!entry.text.contains("s3cret"), "{line:?} leaked");
        }
        let ctcp = incoming(
            "freenode",
            &msg(":me!u@h PRIVMSG nickserv :\u{1}IDENTIFY my_nick s3cret\u{1}"),
            "",
            now(),
        );
        assert_eq!(ctcp.label, "PRIVMSG");
    }

    #[test]
    fn a_cap_req_check_never_splits_a_character() {
        // A multi-byte first character must not panic the byte-prefix test.
        assert!(!keeps_outgoing("é\r\n".as_bytes()));
        assert!(format_outgoing_cap_req("éCAP").is_none());
    }
}
