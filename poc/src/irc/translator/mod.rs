//! Port of `tui/internal/irc/translator.go` (Go) and `src/irc/irceventtranslator.cpp` (C++).
//!
//! Turns one inbound `IrcMessage` into the `Event`s the reducer consumes, and a
//! completed history or playback batch into one `HistoryEvent`. This is the one
//! place inbound bytes become display text after the parser: every value goes
//! through `wire_text`. The clock is a parameter. A missing or unparseable `time`
//! tag falls back to `now` and never to the wall clock.
//!
//! The batch types the session hands over (`HistoryBatch`, `HistoryTarget`) and
//! the two time readers (`server_time_of`, `parse_history_target_time`) live here
//! too, so `session/` imports them from the portable core.
//!
//! `translate` is the dispatch: one arm per command, each arm's guard the parameter
//! check its handler relies on. The handlers sit in the files beside this one:
//! `messages` (NOTICE, PRIVMSG, TAGMSG), `membership` (JOIN and the channel state),
//! `numerics` (WHO, WHOIS, metadata and join refusals), `history` (batch replay) and
//! `context` (the shared reads and the conversation an inbound line belongs to).

mod context;
mod history;
mod membership;
mod messages;
mod numerics;

pub use context::{conversation_for, parse_history_target_time, server_time_of, JoinedMemberLookup};
pub use history::{translate_history, HistoryBatch, HistoryTarget};

use chrono::{DateTime, Utc};

use crate::irc::event::{AccountEvent, Event};
use crate::irc::message::{IrcMessage, IrcPrefix};
use crate::irc::serverfeatures::IrcServerFeatures;

use context::{author, present_tag_value, timestamp_for, Inbound};

/// Translates one inbound message (`IrcEventTranslator::translate`). `now` is the
/// wall-clock fallback for a missing or unparseable `time` tag. `joined` answers
/// channel-context delivery; `None` leaves every line on its direct-message path.
pub fn translate(
    network_id: &str,
    current_nick: &str,
    features: &IrcServerFeatures,
    message: &IrcMessage,
    now: DateTime<Utc>,
    joined: Option<JoinedMemberLookup<'_>>,
) -> Vec<Event> {
    let mut events = Vec::new();
    let command = message.command.to_ascii_uppercase();
    let sender = author(message);

    // The account tag names the sender. Record it before command handling so
    // NOTICE, which produces no transcript event, still updates the nick.
    if let Some(account) = present_tag_value(message, "account") {
        if message.prefix.as_ref().is_some_and(IrcPrefix::is_user) && !sender.is_empty() {
            events.push(
                AccountEvent {
                    network_id: network_id.to_owned(),
                    nick: sender.clone(),
                    account,
                }
                .into(),
            );
        }
    }

    let inbound = Inbound {
        command: &command,
        network_id,
        current_nick,
        features,
        message,
        joined,
        sender,
        timestamp: timestamp_for(message, now),
        server_time: server_time_of(message),
    };
    events.extend(match command.as_str() {
        "NOTICE" if message.params.len() >= 2 => messages::notice(&inbound),
        "PRIVMSG" if message.params.len() >= 2 => messages::privmsg(&inbound),
        "TAGMSG" if !message.params.is_empty() => messages::tagmsg(&inbound),
        "JOIN" if !message.params.is_empty() => membership::join(&inbound),
        "ACCOUNT" if !inbound.sender.is_empty() => membership::account(&inbound),
        "330" if message.params.len() >= 3 => numerics::whois_account(&inbound),
        "PART" if !message.params.is_empty() => membership::part(&inbound),
        "QUIT" => membership::quit(&inbound),
        "NICK" if !message.params.is_empty() => membership::nick(&inbound),
        "KICK" if message.params.len() >= 2 => membership::kick(&inbound),
        "TOPIC" if message.params.len() >= 2 => membership::topic(&inbound),
        "332" if message.params.len() >= 3 => membership::topic_reply(&inbound),
        "353" if message.params.len() >= 4 => membership::names(&inbound),
        "366" if message.params.len() >= 2 => membership::names_end(&inbound),
        "MODE" if message.params.len() >= 2 => membership::mode(&inbound),
        "AWAY" if !inbound.sender.is_empty() => membership::away(&inbound),
        "306" => membership::self_away(&inbound, true),
        "305" => membership::self_away(&inbound, false),
        "352" if message.params.len() >= 7 => numerics::who_reply(&inbound),
        "311" if message.params.len() >= 6 => numerics::whois_realname(&inbound),
        "313" if message.params.len() >= 2 => numerics::whois_operator(&inbound),
        // Members store nick plus ranks. User and host are not modeled.
        "CHGHOST" => Vec::new(),
        "METADATA" => numerics::metadata(&inbound, 0),
        "761" => numerics::metadata(&inbound, 1),
        "766" => numerics::key_not_set(&inbound),
        numeric if numerics::is_join_failure(numeric) && message.params.len() >= 3 => {
            numerics::join_failure(&inbound)
        }
        _ => Vec::new(),
    });
    events
}

#[cfg(test)]
mod tests;
