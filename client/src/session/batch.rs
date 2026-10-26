//! `OpenBatch` bookkeeping of `tui/internal/session/session.go`.
//! A `BATCH` opens a reference, its lines name it in `@batch=`, and `BATCH -ref`
//! closes it. The ledger decides which batches are replay answers worth
//! collecting (`chathistory`, `draft/chathistory-targets`, `znc.in/playback`)
//! which are ignored, and which share one open-batch budget, so a hostile
//! server cannot grow the ledger without bound. Lines inside a collected batch
//! are swallowed; the completed batch is emitted as one `HistoryBatchReceived`.

use std::collections::{BTreeMap, BTreeSet};

use super::events::{HistoryBatch, HistoryKind, HistoryTarget, SessionEvent};
use super::history::HistoryRequestKind;
use super::session::{
    replay_kind_for, Core, HISTORY_BUFFER_CEILING, MAX_IGNORED_BATCHES, MAX_OPEN_BATCHES,
};
use crate::irc::capability::IrcCapability;
use crate::irc::message::IrcMessage;

/// One batch the session is collecting, or a child of one.
#[derive(Clone, Debug, Default)]
pub(crate) struct OpenBatch {
    /// The reference of the batch that owns the collected lines. Equal to the
    /// batch's own reference when it is a replay root, empty when it is not
    /// part of a replay.
    pub(crate) replay_root: String,
    /// The replay kind, set on a replay root only.
    pub(crate) kind: Option<HistoryKind>,
    /// The target, and the lines collected so far.
    pub(crate) collected: HistoryBatch,
    /// The membership generation of the target when the batch opened.
    pub(crate) generation: u64,
    /// The labeled-response request this batch answers, if any.
    pub(crate) request_label: String,
    /// `draft/chathistory-end` was on the opening `BATCH`.
    pub(crate) history_ended: bool,
    /// The CHATHISTORY request this batch answers, captured on open.
    pub(crate) history_request: Option<HistoryRequestKind>,
}

/// The open and ignored batch references of one connection.
#[derive(Clone, Debug, Default)]
pub(crate) struct BatchLedger {
    pub(crate) open: BTreeMap<String, OpenBatch>,
    pub(crate) ignored: BTreeSet<String>,
}

impl Core {
    /// Opens (`+ref`) or closes (`-ref`) a batch (`handleBatchLocked`).
    pub(crate) fn handle_batch(&mut self, message: &IrcMessage) {
        if message.params.is_empty() {
            return;
        }
        let token = message.text_param(0);
        if token.len() < 2 {
            return;
        }
        // The sign is one character; a multi-byte first character is not a sign.
        let Some(sign) = token.chars().next().filter(|sign| *sign == '+' || *sign == '-') else {
            return;
        };
        let reference = token[sign.len_utf8()..].to_owned();
        if reference.is_empty() {
            return;
        }

        if sign == '-' {
            self.close_batch(&reference);
            return;
        }

        let parent = message.tag_text("batch").unwrap_or_default();
        let batch_type = message.text_param(1);
        let carried_label = message.tag_text("label").unwrap_or_default();
        let labeled_response = batch_type.eq_ignore_ascii_case("labeled-response");
        if self.batches.open.contains_key(&reference) {
            return;
        }

        let kind = replay_kind_for(&batch_type);
        if let Some(kind) = kind {
            if !self.replay_enabled(kind) {
                self.ignore_batch(&reference);
                return;
            }
        }

        // A history batch that answers a request still outstanding, or a
        // labeled-response batch whose opening label is pending, is never
        // crowded out. Everything else shares the open-batch budget.
        let target_arg = message.text_param(2);
        let solicited_targets = kind == Some(HistoryKind::Targets) && self.history.targets_pending;
        let solicited = solicited_targets
            || (kind == Some(HistoryKind::Chat) && self.answers_pending_history(&target_arg));
        let pending_labeled_response =
            labeled_response && self.has_pending_request_label(&carried_label);
        let over_open_cap =
            !solicited && !pending_labeled_response && self.batches.open.len() >= MAX_OPEN_BATCHES;
        let ignored = self.batches.ignored.contains(&reference);
        let parent_ignored = !parent.is_empty() && self.batches.ignored.contains(&parent);
        if ignored
            || parent_ignored
            || (over_open_cap && self.is_history_batch(&batch_type, &parent))
        {
            self.ignore_batch(&reference);
            if kind == Some(HistoryKind::Chat) {
                self.clear_history_pending(&target_arg);
            }
            if kind == Some(HistoryKind::Targets) {
                self.history.targets_pending = false;
            }
            return;
        }
        if over_open_cap {
            return;
        }

        let mut frame = OpenBatch::default();
        if labeled_response {
            frame.request_label = carried_label;
        }
        let parent_root = self
            .batches
            .open
            .get(&parent)
            .map(|batch| batch.replay_root.clone())
            .unwrap_or_default();
        if !parent_root.is_empty() {
            frame.replay_root = parent_root;
        } else if kind.is_some() {
            frame.replay_root = reference.clone();
        }
        if frame.replay_root == reference {
            frame.kind = kind;
            frame.history_ended = message.has_tag("draft/chathistory-end");
            frame.collected.target = target_arg.clone();
            frame.generation = self.history_generation(&frame.collected.target);
            if kind == Some(HistoryKind::Chat) && self.answers_pending_history(&target_arg) {
                let folded = self.fold_channel(&target_arg);
                if let Some(request) = self.history.pending_kind.get(&folded) {
                    frame.history_request = Some(*request);
                }
            }
        }
        self.batches.open.insert(reference, frame);
    }

    /// Closes one batch and delivers what it collected (`closeBatchLocked`).
    fn close_batch(&mut self, reference: &str) {
        if self.batches.ignored.remove(reference) {
            return;
        }
        let Some(frame) = self.batches.open.remove(reference) else {
            return;
        };

        let children: Vec<String> = self
            .batches
            .open
            .iter()
            .filter(|(_, batch)| batch.replay_root == reference)
            .map(|(child, _)| child.clone())
            .collect();
        for child in &children {
            self.batches.open.remove(child);
            self.ignore_batch(child);
        }

        if !frame.request_label.is_empty() {
            self.finish_request_label(&frame.request_label);
        }

        let is_root = frame.replay_root == reference;
        if is_root && frame.kind == Some(HistoryKind::Targets) {
            self.history.targets_pending = false;
            let batch = HistoryBatch {
                kind: Some(HistoryKind::Targets),
                history_ended: frame.history_ended,
                targets: history_targets_from(&frame.collected.lines),
                ..HistoryBatch::default()
            };
            let network_id = self.network_id();
            self.emit(SessionEvent::HistoryBatchReceived { network_id, batch });
            return;
        }

        if !(is_root && !frame.collected.target.is_empty()) {
            return;
        }
        let target = frame.collected.target.clone();
        let current = frame.generation == self.history_generation(&target);
        let folded = self.fold_channel(&target);
        let after = frame.history_request == Some(HistoryRequestKind::After);
        let after_tail = frame.history_request == Some(HistoryRequestKind::AfterTail);
        if current && frame.kind == Some(HistoryKind::Chat) && frame.history_request.is_some() {
            self.history.pending.remove(&folded);
            self.history.pending_kind.remove(&folded);
            // An empty AFTER, and the one LATEST sent when the AFTER page cap is
            // hit, are not the end of older history. Scrolling up must still send
            // BEFORE.
            if frame.collected.lines.is_empty() && !after && !after_tail {
                self.history.exhausted.insert(folded.clone());
            }
        }
        // A self-JOIN bumps the generation while a playback batch can still be
        // open. That batch is not a CHATHISTORY answer, so it is delivered.
        if frame.kind != Some(HistoryKind::BouncerPlayback) && !current {
            return;
        }

        let mut batch = frame.collected.clone();
        batch.kind = frame.kind;
        if frame.kind == Some(HistoryKind::Chat) {
            batch.older_page = frame.history_request == Some(HistoryRequestKind::Before);
            self.history.pending_before.remove(&folded);
            if current && after {
                self.maybe_chain_history_after(&target, &batch, frame.history_ended);
            }
            batch.after_request = current && after;
            batch.history_ended = frame.history_ended;
        }
        let network_id = self.network_id();
        self.emit(SessionEvent::HistoryBatchReceived { network_id, batch });
    }

    /// Collects a tagged line into its replay root, when its batch is one
    /// (`captureInBatchLocked`). Returns true when the line was swallowed.
    pub(crate) fn capture_in_batch(&mut self, message: &IrcMessage) -> bool {
        let batch = message.tag_text("batch").unwrap_or_default();
        if batch.is_empty() {
            return false;
        }
        if self.batches.ignored.contains(&batch) {
            return true;
        }
        let Some(found) = self.batches.open.get(&batch) else {
            return false;
        };
        if found.replay_root.is_empty() {
            return false;
        }
        let root_key = found.replay_root.clone();
        let Some(root) = self.batches.open.get_mut(&root_key) else {
            return false;
        };
        // Playback and LATEST are oldest-first. A full buffer keeps the newest
        // lines. Overflow stays swallowed, so it cannot surface as live traffic.
        if root.collected.lines.len() >= HISTORY_BUFFER_CEILING {
            root.collected.lines.remove(0);
        }
        root.collected.lines.push(message.clone());
        true
    }

    /// Whether a replay kind may be collected: `batch` is needed for every kind
    /// and `chathistory` on top for a CHATHISTORY answer (`replayEnabledLocked`).
    /// Bouncer playback needs only `batch`.
    pub(crate) fn replay_enabled(&self, kind: HistoryKind) -> bool {
        let enabled = self.caps.enabled();
        if !enabled.contains(IrcCapability::Batch) {
            return false;
        }
        if kind == HistoryKind::BouncerPlayback {
            return true;
        }
        enabled.contains(IrcCapability::ChatHistory)
    }

    /// Remembers a reference whose lines must be swallowed (`ignoreBatchLocked`).
    /// At the ceiling new references are forgotten rather than tagged lines
    /// discarded unaccounted.
    pub(crate) fn ignore_batch(&mut self, reference: &str) {
        if reference.is_empty() {
            return;
        }
        self.batches.open.remove(reference);
        if self.batches.ignored.contains(reference) {
            return;
        }
        if self.batches.ignored.len() >= MAX_IGNORED_BATCHES {
            return;
        }
        self.batches.ignored.insert(reference.to_owned());
    }

    /// Whether a batch is a replay batch or sits inside one
    /// (`isHistoryBatchLocked`).
    pub(crate) fn is_history_batch(&self, batch_type: &str, parent: &str) -> bool {
        if replay_kind_for(batch_type).is_some() {
            return true;
        }
        if parent.is_empty() {
            return false;
        }
        if self.batches.ignored.contains(parent) {
            return true;
        }
        self.batches
            .open
            .get(parent)
            .is_some_and(|batch| !batch.replay_root.is_empty())
    }
}

/// Reads the rows of a `draft/chathistory-targets` batch (`historyTargetsFrom`).
fn history_targets_from(lines: &[IrcMessage]) -> Vec<HistoryTarget> {
    lines
        .iter()
        .filter(|line| line.command.eq_ignore_ascii_case("CHATHISTORY"))
        .filter(|line| line.text_param(0).eq_ignore_ascii_case("TARGETS"))
        .filter_map(|line| {
            let name = line.text_param(1);
            if name.is_empty() {
                return None;
            }
            Some(HistoryTarget {
                name,
                latest: super::history::parse_history_target_time(&line.text_param(2)),
            })
        })
        .collect()
}
