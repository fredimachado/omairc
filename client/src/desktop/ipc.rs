//! The product CLI's JSON contract: the request line a control command sends to the running window
//! and the one JSON line the window answers with. Desktop-only (no Go twin). The field
//! lists are the ones `skills/omairc/reference.md` documents. The TUI has no local socket, so it has
//! no twin here.
//! `QJsonObject` writes its keys in sorted order, so every row below is a struct whose fields are
//! declared in that order. Serialising the struct gives the bytes Qt writes. Rows leave out empty
//! strings and `false` flags, as `OmaircIpc::putText` and `putFlag` do.

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use serde::Serialize;
use serde_json::{Map, Value};

/// The longest request or response line the socket carries (`kMaxIpcLineBytes`).
pub const MAX_LINE_BYTES: usize = 64 * 1024;
/// The `read` window when none is given.
pub const DEFAULT_LAST: usize = 50;
/// The most lines one `read` returns.
pub const MAX_LAST: usize = 100;
/// The whole payload of an activation request (`OmaircIpc::raisePing`).
pub const RAISE_PING: &str = "!";

/// The control commands. `Raise` is the activation ping as well.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Raise,
    Connections,
    Status,
    Send,
    Read,
    Names,
    Conversations,
}

/// A `--since` window: the cutoff instant, and the duration token it came from when there was one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SinceWindow {
    pub cutoff: DateTime<Utc>,
    pub token: String,
}

/// The one window a `read` asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadWindow {
    Last(usize),
    Since(SinceWindow),
    Unread,
}

/// One parsed control request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    pub command: Command,
    pub network_id: String,
    pub target: String,
    pub text: String,
    pub window: ReadWindow,
}

impl Request {
    pub fn new(command: Command) -> Self {
        Self {
            command,
            network_id: String::new(),
            target: String::new(),
            text: String::new(),
            window: ReadWindow::Last(DEFAULT_LAST),
        }
    }
}

/// Parses one request line (`OmaircIpc::parseRequest`). The error text is what the window
/// answers with.
pub fn parse_request(line: &str) -> Result<Request, String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return Err("Empty request".to_owned());
    }
    if trimmed == RAISE_PING {
        return Ok(Request::new(Command::Raise));
    }
    let object = match serde_json::from_str::<Value>(trimmed) {
        Ok(Value::Object(object)) => object,
        _ => return Err("Invalid JSON request".to_owned()),
    };

    let cmd = text_field(&object, "cmd");
    let cmd = cmd.trim();
    match cmd {
        "raise" => Ok(Request::new(Command::Raise)),
        "connections" | "list" => Ok(Request::new(Command::Connections)),
        "status" => Ok(network_only(Command::Status, &object)),
        "conversations" => Ok(network_only(Command::Conversations, &object)),
        "send" => parse_send(&object),
        "read" => parse_read(&object),
        "names" => parse_names(&object),
        "" => Err("Missing cmd".to_owned()),
        other => Err(format!("Unknown command: {other}")),
    }
}

fn parse_send(object: &Map<String, Value>) -> Result<Request, String> {
    let mut request = Request::new(Command::Send);
    request.network_id = text_field(object, "network").trim().to_owned();
    request.target = text_field(object, "target").trim().to_owned();
    request.text = text_field(object, "text");
    if request.target.is_empty() {
        return Err("send requires a target".to_owned());
    }
    if request.text.is_empty() {
        return Err("send requires text".to_owned());
    }
    Ok(request)
}

fn parse_names(object: &Map<String, Value>) -> Result<Request, String> {
    let mut request = Request::new(Command::Names);
    request.network_id = text_field(object, "network").trim().to_owned();
    request.target = text_field(object, "target").trim().to_owned();
    if request.target.is_empty() {
        return Err("names requires a target".to_owned());
    }
    Ok(request)
}

fn parse_read(object: &Map<String, Value>) -> Result<Request, String> {
    let mut request = Request::new(Command::Read);
    request.network_id = text_field(object, "network").trim().to_owned();
    request.target = text_field(object, "target").trim().to_owned();
    let window_keys = ["last", "since", "unread"]
        .iter()
        .filter(|key| object.contains_key(**key))
        .count();
    if window_keys > 1 {
        return Err("last, since, and unread cannot be combined".to_owned());
    }
    request.window = if object.contains_key("unread") {
        if object.get("unread").and_then(Value::as_bool) != Some(true) {
            return Err("unread must be true".to_owned());
        }
        ReadWindow::Unread
    } else if object.contains_key("since") {
        let token = text_field(object, "since");
        match parse_since(&token, Utc::now()) {
            Some(since) => ReadWindow::Since(since),
            None => return Err("Invalid since value".to_owned()),
        }
    } else if object.contains_key("last") {
        let count = object.get("last").and_then(json_int).unwrap_or(-1);
        if !(1..=MAX_LAST as i64).contains(&count) {
            return Err(format!("last must be between 1 and {MAX_LAST}"));
        }
        ReadWindow::Last(count as usize)
    } else {
        ReadWindow::Last(DEFAULT_LAST)
    };
    Ok(request)
}

fn network_only(command: Command, object: &Map<String, Value>) -> Request {
    let mut request = Request::new(command);
    request.network_id = text_field(object, "network").trim().to_owned();
    request
}

/// A JSON string field as text, or empty when it is missing or not a string (`QJsonValue::toString`).
fn text_field(object: &Map<String, Value>, key: &str) -> String {
    object
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// A JSON number as an integer (`QJsonValue::toInt`), truncating a fraction.
fn json_int(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_f64().map(|float| float as i64))
}

/// The milliseconds a `N` plus unit token spans: `s`, `m`, `h`, or `d`. `None` for anything else
/// for a count below one, and for a span that overflows (`OmaircIpc::durationMs`).
pub fn duration_ms(token: &str) -> Option<i64> {
    let unit = token.chars().last()?;
    if token.chars().count() < 2 {
        return None;
    }
    let digits = &token[..token.len() - unit.len_utf8()];
    let count: i64 = digits.parse().ok()?;
    if count < 1 {
        return None;
    }
    let factor: i64 = match unit {
        's' => 1_000,
        'm' => 60_000,
        'h' => 3_600_000,
        'd' => 86_400_000,
        _ => return None,
    };
    count.checked_mul(factor)
}

/// Parses a `--since` value: a duration token counted back from `now`, or an ISO-8601 instant
/// (`OmaircIpc::parseSince`). An instant without a zone is local time, as Qt reads it.
pub fn parse_since(token: &str, now: DateTime<Utc>) -> Option<SinceWindow> {
    if let Some(ms) = duration_ms(token) {
        let span = chrono::TimeDelta::try_milliseconds(ms)?;
        let cutoff = now.checked_sub_signed(span)?;
        return Some(SinceWindow {
            cutoff,
            token: token.to_owned(),
        });
    }
    Some(SinceWindow {
        cutoff: parse_iso(token)?,
        token: String::new(),
    })
}

fn parse_iso(token: &str) -> Option<DateTime<Utc>> {
    if let Ok(parsed) = DateTime::parse_from_rfc3339(token) {
        return Some(parsed.with_timezone(&Utc));
    }
    let naive = NaiveDateTime::parse_from_str(token, "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(token, "%Y-%m-%dT%H:%M:%S").ok())
        .or_else(|| {
            NaiveDate::parse_from_str(token, "%Y-%m-%d")
                .ok()
                .and_then(|date| date.and_hms_opt(0, 0, 0))
        })?;
    let local = Local.from_local_datetime(&naive).single()?;
    Some(local.with_timezone(&Utc))
}

/// The wire form of a request line, without its newline (`OmaircIpc::encodeRequest`).
pub fn encode_request(request: &Request) -> String {
    let mut object = Map::new();
    let name = match request.command {
        Command::Raise => "raise",
        Command::Connections => "connections",
        Command::Status => "status",
        Command::Send => "send",
        Command::Read => "read",
        Command::Names => "names",
        Command::Conversations => "conversations",
    };
    object.insert("cmd".to_owned(), Value::from(name));
    if !matches!(request.command, Command::Raise | Command::Connections) && !request.network_id.is_empty() {
        object.insert("network".to_owned(), Value::from(request.network_id.as_str()));
    }
    match request.command {
        Command::Send => {
            object.insert("target".to_owned(), Value::from(request.target.as_str()));
            object.insert("text".to_owned(), Value::from(request.text.as_str()));
        }
        Command::Names => {
            object.insert("target".to_owned(), Value::from(request.target.as_str()));
        }
        Command::Read => {
            if !request.target.is_empty() {
                object.insert("target".to_owned(), Value::from(request.target.as_str()));
            }
            match &request.window {
                ReadWindow::Last(count) => {
                    object.insert("last".to_owned(), Value::from(*count));
                }
                ReadWindow::Since(since) if !since.token.is_empty() => {
                    object.insert("since".to_owned(), Value::from(since.token.as_str()));
                }
                ReadWindow::Since(since) => {
                    object.insert("since".to_owned(), Value::from(iso_millis(since.cutoff)));
                }
                ReadWindow::Unread => {
                    object.insert("unread".to_owned(), Value::from(true));
                }
            }
        }
        Command::Raise | Command::Connections | Command::Status | Command::Conversations => {}
    }
    Value::Object(object).to_string()
}

/// `2011-10-19T16:40:51.620Z`, the UTC instant with milliseconds that Qt's `ISODateWithMs` writes.
pub fn iso_millis(instant: DateTime<Utc>) -> String {
    instant.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// Resolves the network a request names (`OmaircIpc::resolveNetworkId`). One connection answers
/// for an empty request; anything else must name a connection in `available`.
pub fn resolve_network_id(requested: &str, available: &[String]) -> Result<String, String> {
    if !requested.is_empty() {
        return if available.iter().any(|id| id == requested) {
            Ok(requested.to_owned())
        } else {
            Err(format!("Unknown network id '{requested}'."))
        };
    }
    match available {
        [] => Err("No connections are available.".to_owned()),
        [only] => Ok(only.clone()),
        _ => Err("Multiple connections are available; specify a network id.".to_owned()),
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// One connection row (`connectionObject`). `port` always appears; the rest follow the agent rule.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ConnectionInfo {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub host: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub id: String,
    #[serde(rename = "lastError", skip_serializing_if = "String::is_empty")]
    pub last_error: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub nick: String,
    pub port: u16,
    #[serde(skip_serializing_if = "is_false")]
    pub selected: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub state: String,
    #[serde(skip_serializing_if = "is_false")]
    pub tls: bool,
}

/// One transcript row of `read` (`handleRead`). `kind` is `message`, `notice` or `action`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MessageRow {
    pub kind: String,
    #[serde(skip_serializing_if = "is_false")]
    pub mention: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub message: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub msgid: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub network: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub sender: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub target: String,
    pub timestamp: String,
}

/// One member row of `names`, in panel order.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MemberRow {
    #[serde(skip_serializing_if = "is_false")]
    pub away: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub nick: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub status: String,
}

/// One row of `conversations`. `topic` appears only on channels, and only when the server sent one.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ConversationRow {
    #[serde(skip_serializing_if = "is_false")]
    pub channel: bool,
    #[serde(skip_serializing_if = "is_false")]
    pub mention: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub target: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub topic: String,
    pub unread: u32,
}

#[derive(Serialize)]
struct ConnectionsEnvelope<'a> {
    connections: &'a [ConnectionInfo],
    ok: bool,
}

#[derive(Serialize)]
struct StatusEnvelope<'a> {
    ok: bool,
    status: &'a ConnectionInfo,
}

#[derive(Serialize)]
struct MessagesEnvelope<'a> {
    messages: &'a [MessageRow],
    ok: bool,
    #[serde(skip_serializing_if = "is_false")]
    truncated: bool,
}

#[derive(Serialize)]
struct MembersEnvelope<'a> {
    members: &'a [MemberRow],
    ok: bool,
}

#[derive(Serialize)]
struct ConversationsEnvelope<'a> {
    conversations: &'a [ConversationRow],
    ok: bool,
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    error: &'a str,
    ok: bool,
    #[serde(skip_serializing_if = "is_false")]
    uncertain: bool,
}

#[derive(Serialize)]
struct OkEnvelope {
    ok: bool,
}

fn to_line<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| r##"{"ok":false,"error":"Could not encode the response"}"##.to_owned())
}

/// `{"ok":true}`: what `send` and `raise` answer with.
pub fn ok_response() -> String {
    to_line(&OkEnvelope { ok: true })
}

pub fn ok_connections(connections: &[ConnectionInfo]) -> String {
    to_line(&ConnectionsEnvelope {
        connections,
        ok: true,
    })
}

pub fn ok_status(status: &ConnectionInfo) -> String {
    to_line(&StatusEnvelope { ok: true, status })
}

pub fn ok_messages(messages: &[MessageRow], truncated: bool) -> String {
    to_line(&MessagesEnvelope {
        messages,
        ok: true,
        truncated,
    })
}

pub fn ok_members(members: &[MemberRow]) -> String {
    to_line(&MembersEnvelope { members, ok: true })
}

pub fn ok_conversations(conversations: &[ConversationRow]) -> String {
    to_line(&ConversationsEnvelope {
        conversations,
        ok: true,
    })
}

pub fn error_response(message: &str) -> String {
    to_line(&ErrorEnvelope {
        error: message,
        ok: false,
        uncertain: false,
    })
}

/// A failure whose effect is unknown: a `send` that may have been delivered.
pub fn uncertain_response(message: &str) -> String {
    to_line(&ErrorEnvelope {
        error: message,
        ok: false,
        uncertain: true,
    })
}

fn response_object(line: &str) -> Option<Map<String, Value>> {
    match serde_json::from_str::<Value>(line.trim()) {
        Ok(Value::Object(object)) => Some(object),
        _ => None,
    }
}

/// Whether a reply is a JSON object whose `ok` is a boolean (`OmaircIpc::responseHasOk`).
pub fn response_has_ok(line: &str) -> bool {
    response_object(line).is_some_and(|object| object.get("ok").is_some_and(Value::is_boolean))
}

/// Whether a reply says `"ok": true` (`OmaircIpc::responseOk`).
pub fn response_ok(line: &str) -> bool {
    response_object(line)
        .and_then(|object| object.get("ok").and_then(Value::as_bool))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap()
    }

    fn parse(line: &str) -> Request {
        parse_request(line).unwrap_or_else(|error| panic!("{line}: {error}"))
    }

    fn parse_error(line: &str) -> String {
        parse_request(line).expect_err(line)
    }

    #[test]
    fn a_raise_ping_is_a_raise_request() {
        assert_eq!(parse("!").command, Command::Raise);
        assert_eq!(parse("  !\n").command, Command::Raise);
    }

    #[test]
    fn raise_and_list_parse_as_json() {
        assert_eq!(parse(r##"{"cmd":"raise"}"##).command, Command::Raise);
        assert_eq!(parse(r##"{"cmd":"connections"}"##).command, Command::Connections);
        assert_eq!(parse(r##"{"cmd":"list"}"##).command, Command::Connections);
    }

    #[test]
    fn status_and_conversations_take_an_optional_network() {
        let status = parse(r##"{"cmd":" status ","network":" abc "}"##);
        assert_eq!(status.command, Command::Status);
        assert_eq!(status.network_id, "abc");
        assert_eq!(parse(r##"{"cmd":"conversations"}"##).network_id, "");
    }

    #[test]
    fn send_keeps_the_text_and_trims_the_target() {
        let send = parse(r##"{"cmd":"send","network":"n","target":" #chan ","text":" hi there "}"##);
        assert_eq!(send.command, Command::Send);
        assert_eq!(send.target, "#chan");
        assert_eq!(send.text, " hi there ");
    }

    #[test]
    fn send_names_its_missing_field() {
        assert_eq!(parse_error(r##"{"cmd":"send","text":"x"}"##), "send requires a target");
        assert_eq!(parse_error(r##"{"cmd":"send","target":"#c"}"##), "send requires text");
    }

    #[test]
    fn names_requires_a_target() {
        assert_eq!(parse(r##"{"cmd":"names","target":"#c"}"##).target, "#c");
        assert_eq!(parse_error(r##"{"cmd":"names"}"##), "names requires a target");
    }

    #[test]
    fn unknown_and_malformed_requests_say_why() {
        assert_eq!(parse_error(""), "Empty request");
        assert_eq!(parse_error("[1]"), "Invalid JSON request");
        assert_eq!(parse_error("{"), "Invalid JSON request");
        assert_eq!(parse_error("{}"), "Missing cmd");
        assert_eq!(parse_error(r##"{"cmd":"nope"}"##), "Unknown command: nope");
    }

    #[test]
    fn read_defaults_to_the_last_fifty_lines() {
        let read = parse(r##"{"cmd":"read"}"##);
        assert_eq!(read.window, ReadWindow::Last(50));
        assert_eq!(read.target, "");
        assert_eq!(parse(r##"{"cmd":"read","last":20,"target":"#c"}"##).window, ReadWindow::Last(20));
    }

    #[test]
    fn read_last_is_bounded_to_one_hundred() {
        assert_eq!(parse_error(r##"{"cmd":"read","last":101}"##), "last must be between 1 and 100");
        assert_eq!(parse_error(r##"{"cmd":"read","last":0}"##), "last must be between 1 and 100");
        assert_eq!(parse_error(r##"{"cmd":"read","last":"5"}"##), "last must be between 1 and 100");
        assert_eq!(parse(r##"{"cmd":"read","last":100}"##).window, ReadWindow::Last(100));
    }

    #[test]
    fn read_windows_do_not_combine() {
        assert_eq!(
            parse_error(r##"{"cmd":"read","last":20,"unread":true}"##),
            "last, since, and unread cannot be combined"
        );
        assert_eq!(parse_error(r##"{"cmd":"read","unread":false}"##), "unread must be true");
    }

    #[test]
    fn read_unread_is_its_own_window() {
        assert_eq!(parse(r##"{"cmd":"read","unread":true}"##).window, ReadWindow::Unread);
    }

    #[test]
    fn read_since_accepts_a_duration_or_an_instant() {
        match parse(r##"{"cmd":"read","since":"5m"}"##).window {
            ReadWindow::Since(since) => {
                assert_eq!(since.token, "5m");
                assert!(Utc::now().signed_duration_since(since.cutoff).num_minutes() >= 4);
            }
            other => panic!("expected a since window, got {other:?}"),
        }
        match parse(r##"{"cmd":"read","since":"2011-10-19T16:40:51.620Z"}"##).window {
            ReadWindow::Since(since) => {
                assert_eq!(since.token, "");
                assert_eq!(iso_millis(since.cutoff), "2011-10-19T16:40:51.620Z");
            }
            other => panic!("expected a since window, got {other:?}"),
        }
        assert_eq!(parse_error(r##"{"cmd":"read","since":"no"}"##), "Invalid since value");
    }

    #[test]
    fn durations_use_s_m_h_d_and_reject_overflow() {
        assert_eq!(duration_ms("1s"), Some(1_000));
        assert_eq!(duration_ms("5m"), Some(300_000));
        assert_eq!(duration_ms("1h"), Some(3_600_000));
        assert_eq!(duration_ms("2d"), Some(172_800_000));
        assert_eq!(duration_ms("0m"), None);
        assert_eq!(duration_ms("5"), None);
        assert_eq!(duration_ms("5w"), None);
        assert_eq!(duration_ms("9999999999999999s"), None);
        assert_eq!(duration_ms("9223372036854775807d"), None);
    }

    #[test]
    fn since_counts_a_duration_back_from_now() {
        let since = parse_since("1h", now()).expect("valid");
        assert_eq!(since.cutoff, now() - chrono::TimeDelta::hours(1));
        assert_eq!(since.token, "1h");
    }

    #[test]
    fn network_resolution_follows_the_connection_count() {
        let none: Vec<String> = Vec::new();
        assert_eq!(resolve_network_id("", &none), Err("No connections are available.".to_owned()));
        let one = vec!["a".to_owned()];
        assert_eq!(resolve_network_id("", &one), Ok("a".to_owned()));
        assert_eq!(resolve_network_id("a", &one), Ok("a".to_owned()));
        assert_eq!(resolve_network_id("zz", &one), Err("Unknown network id 'zz'.".to_owned()));
        let two = vec!["a".to_owned(), "b".to_owned()];
        assert_eq!(
            resolve_network_id("", &two),
            Err("Multiple connections are available; specify a network id.".to_owned())
        );
        assert_eq!(resolve_network_id("b", &two), Ok("b".to_owned()));
    }

    #[test]
    fn encoded_requests_parse_back_to_the_same_request() {
        let mut read = Request::new(Command::Read);
        read.network_id = "n".to_owned();
        read.target = "#chan".to_owned();
        read.window = ReadWindow::Last(7);
        let mut send = Request::new(Command::Send);
        send.network_id = "n".to_owned();
        send.target = "nick".to_owned();
        send.text = "hello there".to_owned();
        let mut names = Request::new(Command::Names);
        names.target = "#chan".to_owned();
        let since = Request {
            window: ReadWindow::Since(parse_since("5m", now()).expect("valid")),
            ..Request::new(Command::Read)
        };
        let mut unread = Request::new(Command::Read);
        unread.window = ReadWindow::Unread;
        for request in [
            Request::new(Command::Raise),
            Request::new(Command::Connections),
            Request::new(Command::Status),
            Request::new(Command::Conversations),
            send,
            read,
            names,
            since,
            unread,
        ] {
            let line = encode_request(&request);
            let parsed = parse(&line);
            assert_eq!(parsed.command, request.command, "{line}");
            assert_eq!(parsed.network_id, request.network_id, "{line}");
            assert_eq!(parsed.target, request.target, "{line}");
            assert_eq!(parsed.text, request.text, "{line}");
        }
    }

    fn connection() -> ConnectionInfo {
        ConnectionInfo {
            id: "nid".to_owned(),
            name: "Example Net".to_owned(),
            host: "irc.example".to_owned(),
            port: 6697,
            tls: true,
            nick: "omairc".to_owned(),
            state: "Connected".to_owned(),
            selected: true,
            last_error: String::new(),
        }
    }

    #[test]
    fn a_connection_row_is_sorted_and_leaves_out_empty_fields() {
        assert_eq!(
            ok_connections(&[connection(), ConnectionInfo { id: "plain".to_owned(), port: 6667, ..Default::default() }]),
            r##"{"connections":[{"host":"irc.example","id":"nid","name":"Example Net","nick":"omairc","port":6697,"selected":true,"state":"Connected","tls":true},{"id":"plain","port":6667}],"ok":true}"##
        );
    }

    #[test]
    fn last_error_appears_only_when_set() {
        let info = ConnectionInfo {
            last_error: "Not connected".to_owned(),
            ..connection()
        };
        assert_eq!(
            ok_status(&info),
            r##"{"ok":true,"status":{"host":"irc.example","id":"nid","lastError":"Not connected","name":"Example Net","nick":"omairc","port":6697,"selected":true,"state":"Connected","tls":true}}"##
        );
    }

    #[test]
    fn error_and_uncertain_envelopes_keep_the_ok_flag() {
        assert_eq!(error_response("Nope."), r##"{"error":"Nope.","ok":false}"##);
        assert_eq!(
            uncertain_response("Maybe."),
            r##"{"error":"Maybe.","ok":false,"uncertain":true}"##
        );
        assert_eq!(ok_response(), r##"{"ok":true}"##);
    }

    #[test]
    fn message_rows_drop_empty_fields_and_keep_the_timestamp() {
        let row = MessageRow {
            kind: "action".to_owned(),
            mention: true,
            message: "waves".to_owned(),
            msgid: "act".to_owned(),
            network: "nid".to_owned(),
            sender: "alice".to_owned(),
            target: "#omarchy".to_owned(),
            timestamp: "2011-10-19T16:43:00.000Z".to_owned(),
        };
        assert_eq!(
            ok_messages(&[row, MessageRow { kind: "notice".to_owned(), timestamp: "2011-10-19T16:44:00.000Z".to_owned(), ..Default::default() }], true),
            r##"{"messages":[{"kind":"action","mention":true,"message":"waves","msgid":"act","network":"nid","sender":"alice","target":"#omarchy","timestamp":"2011-10-19T16:43:00.000Z"},{"kind":"notice","timestamp":"2011-10-19T16:44:00.000Z"}],"ok":true,"truncated":true}"##
        );
        assert!(ok_messages(&[], false).ends_with(r##""ok":true}"##));
    }

    #[test]
    fn member_and_conversation_rows_follow_the_agent_rule() {
        assert_eq!(
            ok_members(&[
                MemberRow { away: false, label: "~fred".to_owned(), nick: "fred".to_owned(), status: "building".to_owned() },
                MemberRow { away: true, label: "&anna".to_owned(), nick: "anna".to_owned(), status: String::new() },
            ]),
            r##"{"members":[{"label":"~fred","nick":"fred","status":"building"},{"away":true,"label":"&anna","nick":"anna"}],"ok":true}"##
        );
        assert_eq!(
            ok_conversations(&[
                ConversationRow { channel: true, mention: false, target: "#omarchy".to_owned(), topic: "A cozy corner".to_owned(), unread: 0 },
                ConversationRow { channel: false, mention: true, target: "alice".to_owned(), topic: String::new(), unread: 2 },
            ]),
            r##"{"conversations":[{"channel":true,"target":"#omarchy","topic":"A cozy corner","unread":0},{"mention":true,"target":"alice","unread":2}],"ok":true}"##
        );
    }

    #[test]
    fn reply_readers_see_ok_and_nothing_else() {
        assert!(response_has_ok(r##"{"ok":false,"error":"x"}"##));
        assert!(!response_has_ok(r##"{"error":"x"}"##));
        assert!(!response_has_ok("not json"));
        assert!(response_ok(&ok_response()));
        assert!(!response_ok(&error_response("x")));
    }
}
