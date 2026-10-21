//! Port of the labeled-request half of `tui/internal/session/session.go` (Go)
//! and the request-label table of `src/irc/ircsession.cpp` (C++).
//!
//! A labeled request carries an `@label=` tag the server echoes on each reply
//! (`labeled-response`). Each outstanding label has a deadline; a reply, a
//! `FAIL`, an `ACK`, a disconnect or the deadline finishes it, and each finish
//! emits one `RequestLabelFinished` so the UI can close its waiter.

use chrono::{DateTime, Utc};

use super::events::SessionEvent;
use super::session::{Core, TimerSlot};

impl Core {
    /// Starts a labeled request and returns its label (`beginLabeledRequestLocked`).
    pub(crate) fn begin_labeled_request(&mut self) -> String {
        self.next_request_label += 1;
        let label = format!("lr{}", self.next_request_label);
        let deadline = self.now() + chrono::TimeDelta::milliseconds(
            i64::try_from(self.config.labeled_response_timeout_ms).unwrap_or(i64::MAX),
        );
        self.pending_labels.insert(label.clone(), deadline);
        self.arm_label_timer();
        label
    }

    /// Whether `label` is outstanding (`hasPendingRequestLabelLocked`). An empty
    /// label never is.
    pub(crate) fn has_pending_request_label(&self, label: &str) -> bool {
        !label.is_empty() && self.pending_labels.contains_key(label)
    }

    /// Drops a label without notifying (`dropRequestLabelLocked`).
    pub(crate) fn drop_request_label(&mut self, label: &str) {
        if label.is_empty() || self.pending_labels.remove(label).is_none() {
            return;
        }
        self.arm_label_timer();
    }

    /// Finishes a label and notifies once (`finishRequestLabelLocked`). A label
    /// that is not outstanding is ignored, so a late reply cannot double-notify.
    pub(crate) fn finish_request_label(&mut self, label: &str) {
        if label.is_empty() || self.pending_labels.remove(label).is_none() {
            return;
        }
        self.arm_label_timer();
        let network_id = self.network_id();
        self.emit(SessionEvent::RequestLabelFinished {
            network_id,
            label: label.to_owned(),
        });
    }

    /// Clears every outstanding label. With `notify`, each one emits a finish, in
    /// label order (`clearPendingRequestLabelsLocked`).
    pub(crate) fn clear_pending_request_labels(&mut self, notify: bool) {
        let labels: Vec<String> = self.pending_labels.keys().cloned().collect();
        self.pending_labels.clear();
        self.cancel_timer(TimerSlot::Label);
        if !notify {
            return;
        }
        let network_id = self.network_id();
        for label in labels {
            self.emit(SessionEvent::RequestLabelFinished {
                network_id: network_id.clone(),
                label,
            });
        }
    }

    /// Arms the label timer for the earliest deadline, or disarms it when none
    /// is outstanding (`armLabelTimerLocked`).
    fn arm_label_timer(&mut self) {
        let earliest = self.pending_labels.values().min().copied();
        let Some(earliest) = earliest else {
            self.cancel_timer(TimerSlot::Label);
            return;
        };
        let now: DateTime<Utc> = self.now();
        let delay = (earliest - now).to_std().unwrap_or_default();
        self.arm_timer(TimerSlot::Label, delay);
    }

    /// Expires every label whose deadline has passed, then re-arms
    /// (`onLabelTimerFiredLocked`).
    pub(crate) fn on_label_timer_fired(&mut self) {
        let now = self.now();
        let expired: Vec<String> = self
            .pending_labels
            .iter()
            .filter(|(_, deadline)| **deadline <= now)
            .map(|(label, _)| label.clone())
            .collect();
        for label in &expired {
            self.pending_labels.remove(label);
        }
        let network_id = self.network_id();
        for label in expired {
            self.emit(SessionEvent::RequestLabelFinished {
                network_id: network_id.clone(),
                label,
            });
        }
        self.arm_label_timer();
    }
}
