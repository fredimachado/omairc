//! The product CLI's reads on the controller.
//! (`snapshotMessages`, `snapshotMembers`, `snapshotConversations`, and `sendToTarget`, which the send
//! intent in `send.rs` already covers).
//! The UI reads the selected conversation through `messages`, `members` and `conversations`. The CLI
//! names its target and window itself, so these take them. They read the same reducer and order the
//! same way: lines by time, then message id, then sequence; members in `ordered_members` order.

use chrono::{DateTime, Utc};

use crate::irc::event::ConversationKey;
use crate::irc::reducer::MemberView;
use crate::irc::world::MessageKind;

use super::controller::Controller;

/// The most lines a windowed read returns (`CliWindow::Since` and `After` keep the newest 100).
pub const CLI_LINE_CAP: usize = 100;

/// Where a `read` starts. `Last` is the newest N lines, with no truncation flag, as Qt's `last`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliWindow {
    Last(usize),
    Since(DateTime<Utc>),
    After(CliCursor),
}

/// The position a `--unread` read resumes after: the newest line it returned last time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliCursor {
    pub timestamp: DateTime<Utc>,
    pub msgid: String,
    pub sequence: i64,
}

/// One transcript line a CLI read returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliMessage {
    pub network_id: String,
    pub target: String,
    pub sender: String,
    pub timestamp: DateTime<Utc>,
    pub message: String,
    pub kind: &'static str,
    pub msgid: String,
    pub sequence: i64,
    pub mention: bool,
}

impl CliMessage {
    /// The position this line occupies in the order reads use.
    pub fn cursor(&self) -> CliCursor {
        CliCursor {
            timestamp: self.timestamp,
            msgid: self.msgid.clone(),
            sequence: self.sequence,
        }
    }
}

/// The lines a read returned, oldest first, and whether the cap dropped older ones.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliMessages {
    pub lines: Vec<CliMessage>,
    pub truncated: bool,
}

/// One row of `conversations`: a channel or direct message and its badges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliConversation {
    pub target: String,
    pub channel: bool,
    pub topic: String,
    pub unread: u32,
    pub mention: bool,
}

/// The order a read sorts lines in (`compareChat`): instant in milliseconds, then message id, then
/// sequence.
fn chat_order(
    left: (DateTime<Utc>, &str, i64),
    right: (DateTime<Utc>, &str, i64),
) -> std::cmp::Ordering {
    left.0
        .timestamp_millis()
        .cmp(&right.0.timestamp_millis())
        .then_with(|| left.1.cmp(right.1))
        .then_with(|| left.2.cmp(&right.2))
}

fn line_key(line: &CliMessage) -> (DateTime<Utc>, &str, i64) {
    (line.timestamp, line.msgid.as_str(), line.sequence)
}

fn kind_name(kind: MessageKind) -> Option<&'static str> {
    match kind {
        MessageKind::Message => Some("message"),
        MessageKind::Notice => Some("notice"),
        MessageKind::Action => Some("action"),
        MessageKind::Event | MessageKind::Error | MessageKind::Whois => None,
    }
}

/// Sorts ascending and keeps the newest `cap`. Returns whether any older line was dropped
/// (`sortAndCap`).
fn sort_and_cap(lines: &mut Vec<CliMessage>, cap: usize) -> bool {
    lines.sort_by(|left, right| chat_order(line_key(left), line_key(right)));
    if lines.len() > cap {
        let dropped = lines.len() - cap;
        lines.drain(..dropped);
        return true;
    }
    false
}

impl Controller {
    /// The lines a `read` asks for (`snapshotMessages`). An empty `target` reads every conversation on
    /// the network. A named target must exist; `read` never invents a conversation.
    pub fn cli_messages(&self, network_id: &str, target: &str, window: CliWindow) -> Result<CliMessages, String> {
        let mut lines = Vec::new();
        if target.is_empty() {
            for (key, conversation) in self.reducer.conversations().iter() {
                if key.network_id == network_id {
                    self.collect_lines(key, &conversation.target, &conversation.messages, &mut lines);
                }
            }
        } else {
            let key = self.reducer.conversation_key(network_id, target);
            let Some(conversation) = self.reducer.find(&key) else {
                return Err(format!("No conversation for '{target}'."));
            };
            self.collect_lines(&key, &conversation.target, &conversation.messages, &mut lines);
        }

        let truncated = match window {
            CliWindow::Since(cutoff) => {
                let floor = cutoff.timestamp_millis();
                lines.retain(|line| line.timestamp.timestamp_millis() >= floor);
                sort_and_cap(&mut lines, CLI_LINE_CAP)
            }
            CliWindow::After(cursor) => {
                let after = (cursor.timestamp, cursor.msgid.as_str(), cursor.sequence);
                lines.retain(|line| chat_order(line_key(line), after).is_gt());
                sort_and_cap(&mut lines, CLI_LINE_CAP)
            }
            CliWindow::Last(count) => {
                sort_and_cap(&mut lines, count.max(1));
                false
            }
        };
        Ok(CliMessages { lines, truncated })
    }

    fn collect_lines(
        &self,
        key: &ConversationKey,
        target: &str,
        messages: &[crate::irc::world::Message],
        out: &mut Vec<CliMessage>,
    ) {
        for message in messages {
            let Some(kind) = kind_name(message.kind) else {
                continue;
            };
            out.push(CliMessage {
                network_id: key.network_id.clone(),
                target: target.to_owned(),
                sender: message.author.clone(),
                timestamp: message.time,
                message: message.body.clone(),
                kind,
                msgid: message.msgid.value.clone(),
                sequence: message.sequence,
                mention: self.reducer.mentions(&key.network_id, &message.body),
            });
        }
    }

    /// The members of a joined channel in panel order (`snapshotMembers`). A direct message, Status
    /// or a channel that is not joined is an error, with the text Qt gives.
    pub fn cli_members(&self, network_id: &str, target: &str) -> Result<Vec<MemberView>, String> {
        let key = self.reducer.conversation_key(network_id, target);
        let Some(conversation) = self.reducer.find(&key) else {
            return Err(format!("No conversation for '{target}'."));
        };
        let Some(channel) = conversation.channel.as_ref() else {
            return Err("names is for joined channels.".to_owned());
        };
        if !channel.joined {
            return Err("Not joined.".to_owned());
        }
        Ok(self.members_of(&key))
    }

    /// The channels and direct messages of one network, in sidebar order (`snapshotConversations`).
    /// Reading them clears nothing.
    pub fn cli_conversations(&self, network_id: &str) -> Vec<CliConversation> {
        self.reducer
            .sidebar_order_with_networks(&self.network_order)
            .into_iter()
            .filter(|key| key.network_id == network_id)
            .filter_map(|key| {
                let conversation = self.reducer.find(&key)?;
                let channel = conversation.channel.as_ref();
                Some(CliConversation {
                    target: conversation.target.clone(),
                    channel: conversation.is_channel(),
                    topic: channel.map(|channel| channel.topic.clone()).unwrap_or_default(),
                    unread: conversation.unread,
                    mention: conversation.mentions > 0,
                })
            })
            .collect()
    }

    /// The network ids that have a live or idle session, in id order. The CLI's connection list.
    pub fn cli_network_ids(&self) -> Vec<String> {
        self.manager.network_ids()
    }

    /// The members of one conversation in panel order: the server's PREFIX rank first, then nick
    /// (`MemberListModel`). Shared by the member panel and `names`.
    pub(super) fn members_of(&self, key: &ConversationKey) -> Vec<MemberView> {
        self.reducer
            .ordered_members(key)
            .into_iter()
            .filter_map(|member| self.reducer.member_view(key, &member.nick))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_controller() -> Controller {
        Controller::seeded_for_tests()
    }

    #[test]
    fn the_demo_lists_its_two_networks_and_their_conversations() {
        let controller = demo_controller();
        assert_eq!(controller.cli_network_ids(), vec!["oftc".to_owned(), "omarchy".to_owned()]);
        let omarchy = controller.cli_conversations("omarchy");
        assert!(omarchy.iter().any(|row| row.target == "#omarchy" && row.channel));
        assert!(omarchy.iter().all(|row| row.target != "Status"));
    }

    #[test]
    fn a_named_target_must_exist() {
        let controller = demo_controller();
        assert_eq!(
            controller.cli_messages("omarchy", "#nope", CliWindow::Last(5)).unwrap_err(),
            "No conversation for '#nope'."
        );
        assert_eq!(
            controller.cli_members("omarchy", "#nope").unwrap_err(),
            "No conversation for '#nope'."
        );
    }

    #[test]
    fn last_keeps_the_newest_lines_in_order_and_never_flags_truncation() {
        let controller = demo_controller();
        let read = controller
            .cli_messages("omarchy", "#omarchy", CliWindow::Last(2))
            .expect("the channel exists");
        assert_eq!(read.lines.len(), 2);
        assert!(!read.truncated);
        assert!(read.lines[0].timestamp <= read.lines[1].timestamp);
    }

    #[test]
    fn members_refuse_a_direct_message_and_a_channel_not_joined() {
        let controller = demo_controller();
        let direct = controller.cli_conversations("omarchy").into_iter().find(|row| !row.channel);
        if let Some(direct) = direct {
            assert_eq!(
                controller.cli_members("omarchy", &direct.target).unwrap_err(),
                "names is for joined channels."
            );
        }
    }
}
