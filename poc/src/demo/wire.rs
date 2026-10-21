//! Port of `tui/internal/demo/wire.go` (Go) and the wire part of
//! `src/irc/ircdemoserver.cpp` (C++): line assembly, client-frame parsing, the
//! registration burst, channel state, presence, accounts, transcript and typing
//! lines, and the `--lines` bench channel.
//!
//! Each builder returns IRC lines without their CRLF, so the text reads as the
//! Go and C++ does. `bytes` joins a batch into the wire bytes a transport
//! delivers. The seed is read by reference and nothing here touches a
//! transport, a session or the controller.

use std::collections::{BTreeMap, BTreeSet};

use crate::irc::message::IrcMessage;
use crate::irc::parser::parse;
use crate::irc::wiretext::wire_text;

use super::seed::{
    SeedChannel, SeedKind, SeedLine, SeedNetwork, CAPS, DEMO_SELF_AVATAR_URL, ISUPPORT,
    OMARCHY_NETWORK_ID, TODAY,
};

/// The bench channel's name (`--lines`).
pub const BENCH_CHANNEL: &str = "#bench";

/// One line as wire bytes, CRLF included (`line`).
pub fn line(text: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len() + 2);
    out.extend_from_slice(text.as_bytes());
    out.extend_from_slice(b"\r\n");
    out
}

/// Joins lines into the wire bytes a transport delivers: each line gets its CRLF.
pub fn bytes(lines: &[String]) -> Vec<u8> {
    lines.iter().flat_map(|text| line(text)).collect()
}

/// Strips one trailing CRLF, or a bare LF (`stripCrlf`).
pub fn strip_crlf(frame: &[u8]) -> &[u8] {
    if let Some(rest) = frame.strip_suffix(b"\r\n") {
        return rest;
    }
    frame.strip_suffix(b"\n").unwrap_or(frame)
}

/// Strips the CRLF and parses the line. `None` on a parse error
/// (`parseDemoFrame`).
pub fn parse_demo_frame(frame: &[u8]) -> Option<IrcMessage> {
    parse(strip_crlf(frame)).ok()
}

/// The first decoded value of the tag `name`. A valueless tag of that name does
/// not count (`demoTagValue`).
pub fn demo_tag_value(message: &IrcMessage, name: &str) -> String {
    message
        .tags
        .iter()
        .find(|tag| tag.name == name && tag.value.is_some())
        .and_then(|tag| tag.value.as_deref())
        .map(wire_text)
        .unwrap_or_default()
}

/// The IRCv3 `@label=` prefix for a labeled reply, or nothing (`demoLabelPrefix`).
pub fn demo_label_prefix(label: &str) -> String {
    if label.is_empty() {
        String::new()
    } else {
        format!("@label={label} ")
    }
}

/// One tagged PRIVMSG. The account tag comes first, then the server-time tag,
/// and only the tags present are joined with a semicolon (`privmsg`).
pub fn privmsg_line(
    nick: &str,
    target: &str,
    body: &str,
    day: &str,
    hhmm: &str,
    account: &str,
) -> String {
    let mut tags = String::new();
    if !account.is_empty() {
        tags.push_str("account=");
        tags.push_str(account);
    }
    if !day.is_empty() && !hhmm.is_empty() {
        if !tags.is_empty() {
            tags.push(';');
        }
        tags.push_str(&format!("time={day}T{hhmm}:00.000Z"));
    }
    let prefix = if tags.is_empty() {
        String::new()
    } else {
        format!("@{tags} ")
    };
    format!("{prefix}:{nick}!u@h PRIVMSG {target} :{body}")
}

/// One seed chat row, addressed to `target`.
fn chat_line(row: &SeedLine, target: &str) -> String {
    privmsg_line(row.nick, target, row.body, row.day, row.hhmm, row.account)
}

/// A JOIN. An account carrier uses the extended-join trailing form
/// (`joinLine`).
pub fn join_line(nick: &str, channel: &str, account: &str) -> String {
    if account.is_empty() {
        format!(":{nick}!u@h JOIN :{channel}")
    } else {
        format!(":{nick}!u@h JOIN {channel} {account} :{nick} Example")
    }
}

/// An account-notify ACCOUNT line (`accountLine`).
pub fn account_line(nick: &str, account: &str) -> String {
    format!(":{nick}!u@h ACCOUNT {account}")
}

/// The channel's members who are not late joiners (`initialMembers`).
pub fn initial_members(channel: &SeedChannel) -> Vec<&'static str> {
    channel
        .members
        .iter()
        .copied()
        .filter(|member| !channel.late_join.contains(member))
        .collect()
}

/// The NAMES tokens: each member, prefixed with its PREFIX symbols. A member may
/// carry more than one, as mira's `@+` does (`namesTokens`).
pub fn names_tokens(channel: &SeedChannel) -> Vec<String> {
    initial_members(channel)
        .into_iter()
        .map(
            |member| match channel.ranks.iter().find(|rank| rank.nick == member) {
                Some(rank) => format!("{}{member}", rank.value),
                None => member.to_owned(),
            },
        )
        .collect()
}

/// The CAP LS, CAP ACK, connection notice, 001 to 005 burst (`registrationBytes`).
/// The 005 gains `draft/ICON` only when the network advertises one. The seed
/// sends no 376, exactly as the Go and C++ demos do.
pub fn registration_lines(nick: &str, welcome: &str, icon_url: &str) -> Vec<String> {
    let mut isupport = ISUPPORT.to_owned();
    if !icon_url.is_empty() {
        isupport.push_str(" draft/ICON=");
        isupport.push_str(icon_url);
    }
    vec![
        format!(":server CAP {nick} LS :{CAPS}"),
        format!(":server CAP {nick} ACK :{CAPS}"),
        format!(":AUTH!AUTH@localhost NOTICE {nick} :*** Looking up your hostname..."),
        format!(":server 001 {nick} :{welcome}"),
        format!(":server 002 {nick} :Your host is demo.omairc, running version 1.0"),
        format!(":server 003 {nick} :This server was created Fri Sep 18 2026"),
        format!(":server 004 {nick} demo.omairc OmaircDemo iw abc def"),
        format!(":server 250 {nick} :Highest connection count: 42"),
        format!(":server 251 {nick} :There are 12 users and 3 invisible on this server"),
        format!(":server 252 {nick} 1 :IRC Operator online"),
        format!(":server 253 {nick} 0 :unknown connections"),
        format!(":server 254 {nick} 3 :channels formed"),
        format!(":server 255 {nick} :I have 9 clients and 1 servers"),
        format!(":server 265 {nick} 9 12 :Current local users 9, max 12"),
        format!(":server 266 {nick} 12 42 :Current global users 12, max 42"),
        format!(":server 005 {nick} {isupport} :are supported by this server"),
    ]
}

/// One channel's self JOIN, topic, NAMES and end-of-NAMES (`channelStateBytes`).
pub fn channel_state_lines(network: &SeedNetwork, channel: &SeedChannel) -> Vec<String> {
    let me = network.nick;
    vec![
        join_line(me, channel.name, ""),
        format!(":server 332 {me} {} :{}", channel.name, channel.topic),
        format!(
            ":server 353 {me} = {} :{}",
            channel.name,
            names_tokens(channel).join(" ")
        ),
        format!(":server 366 {me} {} :End of NAMES", channel.name),
    ]
}

/// The fixed WHO (352) identities, in the order the C++ seed sends them. Only
/// omarchy's own nick is a server operator.
const IDENTITIES: &[(&str, &str, bool)] = &[
    ("anna", "Anna Vale", false),
    ("dax", "Packet Bot", false),
    ("fred", "Fred Machado", true),
    ("lena", "Lena Pink", false),
    ("mira", "Mira Chen", false),
];

/// The WHO replies for the seeded identities present in `members`
/// (`identityBytes`). They land before AWAY, so a later away notice restores the
/// reason.
pub fn identity_lines(network: &SeedNetwork, members: &BTreeSet<&str>) -> Vec<String> {
    let Some(first) = network.channels.first() else {
        return Vec::new();
    };
    let omarchy = network.network_id == OMARCHY_NETWORK_ID;
    let mut out = Vec::new();
    for (nick, realname, oper_on_omarchy) in IDENTITIES {
        if !members.contains(nick) {
            continue;
        }
        let flags = if *oper_on_omarchy && omarchy {
            "H*"
        } else {
            "H"
        };
        out.push(format!(
            ":server 352 {} {} u h server {nick} {flags} :0 {realname}",
            network.nick, first.name
        ));
    }
    out
}

/// The union of every channel's away and status facts, plus the metadata lines
/// (`presenceBytes`). The C++ iterates a QSet, so the order of the away and
/// status lines is not part of the contract. This port sorts them by nick.
pub fn presence_lines(network: &SeedNetwork) -> Vec<String> {
    let mut away = BTreeSet::new();
    let mut status = BTreeMap::new();
    let mut members = BTreeSet::new();
    for channel in network.channels {
        for nick in channel.away {
            away.insert(*nick);
        }
        for entry in channel.statuses {
            status.insert(entry.nick, entry.value);
        }
        for nick in channel.members {
            members.insert(*nick);
        }
    }

    let me = network.nick;
    let mut out = identity_lines(network, &members);
    for nick in &away {
        out.push(format!(":{nick}!u@h AWAY :away"));
    }
    for (nick, value) in &status {
        out.push(format!(":server 761 {me} {nick} status * :{value}"));
    }
    if members.contains("dax") {
        out.push(format!(":server 761 {me} dax bot * :PacketBot"));
    }
    // Bundled demo art loads through the avatar store's qrc path, so the demo
    // shows real glyphs without outbound HTTPS.
    if members.contains("mira") {
        out.push(format!(
            ":server 761 {me} mira avatar * :qrc:/demo/mira-avatar.png"
        ));
    }
    if members.contains("anna") {
        out.push(format!(
            ":server 761 {me} anna avatar * :qrc:/demo/anna-avatar.png"
        ));
        out.push(format!(":server 761 {me} anna display-name * :Anna Docs"));
        out.push(format!(":server 761 {me} anna pronouns * :she/her"));
    }
    if members.contains("kai") {
        out.push(format!(
            ":server 761 {me} kai avatar * :qrc:/demo/kai-avatar.png"
        ));
    }
    // The operator's own avatar is a real HTTPS URL, unlike the bundled peer art.
    // The identity footer and the operator's transcript rows show the Omairc
    // mark, and the demo exercises the store's network path.
    out.push(format!(
        ":server 761 {me} {me} avatar * :{DEMO_SELF_AVATAR_URL}"
    ));
    out
}

/// The account lines for the omarchy network: the peers already in NAMES and the
/// self row, plus a known logout (`accountBytes`). OFTC seeds none.
pub fn account_lines(network: &SeedNetwork) -> Vec<String> {
    if network.network_id != OMARCHY_NETWORK_ID {
        return Vec::new();
    }
    vec![
        account_line("lena", "pinkieval"),
        account_line(network.nick, "fredm"),
        account_line("ivy", "*"),
    ]
}

/// The account-notify that arrives after the seeded world is up
/// (`liveAccountBytes`). Only omarchy seeds it.
pub fn live_account_lines(network: &SeedNetwork) -> Vec<String> {
    if network.network_id != OMARCHY_NETWORK_ID {
        return Vec::new();
    }
    vec![account_line("teo", "teoval")]
}

/// Everything `injectWorld` pushes for one network: each channel's state burst,
/// then the presence lines, then the account lines.
pub fn world_lines(network: &SeedNetwork) -> Vec<String> {
    let mut out = Vec::new();
    for channel in network.channels {
        out.extend(channel_state_lines(network, channel));
    }
    out.extend(presence_lines(network));
    out.extend(account_lines(network));
    out
}

/// The replayed transcript: every channel line, then every direct line. Channel
/// JOINs carry their account. Channel chats target the channel, and direct chats
/// target our own nick (`transcriptBytes`).
pub fn transcript_lines(network: &SeedNetwork) -> Vec<String> {
    let mut out = Vec::new();
    for channel in network.channels {
        for row in channel.lines {
            match row.kind {
                SeedKind::Join => out.push(join_line(row.nick, channel.name, row.account)),
                SeedKind::Chat => out.push(chat_line(row, channel.name)),
            }
        }
    }
    for direct in network.directs {
        for row in direct.lines {
            out.push(chat_line(row, network.nick));
        }
    }
    out
}

/// An active typing hint for each direct that seeds one: once into #omarchy and
/// once addressed to our own nick (`typingBytes`).
pub fn typing_lines(network: &SeedNetwork) -> Vec<String> {
    let mut out = Vec::new();
    for direct in network.directs {
        if !direct.typing {
            continue;
        }
        out.push(format!(
            "@+typing=active :{}!u@h TAGMSG #omarchy",
            direct.nick
        ));
        out.push(format!(
            "@+typing=active :{}!u@h TAGMSG {}",
            direct.nick, network.nick
        ));
    }
    out
}

/// Every seeded channel name, in seed order (`autojoinNames`).
pub fn autojoin_names(network: &SeedNetwork) -> Vec<String> {
    network
        .channels
        .iter()
        .map(|channel| channel.name.to_owned())
        .collect()
}

/// The INVITE that lands on OFTC at launch, so the demo has one inbox row without
/// any replayed history (the line `IrcDemoServer::attach` injects).
pub fn oftc_invite_line() -> String {
    ":rio!u@h INVITE oak :#debian".to_owned()
}

/// The `--lines` bench channel. It is `count` mixed lines after the channel's
/// JOIN, topic and NAMES: a join every tenth roll, and otherwise a PRIVMSG in
/// one of three shapes. The lines use the same deterministic generator as the
/// Go and Rust mock, so a run is reproducible. Day separators come from the
/// transcript's dates, and the bench world does not vary them.
pub fn bench_channel_lines(self_nick: &str, count: usize) -> Vec<String> {
    let users: Vec<String> = (0..17).map(|index| format!("user{index}")).collect();
    let mut out = vec![
        join_line(self_nick, BENCH_CHANNEL, ""),
        format!(":server 332 {self_nick} {BENCH_CHANNEL} :Generated bench channel"),
        format!(
            ":server 353 {self_nick} = {BENCH_CHANNEL} :{}",
            users.join(" ")
        ),
        format!(":server 366 {self_nick} {BENCH_CHANNEL} :End of NAMES"),
    ];
    let mut state: u32 = 0xDEAD_BEEF;
    for index in 0..count {
        state = lcg(state);
        let author = format!("user{}", index % 17);
        let hhmm = format!("{:02}:{:02}", (index / 3) % 24, (index * 7) % 60);
        if state % 10 == 1 {
            out.push(join_line(&author, BENCH_CHANNEL, ""));
            continue;
        }
        let body = if index % 4 == 0 {
            format!("\u{2}bench-{index}\u{f} with \u{3}4color\u{3} and https://example.com/{index}")
        } else if index % 5 == 0 {
            "word ".repeat(40 + (index % 20))
        } else {
            format!("bench line {index} seed {state}")
        };
        out.push(privmsg_line(
            &author,
            BENCH_CHANNEL,
            &body,
            TODAY,
            &hhmm,
            "",
        ));
    }
    out
}

/// The bench channel as wire bytes. Feed it through `DemoServer::inject` (or any
/// loopback) to seed a channel with `count` lines in place of the `--lines` world.
pub fn bench_channel_bytes(self_nick: &str, count: usize) -> Vec<u8> {
    bytes(&bench_channel_lines(self_nick, count))
}

fn lcg(state: u32) -> u32 {
    state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223)
}
