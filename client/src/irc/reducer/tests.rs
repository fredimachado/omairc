//! Ported reducer tests and their fixtures. The Go suite
//! (`tui/internal/irc/reducer_test.go`, `history_page_test.go`, `typing_test.go`
//! `readmarker_test.go`) builds its state the same way: a welcomed network, event
//! structs or translated wire lines, and reads of the conversations it created.
//! Each Go test keeps its name here in snake_case.

use std::sync::{Arc, Mutex};

use chrono::{DateTime, TimeZone, Utc};

use crate::irc::event::{
    AccountEvent, ActionEvent, AwayEvent, ConversationKey, Event, HistoryEvent, HistoryKind,
    JoinEvent, KickEvent, MemberMetadataEvent, MessageEvent, MessageKindTag, ModeEvent, MsgId,
    Name, NamesEvent, NickEvent, NoticeEvent, PartEvent, QuitEvent, ReplayLine, TopicEvent,
    WelcomeEvent,
};
use crate::irc::message::IrcMessage;
use crate::irc::parser;
use crate::irc::presence::Away;
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::translator::{translate, translate_history, HistoryBatch};
use crate::irc::viewnotify::ViewNotify;
use crate::irc::world::Message;

use super::log::{ConversationLog, TranscriptLine};
use super::state::ConversationState;
use super::EventReducer;

mod conversations;
mod history;
mod membership;
mod notify;
mod presence;
mod readmarker;
mod sidebar;
mod typing;
mod unread;

pub(super) const NETWORK_A: &str = "network-a";
pub(super) const NETWORK_B: &str = "network-b";

/// The fixed wall clock the ported reducer tests stamp onto live events
/// (`reducerTimestamp`).
pub(super) fn reducer_timestamp() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 4, 0, 0, 0).unwrap()
}

/// The typing suite's clock (`typingT0`).
pub(super) fn typing_t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 4, 12, 0, 0).unwrap()
}

/// Folds one event in at the fixed clock and returns the surfaces it dirtied.
pub(super) fn fold(reducer: &mut EventReducer, event: impl Into<Event>) -> Vec<ViewNotify> {
    reducer.apply(event.into(), reducer_timestamp())
}

/// Folds one event in at `now`.
pub(super) fn fold_at(
    reducer: &mut EventReducer,
    event: impl Into<Event>,
    now: DateTime<Utc>,
) -> Vec<ViewNotify> {
    reducer.apply(event.into(), now)
}

/// Seeds a registered network (`welcome`).
pub(super) fn welcome(reducer: &mut EventReducer, network_id: &str, nick: &str) {
    fold(
        reducer,
        WelcomeEvent {
            network_id: network_id.to_owned(),
            current_nick: nick.to_owned(),
        },
    );
}

/// `welcome` with the default nick `omairc`.
pub(super) fn welcome_omairc(reducer: &mut EventReducer, network_id: &str) {
    welcome(reducer, network_id, "omairc");
}

/// Parses one 353 NAMES token with the default feature set (`parsedName`).
pub(super) fn parsed_name(token: &str) -> Name {
    let parsed = IrcServerFeatures::new()
        .parse_names_token(token)
        .unwrap_or_else(|| panic!("parse_names_token({token:?}) failed"));
    Name {
        nick: parsed.nick,
        ranks: parsed.ranks,
    }
}

/// A `353` batch of the given tokens (`NamesEvent`).
pub(super) fn names(network_id: &str, channel: &str, tokens: &[&str], complete: bool) -> NamesEvent {
    NamesEvent {
        network_id: network_id.to_owned(),
        channel: channel.to_owned(),
        names: tokens.iter().map(|token| parsed_name(token)).collect(),
        complete,
    }
}

/// Parses one wire line that must succeed (`mustParse`).
pub(super) fn must_parse(line: &str) -> IrcMessage {
    parser::parse(line.as_bytes()).unwrap_or_else(|error| panic!("parse({line:?}) failed: {error:?}"))
}

/// Translates one wire line with the reducer's own features and folds the events
/// in (`applyWire`). The joined-member lookup reads the reducer as it stands.
pub(super) fn apply_wire(reducer: &mut EventReducer, line: &str) {
    let message = must_parse(line);
    let events: Vec<Event> = {
        let lookup = |channel: &str, sender: &str| {
            reducer.joined_member_channel(NETWORK_A, channel, sender)
        };
        let joined: &dyn Fn(&str, &str) -> Option<String> = &lookup;
        translate(
            NETWORK_A,
            "omairc",
            reducer.server_features(NETWORK_A),
            &message,
            reducer_timestamp(),
            Some(joined),
        )
    };
    for event in events {
        reducer.apply(event, reducer_timestamp());
    }
}

/// Translates a replay batch against the reducer's current state (`TranslateHistory`).
pub(super) fn history_of(reducer: &EventReducer, batch: &HistoryBatch) -> HistoryEvent {
    let lookup = |channel: &str, sender: &str| reducer.joined_member_channel(NETWORK_A, channel, sender);
    let joined: &dyn Fn(&str, &str) -> Option<String> = &lookup;
    translate_history(
        NETWORK_A,
        "omairc",
        reducer.server_features(NETWORK_A),
        batch,
        reducer_timestamp(),
        Some(joined),
    )
    .expect("the batch translates")
}

/// A replay batch of raw lines for `target`.
pub(super) fn batch(target: &str, kind: HistoryKind, lines: &[&str]) -> HistoryBatch {
    HistoryBatch {
        target: target.to_owned(),
        lines: lines.iter().map(|line| must_parse(line)).collect(),
        kind: Some(kind),
        ..HistoryBatch::default()
    }
}

/// The channel named `target` on network A (`roomOf`).
pub(super) fn room_of<'a>(reducer: &'a EventReducer, target: &str) -> Option<&'a ConversationState> {
    reducer.find(&reducer.conversation_key(NETWORK_A, target))
}

/// The body of the last row of a channel transcript (`lastBody`).
pub(super) fn last_body(reducer: &EventReducer, target: &str) -> String {
    room_of(reducer, target)
        .and_then(|conv| conv.messages.last())
        .map(|message| message.body.clone())
        .unwrap_or_default()
}

/// A conversation that must exist (`stateOf`).
pub(super) fn state_of<'a>(reducer: &'a EventReducer, key: &ConversationKey) -> &'a ConversationState {
    reducer
        .find(key)
        .unwrap_or_else(|| panic!("conversation {key:?} is missing"))
}

pub(super) fn bodies(conv: &ConversationState) -> Vec<String> {
    conv.messages.iter().map(|message| message.body.clone()).collect()
}

/// One replay row with the fixed timestamp and no server time (`replayLine`).
pub(super) fn replay_line(author: &str, body: &str, msgid: &str) -> ReplayLine {
    ReplayLine {
        author: author.to_owned(),
        body: body.to_owned(),
        timestamp: reducer_timestamp(),
        kind: MessageKindTag::Chat,
        msgid: MsgId::new(msgid),
        server_time: None,
        only_visible_to_you: false,
    }
}

/// A replay batch the history event builds from `lines` (`HistoryEvent` literal).
pub(super) fn history_event(
    conversation: &ConversationKey,
    target: &str,
    kind: HistoryKind,
    older_page: bool,
    lines: Vec<ReplayLine>,
) -> HistoryEvent {
    HistoryEvent {
        conversation: conversation.clone(),
        target: target.to_owned(),
        lines,
        kind,
        older_page,
    }
}

/// A PRIVMSG to a conversation, with the fixed timestamp (`MessageEvent`).
pub(super) fn message(conversation: &ConversationKey, author: &str, body: &str, target: &str) -> MessageEvent {
    MessageEvent {
        conversation: conversation.clone(),
        author: author.to_owned(),
        body: body.to_owned(),
        timestamp: reducer_timestamp(),
        server_time: None,
        target: target.to_owned(),
        msgid: MsgId::default(),
        only_visible_to_you: false,
    }
}

/// A NOTICE to a conversation (`NoticeEvent`).
pub(super) fn notice(conversation: &ConversationKey, author: &str, body: &str, target: &str) -> NoticeEvent {
    NoticeEvent {
        conversation: conversation.clone(),
        author: author.to_owned(),
        body: body.to_owned(),
        timestamp: reducer_timestamp(),
        server_time: None,
        target: target.to_owned(),
        msgid: MsgId::default(),
        only_visible_to_you: false,
    }
}

/// A CTCP ACTION to a conversation (`ActionEvent`).
pub(super) fn action(conversation: &ConversationKey, author: &str, body: &str, target: &str) -> ActionEvent {
    ActionEvent {
        conversation: conversation.clone(),
        author: author.to_owned(),
        body: body.to_owned(),
        timestamp: reducer_timestamp(),
        server_time: None,
        target: target.to_owned(),
        msgid: MsgId::default(),
        only_visible_to_you: false,
    }
}

/// A JOIN (`JoinEvent`). The account and realname are unset, as a classic JOIN has them.
pub(super) fn join(network_id: &str, channel: &str, nick: &str) -> JoinEvent {
    JoinEvent {
        network_id: network_id.to_owned(),
        channel: channel.to_owned(),
        nick: nick.to_owned(),
        account: None,
        realname: None,
    }
}

/// A PART (`PartEvent`).
pub(super) fn part(network_id: &str, channel: &str, nick: &str) -> PartEvent {
    PartEvent {
        network_id: network_id.to_owned(),
        channel: channel.to_owned(),
        nick: nick.to_owned(),
        reason: String::new(),
    }
}

/// A QUIT (`QuitEvent`).
pub(super) fn quit(network_id: &str, nick: &str) -> QuitEvent {
    QuitEvent {
        network_id: network_id.to_owned(),
        nick: nick.to_owned(),
        reason: String::new(),
    }
}

/// A NICK change (`NickEvent`).
pub(super) fn nick_change(network_id: &str, old_nick: &str, new_nick: &str) -> NickEvent {
    NickEvent {
        network_id: network_id.to_owned(),
        old_nick: old_nick.to_owned(),
        new_nick: new_nick.to_owned(),
    }
}

/// A KICK (`KickEvent`).
pub(super) fn kick(network_id: &str, channel: &str, target: &str, author: &str, reason: &str) -> KickEvent {
    KickEvent {
        network_id: network_id.to_owned(),
        channel: channel.to_owned(),
        target: target.to_owned(),
        author: author.to_owned(),
        reason: reason.to_owned(),
    }
}

/// A channel MODE with its arguments (`ModeEvent`).
pub(super) fn mode(network_id: &str, target: &str, author: &str, mode: &str, arguments: &[&str]) -> ModeEvent {
    ModeEvent {
        network_id: network_id.to_owned(),
        target: target.to_owned(),
        author: author.to_owned(),
        mode: mode.to_owned(),
        arguments: arguments.iter().map(|argument| (*argument).to_owned()).collect(),
    }
}

/// A TOPIC (`TopicEvent`).
pub(super) fn topic(network_id: &str, channel: &str, text: &str, author: &str) -> TopicEvent {
    TopicEvent {
        network_id: network_id.to_owned(),
        channel: channel.to_owned(),
        topic: text.to_owned(),
        author: author.to_owned(),
        keep_existing_topic: false,
    }
}

/// A metadata value for a nick (`MemberMetadataEvent`). An empty value clears it.
pub(super) fn metadata(network_id: &str, nick: &str, key: &str, value: &str) -> MemberMetadataEvent {
    MemberMetadataEvent {
        network_id: network_id.to_owned(),
        nick: nick.to_owned(),
        key: key.to_owned(),
        value: value.to_owned(),
    }
}

/// An away-notify change (`AwayEvent`). `None` means the nick is back.
pub(super) fn away_event(network_id: &str, nick: &str, away: Option<Away>) -> AwayEvent {
    AwayEvent {
        network_id: network_id.to_owned(),
        nick: nick.to_owned(),
        away,
        realname: None,
        realname_only: false,
    }
}

/// An account-notify change (`AccountEvent`).
pub(super) fn account_event(network_id: &str, nick: &str, account: &str) -> AccountEvent {
    AccountEvent {
        network_id: network_id.to_owned(),
        nick: nick.to_owned(),
        account: account.to_owned(),
    }
}

/// A reason-bearing away record (`Away{Reason}`).
pub(super) fn away(reason: &str) -> Option<Away> {
    Some(Away {
        reason: reason.to_owned(),
    })
}

/// A transcript log two reducers share, as the Go suite passes one
/// `recordingConversationLog` to both (`recordingConversationLog`).
#[derive(Clone, Default)]
pub(super) struct RecordingLog {
    pub(super) lines: Arc<Mutex<Vec<TranscriptLine>>>,
}

impl RecordingLog {
    pub(super) fn snapshot(&self) -> Vec<TranscriptLine> {
        self.lines.lock().expect("the log lock").clone()
    }
}

impl ConversationLog for RecordingLog {
    fn append(
        &self,
        _network_id: &str,
        _target: &str,
        _mapping: crate::irc::casemapping::IrcCaseMapping,
        line: &TranscriptLine,
    ) -> bool {
        self.lines.lock().expect("the log lock").push(line.clone());
        true
    }

    fn prepend(
        &self,
        _network_id: &str,
        _target: &str,
        _mapping: crate::irc::casemapping::IrcCaseMapping,
        lines: &[TranscriptLine],
    ) -> bool {
        let mut stored = self.lines.lock().expect("the log lock");
        let tail = std::mem::take(&mut *stored);
        stored.extend(lines.iter().cloned());
        stored.extend(tail);
        true
    }

    fn read_tail(
        &self,
        _network_id: &str,
        _target: &str,
        _mapping: crate::irc::casemapping::IrcCaseMapping,
        max_lines: usize,
    ) -> Vec<TranscriptLine> {
        let stored = self.lines.lock().expect("the log lock");
        let start = stored.len().saturating_sub(max_lines);
        stored[start..].to_vec()
    }
}

/// The row with the given body (`messageByBody`).
pub(super) fn message_by_body<'a>(messages: &'a [Message], body: &str) -> &'a Message {
    messages
        .iter()
        .find(|message| message.body == body)
        .unwrap_or_else(|| panic!("missing body {body:?}"))
}
