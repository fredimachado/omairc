//! The reducer's use of the server read marker: a newer marker clears the unread
//! badges it covers, and the newest server-stamped line stands in for a marker
//! the server did not send.
//!
//! Port of `tui/internal/irc/readmarker.go` (Go: `coveredByReadMarker`,
//! `ApplyServerReadMarker`, `LatestServerMessageTime`,
//! `recountUnreadFromReadMarker`) and of `src/irc/irceventreducer.cpp` (C++).
//! The pure time helpers live in `irc/readmarker.rs`.

use chrono::{DateTime, Utc};

use crate::irc::event::ConversationKey;
use crate::irc::readmarker::{read_marker_time_after, read_marker_time_millis};
use crate::irc::world::{MessageKind, Origin};

use super::facts::Facts;
use super::state::ConversationState;
use super::transcript::{classify_chat_line, ChatLineReason};
use super::EventReducer;

/// Whether a line sent at `server_time` is already read under `marker`. A line
/// with no server time is never covered, so it cannot clear the badge by itself.
pub(super) fn covered(marker: Option<DateTime<Utc>>, server_time: Option<DateTime<Utc>>) -> bool {
    match (marker, server_time) {
        (Some(marker), Some(server_time)) => !read_marker_time_after(server_time, marker),
        _ => false,
    }
}

/// Whether a row's line is covered by the conversation's read marker
/// (`coveredByReadMarker`).
pub(super) fn covered_by_read_marker(
    conv: &ConversationState,
    server_time: Option<DateTime<Utc>>,
) -> bool {
    covered(conv.read_marker, server_time)
}

/// The newest chat line's server time, read at millisecond precision, or `None`
/// when no chat line carries one (`LatestServerMessageTime`). A wall-clock
/// fallback never counts, so a line without a server time cannot advance the
/// marker.
pub fn latest_server_message_time(conv: &ConversationState) -> Option<DateTime<Utc>> {
    conv.messages.iter().rev().find_map(|message| {
        let server_time = message.server_time?;
        matches!(
            message.kind,
            MessageKind::Message | MessageKind::Action | MessageKind::Notice
        )
        .then(|| read_marker_time_millis(server_time))
    })
}

/// Recounts unread and mention badges from the start of the transcript, under
/// the stored read marker (`recountUnreadFromReadMarker`).
fn recount_unread_from_read_marker(facts: &Facts, conv: &mut ConversationState, key: &ConversationKey) {
    conv.unread = 0;
    conv.mentions = 0;
    conv.unread_mark = None;
    let marker = conv.read_marker;
    let muted = conv.muted;
    let is_channel = conv.is_channel();
    let selected = facts.selected.as_ref() == Some(key);
    let network = key.network_id.as_str();
    for message in &conv.messages {
        if !matches!(
            message.kind,
            MessageKind::Message | MessageKind::Action | MessageKind::Notice
        ) {
            continue;
        }
        if facts.is_self(network, &message.author) {
            continue;
        }
        if covered(marker, message.server_time) {
            continue;
        }
        let nick_hit = facts.is_nick_mention(network, &message.body);
        let highlight_hit = !nick_hit && facts.is_highlight_hit(network, &message.body);
        let (reason, has_reason) =
            classify_chat_line(is_channel, message.kind, false, nick_hit, highlight_hit);
        let live = message.origin == Origin::Live;
        let skip_live_focused = selected && live && !facts.window_inactive;
        let skip_replay_unfocused = selected && !live && facts.window_inactive;
        if skip_live_focused || skip_replay_unfocused {
            continue;
        }
        let hold_focused_replay_mark = selected && !live && conv.unread_mark.is_some();
        if conv.unread == 0 && !hold_focused_replay_mark {
            conv.unread_mark = Some(message.sequence);
        }
        if selected && !live {
            continue;
        }
        conv.unread += 1;
        if has_reason
            && matches!(
                reason,
                ChatLineReason::NickMention | ChatLineReason::Highlight
            )
            && !muted
        {
            conv.mentions += 1;
        }
    }
}

impl EventReducer {
    /// Stores a newer server read marker and recounts the badges it covers. A
    /// marker that is not newer, or `None` (the server's `*` sentinel), leaves
    /// state alone (`ApplyServerReadMarker`).
    pub fn apply_server_read_marker(
        &mut self,
        key: &ConversationKey,
        marker: Option<DateTime<Utc>>,
    ) -> bool {
        let Some(marker) = marker else {
            return false;
        };
        let marker = read_marker_time_millis(marker);
        let Some(conv) = self.conversations.get_mut(key) else {
            return false;
        };
        if let Some(current) = conv.read_marker {
            if !read_marker_time_after(marker, current) {
                return false;
            }
        }
        conv.read_marker = Some(marker);
        recount_unread_from_read_marker(&self.facts, conv, key);
        true
    }
}
