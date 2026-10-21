//! The conversation store as the controller uses it: creating a row under its
//! cause, dropping and closing rows, the selection and unread accounting, the
//! network-wide forgets, and the member and typing reads the panel draws from.
//!
//! Port of `tui/internal/irc/reducer.go` (Go: `EnsureConversation`, `DropDirectMessage`,
//! `DropChannel`, `NoteClosed`, `ClearClosed`, `Closed`, `MarkChannelLeft`,
//! `ForgetNetwork`, `ClearMessages`, `SetMuted`, `MarkSelected`, `MarkRead`,
//! `MarkAllRead`, `MemberView`, `NickPresence`, `DisplayAccount`,
//! `MeaningfulRealname`, `PeerFactLabels`, `PeerPresence`, `OrderedMembers`,
//! `TypingNicks`, `DirectPeerIsTyping`, `ClearTypingFacts`, `ClearPresenceFacts`,
//! `ReleaseStaleNamesSync`, `JoinedMemberChannel`, `SetServerFeatures`,
//! `SetHighlightWords`) and of `src/irc/irceventreducer.cpp` (C++).

use chrono::{DateTime, TimeDelta, Utc};

use crate::irc::conversation::target_looks_like_service;
use crate::irc::event::{ConversationKey, InboxArrival, IrcConversationCause, MentionArrival};
use crate::irc::membershipnoise::MembershipNoise;
use crate::irc::presence::{
    self, Away, NickPresence, COLOR_KEY, DISPLAY_NAME_KEY, HOMEPAGE_KEY, PRONOUNS_KEY,
};
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::typing::typing_shows_indicator;
use crate::irc::world::MessageKind;

use super::facts::is_visible;
use super::log::ConversationLog;
use super::state::{
    ChannelState, ConversationState, MemberView, OrderedMember, PeerPresence, STALE_NAMES_SYNC_MS,
};
use super::EventReducer;

/// Marks a channel's NAMES sync as running from `now` (`startNamesSync`). It
/// clears the members, which the sync refills.
pub(super) fn start_names_sync(channel: &mut ChannelState, now: DateTime<Utc>) {
    channel.members.clear();
    channel.names_syncing = true;
    channel.names_sync_started = Some(now);
}

/// Clears the NAMES sync flag and its clock (`stopNamesSync`).
pub(super) fn stop_names_sync(channel: &mut ChannelState) {
    channel.names_syncing = false;
    channel.names_sync_started = None;
}

impl EventReducer {
    /// Creates the conversation for `key` when `cause` may insert one, and
    /// reports whether it exists afterwards. An existing conversation always
    /// wins, before the cause is consulted. A closed channel stays closed until
    /// an explicit join clears its mark (`ensureConversation`).
    pub(super) fn ensure_key(
        &mut self,
        key: &ConversationKey,
        display_target: &str,
        cause: IrcConversationCause,
    ) -> bool {
        if self.conversations.contains_key(key) {
            return true;
        }
        let features = self.facts.features(&key.network_id);
        let target_is_channel = features.is_channel(&key.normalized_target);
        let looks_like_service = target_looks_like_service(display_target, features);
        if !cause.inserts(target_is_channel, looks_like_service) {
            return false;
        }
        if target_is_channel && (self.facts.is_closed(key) || self.facts.persisted_closed(key)) {
            return false;
        }
        let mut conv = ConversationState::new(key.clone(), display_target);
        conv.muted = self.facts.muted_keys.contains(key);
        if target_is_channel {
            conv.channel = Some(ChannelState::default());
        }
        super::transcript::hydrate_from_log(&self.facts, &mut conv);
        self.conversations.insert(key.clone(), conv);
        true
    }

    /// Creates the conversation when `cause` allows it, and returns it
    /// (`EnsureConversation`).
    pub fn ensure_conversation(
        &mut self,
        key: &ConversationKey,
        display_target: &str,
        cause: IrcConversationCause,
    ) -> Option<&ConversationState> {
        if self.ensure_key(key, display_target, cause) {
            self.conversations.get(key)
        } else {
            None
        }
    }

    /// The conversation for a network and target, normalized under the network's
    /// mapping (`ConversationKey`).
    pub fn conversation_key(&self, network_id: &str, target: &str) -> ConversationKey {
        ConversationKey::new(network_id, self.facts.normalize(network_id, target))
    }

    /// The conversation for a key, if it exists (`Find`).
    pub fn find(&self, key: &ConversationKey) -> Option<&ConversationState> {
        self.conversations.get(key)
    }

    /// Every conversation, keyed by network then normalized target
    /// (`Conversations`).
    pub fn conversations(&self) -> &super::state::Store {
        &self.conversations
    }

    /// Removes a direct message. Refuses a channel or a missing row
    /// (`DropDirectMessage`).
    pub fn drop_direct_message(&mut self, key: &ConversationKey) -> bool {
        let Some(conv) = self.conversations.get(key) else {
            return false;
        };
        if conv.is_channel() {
            return false;
        }
        self.remove_conversation(key);
        true
    }

    /// Removes a channel. Refuses a direct message or a missing row
    /// (`DropChannel`).
    pub fn drop_channel(&mut self, key: &ConversationKey) -> bool {
        let Some(conv) = self.conversations.get(key) else {
            return false;
        };
        if !conv.is_channel() {
            return false;
        }
        self.remove_conversation(key);
        true
    }

    fn remove_conversation(&mut self, key: &ConversationKey) {
        if self.facts.selected.as_ref() == Some(key) {
            self.facts.selected = None;
        }
        self.conversations.remove(key);
    }

    /// Wires the mark that survives a welcome: `persist` records or forgets a
    /// key, and `contains` reports a mark welcome already dropped from memory
    /// (`SetClosedPersistence`).
    pub fn set_closed_persistence(
        &mut self,
        persist: impl Fn(&ConversationKey, bool) + Send + 'static,
        contains: impl Fn(&ConversationKey) -> bool + Send + 'static,
    ) {
        self.facts.persist_closed = Some(Box::new(persist));
        self.facts.contains_closed = Some(Box::new(contains));
    }

    /// Records that the user closed this channel, so catch-up cannot bring the row
    /// back (`NoteClosed`).
    pub fn note_closed(&mut self, key: &ConversationKey) {
        if key.network_id.is_empty() || key.normalized_target.is_empty() {
            return;
        }
        self.facts.closed_channels.insert(key.clone());
        if let Some(persist) = &self.facts.persist_closed {
            persist(key, true);
        }
    }

    /// Forgets a closed channel so an explicit join may open it (`ClearClosed`).
    pub fn clear_closed(&mut self, key: &ConversationKey) {
        self.facts.closed_channels.remove(key);
        if let Some(persist) = &self.facts.persist_closed {
            persist(key, false);
        }
    }

    /// Whether a channel was closed and not joined again (`Closed`).
    pub fn closed(&self, key: &ConversationKey) -> bool {
        self.facts.is_closed(key)
    }

    /// Drops every closed channel on one network.
    pub(super) fn clear_closed_network(&mut self, network_id: &str) {
        if network_id.is_empty() {
            return;
        }
        self.facts
            .closed_channels
            .retain(|key| key.network_id != network_id);
    }

    /// Keeps the channel row and marks it not joined. The PART echo still appends
    /// the leave line (`MarkChannelLeft`).
    pub fn mark_channel_left(&mut self, key: &ConversationKey) -> bool {
        let Some(conv) = self.conversations.get_mut(key) else {
            return false;
        };
        let Some(channel) = conv.channel.as_mut() else {
            return false;
        };
        channel.joined = false;
        channel.history_anchor = None;
        conv.history_page_cap_tail = false;
        channel.members.clear();
        stop_names_sync(channel);
        conv.typing.clear();
        true
    }

    /// Drops every fact about one network: conversations, features, nicks,
    /// presence, mutes and held playback (`ForgetNetwork`).
    pub fn forget_network(&mut self, network_id: &str) {
        if network_id.is_empty() {
            return;
        }
        self.drop_pending_playback(network_id);
        self.drop_kept_playback(network_id);
        self.facts.query_restore_pending.remove(network_id);
        self.clear_closed_network(network_id);
        self.conversations
            .retain(|key, _| key.network_id != network_id);
        self.facts.features.remove(network_id);
        self.facts.current_nicks.remove(network_id);
        self.facts.self_nicks.remove(network_id);
        self.facts.highlight_words.remove(network_id);
        self.facts.presence.remove(network_id);
        self.facts.self_away.remove(network_id);
        self.facts
            .muted_keys
            .retain(|key| key.network_id != network_id);
        if self
            .facts
            .selected
            .as_ref()
            .is_some_and(|key| key.network_id == network_id)
        {
            self.facts.selected = None;
        }
    }

    /// Empties a transcript while keeping its msgid set, so a bouncer replay at the
    /// same millisecond cannot put a line back. Advances the splice epoch and drops
    /// the history anchor (`ClearMessages`).
    pub fn clear_messages(&mut self, key: &ConversationKey) {
        let Some(conv) = self.conversations.get_mut(key) else {
            return;
        };
        conv.trimmed += conv.messages.len();
        conv.messages.clear();
        conv.splice_epoch += 1;
        if let Some(channel) = conv.channel.as_mut() {
            channel.history_anchor = None;
        }
    }

    /// Records the muted flag, clearing mentions when it becomes muted
    /// (`SetMuted`).
    pub fn set_muted(&mut self, key: &ConversationKey, muted: bool) {
        if key.network_id.is_empty() || key.normalized_target.is_empty() {
            return;
        }
        if muted {
            self.facts.muted_keys.insert(key.clone());
        } else {
            self.facts.muted_keys.remove(key);
        }
        if let Some(conv) = self.conversations.get_mut(key) {
            conv.muted = muted;
            if muted {
                conv.mentions = 0;
            }
        }
    }

    /// Records the selected conversation and clears its unread and mention counts,
    /// keeping the boundary when nothing new arrived (`MarkSelected`).
    pub fn mark_selected(&mut self, key: &ConversationKey) {
        if self.facts.selected.as_ref() == Some(key) {
            return;
        }
        self.facts.selected = Some(key.clone());
        if let Some(conv) = self.conversations.get_mut(key) {
            if conv.unread == 0 {
                conv.unread_mark = None;
            }
            conv.unread = 0;
            conv.mentions = 0;
        }
    }

    /// Clears the unread and mention counts, keeping the boundary. Reports whether
    /// anything changed (`MarkRead`).
    pub fn mark_read(&mut self, key: &ConversationKey) -> bool {
        let Some(conv) = self.conversations.get_mut(key) else {
            return false;
        };
        if conv.unread == 0 && conv.mentions == 0 {
            return false;
        }
        conv.unread = 0;
        conv.mentions = 0;
        true
    }

    /// Clears unread and mention counts on every conversation. Reports whether any
    /// changed (`MarkAllRead`).
    pub fn mark_all_read(&mut self) -> bool {
        let mut changed = false;
        for conv in self.conversations.values_mut() {
            if conv.unread == 0 && conv.mentions == 0 {
                continue;
            }
            conv.unread = 0;
            conv.mentions = 0;
            changed = true;
        }
        changed
    }

    /// Arms tail-first trimming for the next CHATHISTORY splice into `key`
    /// (`MarkHistoryPageCapTail`).
    pub fn mark_history_page_cap_tail(&mut self, key: &ConversationKey) {
        if let Some(conv) = self.conversations.get_mut(key) {
            conv.history_page_cap_tail = true;
        }
    }

    /// Drops tail-first trimming for the transcript (`ClearHistoryPageCapTail`).
    pub fn clear_history_page_cap_tail(&mut self, key: &ConversationKey) {
        if let Some(conv) = self.conversations.get_mut(key) {
            conv.history_page_cap_tail = false;
        }
    }

    /// Forgets the selected conversation (`ClearSelection`).
    pub fn clear_selection(&mut self) {
        self.facts.selected = None;
    }

    /// The selected conversation, if any (`Selected`).
    pub fn selected(&self) -> Option<&ConversationKey> {
        self.facts.selected.as_ref()
    }

    /// Chooses how join, part, quit and nick lines show. Lines already on screen
    /// keep their form (`SetMembershipNoise`).
    pub fn set_membership_noise(&mut self, noise: MembershipNoise) {
        self.facts.membership_noise = noise;
    }

    /// The join, part, quit and nick display (`MembershipNoise`).
    pub fn membership_noise(&self) -> MembershipNoise {
        self.facts.membership_noise
    }

    /// Records whether the window has focus. It is a flag only: consuming unread on
    /// focus regain belongs to the controller (`SetWindowActive`).
    pub fn set_window_active(&mut self, active: bool) {
        self.facts.window_inactive = !active;
    }

    /// Whether the window has focus (`WindowActive`).
    pub fn window_active(&self) -> bool {
        !self.facts.window_inactive
    }

    /// Wires the transcript log used for hydration and persistence. `None`
    /// disables both (`SetConversationLog`).
    pub fn set_conversation_log(&mut self, log: Option<Box<dyn ConversationLog>>) {
        self.facts.log = log;
    }

    /// Records the ISUPPORT feature set for one network (`SetServerFeatures`).
    pub fn set_server_features(&mut self, network_id: &str, features: IrcServerFeatures) {
        self.facts.features.insert(network_id.to_owned(), features);
        self.refresh_mentions(network_id);
    }

    /// The feature set for one network, or the default when none was set
    /// (`ServerFeatures`).
    pub fn server_features(&self, network_id: &str) -> &IrcServerFeatures {
        self.facts.features(network_id)
    }

    /// Replaces the highlight words for one network. An empty list clears them
    /// (`SetHighlightWords`).
    pub fn set_highlight_words(&mut self, network_id: &str, words: &[String]) {
        if network_id.is_empty() {
            return;
        }
        if words.is_empty() {
            self.facts.highlight_words.remove(network_id);
        } else {
            self.facts
                .highlight_words
                .insert(network_id.to_owned(), words.to_vec());
        }
        self.refresh_mentions(network_id);
    }

    /// Re-derives each transcript row's mention flag for one network after the
    /// nick or the highlight words changed. Go reads that flag at view time, so a
    /// row that named the old nick stops highlighting once the nick moves.
    pub(super) fn refresh_mentions(&mut self, network_id: &str) {
        let facts = &self.facts;
        for conv in self.conversations.values_mut() {
            if conv.key.network_id != network_id {
                continue;
            }
            for message in &mut conv.messages {
                message.mentioned = facts.transcript_highlight(
                    network_id,
                    &message.author,
                    message.kind,
                    &message.body,
                );
            }
        }
    }

    /// Releases a NAMES sync that has run past `STALE_NAMES_SYNC_MS`. Reports
    /// whether one was released (`ReleaseStaleNamesSync`).
    pub fn release_stale_names_sync(&mut self, key: Option<&ConversationKey>, now: DateTime<Utc>) -> bool {
        let Some(key) = key else {
            return false;
        };
        let Some(conv) = self.conversations.get_mut(key) else {
            return false;
        };
        let Some(channel) = conv.channel.as_mut() else {
            return false;
        };
        if !channel.names_syncing {
            return false;
        }
        if let Some(started) = channel.names_sync_started {
            if now - started < TimeDelta::milliseconds(STALE_NAMES_SYNC_MS) {
                return false;
            }
        }
        stop_names_sync(channel);
        true
    }

    /// The display name of a joined channel when `sender` is a known member
    /// (`JoinedMemberChannel`). The translator asks this to route channel-context
    /// delivery.
    pub fn joined_member_channel(&self, network_id: &str, channel: &str, sender: &str) -> Option<String> {
        if network_id.is_empty() || channel.is_empty() || sender.is_empty() {
            return None;
        }
        let key = self.conversation_key(network_id, channel);
        let conv = self.conversations.get(&key)?;
        let channel_state = conv.channel.as_ref()?;
        if !channel_state.joined {
            return None;
        }
        let normalized = self.facts.normalize(network_id, sender);
        if normalized.is_empty() || !channel_state.members.contains_key(&normalized) {
            return None;
        }
        Some(conv.target.clone())
    }

    /// A channel member joined with the network presence facts, or `None` when the
    /// key is not a channel or the nick is not a member (`MemberView`).
    pub fn member_view(&self, key: &ConversationKey, normalized_nick: &str) -> Option<MemberView> {
        let conv = self.conversations.get(key)?;
        let channel = conv.channel.as_ref()?;
        let member = channel.members.get(normalized_nick)?;
        let network = key.network_id.as_str();
        let stored = self.facts.presence_of(network, normalized_nick);
        let features = self.facts.features(network);
        // Self-away arrives as a network-level numeric, and a server need not echo our
        // own AWAY back. Overlay it on our own row so the footer and the list agree.
        let mut away: Option<Away> = stored.away.clone();
        if away.is_none() && self.self_away(network) && self.facts.is_self(network, &member.display_nick) {
            away = Some(Away::default());
        }
        Some(MemberView {
            nick: member.display_nick.clone(),
            label: features.member_label(member.ranks, &member.display_nick),
            ranks: member.ranks,
            away,
            status: stored.status().to_owned(),
            avatar: stored.avatar().to_owned(),
            bot: stored.is_bot(),
            display_name: stored.metadata(DISPLAY_NAME_KEY).to_owned(),
            pronouns: stored.metadata(PRONOUNS_KEY).to_owned(),
            homepage: stored.metadata(HOMEPAGE_KEY).to_owned(),
            color: stored.metadata(COLOR_KEY).to_owned(),
            account: self.facts.display_account(network, &member.display_nick),
            realname: presence::displayed_realname(&stored.realname, &member.display_nick).to_owned(),
            labels: self.facts.peer_fact_labels(network, &member.display_nick),
        })
    }

    /// The stored presence for a nick, normalized first (`NickPresence`).
    pub fn nick_presence(&self, network_id: &str, nick: &str) -> NickPresence {
        self.facts.nick_presence(network_id, nick)
    }

    /// The account when it adds information, else empty (`DisplayAccount`).
    pub fn display_account(&self, network_id: &str, nick: &str) -> String {
        self.facts.display_account(network_id, nick)
    }

    /// The stored gecos when it is worth showing (`MeaningfulRealname`).
    pub fn meaningful_realname(&self, network_id: &str, nick: &str) -> String {
        presence::displayed_realname(&self.facts.nick_presence(network_id, nick).realname, nick)
            .to_owned()
    }

    /// The short identity labels for a nick (`PeerFactLabels`).
    pub fn peer_fact_labels(&self, network_id: &str, nick: &str) -> Vec<String> {
        self.facts.peer_fact_labels(network_id, nick)
    }

    /// What the network proves about a nick: `Unknown` without a shared channel,
    /// else `Away` or `Online` (`PeerPresence`).
    pub fn peer_presence(&self, network_id: &str, normalized_nick: &str) -> PeerPresence {
        if normalized_nick.is_empty() || !is_visible(&self.conversations, network_id, normalized_nick) {
            return PeerPresence::Unknown;
        }
        match self.facts.presence.get(network_id) {
            None => PeerPresence::Online,
            Some(presence) if presence.lookup(normalized_nick).away.is_some() => PeerPresence::Away,
            Some(_) => PeerPresence::Online,
        }
    }

    /// The channel members in panel order: PREFIX rank first, then nick
    /// (`OrderedMembers`). The panel and `names` both read this order.
    pub fn ordered_members(&self, key: &ConversationKey) -> Vec<OrderedMember> {
        let Some(conv) = self.conversations.get(key) else {
            return Vec::new();
        };
        let Some(channel) = conv.channel.as_ref() else {
            return Vec::new();
        };
        let features = self.facts.features(&key.network_id);
        let mut ordered: Vec<OrderedMember> = channel
            .members
            .iter()
            .map(|(nick, member)| OrderedMember {
                nick: nick.clone(),
                priority: features.rank_priority(member.ranks),
            })
            .collect();
        ordered.sort_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then_with(|| left.nick.cmp(&right.nick))
        });
        ordered
    }

    /// Whether a body names the current nick or a highlight word (`Mentions`).
    pub fn mentions(&self, network_id: &str, body: &str) -> bool {
        self.facts.is_mention(network_id, body)
    }

    /// Whether a transcript row should be washed: another author's chat line that
    /// names the user. A direct message is not tagged on that basis alone
    /// (`IsTranscriptHighlight`).
    pub fn is_transcript_highlight(
        &self,
        network_id: &str,
        author: &str,
        kind: MessageKind,
        body: &str,
    ) -> bool {
        self.facts.transcript_highlight(network_id, author, kind, body)
    }

    /// The display nicks whose typing indicator should paint at `now`, in
    /// normalized-nick order (`TypingNicks`).
    pub fn typing_nicks(&self, key: &ConversationKey, now: DateTime<Utc>) -> Vec<String> {
        let Some(conv) = self.conversations.get(key) else {
            return Vec::new();
        };
        conv.typing
            .values()
            .filter(|hint| !self.facts.is_self(&key.network_id, &hint.display_nick))
            .filter(|hint| typing_shows_indicator(hint, now))
            .map(|hint| hint.display_nick.clone())
            .collect()
    }

    /// Whether the direct-message peer is typing at `now` (`DirectPeerIsTyping`).
    pub fn direct_peer_is_typing(&self, key: &ConversationKey, now: DateTime<Utc>) -> bool {
        let Some(conv) = self.conversations.get(key) else {
            return false;
        };
        if conv.is_channel() {
            return false;
        }
        conv.typing
            .get(&key.normalized_target)
            .is_some_and(|hint| typing_shows_indicator(hint, now))
    }

    /// Drops every typing hint on one network (`ClearTypingFacts`).
    pub fn clear_typing_facts(&mut self, network_id: &str) {
        for conv in self.conversations.values_mut() {
            if conv.key.network_id == network_id {
                conv.typing.clear();
            }
        }
    }

    /// Drops away and/or metadata facts for every nick on one network. The
    /// network-level self-away flag is not a presence fact and survives
    /// (`ClearPresenceFacts`).
    pub fn clear_presence_facts(&mut self, network_id: &str, away: bool, metadata: bool) {
        let Some(presence) = self.facts.presence.get_mut(network_id) else {
            return;
        };
        if away {
            presence.clear_away();
        }
        if metadata {
            presence.clear_metadata();
        }
    }

    /// Whether the local user is away on this network (`SelfAway`).
    pub fn self_away(&self, network_id: &str) -> bool {
        self.facts.self_away.contains(network_id)
    }

    /// Takes the mention the last `apply` planted, if any (`TakeMentionArrival`).
    pub fn take_mention_arrival(&mut self) -> Option<MentionArrival> {
        self.facts.mention_arrival.take()
    }

    /// Takes the inbox item the last `apply` planted, if any (`TakeInboxArrival`).
    pub fn take_inbox_arrival(&mut self) -> Option<InboxArrival> {
        self.facts.inbox_arrival.take()
    }
}
