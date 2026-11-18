//! Channel membership and channel state: JOIN, PART, KICK, QUIT and NICK; ACCOUNT and
//! AWAY; SETNAME; INVITE to another user; the user's own away state (305, 306); TOPIC with its 332 and 333 replies
//! NAMES (353, 366); and MODE. Each handler's parameter guard lives in `translate`'s
//! `match`, so these run only on a message that has the parameters they read.

use crate::irc::event::{
    AccountEvent, AwayEvent, Event, InviteEvent, JoinEvent, KickEvent, ModeEvent, Name, NamesEvent, NickEvent,
    NickFactsEvent,
    PartEvent, QuitEvent, SelfAwayEvent, TopicEvent,
};
use crate::irc::message::IrcMessage;
use crate::irc::presence::Away;
use crate::irc::wiretext::wire_text;

use super::context::Inbound;

/// `JOIN`: the channel, and the account and real name when the extended form sent them.
pub(super) fn join(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    let account = (message.params.len() >= 2).then(|| message.text_param(1));
    let realname = (message.params.len() >= 3).then(|| message.text_param(2));
    let mut events = vec![JoinEvent {
        network_id: inbound.network_id.to_owned(),
        channel: message.text_param(0),
        nick: inbound.sender.clone(),
        account,
        realname,
    }
    .into()];
    // A JOIN names the joiner's user and host in its source.
    if let Some(prefix) = message.prefix.as_ref().filter(|prefix| prefix.is_user() && !prefix.host.is_empty()) {
        let userhost = format!("{}@{}", wire_text(&prefix.user), wire_text(&prefix.host));
        events.push(userhost_fact(inbound, &inbound.sender, &userhost));
    }
    events
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
        keep_existing_topic: false,
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
        keep_existing_topic: false,
    }
    .into()]
}

/// `333` RPL_TOPICWHOTIME: the channel and who set the topic. The topic text is
/// not on the wire; `keep_existing_topic` tells the reducer not to wipe it.
pub(super) fn topic_who_time(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    vec![TopicEvent {
        network_id: inbound.network_id.to_owned(),
        channel: message.text_param(1),
        topic: String::new(),
        author: message.text_param(2),
        keep_existing_topic: true,
    }
    .into()]
}

/// `353` names reply: one batch of member tokens for the channel, not yet complete.
pub(super) fn names(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    let parsed: Vec<_> = message
        .text_param(3)
        .split(' ')
        .filter(|token| !token.is_empty())
        .filter_map(|token| inbound.features.parse_names_token(token))
        .collect();
    // With userhost-in-names a token carries `user@host`. It is a fact about the nick,
    // not part of the member row, so it rides on its own events after the names.
    let userhosts: Vec<Event> = parsed
        .iter()
        .filter(|name| !name.userhost.is_empty())
        .map(|name| userhost_fact(inbound, &name.nick, &name.userhost))
        .collect();
    let names = parsed
        .into_iter()
        .map(|name| Name {
            nick: name.nick,
            ranks: name.ranks,
        })
        .collect();
    let mut events = vec![NamesEvent {
        network_id: inbound.network_id.to_owned(),
        channel: message.text_param(2),
        names,
        complete: false,
    }
    .into()];
    events.extend(userhosts);
    events
}

/// The `user@host` of `nick` as a fact for the reducer.
pub(super) fn userhost_fact(inbound: &Inbound<'_>, nick: &str, userhost: &str) -> Event {
    NickFactsEvent {
        network_id: inbound.network_id.to_owned(),
        nick: nick.to_owned(),
        realname: None,
        server_operator: None,
        userhost: Some(userhost.to_owned()),
    }
    .into()
}

/// `CHGHOST` from a member: their new user and host.
pub(super) fn chghost(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    let userhost = format!("{}@{}", message.text_param(0), message.text_param(1));
    vec![userhost_fact(inbound, &inbound.sender, &userhost)]
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

/// `SETNAME` from a member (or from us): the new real name. A missing or empty
/// parameter is `Some("")`, which clears the stored name.
pub(super) fn setname(inbound: &Inbound<'_>) -> Vec<Event> {
    vec![NickFactsEvent {
        network_id: inbound.network_id.to_owned(),
        nick: inbound.sender.clone(),
        realname: Some(inbound.message.text_param(0)),
        server_operator: None,
        userhost: None,
    }
    .into()]
}

/// `INVITE` (with `invite-notify`) from a channel operator to another user. An invite
/// to us is no event: Status, the inbox and `/join` read it from the wire. A target
/// that is not a channel is dropped.
pub(super) fn invite(inbound: &Inbound<'_>) -> Vec<Event> {
    let message = inbound.message;
    let target = message.text_param(0);
    let channel = message.text_param(1);
    let features = inbound.features;
    if features.case_mapping().equals(&target, inbound.current_nick) || !features.is_channel(&channel) {
        return Vec::new();
    }
    vec![InviteEvent {
        network_id: inbound.network_id.to_owned(),
        channel,
        inviter: inbound.sender.clone(),
        target,
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
