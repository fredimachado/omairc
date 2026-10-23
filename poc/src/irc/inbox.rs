//! Port of `tui/internal/irc/inbox.go` (Go), `src/irc/ircinbox.{h,cpp}` (C++,
//! `IrcInbox` consume and append rules) and `IrcInboxModel::labelFor` /
//! `kindName` (`src/irc/ircinboxmodel.cpp`).
//!
//! The session inbox is the waiting list of things that want the user: mentions,
//! highlights, direct messages, invites, monitored nicks coming online, and kicks.
//! It is newest first and capped at `MAX_INBOX_ITEMS`. Names compare under the
//! network's case mapping, which the caller passes in.
//!
//! Pure: the inbox holds rows and answers questions about them. The demo seed and
//! the channel join that accepting an invite triggers are model and controller
//! concerns; they stay outside this file.

use chrono::{DateTime, Utc};

use super::casemapping::IrcCaseMapping;
use super::event::MsgId;

/// The waiting-list cap (`IrcInbox::kMaxItems`).
pub const MAX_INBOX_ITEMS: usize = 50;

/// What one inbox entry is (`InboxKind`, `IrcInboxKind`). The order follows Qt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InboxKind {
    /// A channel line naming the user.
    Mention,
    /// A channel line the highlight rules matched.
    Highlight,
    /// A direct message.
    Direct,
    /// A channel invite.
    Invite,
    /// A monitor target coming online.
    MonitorOnline,
    /// A kick aimed at the user.
    Kick,
}

impl InboxKind {
    /// The stable wire name (`IrcInboxModel::kindName`).
    #[cfg(test)]
    pub fn name(self) -> &'static str {
        match self {
            InboxKind::Mention => "mention",
            InboxKind::Highlight => "highlight",
            InboxKind::Direct => "direct",
            InboxKind::Invite => "invite",
            InboxKind::MonitorOnline => "monitorOnline",
            InboxKind::Kick => "kick",
        }
    }

    /// Mention, highlight, direct and kick rows belong to a conversation and clear
    /// when it is selected (`isConversationKind`). Invites and presence rows do
    /// not.
    pub fn is_conversation_kind(self) -> bool {
        matches!(
            self,
            InboxKind::Mention | InboxKind::Highlight | InboxKind::Direct | InboxKind::Kick
        )
    }
}

/// One inbox entry (`InboxItem`, `IrcInboxItem`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InboxItem {
    pub kind: InboxKind,
    /// When the arrival was stamped (`Timestamp`).
    pub timestamp: DateTime<Utc>,
    pub network_id: String,
    pub actor: String,
    pub target: String,
    pub preview: String,
    /// The IRCv3 `msgid` of the line that raised the row, empty when none.
    pub msgid: MsgId,
}

impl InboxItem {
    /// The one-line text the row shows (`IrcInboxModel::labelFor`).
    pub fn label(&self) -> String {
        match self.kind {
            InboxKind::Mention => format!("{} mentioned you in {}", self.actor, self.target),
            InboxKind::Highlight => format!("Highlight in {}", self.target),
            InboxKind::Direct => format!("Message from {}", self.actor),
            InboxKind::Invite => format!("{} invited you to {}", self.actor, self.target),
            InboxKind::MonitorOnline => format!("{} is online", self.actor),
            InboxKind::Kick => format!("You were kicked from {}", self.target),
        }
    }
}

/// The session waiting list, newest first (`Inbox`, `IrcInbox`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Inbox {
    items: Vec<InboxItem>,
}

impl Inbox {
    /// An empty waiting list (`NewInbox`).
    pub fn new() -> Self {
        Self::default()
    }

    /// The number of waiting entries (`Count`).
    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether nothing is waiting.
    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The entry at `index`, or `None` when it is out of range (`At`).
    #[cfg(test)]
    pub fn at(&self, index: usize) -> Option<&InboxItem> {
        self.items.get(index)
    }

    /// The waiting list, newest first (`Items`).
    pub fn items(&self) -> &[InboxItem] {
        &self.items
    }

    /// Inserts `item` at the front. An invite replaces an older invite to the same
    /// channel on the same network, and a monitor-online row replaces an older one
    /// for the same nick. Both compare under `mapping`. The oldest rows past the
    /// cap are dropped (`Append`).
    pub fn append(&mut self, item: InboxItem, mapping: IrcCaseMapping) {
        let replaces = |existing: &InboxItem| {
            existing.network_id == item.network_id
                && match item.kind {
                    InboxKind::Invite => {
                        existing.kind == InboxKind::Invite
                            && mapping.equals(&existing.target, &item.target)
                    }
                    InboxKind::MonitorOnline => {
                        existing.kind == InboxKind::MonitorOnline
                            && mapping.equals(&existing.actor, &item.actor)
                    }
                    _ => false,
                }
        };
        if let Some(index) = self.items.iter().position(replaces) {
            self.items.remove(index);
        }
        self.items.insert(0, item);
        self.items.truncate(MAX_INBOX_ITEMS);
    }

    /// Removes and returns the entry at `index`, or `None` when it is out of range
    /// (`ConsumeAt`).
    pub fn remove_at(&mut self, index: usize) -> Option<InboxItem> {
        (index < self.items.len()).then(|| self.items.remove(index))
    }

    /// Removes every mention, highlight, direct or kick row for `target` on
    /// `network_id` (`ConsumeConversation`).
    pub fn consume_conversation(
        &mut self,
        network_id: &str,
        target: &str,
        mapping: IrcCaseMapping,
    ) {
        self.consume_matching(|item| {
            item.network_id == network_id
                && item.kind.is_conversation_kind()
                && mapping.equals(&item.target, target)
        });
    }

    /// Removes every invite row for `channel` on `network_id` (`ConsumeInvite`).
    pub fn consume_invite(&mut self, network_id: &str, channel: &str, mapping: IrcCaseMapping) {
        self.consume_matching(|item| {
            item.kind == InboxKind::Invite
                && item.network_id == network_id
                && mapping.equals(&item.target, channel)
        });
    }

    /// Removes every monitor-online row for `nick` on `network_id`
    /// (`ConsumeMonitor`).
    pub fn consume_monitor(&mut self, network_id: &str, nick: &str, mapping: IrcCaseMapping) {
        self.consume_matching(|item| {
            item.kind == InboxKind::MonitorOnline
                && item.network_id == network_id
                && mapping.equals(&item.actor, nick)
        });
    }

    /// Removes every row for `network_id` (`PurgeNetwork`).
    pub fn purge_network(&mut self, network_id: &str) {
        self.consume_matching(|item| item.network_id == network_id);
    }

    /// Drops every entry that matches, keeping the order of the rest.
    fn consume_matching(&mut self, matches: impl Fn(&InboxItem) -> bool) {
        self.items.retain(|item| !matches(item));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::casemapping::IrcCaseMappingKind;

    fn rfc1459() -> IrcCaseMapping {
        IrcCaseMapping::new(IrcCaseMappingKind::Rfc1459)
    }

    fn item(kind: InboxKind, network: &str, actor: &str, target: &str) -> InboxItem {
        InboxItem {
            kind,
            timestamp: DateTime::<Utc>::UNIX_EPOCH,
            network_id: network.to_owned(),
            actor: actor.to_owned(),
            target: target.to_owned(),
            preview: String::new(),
            msgid: MsgId::default(),
        }
    }

    fn direct(actor: &str) -> InboxItem {
        item(InboxKind::Direct, "", actor, "")
    }

    /// The fifty-row cap: the newest fifty stay, the oldest falls off the tail.
    #[test]
    fn the_list_keeps_the_newest_fifty() {
        let mut inbox = Inbox::new();
        for index in 0..=MAX_INBOX_ITEMS {
            inbox.append(direct(&format!("actor{index:02}")), rfc1459());
        }
        assert_eq!(inbox.len(), MAX_INBOX_ITEMS);
        assert_eq!(inbox.at(0).map(|i| i.actor.as_str()), Some("actor50"));
        assert_eq!(
            inbox.at(MAX_INBOX_ITEMS - 1).map(|i| i.actor.as_str()),
            Some("actor01"),
            "the oldest kept row"
        );
        assert_eq!(inbox.at(MAX_INBOX_ITEMS), None);
    }

    #[test]
    fn invites_dedup_by_network_and_channel_under_the_mapping() {
        let mut inbox = Inbox::new();
        inbox.append(item(InboxKind::Invite, "net", "a1", "#Chan"), rfc1459());
        inbox.append(item(InboxKind::Invite, "net", "a2", "#chan"), rfc1459());
        assert_eq!(inbox.len(), 1, "case-insensitive dedup");
        assert_eq!(
            inbox.at(0).map(|i| i.actor.as_str()),
            Some("a2"),
            "the newer row"
        );

        inbox.append(item(InboxKind::Invite, "net", "a3", "#other"), rfc1459());
        assert_eq!(inbox.len(), 2, "a different channel stays");
        inbox.append(
            item(InboxKind::Invite, "elsewhere", "a4", "#chan"),
            rfc1459(),
        );
        assert_eq!(inbox.len(), 3, "a different network stays");
    }

    #[test]
    fn monitor_rows_dedup_by_network_and_nick_under_the_mapping() {
        let mut inbox = Inbox::new();
        inbox.append(item(InboxKind::MonitorOnline, "net", "Fred", ""), rfc1459());
        inbox.append(item(InboxKind::MonitorOnline, "net", "fred", ""), rfc1459());
        assert_eq!(inbox.len(), 1);
        assert_eq!(inbox.at(0).map(|i| i.actor.as_str()), Some("fred"));

        inbox.append(
            item(InboxKind::MonitorOnline, "net", "other", ""),
            rfc1459(),
        );
        assert_eq!(inbox.len(), 2);
        inbox.append(
            item(InboxKind::MonitorOnline, "elsewhere", "fred", ""),
            rfc1459(),
        );
        assert_eq!(inbox.len(), 3);
    }

    #[test]
    fn the_mapping_decides_what_counts_as_the_same_nick() {
        // Under RFC 1459, `[` and `{` fold together; under ASCII they stay apart.
        let mut strict = Inbox::new();
        let ascii = IrcCaseMapping::new(IrcCaseMappingKind::Ascii);
        strict.append(item(InboxKind::MonitorOnline, "net", "a[b", ""), ascii);
        strict.append(item(InboxKind::MonitorOnline, "net", "a{b", ""), ascii);
        assert_eq!(strict.len(), 2);

        let mut folded = Inbox::new();
        folded.append(item(InboxKind::MonitorOnline, "net", "a[b", ""), rfc1459());
        folded.append(item(InboxKind::MonitorOnline, "net", "a{b", ""), rfc1459());
        assert_eq!(folded.len(), 1);
    }

    #[test]
    fn remove_at_drops_exactly_that_row_and_ignores_out_of_range() {
        let mut inbox = Inbox::new();
        inbox.append(direct("a"), rfc1459());
        inbox.append(direct("b"), rfc1459());
        inbox.append(direct("c"), rfc1459());

        let taken = inbox.remove_at(1).expect("row 1 exists");
        assert_eq!(taken.actor, "b");
        assert_eq!(inbox.len(), 2);
        assert_eq!(inbox.at(0).map(|i| i.actor.as_str()), Some("c"));
        assert_eq!(inbox.at(1).map(|i| i.actor.as_str()), Some("a"));

        assert!(inbox.remove_at(2).is_none());
        assert_eq!(inbox.len(), 2, "an out-of-range removal changes nothing");
    }

    #[test]
    fn a_conversation_clears_its_mentions_highlights_directs_and_kicks() {
        let mut inbox = Inbox::new();
        inbox.append(item(InboxKind::Mention, "net", "a", "#chan"), rfc1459());
        inbox.append(item(InboxKind::Highlight, "net", "b", "#CHAN"), rfc1459());
        inbox.append(item(InboxKind::Direct, "net", "bob", "bob"), rfc1459());
        inbox.append(item(InboxKind::Kick, "net", "op", "#chan"), rfc1459());
        inbox.append(item(InboxKind::Invite, "net", "op", "#chan"), rfc1459());
        inbox.append(item(InboxKind::MonitorOnline, "net", "bob", ""), rfc1459());
        inbox.append(
            item(InboxKind::Mention, "elsewhere", "c", "#chan"),
            rfc1459(),
        );

        inbox.consume_conversation("net", "#chan", rfc1459());

        assert_eq!(inbox.len(), 4);
        assert!(inbox.items().iter().any(|i| i.network_id == "elsewhere"));
        assert!(!inbox.items().iter().any(|i| i.network_id == "net"
            && i.kind.is_conversation_kind()
            && i.target.eq_ignore_ascii_case("#chan")));
    }

    #[test]
    fn an_invite_is_consumed_only_on_its_own_network_and_channel() {
        let mut inbox = Inbox::new();
        inbox.append(item(InboxKind::Invite, "net", "a", "#chan"), rfc1459());
        inbox.append(
            item(InboxKind::Invite, "elsewhere", "b", "#chan"),
            rfc1459(),
        );
        inbox.append(
            item(InboxKind::MonitorOnline, "net", "#chan", ""),
            rfc1459(),
        );

        inbox.consume_invite("net", "#CHAN", rfc1459());

        assert_eq!(inbox.len(), 2);
        assert_eq!(inbox.at(0).map(|i| i.kind), Some(InboxKind::MonitorOnline));
        assert_eq!(
            inbox.at(1).map(|i| i.network_id.as_str()),
            Some("elsewhere")
        );
    }

    #[test]
    fn a_monitor_row_is_consumed_only_for_its_own_network_and_nick() {
        let mut inbox = Inbox::new();
        inbox.append(item(InboxKind::MonitorOnline, "net", "bob", ""), rfc1459());
        inbox.append(
            item(InboxKind::MonitorOnline, "net", "carol", ""),
            rfc1459(),
        );
        inbox.append(
            item(InboxKind::MonitorOnline, "elsewhere", "bob", ""),
            rfc1459(),
        );
        inbox.append(item(InboxKind::Direct, "net", "bob", "bob"), rfc1459());

        inbox.consume_monitor("net", "BOB", rfc1459());

        assert_eq!(inbox.len(), 3);
        assert!(!inbox
            .items()
            .iter()
            .any(|i| i.kind == InboxKind::MonitorOnline
                && i.network_id == "net"
                && i.actor == "bob"));
    }

    #[test]
    fn purging_a_network_keeps_the_other_networks() {
        let mut inbox = Inbox::new();
        inbox.append(item(InboxKind::Mention, "net", "a", "#chan"), rfc1459());
        inbox.append(item(InboxKind::Invite, "net", "a", "#other"), rfc1459());
        inbox.append(
            item(InboxKind::Direct, "elsewhere", "bob", "bob"),
            rfc1459(),
        );
        inbox.append(
            item(InboxKind::MonitorOnline, "elsewhere", "carol", ""),
            rfc1459(),
        );

        inbox.purge_network("net");

        assert_eq!(inbox.len(), 2);
        assert!(inbox.items().iter().all(|i| i.network_id == "elsewhere"));
    }

    #[test]
    fn items_is_a_view_and_empty_reads_true() {
        let mut inbox = Inbox::new();
        assert!(inbox.is_empty());
        assert!(inbox.at(0).is_none());
        assert!(inbox.items().is_empty());
        inbox.append(direct("a"), rfc1459());
        let copy = inbox.items().to_vec();
        inbox.remove_at(0);
        assert_eq!(copy.len(), 1, "a copy taken earlier is unaffected");
        assert!(inbox.is_empty());
    }

    #[test]
    fn kind_names_cover_every_kind_and_match_qt() {
        let cases = [
            (InboxKind::Mention, "mention"),
            (InboxKind::Highlight, "highlight"),
            (InboxKind::Direct, "direct"),
            (InboxKind::Invite, "invite"),
            (InboxKind::MonitorOnline, "monitorOnline"),
            (InboxKind::Kick, "kick"),
        ];
        for (kind, want) in cases {
            assert_eq!(kind.name(), want);
        }
    }

    #[test]
    fn labels_match_qt_wording_for_every_kind() {
        let cases = [
            (
                item(InboxKind::Mention, "net", "alice", "#chan"),
                "alice mentioned you in #chan",
            ),
            (
                item(InboxKind::Highlight, "net", "x", "#chan"),
                "Highlight in #chan",
            ),
            (
                item(InboxKind::Direct, "net", "alice", ""),
                "Message from alice",
            ),
            (
                item(InboxKind::Invite, "net", "alice", "#chan"),
                "alice invited you to #chan",
            ),
            (
                item(InboxKind::MonitorOnline, "net", "alice", ""),
                "alice is online",
            ),
            (
                item(InboxKind::Kick, "net", "op", "#chan"),
                "You were kicked from #chan",
            ),
        ];
        for (entry, want) in cases {
            assert_eq!(entry.label(), want);
        }
    }
}
