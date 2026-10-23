//! Port of `tui/internal/controller/replyrouter.go` (Go) and `src/irc/ircreplyrouter.{h,cpp}`
//! (C++): the WHOIS, CTCP and labeled-response watches, and the routing of their replies
//! to a conversation transcript or to Status.
//!
//! A WHOIS or a CTCP query arms a watch keyed by network and nick. With `labeled-response`
//! negotiated the watch is keyed by the request label instead, so the server's answer
//! names its own request. A reply that matches a watch is copied to the destination the
//! watch was armed with, and the watch ends with the reply: a 318 for a WHOIS, the first
//! reply for a CTCP, and a FAIL, WARN, NOTE or 4xx and 5xx for a labeled request that
//! failed.
//!
//! A WHOIS detail line also teaches the reducer its metadata, and the first detail line of
//! a WHOIS writes those facts to Status once. A watch armed while the nick's delivery
//! changed is marked ambiguous, so the 401 that follows does not read as "no such nick".
//!
//! The user's own `/status` and `/avatar` are METADATA writes the same way: a set or a clear
//! arms a watch keyed by network and canonical key, and the 761 or 766 that confirms it (or a
//! FAIL, or a 764, 767 or 769 that refuses it) ends the watch and echoes the outcome to the
//! conversation that asked, or to Status. A reply that lands while the write is still being
//! armed waits until the watch exists (`IrcReplyRouter::routeOwnMetadataReply`).
//!
//! Port of `tui/internal/controller/replyrouter.go` (Go) and `src/irc/ircreplyrouter.{h,cpp}`
//! (C++). The `/whois`, `/ping`, `/time`, `/version`, `/status` and `/avatar` dispatch lives
//! here too, so the dispatcher only routes the verb.

use std::collections::BTreeMap;

use crate::irc::avatarurl::avatar_metadata_value;
use crate::irc::capability::IrcCapability;
use crate::irc::command::{CommandOutcome, ComposerSurface, Words};
use crate::irc::event::{ConversationKey, Event, WhoisTranscriptEvent};
use crate::irc::message::IrcMessage;
use crate::irc::presence::{
    canonical_key, clamped_to, effective_max_value_bytes, is_known_key, AVATAR_KEY, BOT_KEY,
    COLOR_KEY, DISPLAY_NAME_KEY, HOMEPAGE_KEY, PRONOUNS_KEY, STATUS_KEY,
};
use crate::irc::statusentry::{self, CtcpReplyLine, LogSeverity, StatusEntry, WhoisLine, WhoisProgress};
use crate::irc::wiretext::wire_text;
use crate::session::events::SessionState;
use crate::session::session::Session;

use super::controller::Controller;

/// The key of a plain WHOIS watch: the network and the nick under its case mapping.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct WhoisKey {
    network_id: String,
    normalized_nick: String,
}

/// The key of a plain CTCP watch: the network, the nick, and the uppercased command.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct CtcpKey {
    network_id: String,
    normalized_nick: String,
    command: String,
}

/// The key of a labeled request.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct LabeledKey {
    network_id: String,
    request_label: String,
}

/// Whether a labeled request asked for a WHOIS or for a CTCP query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LabeledKind {
    Whois,
    Ctcp,
}

#[derive(Clone, Debug)]
struct WhoisWatch {
    destination: Option<ConversationKey>,
    failed_is_ambiguous: bool,
    metadata_emitted: bool,
}

#[derive(Clone, Debug)]
struct CtcpWatch {
    destination: Option<ConversationKey>,
}

#[derive(Clone, Debug)]
struct LabeledWatch {
    kind: LabeledKind,
    destination: Option<ConversationKey>,
    metadata_emitted: bool,
}

/// Whether an own-metadata write sets a value or clears it (`IrcOwnMetadataWatch::Kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OwnMetadataKind {
    Set,
    Clear,
}

/// One in-flight set or clear of the user's own `status` or `avatar` (`IrcOwnMetadataWatch`).
/// `value` is the wire value a set sent, which the confirming 761 must echo.
#[derive(Clone, Debug)]
struct OwnMetadataWatch {
    destination: Option<ConversationKey>,
    kind: OwnMetadataKind,
    value: String,
}

/// A 761 or 766 that arrived while its write was still being armed (`replyOwnMetadataPending`).
#[derive(Clone, Debug)]
struct PendingOwnMetadata {
    network_id: String,
    nick: String,
    key: String,
    value: String,
}

/// The reply watches the router holds (`IrcReplyRouter`'s watch maps).
#[derive(Default)]
pub(super) struct ReplyWatches {
    whois: BTreeMap<WhoisKey, WhoisWatch>,
    ctcp: BTreeMap<CtcpKey, CtcpWatch>,
    labeled: BTreeMap<LabeledKey, LabeledWatch>,
    /// The in-flight own-metadata writes, by network and canonical key.
    own_metadata: BTreeMap<String, BTreeMap<&'static str, OwnMetadataWatch>>,
    /// How many own-metadata writes are being armed. While one is, a confirming reply waits
    /// in `pending_own_metadata`, because the loopback can answer before the watch exists.
    own_metadata_depth: usize,
    pending_own_metadata: Vec<PendingOwnMetadata>,
}

#[cfg(test)]
impl ReplyWatches {
    /// How many own `/status` or `/avatar` writes are awaiting their reply (a test seam).
    pub(super) fn own_metadata_count(&self) -> usize {
        self.own_metadata.values().map(BTreeMap::len).sum()
    }

    /// How many labeled watches are armed (a test seam).
    pub(super) fn labeled_count(&self) -> usize {
        self.labeled.len()
    }

    /// How many plain WHOIS and CTCP watches are armed (a test seam).
    pub(super) fn plain_count(&self) -> usize {
        self.whois.len() + self.ctcp.len()
    }
}

impl Controller {
    /// Sends a WHOIS for `nick` on `network_id` and arms its watch, with `destination` as
    /// the transcript the replies copy to (`sendWhois`). Returns false when the nick is
    /// empty or the write is refused.
    // The product CLI (Phase 9) calls these; the dispatcher reaches the same watches through its own verbs.
    #[cfg(test)]
    pub fn whois(&mut self, network_id: &str, nick: &str, destination: Option<ConversationKey>) -> bool {
        self.pump();
        let Some(session) = self.session(network_id) else {
            return false;
        };
        let sent = self.send_whois(&session, nick, destination);
        self.pump();
        sent
    }

    /// Sends a CTCP query (`PING`, `TIME` or `VERSION`) to `nick` and arms its watch, with
    /// `destination` as the transcript the reply copies to (`sendCtcpQuery`).
    // The product CLI (Phase 9) calls these; the dispatcher reaches the same watches through its own verbs.
    #[cfg(test)]
    pub fn ctcp_query(
        &mut self,
        network_id: &str,
        nick: &str,
        command: &str,
        argument: &str,
        destination: Option<ConversationKey>,
    ) -> bool {
        self.pump();
        let Some(session) = self.session(network_id) else {
            return false;
        };
        let sent = self.send_ctcp_query(&session, nick, command, argument, destination);
        self.pump();
        sent
    }

    /// A nick's delivery changed under a plain WHOIS watch, so its 401 is ambiguous. A
    /// channel never has one (`noteNickDelivery`).
    pub fn note_nick_delivery(&mut self, network_id: &str, target: &str) {
        if self.reducer.server_features(network_id).is_channel(target) {
            return;
        }
        let Some(key) = self.whois_key(network_id, target) else {
            return;
        };
        if let Some(watch) = self.reply_watches.whois.get_mut(&key) {
            watch.failed_is_ambiguous = true;
        }
    }

    /// Drops every watch of `network_id` (`Forget`). A welcome, a discard and a forgotten
    /// network call it.
    pub(super) fn forget_replies(&mut self, network_id: &str) {
        self.reply_watches
            .whois
            .retain(|key, _| key.network_id != network_id);
        self.reply_watches
            .ctcp
            .retain(|key, _| key.network_id != network_id);
        self.reply_watches
            .labeled
            .retain(|key, _| key.network_id != network_id);
        self.reply_watches.own_metadata.remove(network_id);
    }

    /// The session finished a labeled request: its watch goes (`requestLabelFinished`).
    pub(super) fn request_label_finished(&mut self, network_id: &str, request_label: &str) {
        if network_id.is_empty() || request_label.is_empty() {
            return;
        }
        self.reply_watches.labeled.remove(&LabeledKey {
            network_id: network_id.to_owned(),
            request_label: request_label.to_owned(),
        });
    }

    /// Writes one classified Status entry and routes it to the watch it answers
    /// (`StatusEntry`). Inbound, outbound and lifecycle lines all come through here.
    pub(super) fn status_entry(&mut self, entry: StatusEntry) {
        self.append_status(entry.clone());
        self.route_status_entry(&entry);
    }

    /// Routes a Status entry to the labeled watch it answers, or to the plain WHOIS and
    /// CTCP watches when it carries no label (`routeStatusEntry`).
    fn route_status_entry(&mut self, entry: &StatusEntry) {
        let network_id = entry.network_id.clone();
        self.route_own_metadata_error(entry);
        if !entry.request_label.is_empty() {
            let key = LabeledKey {
                network_id: network_id.clone(),
                request_label: entry.request_label.clone(),
            };
            let Some(kind) = self.reply_watches.labeled.get(&key).map(|watch| watch.kind) else {
                return;
            };
            match kind {
                LabeledKind::Whois => match entry.whois_line.clone() {
                    Some(line) => self.route_labeled_whois(&network_id, &entry.request_label, &line),
                    None => self.route_labeled_standard_reply(entry),
                },
                LabeledKind::Ctcp => match entry.ctcp_reply.clone() {
                    Some(line) => {
                        self.route_labeled_ctcp(&network_id, &entry.request_label, &line, &entry.text);
                    }
                    None => self.route_labeled_standard_reply(entry),
                },
            }
            return;
        }
        if let Some(line) = entry.whois_line.clone() {
            self.route_whois_line(&network_id, &line);
        }
        if let Some(line) = entry.ctcp_reply.clone() {
            self.route_ctcp_reply(&network_id, &line, &entry.text);
        }
    }

    fn send_whois(&mut self, session: &Session, nick: &str, destination: Option<ConversationKey>) -> bool {
        let network_id = session.network_id();
        let Some(key) = self.whois_key(&network_id, nick) else {
            return false;
        };
        if destination
            .as_ref()
            .is_some_and(|destination| destination.network_id != network_id)
        {
            return false;
        }

        let label = session.start_labeled_request();
        if !label.is_empty() {
            let labeled = LabeledKey {
                network_id: network_id.clone(),
                request_label: label.clone(),
            };
            self.reply_watches.labeled.insert(
                labeled.clone(),
                LabeledWatch {
                    kind: LabeledKind::Whois,
                    destination,
                    metadata_emitted: false,
                },
            );
            if !session.whois(nick, &label) {
                self.reply_watches.labeled.remove(&labeled);
                session.cancel_request_label(&label);
                return false;
            }
            return true;
        }
        self.reply_watches.whois.insert(
            key.clone(),
            WhoisWatch {
                destination,
                failed_is_ambiguous: false,
                metadata_emitted: false,
            },
        );
        if !session.whois(nick, "") {
            self.reply_watches.whois.remove(&key);
            return false;
        }
        true
    }

    fn send_ctcp_query(
        &mut self,
        session: &Session,
        nick: &str,
        command: &str,
        argument: &str,
        destination: Option<ConversationKey>,
    ) -> bool {
        let network_id = session.network_id();
        let Some(key) = self.ctcp_key(&network_id, nick, command) else {
            return false;
        };
        if destination
            .as_ref()
            .is_some_and(|destination| destination.network_id != network_id)
        {
            return false;
        }

        let label = session.start_labeled_request();
        let labeled = LabeledKey {
            network_id: network_id.clone(),
            request_label: label.clone(),
        };
        if !label.is_empty() {
            self.reply_watches.labeled.insert(
                labeled.clone(),
                LabeledWatch {
                    kind: LabeledKind::Ctcp,
                    destination: destination.clone(),
                    metadata_emitted: false,
                },
            );
        }
        self.reply_watches
            .ctcp
            .insert(key.clone(), CtcpWatch { destination });
        if !session.send_ctcp(nick, command, argument, &label) {
            if !label.is_empty() {
                self.reply_watches.labeled.remove(&labeled);
                session.cancel_request_label(&label);
            }
            self.reply_watches.ctcp.remove(&key);
            return false;
        }
        true
    }

    /// Routes a WHOIS line that no label carries (`routeWhoisLine`).
    fn route_whois_line(&mut self, network_id: &str, line: &WhoisLine) {
        let Some(key) = self.whois_key(network_id, &line.nick) else {
            return;
        };
        let Some(watch) = self.reply_watches.whois.get(&key).cloned() else {
            return;
        };
        if line.progress == WhoisProgress::Failed && watch.failed_is_ambiguous {
            return;
        }
        let emit_metadata = line.progress == WhoisProgress::Detail && !watch.metadata_emitted;
        if emit_metadata {
            if let Some(stored) = self.reply_watches.whois.get_mut(&key) {
                stored.metadata_emitted = true;
            }
        }
        self.apply_whois_detail(network_id, line, watch.destination.as_ref(), emit_metadata);
        if line.is_terminal() {
            self.reply_watches.whois.remove(&key);
        }
    }

    /// Routes a WHOIS line that a labeled request carries (`routeLabeledWhois`).
    fn route_labeled_whois(&mut self, network_id: &str, request_label: &str, line: &WhoisLine) {
        let key = LabeledKey {
            network_id: network_id.to_owned(),
            request_label: request_label.to_owned(),
        };
        let Some(watch) = self.reply_watches.labeled.get(&key).cloned() else {
            return;
        };
        if watch.kind != LabeledKind::Whois {
            return;
        }
        let emit_metadata = line.progress == WhoisProgress::Detail && !watch.metadata_emitted;
        if emit_metadata {
            if let Some(stored) = self.reply_watches.labeled.get_mut(&key) {
                stored.metadata_emitted = true;
            }
        }
        self.apply_whois_detail(network_id, line, watch.destination.as_ref(), emit_metadata);
        if line.is_terminal() {
            self.reply_watches.labeled.remove(&key);
        }
    }

    /// The shared body of the two WHOIS routes: the line itself goes to the destination,
    /// and on the first detail line the metadata facts go to Status and the destination.
    fn apply_whois_detail(
        &mut self,
        network_id: &str,
        line: &WhoisLine,
        destination: Option<&ConversationKey>,
        emit_metadata: bool,
    ) {
        if let Some(destination) = destination {
            self.apply_whois_transcript(destination, &line.text);
        }
        if !emit_metadata {
            return;
        }
        for text in self.whois_metadata_lines(network_id, &line.nick) {
            let now = self.clock.now();
            self.append_status(statusentry::lifecycle(
                network_id,
                LogSeverity::Info,
                "whois",
                &text,
                now,
            ));
            if let Some(destination) = destination {
                self.apply_whois_transcript(destination, &text);
            }
        }
    }

    /// Routes a CTCP reply that a labeled request carries. The labeled and the plain
    /// watch both end with the reply (`routeLabeledCtcp`).
    fn route_labeled_ctcp(&mut self, network_id: &str, request_label: &str, line: &CtcpReplyLine, text: &str) {
        let key = LabeledKey {
            network_id: network_id.to_owned(),
            request_label: request_label.to_owned(),
        };
        let Some(watch) = self.reply_watches.labeled.get(&key).cloned() else {
            return;
        };
        if watch.kind != LabeledKind::Ctcp {
            return;
        }
        self.reply_watches.labeled.remove(&key);
        if let Some(ctcp) = self.ctcp_key(network_id, &line.nick, &line.command) {
            self.reply_watches.ctcp.remove(&ctcp);
        }
        self.copy_to_destination(watch.destination.as_ref(), text);
    }

    /// Routes a plain CTCP reply (`routeCtcpReply`).
    fn route_ctcp_reply(&mut self, network_id: &str, line: &CtcpReplyLine, text: &str) {
        let Some(key) = self.ctcp_key(network_id, &line.nick, &line.command) else {
            return;
        };
        let Some(watch) = self.reply_watches.ctcp.remove(&key) else {
            return;
        };
        self.copy_to_destination(watch.destination.as_ref(), text);
    }

    /// A labeled standard reply (FAIL, WARN, NOTE, or a 4xx or 5xx) copies into the
    /// destination, and an alert ends the watch (`routeLabeledStandardReply`).
    fn route_labeled_standard_reply(&mut self, entry: &StatusEntry) {
        let key = LabeledKey {
            network_id: entry.network_id.clone(),
            request_label: entry.request_label.clone(),
        };
        let Some(watch) = self.reply_watches.labeled.get(&key).cloned() else {
            return;
        };
        if copies_labeled_standard_reply(&entry.label) {
            self.copy_to_destination(watch.destination.as_ref(), &entry.text);
        }
        if entry.severity == LogSeverity::Alert {
            self.reply_watches.labeled.remove(&key);
        }
    }

    /// Copies one reply body into a watch's destination, unless there is none or the
    /// body is empty.
    fn copy_to_destination(&mut self, destination: Option<&ConversationKey>, text: &str) {
        if let Some(destination) = destination {
            if !text.is_empty() {
                self.apply_whois_transcript(destination, text);
            }
        }
    }

    pub(super) fn apply_whois_transcript(&mut self, destination: &ConversationKey, body: &str) {
        self.apply(Event::WhoisTranscript(WhoisTranscriptEvent {
            destination: destination.clone(),
            formatted_body: body.to_owned(),
        }));
    }

    /// The metadata facts a WHOIS taught the reducer, as the lines Status shows
    /// (`whoisMetadataLines`).
    fn whois_metadata_lines(&self, network_id: &str, nick: &str) -> Vec<String> {
        let facts = self.reducer.nick_presence(network_id, nick);
        let mut lines = Vec::new();
        for (key, pattern) in [
            (DISPLAY_NAME_KEY, "%1 is also known as %2"),
            (PRONOUNS_KEY, "%1 pronouns %2"),
            (STATUS_KEY, "%1 status %2"),
        ] {
            let value = facts.metadata(key);
            if !value.is_empty() {
                lines.push(format_pair(pattern, nick, value));
            }
        }
        if facts.is_bot() {
            let software = facts.metadata(BOT_KEY);
            if software.is_empty() {
                lines.push(format!("{nick} is a bot"));
            } else {
                lines.push(format!("{nick} is a bot ({software})"));
            }
        }
        for (key, pattern) in [
            (HOMEPAGE_KEY, "%1 homepage %2"),
            (COLOR_KEY, "%1 color %2"),
            (AVATAR_KEY, "%1 avatar %2"),
        ] {
            let value = facts.metadata(key);
            if !value.is_empty() {
                lines.push(format_pair(pattern, nick, value));
            }
        }
        lines
    }

    /// The watch key of `nick` on `network_id`, or `None` for an empty network or nick.
    fn whois_key(&self, network_id: &str, nick: &str) -> Option<WhoisKey> {
        let trimmed = nick.trim();
        if network_id.is_empty() || trimmed.is_empty() {
            return None;
        }
        Some(WhoisKey {
            network_id: network_id.to_owned(),
            normalized_nick: self
                .reducer
                .conversation_key(network_id, trimmed)
                .normalized_target,
        })
    }

    /// The watch key of a CTCP query, or `None` for an empty network, nick or command.
    fn ctcp_key(&self, network_id: &str, nick: &str, command: &str) -> Option<CtcpKey> {
        let trimmed = nick.trim();
        let verb = command.trim().to_uppercase();
        if network_id.is_empty() || trimmed.is_empty() || verb.is_empty() {
            return None;
        }
        Some(CtcpKey {
            network_id: network_id.to_owned(),
            normalized_nick: self
                .reducer
                .conversation_key(network_id, trimmed)
                .normalized_target,
            command: verb,
        })
    }
}

impl Controller {
    /// `/whois` (`IrcReplyRouter::dispatchWhois`). A bare WHOIS asks the selected direct
    /// message's peer. With a nick, the reply copies into the selected conversation, or stays
    /// on Status when Status is the surface.
    pub(super) fn dispatch_whois(&mut self, words: &Words, surface: ComposerSurface) -> CommandOutcome {
        let (nick, session) = if words.first.is_empty() {
            match self.bare_nick_target() {
                Ok(target) => target,
                Err(outcome) => return outcome,
            }
        } else {
            let network = self.surface_network(surface);
            if network.is_empty() {
                return refused_or_wrong_scope(surface);
            }
            (words.first.clone(), self.session_for(surface))
        };
        if nick.is_empty() {
            return CommandOutcome::Refused;
        }
        let Some(session) = session.filter(|session| session.state() == SessionState::Registered)
        else {
            return CommandOutcome::NotConnected;
        };
        let destination = match self.destination_for(surface) {
            Ok(destination) => destination,
            Err(outcome) => return outcome,
        };
        if self.send_whois(&session, &nick, destination) {
            CommandOutcome::Sent
        } else {
            CommandOutcome::Refused
        }
    }

    /// `/ping`, `/time` or `/version` (`IrcReplyRouter::dispatchCtcp`). `query` is the CTCP
    /// command name. The reply copies into the selected conversation as a whois row.
    pub(super) fn dispatch_ctcp(
        &mut self,
        query: &'static str,
        words: &Words,
        surface: ComposerSurface,
    ) -> CommandOutcome {
        if !words.rest.is_empty() {
            return CommandOutcome::Refused;
        }
        let (nick, session) = if words.first.is_empty() {
            match self.bare_nick_target() {
                Ok(target) => target,
                Err(outcome) => return outcome,
            }
        } else {
            let network = self.surface_network(surface);
            if network.is_empty() {
                return refused_or_wrong_scope(surface);
            }
            if self.reducer.server_features(&network).is_channel(&words.first) {
                return CommandOutcome::Refused;
            }
            (words.first.clone(), self.session_for(surface))
        };
        if nick.is_empty() {
            return CommandOutcome::Refused;
        }
        let Some(session) = session.filter(|session| session.state() == SessionState::Registered)
        else {
            return CommandOutcome::NotConnected;
        };
        let destination = match self.destination_for(surface) {
            Ok(destination) => destination,
            Err(outcome) => return outcome,
        };
        // A PING carries the send time in milliseconds, so the reply's latency is readable.
        let argument = if query == "PING" {
            self.clock.now().timestamp_millis().to_string()
        } else {
            String::new()
        };
        if self.send_ctcp_query(&session, &nick, query, &argument, destination) {
            CommandOutcome::Sent
        } else {
            CommandOutcome::Refused
        }
    }

    /// `/status` (`IrcReplyRouter::dispatchStatus`): no argument reads the standing status,
    /// `clear` unsets it, and anything else sets it, clamped to the network's value budget.
    pub(super) fn dispatch_status(&mut self, argument: &str, surface: ComposerSurface) -> CommandOutcome {
        self.dispatch_own_metadata(argument, surface, STATUS_KEY)
    }

    /// `/avatar` (`IrcReplyRouter::dispatchAvatar`): an HTTPS URL or an email, stored as a
    /// Gravatar URL. Like `/status`, it reads, clears or sets.
    pub(super) fn dispatch_avatar(&mut self, argument: &str, surface: ComposerSurface) -> CommandOutcome {
        self.dispatch_own_metadata(argument, surface, AVATAR_KEY)
    }

    /// The shared body of `/status` and `/avatar`. The two differ only in the key, the
    /// inspect read, and how the value is resolved.
    fn dispatch_own_metadata(
        &mut self,
        argument: &str,
        surface: ComposerSurface,
        key: &'static str,
    ) -> CommandOutcome {
        let Some(session) = self.session_for(surface) else {
            return if surface == ComposerSurface::Conversation
                && self.selected.is_none()
                && !self.manager.network_ids().is_empty()
            {
                CommandOutcome::Refused
            } else {
                CommandOutcome::NotConnected
            };
        };
        if session.state() != SessionState::Registered {
            return CommandOutcome::NotConnected;
        }
        let network = session.network_id();
        let capabilities = self.caps(&network);
        if !capabilities.contains(IrcCapability::MemberMetadata)
            || !capabilities.contains(IrcCapability::Batch)
        {
            let text = no_capability_message(key);
            return self.echo_metadata_feedback(surface, &network, &text);
        }

        if argument.is_empty() {
            let presence = self.reducer.nick_presence(&network, &session.nick());
            let current = if key == STATUS_KEY {
                presence.status()
            } else {
                presence.avatar()
            };
            let text = if current.is_empty() {
                inspect_empty_message(key)
            } else {
                inspect_value_message(key, current)
            };
            return self.echo_metadata_feedback(surface, &network, &text);
        }

        if is_clear_alias(argument) {
            return self.dispatch_own_metadata_clear(&session, key);
        }

        let value = if key == AVATAR_KEY {
            match avatar_metadata_value(argument) {
                Some(resolved) => resolved,
                None => {
                    return self.echo_metadata_feedback(
                        surface,
                        &network,
                        "Avatar must be an HTTPS URL or an email address.",
                    );
                }
            }
        } else {
            argument.to_owned()
        };
        let budget = effective_max_value_bytes(session.metadata_capability().max_value_bytes);
        if budget == 0 {
            let text = no_value_message(key);
            return self.echo_metadata_feedback(surface, &network, &text);
        }
        let clamped = clamped_to(&value, budget);
        if clamped.is_empty() {
            return CommandOutcome::Refused;
        }
        self.dispatch_own_metadata_set(&session, key, &clamped)
    }

    /// Clears one own key and arms its clear watch (`dispatchOwnMetadataClear`).
    fn dispatch_own_metadata_clear(&mut self, session: &Session, key: &'static str) -> CommandOutcome {
        self.begin_own_metadata_dispatch();
        if !session.clear_own_metadata(key) {
            self.end_own_metadata_dispatch();
            return CommandOutcome::Refused;
        }
        self.arm_own_metadata_watch(&session.network_id(), key, OwnMetadataKind::Clear, "");
        self.end_own_metadata_dispatch();
        CommandOutcome::Sent
    }

    /// Sets one own key and arms its set watch with the value the wire carried
    /// (`dispatchOwnMetadataSet`).
    fn dispatch_own_metadata_set(
        &mut self,
        session: &Session,
        key: &'static str,
        value: &str,
    ) -> CommandOutcome {
        if value.is_empty() {
            return CommandOutcome::Refused;
        }
        self.begin_own_metadata_dispatch();
        let sent = match session.set_own_metadata(key, value) {
            Some(wire) if !wire.is_empty() => wire,
            _ => {
                self.end_own_metadata_dispatch();
                return CommandOutcome::Refused;
            }
        };
        self.arm_own_metadata_watch(&session.network_id(), key, OwnMetadataKind::Set, &sent);
        self.end_own_metadata_dispatch();
        CommandOutcome::Sent
    }

    fn begin_own_metadata_dispatch(&mut self) {
        self.reply_watches.own_metadata_depth += 1;
    }

    /// Closes one write. When the last one closes, the replies that waited for it are
    /// routed now that their watches exist.
    fn end_own_metadata_dispatch(&mut self) {
        self.reply_watches.own_metadata_depth = self.reply_watches.own_metadata_depth.saturating_sub(1);
        if self.reply_watches.own_metadata_depth > 0 {
            return;
        }
        let pending = std::mem::take(&mut self.reply_watches.pending_own_metadata);
        for item in pending {
            self.route_own_metadata_reply(&item.network_id, &item.nick, &item.key, &item.value);
        }
    }

    /// Echoes a local `/status` or `/avatar` message to the selected conversation of this
    /// network, or else to Status (`echoMetadataCommandFeedback`). A conversation surface with
    /// no selection on this network is a wrong scope.
    fn echo_metadata_feedback(&mut self, surface: ComposerSurface, network_id: &str, text: &str) -> CommandOutcome {
        if let Some(selected) = self.selected.clone() {
            if selected.network_id == network_id {
                self.apply_whois_transcript(&selected, text);
                return CommandOutcome::Sent;
            }
        }
        if surface == ComposerSurface::Conversation {
            return CommandOutcome::WrongScope;
        }
        self.append_status(statusentry::outcome(network_id, text, self.clock.now()));
        CommandOutcome::Sent
    }

    /// Arms the watch for one own-metadata write. The outcome goes to the selection when it
    /// is on the same network (`armOwnMetadataWatch`).
    fn arm_own_metadata_watch(
        &mut self,
        network_id: &str,
        key: &str,
        kind: OwnMetadataKind,
        value: &str,
    ) {
        let Some(canonical) = canonical_key(key) else {
            return;
        };
        let destination = match self.selected.clone() {
            Some(selected) if selected.network_id == network_id => Some(selected),
            _ => None,
        };
        self.reply_watches
            .own_metadata
            .entry(network_id.to_owned())
            .or_default()
            .insert(
                canonical,
                OwnMetadataWatch {
                    destination,
                    kind,
                    value: value.to_owned(),
                },
            );
    }

    /// Ends one own-metadata watch, and the network's map when it was the last.
    fn remove_own_metadata_watch(&mut self, network_id: &str, canonical: &str) {
        let emptied = match self.reply_watches.own_metadata.get_mut(network_id) {
            Some(watches) => {
                watches.remove(canonical);
                watches.is_empty()
            }
            None => false,
        };
        if emptied {
            self.reply_watches.own_metadata.remove(network_id);
        }
    }

    /// Writes an own-metadata outcome to its captured destination, else to the selection when
    /// it is on this network, else to Status (`echoOwnMetadataOutcome`).
    fn echo_own_metadata_outcome(
        &mut self,
        network_id: &str,
        destination: Option<&ConversationKey>,
        text: &str,
    ) {
        if let Some(destination) = destination {
            self.apply_whois_transcript(destination, text);
            return;
        }
        if let Some(selected) = self.selected.clone() {
            if selected.network_id == network_id {
                self.apply_whois_transcript(&selected, text);
                return;
            }
        }
        self.append_status(statusentry::outcome(network_id, text, self.clock.now()));
    }

    /// A METADATA FAIL that answers one of the user's own writes (`routeOwnMetadataFail`).
    /// The FAIL names the keys it refused, and each pending watch for one of them ends with
    /// the refusal, or with the "already unset" reading of a clear.
    pub(super) fn route_own_metadata_fail(&mut self, network_id: &str, message: &IrcMessage) {
        if !param_text(message, 0).eq_ignore_ascii_case("METADATA") {
            return;
        }
        if !self.reply_watches.own_metadata.contains_key(network_id) {
            return;
        }
        let code = param_text(message, 1).to_uppercase();
        if !matches!(
            code.as_str(),
            "KEY_NO_PERMISSION" | "VALUE_INVALID" | "RATE_LIMITED" | "KEY_NOT_SET" | "LIMIT_REACHED" | "KEY_INVALID"
        ) {
            return;
        }

        let last_context = if message.params.len() > 2 {
            message.params.len() - 1
        } else {
            2
        };
        let mut fail_keys: Vec<&'static str> = Vec::new();
        for index in 2..last_context {
            let token = param_text(message, index);
            if !is_known_key(&token) {
                continue;
            }
            if let Some(canonical) = canonical_key(&token) {
                if !fail_keys.contains(&canonical) {
                    fail_keys.push(canonical);
                }
            }
        }
        if fail_keys.is_empty() {
            return;
        }

        let description = if message.params.is_empty() {
            code.clone()
        } else {
            param_text(message, message.params.len() - 1)
        };
        let reason = if description.is_empty() {
            code.clone()
        } else {
            format!("{code} {description}")
        };

        let Some(watches) = self.reply_watches.own_metadata.get_mut(network_id) else {
            return;
        };
        let taken: Vec<(&'static str, OwnMetadataWatch)> = fail_keys
            .iter()
            .filter_map(|key| watches.remove(key).map(|watch| (*key, watch)))
            .collect();
        if watches.is_empty() {
            self.reply_watches.own_metadata.remove(network_id);
        }

        for (canonical, watch) in taken {
            if watch.kind == OwnMetadataKind::Clear && code == "KEY_NOT_SET" {
                self.echo_own_metadata_outcome(
                    network_id,
                    watch.destination.as_ref(),
                    &cleared_message(canonical),
                );
                if canonical == AVATAR_KEY {
                    self.persist_own_avatar(network_id, "");
                }
                continue;
            }
            let text = match watch.kind {
                OwnMetadataKind::Set => set_fail_message(canonical, &reason),
                OwnMetadataKind::Clear => clear_fail_message(canonical, &reason),
            };
            self.echo_own_metadata_outcome(network_id, watch.destination.as_ref(), &text);
        }
    }

    /// A confirmed METADATA value for a nick (761 or 766). It answers the own write only when
    /// the nick is this network's own nick and the value is the one the write sent, or empty
    /// for a clear (`routeOwnMetadataReply`).
    pub(super) fn route_own_metadata_reply(&mut self, network_id: &str, nick: &str, key: &str, value: &str) {
        if self.reply_watches.own_metadata_depth > 0 {
            self.reply_watches.pending_own_metadata.push(PendingOwnMetadata {
                network_id: network_id.to_owned(),
                nick: nick.to_owned(),
                key: key.to_owned(),
                value: value.to_owned(),
            });
            return;
        }
        let Some(canonical) = canonical_key(key) else {
            return;
        };
        let Some(watch) = self
            .reply_watches
            .own_metadata
            .get(network_id)
            .and_then(|watches| watches.get(canonical))
            .cloned()
        else {
            return;
        };
        let own_nick = self.current_nick(network_id);
        if own_nick.is_empty() {
            return;
        }
        if !self.reducer.server_features(network_id).case_mapping().equals(nick, &own_nick) {
            return;
        }

        match watch.kind {
            OwnMetadataKind::Clear => {
                if !value.is_empty() {
                    return;
                }
                self.echo_own_metadata_outcome(network_id, watch.destination.as_ref(), &cleared_message(canonical));
                if canonical == AVATAR_KEY {
                    self.persist_own_avatar(network_id, "");
                }
            }
            OwnMetadataKind::Set => {
                if value.is_empty() || watch.value != value {
                    return;
                }
                let text = set_message(canonical, value);
                self.echo_own_metadata_outcome(network_id, watch.destination.as_ref(), &text);
                if canonical == AVATAR_KEY {
                    self.persist_own_avatar(network_id, value);
                }
            }
        }
        self.remove_own_metadata_watch(network_id, canonical);
    }

    /// A 764, 767 or 769 that refuses one own write (`routeOwnMetadataError`). The first word
    /// of its text names the key; the text, or the numeric when there is none, is the reason.
    fn route_own_metadata_error(&mut self, entry: &StatusEntry) {
        if !self.reply_watches.own_metadata.contains_key(&entry.network_id) {
            return;
        }
        if !matches!(entry.label.as_str(), "764" | "767" | "769") {
            return;
        }
        let key_token = Words::split(&entry.text).first;
        if key_token.is_empty() {
            return;
        }
        let Some(canonical) = canonical_key(&key_token) else {
            return;
        };
        let Some(watch) = self
            .reply_watches
            .own_metadata
            .get(&entry.network_id)
            .and_then(|watches| watches.get(canonical))
            .cloned()
        else {
            return;
        };
        let reason = if entry.text.is_empty() {
            entry.label.as_str()
        } else {
            entry.text.as_str()
        };
        let text = match watch.kind {
            OwnMetadataKind::Set => set_fail_message(canonical, reason),
            OwnMetadataKind::Clear => clear_fail_message(canonical, reason),
        };
        self.echo_own_metadata_outcome(&entry.network_id, watch.destination.as_ref(), &text);
        self.remove_own_metadata_watch(&entry.network_id, canonical);
    }

    /// Stores the own avatar URL the server confirmed. The avatar store lands with the
    /// persistence phase; until then the reducer already holds the server's value for this
    /// nick, so this changes nothing.
    fn persist_own_avatar(&mut self, _network_id: &str, _url: &str) {}

    /// The selected conversation's target for a bare CTCP or WHOIS: a direct message's peer
    /// with its session, or the outcome that refuses it (`dispatchCtcp` and `dispatchWhois`).
    fn bare_nick_target(&self) -> Result<(String, Option<Session>), CommandOutcome> {
        if self.selected_is_closeable_direct() {
            return Ok((self.selected_target().to_owned(), self.selected_session()));
        }
        if self.selected.is_some() {
            return Err(CommandOutcome::WrongScope);
        }
        Err(CommandOutcome::Refused)
    }

    /// The destination a reply copies into: the selection on a conversation, nothing on
    /// Status. A conversation with no selection is a wrong scope.
    fn destination_for(&self, surface: ComposerSurface) -> Result<Option<ConversationKey>, CommandOutcome> {
        match surface {
            ComposerSurface::Status => Ok(None),
            ComposerSurface::Conversation => match self.selected.clone() {
                Some(selected) => Ok(Some(selected)),
                None => Err(CommandOutcome::WrongScope),
            },
        }
    }
}

/// The outcome for a verb whose network the surface cannot name: a conversation with no
/// selection is a wrong scope, and Status with no network is a refusal.
fn refused_or_wrong_scope(surface: ComposerSurface) -> CommandOutcome {
    match surface {
        ComposerSurface::Conversation => CommandOutcome::WrongScope,
        ComposerSurface::Status => CommandOutcome::Refused,
    }
}

/// One message parameter as text, or "" when it is absent (`parameterText`).
fn param_text(message: &IrcMessage, index: usize) -> String {
    message.params.get(index).map(|raw| wire_text(raw)).unwrap_or_default()
}

/// Whether an argument is exactly `clear` (`isOwnMetadataClearAlias`).
fn is_clear_alias(argument: &str) -> bool {
    let words = Words::split(argument);
    words.first.eq_ignore_ascii_case("clear") && words.rest.is_empty()
}

/// The success text for a cleared key (`ownMetadataClearedMessage`).
fn cleared_message(canonical: &str) -> String {
    match canonical {
        STATUS_KEY => "Standing status cleared.".to_owned(),
        AVATAR_KEY => "Avatar cleared.".to_owned(),
        _ => String::new(),
    }
}

/// The success text for a set key (`ownMetadataSetMessage`).
fn set_message(canonical: &str, value: &str) -> String {
    match canonical {
        STATUS_KEY => format!("Standing status set to {value}."),
        AVATAR_KEY => format!("Avatar set to {value}."),
        _ => String::new(),
    }
}

/// The failure text for a refused set (`ownMetadataSetFailMessage`).
fn set_fail_message(canonical: &str, reason: &str) -> String {
    match canonical {
        STATUS_KEY => format!("Could not set standing status: {reason}"),
        AVATAR_KEY => format!("Could not set avatar: {reason}"),
        _ => String::new(),
    }
}

/// The failure text for a refused clear (`ownMetadataClearFailMessage`).
fn clear_fail_message(canonical: &str, reason: &str) -> String {
    match canonical {
        STATUS_KEY => format!("Could not clear standing status: {reason}"),
        AVATAR_KEY => format!("Could not clear avatar: {reason}"),
        _ => String::new(),
    }
}

/// The text for a network without the capability (`ownMetadataNoCapMessage`).
fn no_capability_message(key: &str) -> String {
    match key {
        STATUS_KEY => "This network does not support standing status.".to_owned(),
        _ => "This network does not support avatars.".to_owned(),
    }
}

/// The text for a network whose value budget is zero (`ownMetadataNoValueMessage`).
fn no_value_message(key: &str) -> String {
    match key {
        STATUS_KEY => "This network does not allow standing status text.".to_owned(),
        _ => "This network does not allow avatar URLs.".to_owned(),
    }
}

/// The text for an inspect with nothing stored (`ownMetadataInspectEmptyMessage`).
fn inspect_empty_message(key: &str) -> String {
    match key {
        STATUS_KEY => "No standing status. Use /status <text> or /status clear.".to_owned(),
        _ => "No standing avatar. Use /avatar <url|email> or /avatar clear.".to_owned(),
    }
}

/// The text for an inspect with a stored value (`ownMetadataInspectValueMessage`).
fn inspect_value_message(key: &str, value: &str) -> String {
    match key {
        STATUS_KEY => format!("Standing status: {value}"),
        _ => format!("Standing avatar: {value}"),
    }
}

/// Whether a labeled standard reply is worth copying into the transcript: a FAIL, WARN or
/// NOTE, or a three-digit 4xx or 5xx (`copiesLabeledStandardReply`).
fn copies_labeled_standard_reply(label: &str) -> bool {
    if matches!(label, "FAIL" | "WARN" | "NOTE") {
        return true;
    }
    let bytes = label.as_bytes();
    bytes.len() == 3 && bytes.iter().all(u8::is_ascii_digit) && matches!(bytes[0], b'4' | b'5')
}

/// Substitutes `%1` and `%2` in `pattern`, scanning the pattern only, so a substituted
/// value is never expanded again (`replyFormatPair`).
fn format_pair(pattern: &str, first: &str, second: &str) -> String {
    let mut out = String::new();
    let mut rest = pattern;
    while let Some(index) = rest.find('%') {
        out.push_str(&rest[..index]);
        let after = &rest[index..];
        match after.as_bytes().get(1) {
            Some(b'1') => {
                out.push_str(first);
                rest = &after[2..];
            }
            Some(b'2') => {
                out.push_str(second);
                rest = &after[2..];
            }
            _ => {
                out.push('%');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}
