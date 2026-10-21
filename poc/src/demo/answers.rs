//! Port of `tui/internal/demo/answers.go` (Go) and the anonymous auto-echo
//! handlers in `src/irc/ircdemoserver.cpp` (C++): the replies the demo peer
//! sends for the client's own frames. The server answers PING, LIST, CTCP
//! PING/TIME/VERSION, AWAY, METADATA SET, MONITOR, the PART, NICK and TOPIC
//! echoes, and the JOIN and PRIVMSG/NOTICE echoes. It answers nothing else. The
//! PART, NICK and TOPIC echoes carry the sender's own `!u@h` prefix, as a real
//! server sends them. The C++ and Go demos use the same shapes.
//!
//! Each `answer_*` returns `None` when the frame is not its own. `Some` means it
//! was handled, and the reply may be empty: a MONITOR C or a `-` removal answers
//! with nothing, and the caller must still treat that as handled. The Go
//! `tryAnswer*` ladder uses the same convention.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Local, Utc};

use crate::irc::ctcp::{ctcp_payload, ctcp_version_reply_text, parse_ctcp_request, CtcpRequest};
use crate::irc::presence::{canonical_key, clamped_to, MAXIMUM_VALUE_BYTES};
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::wiretext::wire_text;

use super::seed::{channel_named, SeedListed, SeedNetwork, ISUPPORT, MONITOR_LIMIT};
use super::wire::{
    demo_label_prefix, demo_tag_value, join_line, line, parse_demo_frame, strip_crlf,
};

/// The topics the client has set on the demo, keyed by the channel name lower-cased.
/// A topic set here replaces the seeded topic that a later self JOIN restores.
pub type TopicOverrides = BTreeMap<String, String>;

/// The features the demo advertises, built from the ISUPPORT tokens the same way
/// the session builds them. Nothing here hard-codes CHANTYPES or PREFIX
/// (`demoServerFeatures`).
pub fn demo_server_features() -> IrcServerFeatures {
    let mut features = IrcServerFeatures::new();
    let tokens: Vec<&str> = ISUPPORT
        .split(' ')
        .filter(|token| !token.is_empty())
        .collect();
    features.apply_tokens(&tokens);
    features
}

/// Answers a client PING with a PONG that echoes the token (`tryAnswerPing`).
pub fn answer_ping(frame: &[u8]) -> Option<Vec<u8>> {
    if !strip_crlf(frame).starts_with(b"PING ") {
        return None;
    }
    let message = parse_demo_frame(frame)?;
    if message.command != "PING" || message.params.is_empty() {
        return None;
    }
    let token = wire_text(message.params.last()?);
    Some(line(&format!(":server PONG irc.example :{token}")))
}

/// Whether a `/list` mask matches `name`. The mask is comma-separated, an empty
/// mask matches everything, and each part is either a case-insensitive glob
/// (`*` and `?`) or an exact match (`listMaskMatches`).
pub fn list_mask_matches(name: &str, mask: &str) -> bool {
    if mask.is_empty() {
        return true;
    }
    let parts: Vec<&str> = mask.split(',').filter(|part| !part.is_empty()).collect();
    if parts.is_empty() {
        return true;
    }
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if part.contains(['*', '?']) {
            if wildcard_matches(
                part.to_lowercase().as_bytes(),
                name.to_lowercase().as_bytes(),
            ) {
                return true;
            }
        } else if name.to_lowercase() == part.to_lowercase() {
            return true;
        }
    }
    false
}

/// A case-folded glob over bytes, for `*` and `?`. Callers lower both inputs
/// first (`wildcardMatches`).
fn wildcard_matches(pattern: &[u8], text: &[u8]) -> bool {
    let (mut pattern_index, mut text_index) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while text_index < text.len() {
        if pattern_index < pattern.len()
            && (pattern[pattern_index] == b'?' || pattern[pattern_index] == text[text_index])
        {
            pattern_index += 1;
            text_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star = Some(pattern_index);
            mark = text_index;
            pattern_index += 1;
        } else if let Some(star_index) = star {
            pattern_index = star_index + 1;
            mark += 1;
            text_index = mark;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

/// The `/list` source: seeded channels first, then the directory (`listEntries`).
/// A channel's user count is its member count.
pub fn list_entries(network: &SeedNetwork) -> Vec<SeedListed> {
    let mut rows: Vec<SeedListed> = network
        .channels
        .iter()
        .map(|channel| SeedListed {
            name: channel.name,
            users: channel.members.len(),
            topic: channel.topic,
        })
        .collect();
    rows.extend(network.directory.iter().copied());
    rows
}

/// Answers `LIST` or `LIST <mask>` with 321, one 322 per match, and 323
/// (`tryAnswerList`).
pub fn answer_list(network: &SeedNetwork, self_nick: &str, frame: &[u8]) -> Option<Vec<u8>> {
    let wire = strip_crlf(frame);
    if wire != b"LIST" && !wire.starts_with(b"LIST ") {
        return None;
    }
    let mask = if wire.len() > 5 {
        wire_text(&wire[5..]).trim().to_owned()
    } else {
        String::new()
    };

    let me = self_nick;
    let mut out = line(&format!(":server 321 {me} Channel :Users  Name"));
    for entry in list_entries(network) {
        if !list_mask_matches(entry.name, &mask) {
            continue;
        }
        out.extend(line(&format!(
            ":server 322 {me} {} {} :{}",
            entry.name, entry.users, entry.topic
        )));
    }
    out.extend(line(&format!(":server 323 {me} :End of /LIST")));
    Some(out)
}

/// Answers a CTCP query sent to our nick with a NOTICE from the queried nick. A
/// PING echoes its argument, TIME reports the local time in RFC 1123 form, and
/// VERSION returns the desktop reply. Channel targets, ACTION and unknown
/// commands are not answered (`tryAnswerCtcp`). `now` is the clock reading for
/// TIME.
pub fn answer_ctcp(self_nick: &str, frame: &[u8], now: DateTime<Utc>) -> Option<Vec<u8>> {
    if self_nick.is_empty() {
        return None;
    }
    let message = parse_demo_frame(frame)?;
    if message.command != "PRIVMSG" || message.params.len() < 2 {
        return None;
    }
    let request = parse_ctcp_request(&wire_text(&message.params[1]))?;
    if request.command == "ACTION" {
        return None;
    }
    let target = wire_text(&message.params[0]);
    if demo_server_features().is_channel(&target) {
        return None;
    }

    let argument = match request.command.as_str() {
        "PING" => request.argument.clone(),
        "TIME" => now
            .with_timezone(&Local)
            .format("%a, %d %b %Y %H:%M:%S %z")
            .to_string(),
        "VERSION" => ctcp_version_reply_text().to_owned(),
        _ => return None,
    };
    if target.is_empty() {
        return None;
    }

    let reply = CtcpRequest {
        command: request.command,
        argument,
    };
    let text = format!(
        "{}:{target}!u@h NOTICE {self_nick} :{}",
        demo_label_prefix(&demo_tag_value(&message, "label")),
        ctcp_payload(&reply),
    );
    Some(line(&text))
}

/// Answers our own AWAY with 306 when a reason is given, or 305 when it is not
/// (`tryAnswerAway`).
pub fn answer_away(self_nick: &str, frame: &[u8]) -> Option<Vec<u8>> {
    if self_nick.is_empty() {
        return None;
    }
    let wire = strip_crlf(frame);
    if wire != b"AWAY" && !wire.starts_with(b"AWAY :") {
        return None;
    }
    if wire.len() > "AWAY".len() {
        Some(line(&format!(
            ":server 306 {self_nick} :You have been marked as being away"
        )))
    } else {
        Some(line(&format!(
            ":server 305 {self_nick} :You are no longer marked as being away"
        )))
    }
}

/// Answers `METADATA <self or *> SET <key> [value]` with 761, or 766 when the
/// value is empty (`tryAnswerMetadata`). The key is canonicalized, and an
/// unknown key is not answered.
pub fn answer_metadata(self_nick: &str, frame: &[u8]) -> Option<Vec<u8>> {
    if self_nick.is_empty() || !strip_crlf(frame).starts_with(b"METADATA ") {
        return None;
    }
    let message = parse_demo_frame(frame)?;
    if message.command != "METADATA" || message.params.len() < 3 {
        return None;
    }
    let target = wire_text(&message.params[0]);
    if target != "*" && !target.eq_ignore_ascii_case(self_nick) {
        return None;
    }
    if !wire_text(&message.params[1]).eq_ignore_ascii_case("SET") {
        return None;
    }
    let stored = canonical_key(&wire_text(&message.params[2]))?;
    let value = if message.params.len() >= 4 {
        clamped_to(&wire_text(&message.params[3]), MAXIMUM_VALUE_BYTES)
    } else {
        String::new()
    };
    if value.is_empty() {
        return Some(line(&format!(
            ":server 766 {self_nick} {self_nick} {stored} :unset"
        )));
    }
    Some(line(&format!(
        ":server 761 {self_nick} {self_nick} {stored} * :{value}"
    )))
}

/// Answers MONITOR as the full state machine: C clears the list, L lists it, S
/// re-reports it, and `+` or `-` adds or removes nicks. An add past the ceiling
/// is refused with 734. `watched` is the network's list, and this updates it
/// (`tryAnswerMonitor`).
pub fn answer_monitor(
    self_nick: &str,
    frame: &[u8],
    online: &[String],
    watched: &mut Vec<String>,
) -> Option<Vec<u8>> {
    if self_nick.is_empty() || !frame.starts_with(b"MONITOR ") {
        return None;
    }
    let message = parse_demo_frame(frame)?;
    if message.command != "MONITOR" || message.params.is_empty() {
        return None;
    }

    let mut modifier = wire_text(&message.params[0]);
    let mut target_text = String::new();
    let bytes = modifier.as_bytes();
    if bytes.len() > 1 && (bytes[0] == b'+' || bytes[0] == b'-') {
        target_text = modifier[1..].to_owned();
        modifier.truncate(1);
    } else if message.params.len() >= 2 {
        target_text = wire_text(&message.params[1]);
    }
    let modifier = modifier.to_uppercase();

    match modifier.as_str() {
        "C" => {
            watched.clear();
            return Some(Vec::new());
        }
        "L" => {
            let mut out = Vec::new();
            if !watched.is_empty() {
                out.extend(monitor_numeric(self_nick, "732", &watched.join(",")));
            }
            out.extend(line(&format!(
                ":server 733 {self_nick} :End of MONITOR list"
            )));
            return Some(out);
        }
        "S" => return Some(monitor_states(self_nick, watched, online)),
        "+" | "-" => {}
        _ => return None,
    }

    let targets: Vec<String> = target_text
        .split(',')
        .filter(|nick| !nick.is_empty())
        .map(str::to_owned)
        .collect();
    if modifier == "-" {
        for nick in &targets {
            if let Some(index) = monitor_index(watched, nick) {
                watched.remove(index);
            }
        }
        return Some(Vec::new());
    }

    let mut added: Vec<String> = Vec::new();
    let mut overflow: Vec<String> = Vec::new();
    for nick in targets {
        if monitor_index(watched, &nick).is_some() || monitor_index(&added, &nick).is_some() {
            continue;
        }
        if watched.len() + added.len() >= MONITOR_LIMIT {
            overflow.push(nick);
        } else {
            added.push(nick);
        }
    }
    watched.extend(added.iter().cloned());

    let mut out = Vec::new();
    if !added.is_empty() {
        out.extend(monitor_states(self_nick, &added, online));
    }
    if !overflow.is_empty() {
        out.extend(line(&format!(
            ":server 734 {self_nick} {MONITOR_LIMIT} {} :Monitor list is full.",
            overflow.join(",")
        )));
    }
    Some(out)
}

/// One 730 to 734 numeric whose trailing text is `trailing` (`injectMonitorNumeric`).
fn monitor_numeric(self_nick: &str, code: &str, trailing: &str) -> Vec<u8> {
    line(&format!(":server {code} {self_nick} :{trailing}"))
}

/// Splits `nicks` into online (730, each with a `!u@h` suffix) and offline (731)
/// batches (`injectMonitorStates`).
fn monitor_states(self_nick: &str, nicks: &[String], online: &[String]) -> Vec<u8> {
    let mut on: Vec<String> = Vec::new();
    let mut off: Vec<String> = Vec::new();
    for nick in nicks {
        if monitor_index(online, nick).is_some() {
            on.push(format!("{nick}!u@h"));
        } else {
            off.push(nick.clone());
        }
    }
    let mut out = Vec::new();
    if !on.is_empty() {
        out.extend(monitor_numeric(self_nick, "730", &on.join(",")));
    }
    if !off.is_empty() {
        out.extend(monitor_numeric(self_nick, "731", &off.join(",")));
    }
    out
}

/// The watched index of `nick`, compared case-insensitively (`demoMonitorIndex`).
fn monitor_index(nicks: &[String], nick: &str) -> Option<usize> {
    nicks
        .iter()
        .position(|candidate| candidate.eq_ignore_ascii_case(nick))
}

/// Wraps a client frame as if the server echoed it back under our nick
/// (`injectClientEcho`). The frame keeps its own CRLF.
pub fn client_echo(self_nick: &str, frame: &[u8]) -> Vec<u8> {
    let mut out = format!(":{self_nick}!u@h ").into_bytes();
    out.extend_from_slice(frame);
    out
}

/// Answers a client PART on a channel with the server's echo, `:<nick>!u@h PART #chan`
/// with the reason kept as the client sent it (`tryAnswerPart`). A PART to a target that
/// is not a channel is not answered.
pub fn answer_part(self_nick: &str, frame: &[u8]) -> Option<Vec<u8>> {
    if self_nick.is_empty() || !strip_crlf(frame).starts_with(b"PART ") {
        return None;
    }
    let message = parse_demo_frame(frame)?;
    if message.command != "PART" {
        return None;
    }
    let channel = wire_text(message.params.first()?);
    if !demo_server_features().is_channel(&channel) {
        return None;
    }
    Some(client_echo(self_nick, frame))
}

/// Answers a client NICK with the echo `:<old>!u@h NICK :<new>`, then makes `new` the
/// nick the demo answers as (`tryAnswerNick`). `current` holds that nick and is updated
/// here, so every later answer and echo uses the new one.
pub fn answer_nick(current: &mut String, frame: &[u8]) -> Option<Vec<u8>> {
    if current.is_empty() || !strip_crlf(frame).starts_with(b"NICK ") {
        return None;
    }
    let message = parse_demo_frame(frame)?;
    if message.command != "NICK" || message.params.len() != 1 {
        return None;
    }
    let new_nick = wire_text(&message.params[0]);
    if new_nick.is_empty() || new_nick.contains(' ') {
        return None;
    }
    let echo = line(&format!(":{current}!u@h NICK :{new_nick}"));
    *current = new_nick;
    Some(echo)
}

/// Answers a client TOPIC that sets a channel's topic with the echo
/// `:<self>!u@h TOPIC #chan :<text>`. The text becomes the topic a later self JOIN
/// restores (`tryAnswerTopic`). A TOPIC query, or one on a non-channel target, is not
/// answered.
pub fn answer_topic(
    self_nick: &str,
    topics: &mut TopicOverrides,
    frame: &[u8],
) -> Option<Vec<u8>> {
    if self_nick.is_empty() || !strip_crlf(frame).starts_with(b"TOPIC ") {
        return None;
    }
    let message = parse_demo_frame(frame)?;
    if message.command != "TOPIC" || message.params.len() < 2 {
        return None;
    }
    let channel = wire_text(&message.params[0]);
    if !demo_server_features().is_channel(&channel) {
        return None;
    }
    let text = wire_text(&message.params[1]);
    topics.insert(channel.to_ascii_lowercase(), text.clone());
    Some(line(&format!(":{self_nick}!u@h TOPIC {channel} :{text}")))
}

/// The topic a self JOIN restores for `name`: a topic the client set this session, else
/// the seeded topic of a seeded channel. Empty means no 332 line.
fn restored_topic(network: &SeedNetwork, topics: &TopicOverrides, name: &str) -> Option<String> {
    match topics.get(&name.to_ascii_lowercase()) {
        Some(topic) => Some(topic.clone()),
        None => channel_named(network, name).map(|channel| channel.topic.to_owned()),
    }
}

/// Answers a client JOIN with one self JOIN per channel, each followed by the topic it
/// restores (`injectSelfJoin`). The topic restores the header after a PART and a rejoin.
/// No fresh NAMES is sent, so the member column keeps the nick that just joined.
pub fn self_join(
    network: &SeedNetwork,
    self_nick: &str,
    topics: &TopicOverrides,
    frame: &[u8],
) -> Vec<u8> {
    let Some(rest) = frame.get(5..) else {
        return Vec::new();
    };
    let mut end = rest.len();
    while end > 0 && (rest[end - 1] == b'\r' || rest[end - 1] == b'\n') {
        end -= 1;
    }
    let rest = &rest[..end];
    if rest.is_empty() {
        return Vec::new();
    }
    let channel_field = match rest.iter().position(|&byte| byte == b' ') {
        Some(space) => &rest[..space],
        None => rest,
    };

    let mut out = Vec::new();
    for raw in channel_field.split(|&byte| byte == b',') {
        if raw.is_empty() {
            continue;
        }
        let name = wire_text(raw);
        out.extend(line(&join_line(self_nick, &name, "")));
        if let Some(topic) = restored_topic(network, topics, &name) {
            if !topic.is_empty() {
                out.extend(line(&format!(":server 332 {self_nick} {name} :{topic}")));
            }
        }
    }
    out
}

/// Every nick a MONITOR reply may call online: the nick the session answers as, each
/// channel member, and each direct peer, sorted and unique (`demoOnlineNicks`). NICK
/// may have changed the session's nick from the seeded one.
pub fn demo_online_nicks(network: &SeedNetwork, self_nick: &str) -> Vec<String> {
    let mut nicks = BTreeSet::new();
    nicks.insert(self_nick.to_owned());
    for channel in network.channels {
        for nick in channel.members {
            // The seeded nick is no longer online once NICK has renamed the session.
            if self_nick != network.nick && nick.eq_ignore_ascii_case(network.nick) {
                continue;
            }
            nicks.insert((*nick).to_owned());
        }
    }
    for direct in network.directs {
        nicks.insert(direct.nick.to_owned());
    }
    nicks.into_iter().collect()
}
