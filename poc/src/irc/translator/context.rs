//! The reads every translator handler shares: the author, the conversation key, the
//! message time, the account tag, and `conversation_for`, which says which conversation
//! an inbound line belongs to. `Inbound` is the one place `translate` reads those
//! values, so each handler starts from the same facts.

use chrono::{DateTime, Utc};

use crate::irc::event::ConversationKey;
use crate::irc::message::{IrcMessage, IrcPrefix};
use crate::irc::prefixnick::prefix_nick;
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::servicenick::{is_network_notice_target, is_service_source, nick_is_routable};
use crate::irc::wiretext::wire_text;

/// Resolves the channel a sender is a member of, for channel-context delivery
/// (`IrcJoinedMemberChannel`). Returns the canonical channel name, or `None` to
/// leave the line on the direct-message path.
pub type JoinedMemberLookup<'a> = &'a dyn Fn(&str, &str) -> Option<String>;

/// What the live handlers read from one inbound message, read once in `translate`.
/// `command` is the upper-cased verb. `sender` is the author (`author`), and
/// `timestamp` is the message time with the wall-clock fallback.
pub(super) struct Inbound<'a> {
    pub(super) command: &'a str,
    pub(super) network_id: &'a str,
    pub(super) current_nick: &'a str,
    pub(super) features: &'a IrcServerFeatures,
    pub(super) message: &'a IrcMessage,
    pub(super) joined: Option<JoinedMemberLookup<'a>>,
    pub(super) sender: String,
    pub(super) timestamp: DateTime<Utc>,
    pub(super) server_time: Option<DateTime<Utc>>,
}

/// The conversation an inbound message belongs to, or `None` when it has none
/// (`ConversationFor`). A channel target is its own conversation. A direct
/// message belongs to the peer: the sender when the target is our nick, the target
/// when we sent it. Services, networks notices and pseudo-clients have none.
pub fn conversation_for(
    network_id: &str,
    target: &str,
    message: &IrcMessage,
    current_nick: &str,
    features: &IrcServerFeatures,
) -> Option<ConversationKey> {
    // A bouncer addresses its playback markers to the channel, so the sender is
    // read before the target is classified.
    if !routable_source(message) {
        return None;
    }
    if features.is_channel(target) {
        return Some(key(network_id, target, features));
    }
    if is_network_notice_target(target)
        || !message.prefix.as_ref().is_some_and(IrcPrefix::is_user)
        || is_service_source(message, features)
    {
        return None;
    }
    let sender = author(message);
    if sender.is_empty() {
        return None;
    }
    if same(features, target, current_nick) {
        return Some(key(network_id, &sender, features));
    }
    if same(features, &sender, current_nick) {
        return Some(key(network_id, target, features));
    }
    None
}

/// Qt `ISODateWithMs`, then `ISODate`, normalized to UTC (`serverTimeOf`,
/// `parseHistoryTargetTime`).
fn parse_qt_iso_time(raw: &str) -> Option<DateTime<Utc>> {
    const WITH_MS: &str = "%Y-%m-%dT%H:%M:%S%.3f%#z";
    const WITHOUT_MS: &str = "%Y-%m-%dT%H:%M:%S%#z";
    DateTime::parse_from_str(raw, WITH_MS)
        .or_else(|_| DateTime::parse_from_str(raw, WITHOUT_MS))
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

/// The IRCv3 `time` tag, parsed to UTC. `None` when the tag is absent, empty or
/// unparseable (`ircServerTimeOf`, `ServerTimeOf`).
pub fn server_time_of(message: &IrcMessage) -> Option<DateTime<Utc>> {
    let raw = message.tag_text("time")?;
    if raw.is_empty() {
        return None;
    }
    parse_qt_iso_time(&raw)
}

/// Parses the time of a `CHATHISTORY TARGETS` row, with or without `timestamp=`
/// (`parseHistoryTargetTime`).
pub fn parse_history_target_time(raw: &str) -> Option<DateTime<Utc>> {
    let raw = raw.strip_prefix("timestamp=").unwrap_or(raw);
    if raw.is_empty() {
        return None;
    }
    parse_qt_iso_time(raw)
}

/// The source nick, through the same rule every handler uses (`ircAuthor`).
pub(super) fn author(message: &IrcMessage) -> String {
    prefix_nick(message)
}

/// False when the source nick is a bouncer pseudo-client that cannot take a reply.
pub(super) fn routable_source(message: &IrcMessage) -> bool {
    match &message.prefix {
        Some(prefix) if !prefix.nick.is_empty() => nick_is_routable(&wire_text(&prefix.nick)),
        _ => true,
    }
}

/// The conversation key for `target` on `network_id` (`ircKey`).
pub(super) fn key(network_id: &str, target: &str, features: &IrcServerFeatures) -> ConversationKey {
    ConversationKey::new(network_id, features.case_mapping().normalize(target))
}

/// Whether two identifiers name the same thing under the network's case mapping.
pub(super) fn same(features: &IrcServerFeatures, left: &str, right: &str) -> bool {
    features.case_mapping().equals(left, right)
}

/// Distinguishes "the server omitted the tag" (`None`) from "the server sent it"
/// (`Some`, empty when valueless). A logout (`account=*`) and a valueless
/// `account` therefore both count as present (`ircPresentTagValue`).
pub(super) fn present_tag_value(message: &IrcMessage, name: &str) -> Option<String> {
    let tag = message.tags.iter().find(|tag| tag.name == name)?;
    Some(
        tag.value
            .as_ref()
            .map(|raw| wire_text(raw))
            .unwrap_or_default(),
    )
}

/// The message time: the parsed `time` tag, else the wall clock `now` (`timestampFor`).
pub(super) fn timestamp_for(message: &IrcMessage, now: DateTime<Utc>) -> DateTime<Utc> {
    server_time_of(message).unwrap_or(now)
}
