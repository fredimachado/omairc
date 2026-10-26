//! The per-network facts the reducer folds events against: ISUPPORT features,
//! the current and remembered nicks, highlight words, presence, the selection,
//! the log and the mute and close marks. Plus the identity and mention queries
//! that read them.
//!
//! Port of the `EventReducer` fields and their queries in `tui/internal/irc/reducer.go`
//! (Go: `ensure`, `ServerFeatures`, `normalize`, `equals`, `isSelf`,
//! `isMention`, `isNickMention`, `isHighlightHit`, `containsWord`,
//! `presenceOf`, `NickPresence`, `DisplayAccount`, `PeerFactLabels`,
//! `bouncerQueryOwnLine`, `bouncerChannelOwnLine`, `replayFromPeer`,
//! `replayOnlyRepeatsPersistedIds`, `rememberSelfNick`) and of
//! `tui/internal/irc/reducer.go`.
//!
//! Every field is `pub(super)`: the reducer's handlers borrow `Facts` next to a
//! conversation (a disjoint field borrow of the same `EventReducer`), so they
//! can mutate a row and read a fact in the same step.
//!
//! Normalization is `IrcCaseMapping::normalize` alone. Go wraps it in `WireText`
//! to undo a byte-level decode, and a Rust `&str` is already valid UTF-8, so the
//! wrap is the identity here.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::irc::casemapping::IrcCaseMapping;
use crate::irc::event::{
    ConversationKey, HistoryEvent, HistoryKind, InboxArrival, KeptReplay, MentionArrival,
    RememberedQuery,
};
use crate::irc::membershipnoise::MembershipNoise;
use crate::irc::presence::{self, Away, NetworkPresence, NickPresence};
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::world::MessageKind;

use super::conversations::ClosedStore;
use super::log::ConversationLog;
use super::state::{Store, MAX_MESSAGES};

/// Everything the reducer knows about the networks it folds events for. Handlers
/// read it while a conversation is borrowed, and write the arrivals and kept
/// lines an `apply` reports.
pub(super) struct Facts {
    /// ISUPPORT per network. A network that never sent one uses `default_features`.
    pub(super) features: HashMap<String, IrcServerFeatures>,
    pub(super) default_features: IrcServerFeatures,
    /// The current nick per network, from welcome and self NICK.
    pub(super) current_nicks: HashMap<String, String>,
    /// Every nick this network has welcomed or changed away from.
    pub(super) self_nicks: HashMap<String, BTreeSet<String>>,
    pub(super) highlight_words: HashMap<String, Vec<String>>,
    pub(super) presence: HashMap<String, NetworkPresence>,
    /// Networks where the local user is away (`305` / `306`).
    pub(super) self_away: HashSet<String>,
    pub(super) selected: Option<ConversationKey>,
    /// Inverted so the default is focused, as the Go zero value is.
    pub(super) window_inactive: bool,
    pub(super) membership_noise: MembershipNoise,
    pub(super) muted_keys: HashSet<ConversationKey>,
    pub(super) log: Option<Box<dyn ConversationLog>>,
    /// Bouncer batches held until their conversation or its join exists.
    pub(super) pending_playback: BTreeMap<ConversationKey, HistoryEvent>,
    pub(super) kept_playback_channel: HashSet<ConversationKey>,
    pub(super) kept_replay: Vec<KeptReplay>,
    pub(super) remembered_queries: Vec<RememberedQuery>,
    pub(super) query_restore_pending: HashSet<String>,
    /// Channels the user closed. Catch-up must not recreate them.
    pub(super) closed_channels: HashSet<ConversationKey>,
    /// The closed list that survives welcome (`setClosedConversations`).
    pub(super) closed_store: Option<Box<dyn ClosedStore>>,
    pub(super) mention_arrival: Option<MentionArrival>,
    pub(super) inbox_arrival: Option<InboxArrival>,
}

impl Facts {
    pub(super) fn new() -> Self {
        Self {
            features: HashMap::new(),
            default_features: IrcServerFeatures::new(),
            current_nicks: HashMap::new(),
            self_nicks: HashMap::new(),
            highlight_words: HashMap::new(),
            presence: HashMap::new(),
            self_away: HashSet::new(),
            selected: None,
            window_inactive: false,
            membership_noise: MembershipNoise::default(),
            muted_keys: HashSet::new(),
            log: None,
            pending_playback: BTreeMap::new(),
            kept_playback_channel: HashSet::new(),
            kept_replay: Vec::new(),
            remembered_queries: Vec::new(),
            query_restore_pending: HashSet::new(),
            closed_channels: HashSet::new(),
            closed_store: None,
            mention_arrival: None,
            inbox_arrival: None,
        }
    }

    /// The network's ISUPPORT set, or the default when it never sent one
    /// (`ServerFeatures`).
    pub(super) fn features(&self, network_id: &str) -> &IrcServerFeatures {
        self.features
            .get(network_id)
            .unwrap_or(&self.default_features)
    }

    /// The network's case mapping (`featuresCaseMapping`).
    pub(super) fn case_mapping(&self, network_id: &str) -> IrcCaseMapping {
        self.features(network_id).case_mapping()
    }

    /// Case-maps an identifier under the network's mapping (`normalize`).
    pub(super) fn normalize(&self, network_id: &str, identifier: &str) -> String {
        self.case_mapping(network_id).normalize(identifier)
    }

    /// Compares two identifiers under the network's mapping (`equals`).
    pub(super) fn equals(&self, network_id: &str, left: &str, right: &str) -> bool {
        self.case_mapping(network_id).equals(left, right)
    }

    /// Whether `nick` is the network's current nick (`isSelf`).
    pub(super) fn is_self(&self, network_id: &str, nick: &str) -> bool {
        self.current_nicks
            .get(network_id)
            .is_some_and(|current| self.equals(network_id, current, nick))
    }

    /// Whether a normalized target is a channel under the network's CHANTYPES.
    pub(super) fn is_channel(&self, network_id: &str, normalized_target: &str) -> bool {
        self.features(network_id).is_channel(normalized_target)
    }

    /// Whether `body` names the current nick as a word (`isNickMention`).
    pub(super) fn is_nick_mention(&self, network_id: &str, body: &str) -> bool {
        let Some(current) = self.current_nicks.get(network_id) else {
            return false;
        };
        let normalized_body = self.normalize(network_id, body);
        contains_word(&normalized_body, &self.normalize(network_id, current))
    }

    /// Whether `body` names one of the network's highlight words (`isHighlightHit`).
    pub(super) fn is_highlight_hit(&self, network_id: &str, body: &str) -> bool {
        let Some(words) = self.highlight_words.get(network_id) else {
            return false;
        };
        let normalized_body = self.normalize(network_id, body);
        words
            .iter()
            .any(|word| contains_word(&normalized_body, &self.normalize(network_id, word)))
    }

    /// A nick or highlight hit (`isMention`, `Mentions`).
    pub(super) fn is_mention(&self, network_id: &str, body: &str) -> bool {
        self.is_nick_mention(network_id, body) || self.is_highlight_hit(network_id, body)
    }

    /// Whether a transcript row is another author's chat line that mentions the
    /// local user. It is read against the nick as it is now (`IsTranscriptHighlight`).
    pub(super) fn transcript_highlight(
        &self,
        network_id: &str,
        author: &str,
        kind: MessageKind,
        body: &str,
    ) -> bool {
        if !matches!(kind, MessageKind::Message | MessageKind::Action) {
            return false;
        }
        if self.is_self(network_id, author) {
            return false;
        }
        self.is_mention(network_id, body)
    }

    /// The stored presence for an already normalized nick (`presenceOf`).
    pub(super) fn presence_of(&self, network_id: &str, normalized_nick: &str) -> NickPresence {
        self.presence
            .get(network_id)
            .map(|presence| presence.lookup(normalized_nick))
            .unwrap_or_default()
    }

    /// The stored presence for a nick, normalized first (`NickPresence`).
    pub(super) fn nick_presence(&self, network_id: &str, nick: &str) -> NickPresence {
        if network_id.is_empty() || nick.is_empty() {
            return NickPresence::default();
        }
        self.presence_of(network_id, &self.normalize(network_id, nick))
    }

    /// The account, only when it adds information: empty when unknown, logged
    /// out, or equal to the nick under the mapping (`DisplayAccount`).
    pub(super) fn display_account(&self, network_id: &str, nick: &str) -> String {
        let account = self.nick_presence(network_id, nick).account;
        if account.is_empty() || nick.is_empty() {
            return String::new();
        }
        if self.equals(network_id, &account, nick) {
            return String::new();
        }
        account
    }

    /// The short identity labels for a nick (`PeerFactLabels`).
    pub(super) fn peer_fact_labels(&self, network_id: &str, nick: &str) -> Vec<String> {
        let facts = self.nick_presence(network_id, nick);
        let matches = !facts.account.is_empty() && self.equals(network_id, &facts.account, nick);
        presence::peer_fact_labels(&facts, matches)
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    /// Whether an author in a bouncer query batch is the local user's previous
    /// nick rather than the peer (`bouncerQueryOwnLine`).
    pub(super) fn bouncer_query_own_line(&self, event: &HistoryEvent, author: &str) -> bool {
        if event.kind != HistoryKind::BouncerPlayback || author.is_empty() {
            return false;
        }
        let network = &event.conversation.network_id;
        if self.is_channel(network, &event.conversation.normalized_target) {
            return false;
        }
        !self.equals(network, author, &event.target)
    }

    /// Whether an author in a bouncer channel batch is one of the local user's
    /// nicks (`bouncerChannelOwnLine`).
    pub(super) fn bouncer_channel_own_line(&self, event: &HistoryEvent, author: &str) -> bool {
        if event.kind != HistoryKind::BouncerPlayback || author.is_empty() {
            return false;
        }
        let network = &event.conversation.network_id;
        if !self.is_channel(network, &event.conversation.normalized_target) {
            return false;
        }
        let Some(nicks) = self.self_nicks.get(network) else {
            return false;
        };
        let mapping = self.case_mapping(network);
        nicks.iter().any(|own| mapping.equals(own, author))
    }

    /// Whether a batch holds a line from the peer (`replayFromPeer`).
    pub(super) fn replay_from_peer(&self, event: &HistoryEvent) -> bool {
        let network = &event.conversation.network_id;
        let bouncer_query = event.kind == HistoryKind::BouncerPlayback
            && !self.is_channel(network, &event.conversation.normalized_target);
        for line in &event.lines {
            if line.author.is_empty() {
                continue;
            }
            if bouncer_query {
                if !self.bouncer_query_own_line(event, &line.author) {
                    return true;
                }
                continue;
            }
            if !self.is_self(network, &line.author) {
                return true;
            }
        }
        false
    }

    /// Whether every line carries a msgid the log already stores, so opening a
    /// missing query would add nothing (`replayOnlyRepeatsPersistedIds`).
    pub(super) fn replay_only_repeats_persisted_ids(&self, event: &HistoryEvent) -> bool {
        let Some(log) = self.log.as_ref() else {
            return false;
        };
        if event.lines.is_empty() || event.lines.iter().any(|line| line.msgid.is_empty()) {
            return false;
        }
        let network = &event.conversation.network_id;
        let stored = log.read_tail(
            network,
            &event.conversation.normalized_target,
            self.case_mapping(network),
            MAX_MESSAGES,
        );
        event.lines.iter().all(|line| {
            stored
                .iter()
                .any(|saved| saved.msgid == line.msgid.value)
        })
    }

    /// Whether a channel is in the closed set (`isClosed`).
    pub(super) fn is_closed(&self, key: &ConversationKey) -> bool {
        self.closed_channels.contains(key)
    }

    /// Records a nick this network has welcomed or changed away from
    /// (`rememberSelfNick`).
    pub(super) fn remember_self_nick(&mut self, network_id: &str, nick: &str) {
        if network_id.is_empty() || nick.is_empty() {
            return;
        }
        self.self_nicks
            .entry(network_id.to_owned())
            .or_default()
            .insert(nick.to_owned());
    }

    pub(super) fn set_presence_away(
        &mut self,
        network_id: &str,
        normalized_nick: &str,
        away: Option<Away>,
    ) {
        self.presence
            .entry(network_id.to_owned())
            .or_default()
            .set_away(normalized_nick, away);
    }

    pub(super) fn set_presence_metadata(
        &mut self,
        network_id: &str,
        normalized_nick: &str,
        key: &str,
        value: &str,
    ) {
        self.presence
            .entry(network_id.to_owned())
            .or_default()
            .set_metadata(normalized_nick, key, value);
    }

    pub(super) fn set_presence_account(
        &mut self,
        network_id: &str,
        normalized_nick: &str,
        account: &str,
    ) {
        self.presence
            .entry(network_id.to_owned())
            .or_default()
            .set_account(normalized_nick, account);
    }

    pub(super) fn set_presence_realname(
        &mut self,
        network_id: &str,
        normalized_nick: &str,
        realname: &str,
    ) {
        self.presence
            .entry(network_id.to_owned())
            .or_default()
            .set_realname(normalized_nick, realname);
    }

    pub(super) fn set_presence_operator(
        &mut self,
        network_id: &str,
        normalized_nick: &str,
        operator: bool,
    ) {
        self.presence
            .entry(network_id.to_owned())
            .or_default()
            .set_server_operator(normalized_nick, operator);
    }

    pub(super) fn rekey_presence(&mut self, network_id: &str, from: &str, to: &str) {
        self.presence
            .entry(network_id.to_owned())
            .or_default()
            .rekey(from, to);
    }
}

/// Whether a normalized nick is a member of any channel on the network
/// (`isVisible`). It reads membership, so it takes the store.
pub(super) fn is_visible(store: &Store, network_id: &str, normalized_nick: &str) -> bool {
    store.values().any(|conversation| {
        conversation.key.network_id == network_id
            && conversation
                .channel
                .as_ref()
                .is_some_and(|channel| channel.members.contains_key(normalized_nick))
    })
}

/// Whether `word` appears in `body` on identifier boundaries (`containsWord`).
/// Both are already normalized.
pub(super) fn contains_word(body: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let body: Vec<char> = body.chars().collect();
    let word: Vec<char> = word.chars().collect();
    if word.len() > body.len() {
        return false;
    }
    for position in 0..=body.len() - word.len() {
        if body[position..position + word.len()] != word[..] {
            continue;
        }
        let end = position + word.len();
        let left_ok = position == 0 || !is_identifier_char(body[position - 1]);
        let right_ok = end == body.len() || !is_identifier_char(body[end]);
        if left_ok && right_ok {
            return true;
        }
    }
    false
}

/// Whether a character counts as part of a nick for mention boundaries
/// (`isIdentifierCharacter`).
fn is_identifier_char(character: char) -> bool {
    if character.is_alphabetic() || character.is_numeric() {
        return true;
    }
    matches!(
        character,
        '-' | '_' | '[' | ']' | '\\' | '`' | '^' | '{' | '}' | '|' | '~'
    )
}
