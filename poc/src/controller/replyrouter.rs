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
//! Not here yet: the METADATA reply routes (761, 766 and FAIL for the user's own
//! `/status` and `/avatar`) and the `/status` and `/avatar` dispatch. Those land with the
//! slash dispatcher.

use std::collections::BTreeMap;

use crate::irc::event::{ConversationKey, Event, WhoisTranscriptEvent};
use crate::irc::presence::{
    AVATAR_KEY, BOT_KEY, COLOR_KEY, DISPLAY_NAME_KEY, HOMEPAGE_KEY, PRONOUNS_KEY, STATUS_KEY,
};
use crate::irc::statusentry::{self, CtcpReplyLine, LogSeverity, StatusEntry, WhoisLine, WhoisProgress};
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

/// The reply watches the router holds (`IrcReplyRouter`'s watch maps).
#[derive(Default)]
pub(super) struct ReplyWatches {
    whois: BTreeMap<WhoisKey, WhoisWatch>,
    ctcp: BTreeMap<CtcpKey, CtcpWatch>,
    labeled: BTreeMap<LabeledKey, LabeledWatch>,
}

#[cfg(test)]
impl ReplyWatches {
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

    fn apply_whois_transcript(&mut self, destination: &ConversationKey, body: &str) {
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
