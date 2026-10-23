//! Channel membership and channel state: JOIN, PART, KICK, QUIT and NICK; ACCOUNT and
//! AWAY; the user's own away state (305, 306); TOPIC with its 332 reply; NAMES (353,
//! 366); and MODE. Each handler's parameter guard lives in `translate`'s `match`, so
//! these run only on a message that has the parameters they read.

use crate::irc::event::{
    AccountEvent, AwayEvent, Event, JoinEvent, KickEvent, ModeEvent, Name, NamesEvent, NickEvent,
    PartEvent, QuitEvent, SelfAwayEvent, TopicEvent,
};
use crate::irc::message::IrcMessage;
use crate::irc::presence::Away;

use super::context::Inbound;

/// `JOIN`: the channel, and the account and real name when the extended form sent them.
pub(super) fn join(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    let account = (message.params.len() >= 2).then(|| message.text_param(1));
    let realname = (message.params.len() >= 3).then(|| message.text_param(2));
    vec![JoinEvent {
        network_id: inbound.network_id.to_owned(),
        channel: message.text_param(0),
        nick: inbound.sender.clone(),
        account,
        realname,
    }
    .into()]
}

/// `ACCOUNT`: the sender's new account. An empty account means a logout.
pub(super) fn account(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    let account = if message.params.is_empty() {
        String::new()
    } else {
        message.text_param(0)
    };
    vec![AccountEvent {
        network_id: inbound.network_id.to_owned(),
        nick: inbound.sender.clone(),
        account,
    }
    .into()]
}

/// `PART`: the channel and the optional reason.
pub(super) fn part(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    vec![PartEvent {
        network_id: inbound.network_id.to_owned(),
        channel: message.text_param(0),
        nick: inbound.sender.clone(),
        reason: message.text_param(1),
    }
    .into()]
}

/// `QUIT`: the sender and the optional reason.
pub(super) fn quit(inbound: &Inbound<'_>) -> Vec<Event> {
    vec![QuitEvent {
        network_id: inbound.network_id.to_owned(),
        nick: inbound.sender.clone(),
        reason: inbound.message.text_param(0),
    }
    .into()]
}

/// `NICK`: the sender's old nick and the new one.
pub(super) fn nick(inbound: &Inbound<'_>) -> Vec<Event> {
    vec![NickEvent {
        network_id: inbound.network_id.to_owned(),
        old_nick: inbound.sender.clone(),
        new_nick: inbound.message.text_param(0),
    }
    .into()]
}

/// `KICK`: the channel, the kicked nick, the kicker and the optional reason.
pub(super) fn kick(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    vec![KickEvent {
        network_id: inbound.network_id.to_owned(),
        channel: message.text_param(0),
        target: message.text_param(1),
        author: inbound.sender.clone(),
        reason: message.text_param(2),
    }
    .into()]
}

/// `TOPIC` set by a user: the channel, the topic and the author.
pub(super) fn topic(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    vec![TopicEvent {
        network_id: inbound.network_id.to_owned(),
        channel: message.text_param(0),
        topic: message.text_param(1),
        author: inbound.sender.clone(),
    }
    .into()]
}

/// `332` topic reply: the channel and the topic, with no author.
pub(super) fn topic_reply(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    vec![TopicEvent {
        network_id: inbound.network_id.to_owned(),
        channel: message.text_param(1),
        topic: message.text_param(2),
        author: String::new(),
    }
    .into()]
}

/// `353` names reply: one batch of member tokens for the channel, not yet complete.
pub(super) fn names(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    let names = message
        .text_param(3)
        .split(' ')
        .filter(|token| !token.is_empty())
        .filter_map(|token| inbound.features.parse_names_token(token))
        .map(|parsed| Name {
            nick: parsed.nick,
            ranks: parsed.ranks,
        })
        .collect();
    vec![NamesEvent {
        network_id: inbound.network_id.to_owned(),
        channel: message.text_param(2),
        names,
        complete: false,
    }
    .into()]
}

/// `366` end of names: the channel's list is complete.
pub(super) fn names_end(inbound: &Inbound<'_>) -> Vec<Event> {
    vec![NamesEvent {
        network_id: inbound.network_id.to_owned(),
        channel: inbound.message.text_param(1),
        names: Vec::new(),
        complete: true,
    }
    .into()]
}

/// `MODE`: the target, the mode string and any arguments after it.
pub(super) fn mode(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    vec![ModeEvent {
        network_id: inbound.network_id.to_owned(),
        target: message.text_param(0),
        author: inbound.sender.clone(),
        mode: message.text_param(1),
        arguments: remaining_params(message, 2),
    }
    .into()]
}

/// `AWAY` from a member: set with a reason, or cleared when it has none.
pub(super) fn away(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    let away = (!message.params.is_empty()).then(|| Away {
        reason: message.text_param(0),
    });
    vec![AwayEvent {
        network_id: inbound.network_id.to_owned(),
        nick: inbound.sender.clone(),
        away,
        realname: None,
        realname_only: false,
    }
    .into()]
}

/// `305` and `306`: the user's own away state.
pub(super) fn self_away(inbound: &Inbound<'_>, away: bool) -> Vec<Event> {
    vec![SelfAwayEvent {
        network_id: inbound.network_id.to_owned(),
        away,
    }
    .into()]
}

/// Every parameter from `start` on, as display text (`ircRemainingParameters`).
fn remaining_params(message: &IrcMessage, start: usize) -> Vec<String> {
    (start..message.params.len())
        .map(|index| message.text_param(index))
        .collect()
}
