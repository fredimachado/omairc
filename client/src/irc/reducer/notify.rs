//! Which surfaces an event dirtied: the reducer maps each event to the
//! `ViewNotify` set the shell repaints, reading presence and membership after the
//! fold.
//!
//! Port of `tui/internal/irc/viewnotify.go` `ClassifyViewNotify` and its helpers
//! (`classifyViewNotifyUnchecked`, `ChannelNamesSyncing`).
//! The surface presets are in `irc/viewnotify.rs`.

use crate::irc::event::{ConversationKey, Event};
use crate::irc::presence::{canonical_key, AVATAR_KEY, BOT_KEY};
use crate::irc::viewnotify::{
    chat, member_reset, member_row, membership, merge, names_complete, topic, transcript,
    typing_only, reset_all, without_member_refresh, ViewNotify,
};

use super::state::PeerPresence;
use super::EventReducer;

/// The surfaces one event dirties. An event that changes nothing visible yields an
/// empty list (`ClassifyViewNotify`). While the selected channel's NAMES burst is
/// syncing, the member and selection surfaces stay quiet.
pub fn classify_view_notify(event: &Event, reducer: &EventReducer) -> Vec<ViewNotify> {
    let notifies = classify_unchecked(event, reducer);
    if channel_names_syncing(reducer, reducer.selected()) {
        without_member_refresh(notifies)
    } else {
        notifies
    }
}

fn classify_unchecked(event: &Event, reducer: &EventReducer) -> Vec<ViewNotify> {
    match event {
        Event::Welcome(_) => reset_all(),
        Event::Message(_) | Event::Notice(_) | Event::Action(_) => chat(),
        Event::Join(_)
        | Event::Part(_)
        | Event::Quit(_)
        | Event::Nick(_)
        | Event::Kick(_)
        | Event::Mode(_) => membership(),
        Event::Topic(_) => topic(),
        Event::Names(names) => {
            if names.complete {
                names_complete()
            } else {
                Vec::new()
            }
        }
        Event::Away(away) => {
            // A WHOIS real name does not change away, so the member list stays.
            if away.realname_only {
                return Vec::new();
            }
            let key = reducer.conversation_key(&away.network_id, &away.nick);
            let mut notify = member_row(&key.normalized_target);
            // A direct message row paints this peer's presence from the same facts,
            // so the sidebar repaints when their away state lands. Skip the reload
            // when nothing can change.
            let direct_row = reducer
                .find(&key)
                .is_some_and(|conversation| !conversation.is_channel());
            let shared = reducer.peer_presence(&away.network_id, &key.normalized_target)
                != PeerPresence::Unknown;
            if direct_row || shared {
                merge(&mut notify, vec![ViewNotify::Conversations]);
            }
            notify
        }
        Event::Account(account) => {
            // Transcript headers read the account live, so a logout repaints the rows.
            // The conversation list does not.
            let key = reducer.conversation_key(&account.network_id, &account.nick);
            let mut notify = member_row(&key.normalized_target);
            merge(&mut notify, vec![ViewNotify::Messages]);
            notify
        }
        Event::MemberMetadata(metadata) => {
            let key = reducer.conversation_key(&metadata.network_id, &metadata.nick);
            let mut notify = member_row(&key.normalized_target);
            let canonical = canonical_key(&metadata.key);
            let affects_transcript_chrome =
                canonical == Some(AVATAR_KEY) || canonical == Some(BOT_KEY);
            if affects_transcript_chrome {
                merge(&mut notify, vec![ViewNotify::Messages]);
                let direct_row = reducer
                    .find(&key)
                    .is_some_and(|conversation| !conversation.is_channel());
                if direct_row {
                    merge(&mut notify, vec![ViewNotify::Conversations]);
                }
            }
            notify
        }
        // Our own away state is overlaid on our member row, so the visible
        // channel's member list repaints when it changes.
        Event::SelfAway(_) => member_reset(),
        Event::Typing(_) => typing_only(),
        Event::History(_) => {
            // A query replay batch can open the direct message it belongs to, so the
            // sidebar reloads as well as the transcript.
            let mut notify = transcript();
            merge(&mut notify, vec![ViewNotify::Conversations]);
            notify
        }
        Event::WhoisTranscript(_) | Event::ChannelError(_) => transcript(),
        Event::NickFacts(facts) => {
            let key = reducer.conversation_key(&facts.network_id, &facts.nick);
            member_row(&key.normalized_target)
        }
    }
}

/// Whether the conversation is a channel whose NAMES burst is still syncing, so
/// member refreshes stay suppressed (`ChannelNamesSyncing`).
pub fn channel_names_syncing(reducer: &EventReducer, key: Option<&ConversationKey>) -> bool {
    let Some(key) = key else {
        return false;
    };
    reducer
        .find(key)
        .and_then(|conversation| conversation.channel.as_ref())
        .is_some_and(|channel| channel.names_syncing)
}
