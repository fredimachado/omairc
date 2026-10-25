//! Fixtures and the translator's tests. The tests are grouped by what they pin, one
//! child module each under `tests/`, and the fixtures here are shared by all of them.

use super::context::conversation_for;
use super::messages::action_text;
use super::numerics::{all_digits, is_go_int};
use super::*;
use crate::irc::event::{
    ConversationKey as Key, EventKind, HistoryEvent, HistoryKind, MessageEvent, MessageKindTag,
    MsgId as Id, Name,
};
use crate::irc::parser::parse;
use crate::irc::typing::TypingPhase;
use chrono::TimeZone;

mod addressing;
mod channel;
mod chat;
mod corpus;
mod dispatch;
mod numeric_facts;
mod replay;

const NETWORK_A: &str = "network-a";

/// The fixed reducer clock every Go translator test uses.
fn reducer_timestamp() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 4, 0, 0, 0).unwrap()
}

fn features() -> IrcServerFeatures {
    IrcServerFeatures::new()
}

fn must_parse(line: &str) -> IrcMessage {
    parse(line.as_bytes()).unwrap_or_else(|err| panic!("parse({line:?}) failed: {err}"))
}

fn translate_line(line: &str) -> Vec<Event> {
    translate(
        NETWORK_A,
        "omairc",
        &features(),
        &must_parse(line),
        reducer_timestamp(),
        None,
    )
}

fn translate_joined(line: &str, joined: JoinedMemberLookup<'_>) -> Vec<Event> {
    translate(
        NETWORK_A,
        "omairc",
        &features(),
        &must_parse(line),
        reducer_timestamp(),
        Some(joined),
    )
}

fn room_member_lookup(channel: &str, sender: &str) -> Option<String> {
    if sender.eq_ignore_ascii_case("bot") && channel.eq_ignore_ascii_case("#room") {
        Some("#room".to_owned())
    } else {
        None
    }
}

fn accept_any(channel: &str, _sender: &str) -> Option<String> {
    Some(channel.to_owned())
}

fn parsed_name(token: &str) -> Name {
    let parsed = features()
        .parse_names_token(token)
        .unwrap_or_else(|| panic!("parse_names_token({token:?}) failed"));
    Name {
        nick: parsed.nick,
        ranks: parsed.ranks,
    }
}

fn message_of(events: &[Event]) -> &MessageEvent {
    match events.first() {
        Some(Event::Message(value)) => value,
        other => panic!("want MessageEvent, got {other:?}"),
    }
}

fn history_of(batch: &HistoryBatch, joined: Option<JoinedMemberLookup<'_>>) -> HistoryEvent {
    translate_history(
        NETWORK_A,
        "omairc",
        &features(),
        batch,
        reducer_timestamp(),
        joined,
    )
    .expect("the batch translates")
}

fn batch(target: &str, kind: Option<HistoryKind>, lines: &[&str]) -> HistoryBatch {
    HistoryBatch {
        target: target.to_owned(),
        lines: lines.iter().map(|line| must_parse(line)).collect(),
        kind,
        ..HistoryBatch::default()
    }
}

/// Parses one corpus file as the framer hands it to the translator: the
/// trailing CRLF removed, the bytes otherwise untouched.
fn corpus_frame(bytes: &[u8]) -> IrcMessage {
    let trimmed = bytes
        .strip_suffix(b"\r\n")
        .unwrap_or_else(|| panic!("corpus file must end in CRLF"));
    parse(trimmed).unwrap_or_else(|err| panic!("corpus frame failed to parse: {err}"))
}
