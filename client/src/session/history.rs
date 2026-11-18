//! the history requests of `tui/internal/session/session.go`.
//! The session asks for history in four shapes (`LATEST`, `BEFORE`, `AFTER`
//! `TARGETS`), tracks each request until its batch or its `FAIL` arrives, and
//! chains AFTER pages through the gap after a self-JOIN. Membership changes bump
//! a per-channel generation, so an answer that arrives after a PART or a
//! re-JOIN is recognised as stale. Every request is written through
//! `send_command`, so a send that the frame budget refuses leaves no state
//! behind.
//!
//! Each target has one `HistoryPhase`; `tui/internal/session/history_phase.go`
//! names the same phases. A request in
//! flight carries its kind, its membership generation, and, for AFTER only, the
//! page count and cursor, so none of them can disagree. A missing entry means
//! the target was never asked, or was forgotten by a PART or KICK and may be
//! asked again.

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

impl HistoryRequestKind {
    /// Whether an empty answer, or a FAIL, to this request means older history
    /// has run out. An AFTER page or its tail says nothing about older lines.
    pub(crate) fn exhausts_before(self) -> bool {
        matches!(self, Self::Latest | Self::Before)
    }
}

/// The exclusive bound of the next `CHATHISTORY AFTER` page. A msgid wins; a
/// time is used only when no line in the page carried a msgid.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct HistoryCursor {
    pub(crate) msgid: String,
    pub(crate) when: Option<DateTime<Utc>>,
}

/// What a request in flight carries. `kind` is the only mapping to
/// `HistoryRequestKind`; the AFTER page count and cursor exist only on `After`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InFlightRequest {
    Latest,
    Before,
    After { pages: u32, cursor: HistoryCursor },
    AfterTail,
}

impl InFlightRequest {
    pub(crate) fn kind(&self) -> HistoryRequestKind {
        match self {
            Self::Latest => HistoryRequestKind::Latest,
            Self::Before => HistoryRequestKind::Before,
            Self::After { .. } => HistoryRequestKind::After,
            Self::AfterTail => HistoryRequestKind::AfterTail,
        }
    }
}

/// Where one folded target stands. An absent entry means not yet asked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum HistoryPhase {
    /// Asked; nothing in flight; BEFORE is allowed.
    Settled,
    /// A request is outstanding, sent under `generation`.
    InFlight {
        generation: u64,
        request: InFlightRequest,
    },
    /// BEFORE stops. It cannot coexist with a request in flight.
    Exhausted,
}

/// Per-connection CHATHISTORY state, keyed by the folded channel name.
#[derive(Clone, Debug)]
pub(crate) struct HistoryLedger {
    /// The membership generation of each channel. A JOIN, PART or KICK bumps it.
    /// It outlives `phase` entries, so a PART forgets a target but not its count.
    pub(crate) generation: BTreeMap<String, u64>,
    pub(crate) phase: BTreeMap<String, HistoryPhase>,
    /// The AFTER pages chained before one LATEST bound closes the gap. Tests
    /// lower it; production keeps the default.
    pub(crate) after_page_cap: u32,
    /// A `CHATHISTORY TARGETS` request is outstanding. It is not a channel phase.
    pub(crate) targets_pending: bool,
}

impl HistoryLedger {
    pub(crate) fn new(after_page_cap: u32) -> Self {
        Self {
            generation: BTreeMap::new(),
            phase: BTreeMap::new(),
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

    /// The phase of a folded target, when it has been asked.
    fn history_phase(&self, folded: &str) -> Option<&HistoryPhase> {
        self.history.phase.get(folded)
    }

    /// Records a membership change: the request still in flight for this channel
    /// is now stale, so it settles in the same step the generation moves.
    pub(crate) fn bump_history_generation(&mut self, channel: &str) {
        let folded = self.fold_channel(channel);
        let next = self.history_generation(channel) + 1;
        self.history.generation.insert(folded.clone(), next);
        self.settle_inflight(&folded);
    }

    /// The request outstanding on `target` for the current generation.
    fn inflight_request(&self, target: &str) -> Option<&InFlightRequest> {
        if target.is_empty() {
            return None;
        }
        match self.history_phase(&self.fold_channel(target)) {
            Some(HistoryPhase::InFlight {
                generation,
                request,
            }) if *generation == self.history_generation(target) => Some(request),
            _ => None,
        }
    }

    /// Whether a request is outstanding on `target` for the current generation.
    pub(crate) fn history_inflight(&self, target: &str) -> bool {
        self.inflight_request(target).is_some()
    }

    /// Whether `channel` has a live request whose answer is not yet arriving as a
    /// batch (`answersPendingHistoryLocked`).
    pub(crate) fn answers_pending_history(&self, channel: &str) -> bool {
        self.history_inflight(channel) && !self.has_open_current_history_batch(channel)
    }

    /// The kind of the request `channel`'s next batch would answer, as
    /// `answers_pending_history` sees it. `open_batch` captures it.
    pub(crate) fn pending_history_kind(&self, channel: &str) -> Option<HistoryRequestKind> {
        if !self.answers_pending_history(channel) {
            return None;
        }
        self.inflight_request(channel).map(InFlightRequest::kind)
    }

    /// Stores `request` as in flight at the current generation and returns the
    /// phase it replaced, for `rollback_history`.
    fn begin_history(&mut self, key: &str, request: InFlightRequest) -> Option<HistoryPhase> {
        let generation = self.history_generation(key);
        self.history.phase.insert(
            key.to_owned(),
            HistoryPhase::InFlight {
                generation,
                request,
            },
        )
    }

    /// Undoes `begin_history` after a send the frame budget refused.
    fn rollback_history(&mut self, key: &str, previous: Option<HistoryPhase>) {
        match previous {
            Some(phase) => self.history.phase.insert(key.to_owned(), phase),
            None => self.history.phase.remove(key),
        };
    }

    /// Turns an in-flight request into `Settled` and returns it. Any other phase
    /// stays as it is.
    fn settle_inflight(&mut self, folded: &str) -> Option<InFlightRequest> {
        if !matches!(
            self.history.phase.get(folded),
            Some(HistoryPhase::InFlight { .. })
        ) {
            return None;
        }
        match self
            .history
            .phase
            .insert(folded.to_owned(), HistoryPhase::Settled)
        {
            Some(HistoryPhase::InFlight { request, .. }) => Some(request),
            _ => None,
        }
    }

    /// Settles the request a closing batch answers. The batch counts only when
    /// the target's phase is still the in-flight request it opened under: same
    /// generation, same kind. A late batch that disagrees leaves the phase alone.
    pub(crate) fn finish_history(
        &mut self,
        target: &str,
        opened: u64,
        opened_kind: Option<HistoryRequestKind>,
    ) -> Option<InFlightRequest> {
        let folded = self.fold_channel(target);
        let matches = opened == self.history_generation(target)
            && matches!(
                self.history_phase(&folded),
                Some(HistoryPhase::InFlight { generation, request })
                    if *generation == opened && Some(request.kind()) == opened_kind
            );
        if matches {
            self.settle_inflight(&folded)
        } else {
            None
        }
    }

    /// Stops BEFORE for a target whose request just settled with nothing left.
    pub(crate) fn exhaust_history(&mut self, folded: &str) {
        self.history
            .phase
            .insert(folded.to_owned(), HistoryPhase::Exhausted);
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

    /// Drops one channel's request in flight.
    pub(crate) fn clear_history_pending(&mut self, channel: &str) {
        let folded = self.fold_channel(channel);
        self.settle_inflight(&folded);
    }

    /// Forgets a channel's history after a PART or KICK.
    pub(crate) fn forget_channel_history(&mut self, channel: &str) {
        let folded = self.fold_channel(channel);
        self.history.phase.remove(&folded);
        self.drop_history_batches(channel);
    }

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
        for phase in self.history.phase.values_mut() {
            if matches!(phase, HistoryPhase::InFlight { .. }) {
                *phase = HistoryPhase::Settled;
            }
        }
        self.history.targets_pending = false;
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
            let Some(request) = self.settle_inflight(&folded) else {
                continue;
            };
            let kind = request.kind();
            let before = kind == HistoryRequestKind::Before;
            let after = kind == HistoryRequestKind::After;
            if kind.exhausts_before() {
                self.exhaust_history(&folded);
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
        if self.history.phase.contains_key(&key) {
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
        if self.history.phase.contains_key(&key) {
            return false;
        }
        self.begin_history(&key, InFlightRequest::Latest);
        let command = format!("CHATHISTORY LATEST {target} * {}", self.history_limit);
        if !self.send_command(&command, "") {
            self.rollback_history(&key, None);
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
        // From `Exhausted` this clears the exhaustion: the gap may hold lines
        // BEFORE has not seen, and the server answers an empty BEFORE again.
        let cursor = HistoryCursor {
            msgid: String::new(),
            when: Some(after),
        };
        self.send_history_after(target, 1, cursor)
    }

    /// Sends one `CHATHISTORY AFTER` page, number `pages` of the chain, from
    /// `cursor`. A refused send forgets the target, so it can be asked again.
    fn send_history_after(&mut self, target: &str, pages: u32, cursor: HistoryCursor) -> bool {
        let Some(selector) = history_selector(&cursor) else {
            return false;
        };
        let key = self.fold_channel(target);
        self.begin_history(&key, InFlightRequest::After { pages, cursor });
        let command = format!(
            "CHATHISTORY AFTER {target} {selector} {}",
            self.history_limit
        );
        if !self.send_command(&command, "") {
            self.rollback_history(&key, None);
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
        self.begin_history(&key, InFlightRequest::AfterTail);
        let command = format!(
            "CHATHISTORY LATEST {target} {selector} {}",
            self.history_limit
        );
        if !self.send_command(&command, "") {
            self.rollback_history(&key, None);
            return false;
        }
        true
    }

    /// Chains the next AFTER page when a full page arrived and the gap may not be
    /// closed yet (`maybeChainHistoryAfterLocked`). `pages` and `previous` are the
    /// page number and cursor of the AFTER request this batch answered.
    pub(crate) fn maybe_chain_history_after(
        &mut self,
        target: &str,
        batch: &HistoryBatch,
        history_ended: bool,
        pages: u32,
        previous: &HistoryCursor,
    ) {
        if target.is_empty() || history_ended || batch.lines.len() < self.history_limit {
            return;
        }
        let next = cursor_from_after_page(&batch.lines);
        if next.msgid.is_empty() && next.when.is_none() {
            return;
        }
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
            if let (Some(previous_when), Some(next_when)) = (previous.when, next.when) {
                if next_when <= previous_when {
                    return;
                }
            }
        }
        self.send_history_after(target, pages + 1, next);
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
        if matches!(self.history_phase(&key), Some(HistoryPhase::Exhausted))
            || self.history_inflight(target)
        {
            return false;
        }
        if self.features.is_channel(target) && !self.history.phase.contains_key(&key) {
            return false;
        }
        let cursor = if !msgid.is_empty() {
            format!("msgid={msgid}")
        } else if let Some(when) = timestamp {
            format!("timestamp={}", format_chat_history_timestamp(when))
        } else {
            return false;
        };
        let previous = self.begin_history(&key, InFlightRequest::Before);
        let command = format!(
            "CHATHISTORY BEFORE {target} {cursor} {}",
            self.history_limit
        );
        if !self.send_command(&command, "") {
            self.rollback_history(&key, previous);
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
