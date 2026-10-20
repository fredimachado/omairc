use serde::Deserialize;

use super::ids::ConversationRef;

static WORLD_JSON: &str = include_str!("../../mock/world.json");

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct MockWorld {
    pub networks: Vec<Network>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Network {
    pub id: String,
    pub display_name: String,
    pub nick: String,
    pub conversations: Vec<Conversation>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Conversation {
    pub name: String,
    pub channel: bool,
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub peer_realname: Option<String>,
    #[serde(default)]
    pub unread: u32,
    #[serde(default)]
    pub mention: bool,
    #[serde(default)]
    pub typing: bool,
    #[serde(default)]
    pub members: Vec<Member>,
    pub messages: Vec<MockMessage>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Member {
    pub nick: String,
    #[serde(default)]
    pub prefix: String,
    #[serde(default)]
    pub realname: String,
    #[serde(default = "default_online")]
    pub presence: String,
    #[serde(default)]
    pub bot: bool,
}

fn default_online() -> String {
    "online".to_string()
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct MockMessage {
    pub kind: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub day: String,
    #[serde(default)]
    pub body: String,
}

impl Conversation {
    pub fn is_channel(&self) -> bool {
        self.channel
    }

    pub fn key(&self) -> String {
        if self.channel {
            self.name.to_lowercase()
        } else {
            self.name.clone()
        }
    }

    /// The total behind both the `N PEOPLE` pill and the member heading.
    pub fn people_count(&self) -> usize {
        self.members.len()
    }

    /// Opening a conversation reads it: its unread count and mention flag clear.
    pub fn mark_read(&mut self) {
        self.unread = 0;
        self.mention = false;
    }
}

pub fn load_world() -> MockWorld {
    serde_json::from_str(WORLD_JSON).expect("mock/world.json must parse")
}

impl MockWorld {
    pub fn network(&self, network_id: &str) -> Option<&Network> {
        self.networks.iter().find(|n| n.id == network_id)
    }

    pub fn find(&self, target: &ConversationRef) -> Option<&Conversation> {
        self.network(&target.network_id)?
            .conversations
            .iter()
            .find(|c| c.key() == target.key)
    }

    pub fn find_mut(&mut self, target: &ConversationRef) -> Option<&mut Conversation> {
        self.networks
            .iter_mut()
            .find(|n| n.id == target.network_id)?
            .conversations
            .iter_mut()
            .find(|c| c.key() == target.key)
    }

    /// Stores a line the local user sent, so the conversation keeps it across a switch.
    pub fn record_outgoing(&mut self, target: &ConversationRef, message: MockMessage) {
        self.find_mut(target)
            .expect("record_outgoing: conversation is in the world")
            .messages
            .push(message);
    }
}

/// `/me` is an action with or without a trailing space. Returns the action text.
pub fn action_body(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("/me")?;
    if rest.is_empty() {
        return Some("");
    }
    rest.strip_prefix(' ')
}

/// The stored line for something the local user just sent.
pub fn outgoing_message(text: &str, author: &str, time: &str, day: &str) -> MockMessage {
    let (kind, body) = match action_body(text) {
        Some(body) => ("action", body),
        None => ("message", text),
    };
    MockMessage {
        kind: kind.to_string(),
        author: author.to_string(),
        time: time.to_string(),
        day: day.to_string(),
        body: body.to_string(),
    }
}

/// Local `(day, HH:MM)` for a line sent now, as Qt's `MessageListModel::displayTime` stamps it.
pub fn local_send_stamp() -> (String, String) {
    stamp_of(chrono::Local::now().fixed_offset())
}

fn stamp_of(now: chrono::DateTime<chrono::FixedOffset>) -> (String, String) {
    (
        now.format("%Y-%m-%d").to_string(),
        now.format("%H:%M").to_string(),
    )
}

const PREFIX_RANK: &[char] = &['~', '&', '@', '%', '+'];

fn prefix_priority(prefix: &str) -> usize {
    prefix
        .chars()
        .filter_map(|c| PREFIX_RANK.iter().position(|&p| p == c))
        .min()
        .unwrap_or(PREFIX_RANK.len())
}

pub fn assert_members_sorted(members: &[Member]) {
    for window in members.windows(2) {
        let left = &window[0];
        let right = &window[1];
        let lp = prefix_priority(&left.prefix);
        let rp = prefix_priority(&right.prefix);
        assert!(
            lp < rp || (lp == rp && left.nick <= right.nick),
            "members out of order: {} then {}",
            left.nick,
            right.nick
        );
    }
}

/// Deterministic `#bench` transcript with mixed row kinds for `--lines` profiling.
pub fn bench_channel(line_count: usize) -> Conversation {
    let mut messages = Vec::with_capacity(line_count);
    let mut state = 0xDEADBEEFu32;
    for index in 0..line_count {
        state = lcg(state);
        let kind_roll = state % 10;
        if index > 0 && kind_roll == 0 {
            messages.push(MockMessage {
                kind: "day".to_string(),
                author: String::new(),
                time: String::new(),
                day: String::new(),
                body: "— September 12, 2026 —".to_string(),
            });
            continue;
        }
        if kind_roll == 1 {
            messages.push(MockMessage {
                kind: "join".to_string(),
                author: format!("user{}", index % 17),
                time: format!("{:02}:{:02}", (index / 3) % 24, (index * 7) % 60),
                day: "2026-09-12".to_string(),
                body: String::new(),
            });
            continue;
        }
        let formatted = index % 4 == 0;
        let body = if formatted {
            format!(
                "\x02bench-{index}\x0f with \x034color\x03 and https://example.com/{index}"
            )
        } else if index % 5 == 0 {
            "word ".repeat(40 + (index % 20))
        } else {
            format!("bench line {index} seed {state}")
        };
        messages.push(MockMessage {
            kind: "message".to_string(),
            author: format!("user{}", index % 17),
            time: format!("{:02}:{:02}", (index / 3) % 24, (index * 7) % 60),
            day: "2026-09-12".to_string(),
            body,
        });
    }
    Conversation {
        name: "#bench".to_string(),
        channel: true,
        topic: Some("Generated bench channel".to_string()),
        peer_realname: None,
        unread: 0,
        mention: false,
        typing: false,
        members: Vec::new(),
        messages,
    }
}

fn lcg(state: u32) -> u32 {
    state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn world_loads_and_members_sorted() {
        let world = load_world();
        assert_eq!(world.networks.len(), 2);
        let names: HashSet<_> = world.networks[0]
            .conversations
            .iter()
            .map(|c| c.name.clone())
            .collect();
        assert!(names.contains("#omarchy"));
        for network in &world.networks {
            for conv in &network.conversations {
                if !conv.members.is_empty() {
                    assert_members_sorted(&conv.members);
                }
            }
        }
    }

    #[test]
    fn duplicate_omarchy_across_networks() {
        let world = load_world();
        let mut count = 0;
        for network in &world.networks {
            for conv in &network.conversations {
                if conv.name == "#omarchy" {
                    count += 1;
                }
            }
        }
        assert_eq!(count, 2);
    }

    #[test]
    fn bench_channel_is_deterministic() {
        let a = bench_channel(50);
        let b = bench_channel(50);
        assert_eq!(a.messages, b.messages);
        assert_eq!(a.messages.len(), 50);
    }

    #[test]
    fn typing_dm_present() {
        let world = load_world();
        let anna = world.networks[0]
            .conversations
            .iter()
            .find(|c| c.name == "anna")
            .expect("anna dm");
        assert!(anna.typing);
    }

    #[test]
    fn demo_seed_matches_qt_server() {
        let world = load_world();
        assert_eq!(world.networks.len(), 2);
        assert_eq!(world.networks[0].id, "omarchy");
        assert_eq!(world.networks[0].nick, "fred");
        assert_eq!(world.networks[0].display_name, "omarchy");
        assert_eq!(world.networks[1].id, "oftc");
        assert_eq!(world.networks[1].nick, "oak");
        assert_eq!(world.networks[1].display_name, "OFTC");

        let ricing = world.networks[0]
            .conversations
            .iter()
            .find(|c| c.name == "#ricing")
            .expect("#ricing");
        assert_eq!(ricing.unread, 12);
        assert_eq!(ricing.messages.len(), 12);
        assert!(ricing.mention);
        let mention = ricing
            .messages
            .iter()
            .find(|m| m.body.contains("fred"))
            .expect("mention line");
        assert_eq!(mention.body, "looks good, fred.");

        let omarchy = world.networks[0]
            .conversations
            .iter()
            .find(|c| c.name == "#omarchy")
            .expect("#omarchy");
        let mira = omarchy
            .members
            .iter()
            .find(|m| m.nick == "mira")
            .expect("mira member");
        assert_eq!(mira.prefix, "@+");

        let lab = world.networks[1]
            .conversations
            .iter()
            .find(|c| c.name == "#lab")
            .expect("#lab");
        assert!(
            lab.messages
                .iter()
                .any(|m| m.body.contains("https://ci.example.com/omairc/42"))
        );
    }

    #[test]
    fn people_pill_and_heading_share_one_total() {
        let world = load_world();
        let omarchy = world.networks[0]
            .conversations
            .iter()
            .find(|c| c.name == "#omarchy")
            .expect("#omarchy");
        // Seven of the twelve are online; the heading must still say twelve.
        assert_eq!(omarchy.people_count(), 12);
        let online = omarchy
            .members
            .iter()
            .filter(|m| m.presence == "online")
            .count();
        assert_eq!(online, 7);
    }

    #[test]
    fn mark_read_clears_unread_and_mention() {
        let mut world = load_world();
        let ricing = world.networks[0]
            .conversations
            .iter_mut()
            .find(|c| c.name == "#ricing")
            .expect("#ricing");
        assert_eq!(ricing.unread, 12);
        assert!(ricing.mention);
        ricing.mark_read();
        assert_eq!(ricing.unread, 0);
        assert!(!ricing.mention);
    }

    #[test]
    fn sent_line_survives_switching_away_and_back() {
        let mut world = load_world();
        let desktop = world.networks[0]
            .conversations
            .iter()
            .position(|c| c.name == "#desktop")
            .expect("#desktop");
        let before = world.networks[0].conversations[desktop].messages.len();
        let target = ConversationRef::new("omarchy", "#desktop");
        let sent = outgoing_message("/me waves", "fred", "21:57", "2026-10-19");
        world.record_outgoing(&target, sent);
        // Switching rebuilds the transcript from the stored messages.
        let rows = crate::model::rows_from_messages(&world.find(&target).expect("#desktop").messages);
        assert_eq!(rows.len(), before + 1);
        let last = rows.last().expect("sent row");
        assert!(last.action);
        assert_eq!(last.author.as_deref(), Some("fred"));
        assert_eq!(last.time.as_deref(), Some("21:57"));
        assert_eq!(last.body.plain, "waves");
    }

    #[test]
    fn find_resolves_by_network_and_key() {
        let mut world = load_world();
        let omarchy = world
            .find(&ConversationRef::new("omarchy", "#omarchy"))
            .expect("omarchy");
        assert_eq!(omarchy.name, "#omarchy");
        let oftc = world
            .find(&ConversationRef::new("oftc", "#omarchy"))
            .expect("oftc");
        assert_eq!(oftc.name, "#omarchy");
        assert!(world.find(&ConversationRef::new("oftc", "#nope")).is_none());
        assert!(world.find(&ConversationRef::new("nope", "#omarchy")).is_none());
        world
            .find_mut(&ConversationRef::new("omarchy", "anna"))
            .expect("anna dm")
            .mark_read();
        assert_eq!(
            world.find(&ConversationRef::new("omarchy", "anna")).expect("anna").unread,
            0
        );
    }

    #[test]
    fn me_prefix_is_an_action_with_or_without_space() {
        assert_eq!(action_body("/me waves"), Some("waves"));
        assert_eq!(action_body("/me"), Some(""));
        assert_eq!(action_body("/melon"), None);
        assert_eq!(action_body("hello /me waves"), None);
    }

    #[test]
    fn outgoing_me_is_stored_as_action_without_prefix() {
        let message = outgoing_message("/me waves", "fred", "14:07", "2026-10-19");
        assert_eq!(message.kind, "action");
        assert_eq!(message.author, "fred");
        assert_eq!(message.body, "waves");
        let plain = outgoing_message("hi there", "fred", "14:07", "2026-10-19");
        assert_eq!(plain.kind, "message");
        assert_eq!(plain.body, "hi there");
    }

    #[test]
    fn send_stamp_is_local_hh_mm_of_the_given_clock() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-10-19T09:05:42+02:00").unwrap();
        assert_eq!(
            stamp_of(now),
            ("2026-10-19".to_string(), "09:05".to_string())
        );
        let (day, time) = local_send_stamp();
        assert_eq!(day.len(), 10);
        assert_eq!(time.len(), 5);
        assert_eq!(&time[2..3], ":");
    }
}
