//! the history requests of `tui/internal/session/session.go`.
//! The session asks for history in four shapes (`LATEST`, `BEFORE`, `AFTER`
//! `TARGETS`), tracks each request until its batch or its `FAIL` arrives, and
//! chains AFTER pages through the gap after a self-JOIN. Membership changes bump
//! a per-channel generation, so an answer that arrives after a PART or a
//! re-JOIN is recognised as stale. Every request is written through
//! `send_command`, so a send that the frame budget refuses leaves no state
//! behind.

use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};

use super::events::{HistoryBatch, HistoryKind, SessionEvent};
use super::session::{Core, HISTORY_AFTER_PAGE_CAP};
use crate::irc::message::IrcMessage;
use crate::irc::translator::server_time_of;

/// The CHATHISTORY subcommand a pending request was sent as.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum HistoryRequestKind {
    /// `CHATHISTORY LATEST`, the newest page of a target.
    #[default]
    Latest,
    /// `CHATHISTORY BEFORE`, scrolling back.
    Before,
    /// `CHATHISTORY AFTER`, one page of the gap after a self-JOIN.
    After,
    /// The `LATEST` sent when the AFTER page cap is reached. It is not the end
    /// of older history.
    AfterTail,
}

/// The exclusive bound of the next `CHATHISTORY AFTER` page. A msgid wins; a
/// time is used only when no line in the page carried a msgid.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HistoryCursor {
    pub(crate) msgid: String,
    pub(crate) when: Option<DateTime<Utc>>,
}

/// Per-connection CHATHISTORY state, keyed by the folded channel name.
#[derive(Clone, Debug)]
pub(crate) struct HistoryLedger {
    /// The membership generation of each channel. A JOIN, PART or KICK bumps it.
    pub(crate) generation: BTreeMap<String, u64>,
    /// Channels that have been asked for history since they were joined.
    pub(crate) asked: BTreeSet<String>,
    /// The generation each outstanding request was sent under.
    pub(crate) pending: BTreeMap<String, u64>,
    pub(crate) pending_before: BTreeSet<String>,
    pub(crate) pending_kind: BTreeMap<String, HistoryRequestKind>,
    /// Channels whose history has run out, so BEFORE stops.
    pub(crate) exhausted: BTreeSet<String>,
    pub(crate) after_pages: BTreeMap<String, u32>,
    pub(crate) after_cursor: BTreeMap<String, HistoryCursor>,
    /// The AFTER pages chained before one LATEST bound closes the gap. Tests
    /// lower it; production keeps the default.
    pub(crate) after_page_cap: u32,
    /// A `CHATHISTORY TARGETS` request is outstanding.
    pub(crate) targets_pending: bool,
}

impl HistoryLedger {
    pub(crate) fn new(after_page_cap: u32) -> Self {
        Self {
            generation: BTreeMap::new(),
            asked: BTreeSet::new(),
            pending: BTreeMap::new(),
            pending_before: BTreeSet::new(),
            pending_kind: BTreeMap::new(),
            exhausted: BTreeSet::new(),
            after_pages: BTreeMap::new(),
            after_cursor: BTreeMap::new(),
            after_page_cap,
            targets_pending: false,
        }
    }

    /// Clears per-connection state. The page cap is a test setting, so it stays.
    pub(crate) fn reset(&mut self) {
        let cap = self.after_page_cap;
        *self = Self::new(cap);
    }
}

impl Core {
    /// The membership generation of `channel`.
    pub(crate) fn history_generation(&self, channel: &str) -> u64 {
        self.history
            .generation
            .get(&self.fold_channel(channel))
            .copied()
            .unwrap_or(0)
    }

    /// Records a membership change: the answers still in flight for this channel
    /// are now stale.
    pub(crate) fn bump_history_generation(&mut self, channel: &str) {
        let folded = self.fold_channel(channel);
        let next = self.history_generation(channel) + 1;
        self.history.generation.insert(folded.clone(), next);
        self.history.pending.remove(&folded);
        self.history.pending_before.remove(&folded);
        self.history.pending_kind.remove(&folded);
    }

    /// Whether a request is outstanding on `target` for the current generation.
    pub(crate) fn history_inflight(&self, target: &str) -> bool {
        if target.is_empty() {
            return false;
        }
        let folded = self.fold_channel(target);
        self.history
            .pending
            .get(&folded)
            .is_some_and(|generation| *generation == self.history_generation(target))
    }

    /// Whether `channel` has a live request whose answer is not yet arriving as a
    /// batch (`answersPendingHistoryLocked`).
    pub(crate) fn answers_pending_history(&self, channel: &str) -> bool {
        if channel.is_empty() {
            return false;
        }
        let folded = self.fold_channel(channel);
        let current = self
            .history
            .pending
            .get(&folded)
            .is_some_and(|generation| *generation == self.history_generation(channel));
        current && !self.has_open_current_history_batch(channel)
    }

    /// Whether a CHATHISTORY batch for `channel` is open at the current
    /// generation.
    pub(crate) fn has_open_current_history_batch(&self, channel: &str) -> bool {
        let folded = self.fold_channel(channel);
        let generation = self.history_generation(channel);
        self.batches.open.iter().any(|(reference, batch)| {
            batch.replay_root == *reference
                && batch.kind == Some(HistoryKind::Chat)
                && self.fold_channel(&batch.collected.target) == folded
                && batch.generation == generation
        })
    }

    /// Clears the pending state of one channel's request.
    pub(crate) fn clear_history_pending(&mut self, channel: &str) {
        let folded = self.fold_channel(channel);
        self.history.pending.remove(&folded);
        self.history.pending_before.remove(&folded);
        self.history.pending_kind.remove(&folded);
    }

    /// Forgets a channel's history after a PART or KICK.
    pub(crate) fn forget_channel_history(&mut self, channel: &str) {
        let folded = self.fold_channel(channel);
        self.history.asked.remove(&folded);
        self.history.pending.remove(&folded);
        self.history.pending_before.remove(&folded);
        self.history.pending_kind.remove(&folded);
        self.history.exhausted.remove(&folded);
        self.history.after_pages.remove(&folded);
        self.history.after_cursor.remove(&folded);
        self.drop_history_batches(channel);
    }

    /// Drops the open replay batches of one channel and their children.
    fn drop_history_batches(&mut self, channel: &str) {
        let folded = self.fold_channel(channel);
        let roots: BTreeSet<String> = self
            .batches
            .open
            .iter()
            .filter(|(reference, batch)| {
                batch.replay_root == **reference
                    && self.fold_channel(&batch.collected.target) == folded
            })
            .map(|(reference, _)| reference.clone())
            .collect();
        if roots.is_empty() {
            return;
        }
        let gone: Vec<String> = self
            .batches
            .open
            .iter()
            .filter(|(reference, batch)| {
                roots.contains(*reference) || roots.contains(&batch.replay_root)
            })
            .map(|(reference, _)| reference.clone())
            .collect();
        for reference in &gone {
            self.batches.open.remove(reference);
        }
        for reference in &gone {
            self.ignore_batch(reference);
        }
    }

    /// Abandons every history request after a CAP DEL removed the capability
    /// (`abandonHistoryRequestsLocked`).
    pub(crate) fn abandon_history_requests(&mut self) {
        let targets: BTreeSet<String> = self
            .batches
            .open
            .iter()
            .filter(|(reference, batch)| {
                batch.replay_root == **reference && !batch.collected.target.is_empty()
            })
            .map(|(_, batch)| batch.collected.target.clone())
            .collect();
        for target in targets {
            self.drop_history_batches(&target);
        }
        self.history.pending.clear();
        self.history.pending_before.clear();
        self.history.pending_kind.clear();
        self.history.targets_pending = false;
        self.history.after_pages.clear();
        self.history.after_cursor.clear();
    }

    /// Handles a `FAIL CHATHISTORY` line (`handleChatHistoryFailLocked`). The
    /// request it answers is found by its channel argument, never by position.
    pub(crate) fn handle_chat_history_fail(&mut self, message: &IrcMessage) {
        if !message.text_param(0).eq_ignore_ascii_case("CHATHISTORY") {
            return;
        }
        if message.params.len() < 4 {
            return;
        }
        let network_id = self.network_id();
        if message.text_param(2).eq_ignore_ascii_case("TARGETS") {
            self.history.targets_pending = false;
            self.emit(SessionEvent::ChatHistoryFailed {
                network_id,
                subcommand: "TARGETS",
                target: String::new(),
            });
            return;
        }
        // FAIL CHATHISTORY <code> <subcommand> [<target>] [<context>]:description.
        // The description is skipped, so a channel named in prose cannot cancel
        // a different request.
        for index in 2..message.params.len().saturating_sub(1) {
            let channel = message.text_param(index);
            if !self.answers_pending_history(&channel) {
                continue;
            }
            let folded = self.fold_channel(&channel);
            let before = self.history.pending_before.contains(&folded);
            let kind = self
                .history
                .pending_kind
                .get(&folded)
                .copied()
                .unwrap_or_default();
            let after = kind == HistoryRequestKind::After;
            self.history.pending.remove(&folded);
            self.history.pending_before.remove(&folded);
            self.history.pending_kind.remove(&folded);
            if kind != HistoryRequestKind::After && kind != HistoryRequestKind::AfterTail {
                self.history.exhausted.insert(folded);
            }
            self.emit(SessionEvent::ChatHistoryRequestFinished {
                network_id: network_id.clone(),
                target: channel.clone(),
                failed: true,
                before,
            });
            if after {
                self.emit(SessionEvent::ChatHistoryFailed {
                    network_id,
                    subcommand: "AFTER",
                    target: channel,
                });
            }
            return;
        }
    }

    /// Asks for the history a self-JOIN should show: AFTER the registration
    /// snapshot when the resume lookup knows one, else the newest page
    /// (`requestChannelHistoryLocked`).
    pub(crate) fn request_channel_history(&mut self, channel: &str) {
        if channel.is_empty() || !self.replay_enabled(HistoryKind::Chat) {
            return;
        }
        let key = self.fold_channel(channel);
        if self.history.asked.contains(&key) {
            return;
        }
        let mapping = self.features.case_mapping();
        let resumed = self
            .history_resume
            .as_ref()
            .and_then(|resume| resume(channel, mapping));
        if let Some(when) = resumed {
            if self.request_history_after(channel, when) {
                return;
            }
        }
        self.request_history_latest(channel);
    }

    /// Sends `CHATHISTORY LATEST <target> * <limit>` once per channel and
    /// generation.
    pub(crate) fn request_history_latest(&mut self, target: &str) -> bool {
        if target.is_empty() || !self.replay_enabled(HistoryKind::Chat) {
            return false;
        }
        let key = self.fold_channel(target);
        if self.history.asked.contains(&key) || self.history_inflight(target) {
            return false;
        }
        self.history.asked.insert(key.clone());
        let generation = self.history_generation(target);
        self.history.pending.insert(key.clone(), generation);
        self.history
            .pending_kind
            .insert(key.clone(), HistoryRequestKind::Latest);
        let command = format!("CHATHISTORY LATEST {target} * {}", self.history_limit);
        if !self.send_command(&command, "") {
            self.history.asked.remove(&key);
            self.history.pending.remove(&key);
            self.history.pending_kind.remove(&key);
            return false;
        }
        true
    }

    /// Starts an AFTER chain from `after` (`requestHistoryAfterLocked`).
    pub(crate) fn request_history_after(&mut self, target: &str, after: DateTime<Utc>) -> bool {
        if target.is_empty() || !self.replay_enabled(HistoryKind::Chat) {
            return false;
        }
        if self.history_inflight(target) {
            return false;
        }
        let key = self.fold_channel(target);
        self.history.asked.insert(key.clone());
        self.history.after_pages.insert(key.clone(), 1);
        let cursor = HistoryCursor {
            msgid: String::new(),
            when: Some(after),
        };
        self.history.after_cursor.insert(key, cursor.clone());
        self.send_history_after(target, &cursor)
    }

    fn clear_history_after_attempt(&mut self, key: &str) {
        self.history.pending.remove(key);
        self.history.pending_kind.remove(key);
        self.history.asked.remove(key);
        self.history.after_pages.remove(key);
        self.history.after_cursor.remove(key);
    }

    /// Sends one `CHATHISTORY AFTER` page from `cursor`.
    fn send_history_after(&mut self, target: &str, cursor: &HistoryCursor) -> bool {
        let Some(selector) = history_selector(cursor) else {
            return false;
        };
        let key = self.fold_channel(target);
        let generation = self.history_generation(target);
        self.history.pending.insert(key.clone(), generation);
        self.history
            .pending_kind
            .insert(key.clone(), HistoryRequestKind::After);
        let command = format!(
            "CHATHISTORY AFTER {target} {selector} {}",
            self.history_limit
        );
        if !self.send_command(&command, "") {
            self.clear_history_after_attempt(&key);
            return false;
        }
        true
    }

    /// Sends the closing `CHATHISTORY LATEST` bound once the AFTER page cap is
    /// reached (`sendHistoryLatestBoundLocked`).
    fn send_history_latest_bound(&mut self, target: &str, cursor: &HistoryCursor) -> bool {
        let Some(selector) = history_selector(cursor) else {
            return false;
        };
        let key = self.fold_channel(target);
        let generation = self.history_generation(target);
        self.history.pending.insert(key.clone(), generation);
        self.history
            .pending_kind
            .insert(key.clone(), HistoryRequestKind::AfterTail);
        let command = format!(
            "CHATHISTORY LATEST {target} {selector} {}",
            self.history_limit
        );
        if !self.send_command(&command, "") {
            self.clear_history_after_attempt(&key);
            return false;
        }
        true
    }

    /// Chains the next AFTER page when a full page arrived and the gap may not be
    /// closed yet (`maybeChainHistoryAfterLocked`).
    pub(crate) fn maybe_chain_history_after(
        &mut self,
        target: &str,
        batch: &HistoryBatch,
        history_ended: bool,
    ) {
        if target.is_empty() || history_ended || batch.lines.len() < self.history_limit {
            return;
        }
        let next = cursor_from_after_page(&batch.lines);
        if next.msgid.is_empty() && next.when.is_none() {
            return;
        }
        let key = self.fold_channel(target);
        let pages = self
            .history
            .after_pages
            .get(&key)
            .copied()
            .unwrap_or(1)
            .max(1);
        let cap = if self.history.after_page_cap == 0 {
            HISTORY_AFTER_PAGE_CAP
        } else {
            self.history.after_page_cap
        };
        if pages >= cap {
            self.send_history_latest_bound(target, &next);
            return;
        }
        // A timestamp bound that does not move would ask for the same page again.
        // A msgid always moves, even when two lines share a time.
        if next.msgid.is_empty() {
            let previous = self
                .history
                .after_cursor
                .get(&key)
                .cloned()
                .unwrap_or_default();
            if let (Some(previous_when), Some(next_when)) = (previous.when, next.when) {
                if next_when <= previous_when {
                    return;
                }
            }
        }
        self.history.after_pages.insert(key.clone(), pages + 1);
        self.history.after_cursor.insert(key, next.clone());
        self.send_history_after(target, &next);
    }

    /// `CHATHISTORY TARGETS` between two instants (`requestHistoryTargetsLocked`).
    pub(crate) fn request_history_targets(
        &mut self,
        from: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> bool {
        if !self.replay_enabled(HistoryKind::Chat) || self.history.targets_pending {
            return false;
        }
        if until <= from {
            return false;
        }
        let command = format!(
            "CHATHISTORY TARGETS timestamp={} timestamp={} {}",
            format_chat_history_timestamp(from),
            format_chat_history_timestamp(until),
            self.history_limit
        );
        if !self.send_command(&command, "") {
            return false;
        }
        self.history.targets_pending = true;
        true
    }

    /// Asks for one page older than `msgid` or `timestamp`, unless the channel is
    /// exhausted or a request is already in flight (`requestOlderHistoryLocked`).
    pub(crate) fn request_older_history(
        &mut self,
        target: &str,
        msgid: &str,
        timestamp: Option<DateTime<Utc>>,
    ) -> bool {
        if target.is_empty() || !self.replay_enabled(HistoryKind::Chat) {
            return false;
        }
        let key = self.fold_channel(target);
        if self.history.exhausted.contains(&key) || self.history_inflight(target) {
            return false;
        }
        if self.features.is_channel(target) && !self.history.asked.contains(&key) {
            return false;
        }
        let cursor = if !msgid.is_empty() {
            format!("msgid={msgid}")
        } else if let Some(when) = timestamp {
            format!("timestamp={}", format_chat_history_timestamp(when))
        } else {
            return false;
        };
        let generation = self.history_generation(target);
        self.history.pending.insert(key.clone(), generation);
        self.history.pending_before.insert(key.clone());
        self.history
            .pending_kind
            .insert(key.clone(), HistoryRequestKind::Before);
        let command = format!(
            "CHATHISTORY BEFORE {target} {cursor} {}",
            self.history_limit
        );
        if !self.send_command(&command, "") {
            self.history.pending.remove(&key);
            self.history.pending_before.remove(&key);
            self.history.pending_kind.remove(&key);
            return false;
        }
        true
    }
}

/// `2006-01-02T15:04:05.000Z`, in UTC.
pub(crate) fn format_chat_history_timestamp(when: DateTime<Utc>) -> String {
    when.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

/// The `msgid=` or `timestamp=` selector of a cursor, or `None` when empty.
fn history_selector(cursor: &HistoryCursor) -> Option<String> {
    if !cursor.msgid.is_empty() {
        return Some(format!("msgid={}", cursor.msgid));
    }
    cursor
        .when
        .map(|when| format!("timestamp={}", format_chat_history_timestamp(when)))
}

/// The cursor for the next AFTER page: the last line's msgid, else the newest
/// msgid in the page, else the last server time (`cursorFromAfterPage`).
fn cursor_from_after_page(lines: &[IrcMessage]) -> HistoryCursor {
    let Some(last) = lines.last() else {
        return HistoryCursor::default();
    };
    let last_msgid = lines
        .iter()
        .map(IrcMessage::msgid)
        .filter(|id| !id.is_empty())
        .last()
        .unwrap_or_default();
    let tail_msgid = last.msgid();
    if !tail_msgid.is_empty() {
        return HistoryCursor {
            msgid: tail_msgid,
            when: None,
        };
    }
    if !last_msgid.is_empty() {
        return HistoryCursor {
            msgid: last_msgid,
            when: None,
        };
    }
    if let Some(when) = server_time_of(last) {
        return HistoryCursor {
            msgid: String::new(),
            when: Some(when),
        };
    }
    lines
        .iter()
        .rev()
        .find_map(server_time_of)
        .map(|when| HistoryCursor {
            msgid: String::new(),
            when: Some(when),
        })
        .unwrap_or_default()
}

/// `CHATHISTORY TARGETS` time reader. It lives in the portable translator with the
/// other inbound time rules; re-exported so the session keeps its path.
pub(crate) use crate::irc::translator::parse_history_target_time;
