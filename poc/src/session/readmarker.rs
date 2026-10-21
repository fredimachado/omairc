//! Port of `tui/internal/session/readmarker.go` (Go) and the outbound half of
//! `src/irc/ircreadmarker.cpp` (C++), plus the wire helpers of
//! `tui/internal/irc/readmarker.go`.
//!
//! The session publishes a read marker only when the server acknowledged
//! `draft/read-marker` (`MARKREAD`) or `soju.im/read` (`READ`). One marker per
//! target is in flight at a time; a newer marker waits behind it, and the echo
//! of the in-flight marker releases the next. A failed `FAIL` clears only the
//! target it names.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use super::events::SessionEvent;
use super::session::Core;
use super::wire;
use crate::irc::message::IrcMessage;

/// A marker waiting to be sent (`readMarkerQueue`).
#[derive(Clone, Debug)]
pub(crate) struct ReadMarkerQueue {
    pub(crate) target: String,
    pub(crate) timestamp: DateTime<Utc>,
}

/// Pending and in-flight markers, keyed by the folded target.
#[derive(Clone, Debug, Default)]
pub(crate) struct ReadMarkerLedger {
    pub(crate) pending: BTreeMap<String, ReadMarkerQueue>,
    pub(crate) in_flight: BTreeMap<String, DateTime<Utc>>,
}

const READ_MARKER_TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3fZ";

/// Truncates to whole UTC milliseconds, the precision markers compare at.
pub(crate) fn read_marker_time_millis(when: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(when.timestamp_millis()).unwrap_or(when)
}

/// Whether `left` is strictly newer than `right` at millisecond precision.
pub(crate) fn read_marker_time_after(left: DateTime<Utc>, right: DateTime<Utc>) -> bool {
    read_marker_time_millis(left) > read_marker_time_millis(right)
}

/// A UTC timestamp for a `timestamp=` parameter.
pub(crate) fn format_read_marker_time(when: DateTime<Utc>) -> String {
    read_marker_time_millis(when)
        .format(READ_MARKER_TIME_FORMAT)
        .to_string()
}

/// Parses a `timestamp=` value. Only a trailing `Z` is accepted.
pub(crate) fn parse_read_marker_time(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim();
    if value.is_empty() || !value.ends_with('Z') {
        return None;
    }
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| read_marker_time_millis(parsed.with_timezone(&Utc)))
}

/// Parses the trailing parameter of a `MARKREAD` / `READ` line. `Some(None)` is
/// `*` (no marker), `Some(Some(time))` a marker, and `None` anything else.
pub(crate) fn parse_read_marker_parameter(parameter: &str) -> Option<Option<DateTime<Utc>>> {
    let parameter = parameter.trim();
    if parameter == "*" {
        return Some(None);
    }
    let value = parameter.strip_prefix("timestamp=")?;
    parse_read_marker_time(value).map(Some)
}

impl Core {
    /// The read-marker verb for the acknowledged token: `MARKREAD` for
    /// `draft/read-marker`, `READ` for `soju.im/read`, empty when neither.
    pub(crate) fn read_marker_command(&self) -> &'static str {
        match self.read_marker_token {
            Some("draft/read-marker") => "MARKREAD",
            Some("soju.im/read") => "READ",
            _ => "",
        }
    }

    /// Folds a target the way the session folds channels and nicks.
    fn fold_read_marker_target(&self, target: &str) -> String {
        if target.is_empty() {
            return String::new();
        }
        if self.features.is_channel(target) {
            return self.fold_channel(target);
        }
        self.features.case_mapping().normalize(target)
    }

    /// Handles an inbound `MARKREAD` / `READ` line. Returns true when the line was
    /// a read-marker line, which is then consumed (`deliverReadMarkerLocked`).
    pub(crate) fn deliver_read_marker(&mut self, message: &IrcMessage) -> bool {
        if message.command != "MARKREAD" && message.command != "READ" {
            return false;
        }
        if message.params.is_empty() {
            return true;
        }
        let target = wire::param(message, 0);
        if target.is_empty() {
            return true;
        }
        let folded = self.fold_read_marker_target(&target);
        let network_id = self.network_id();
        if message.params.len() < 2 {
            self.emit(SessionEvent::ReadMarkerReceived {
                network_id,
                target,
                marker: None,
            });
            return true;
        }
        let Some(parsed) = parse_read_marker_parameter(&wire::param(message, 1)) else {
            self.read_markers.in_flight.remove(&folded);
            self.flush_read_marker_set(&folded);
            return true;
        };
        let Some(marker) = parsed else {
            self.emit(SessionEvent::ReadMarkerReceived {
                network_id,
                target,
                marker: None,
            });
            return true;
        };
        let echoed = read_marker_time_millis(marker);
        self.emit(SessionEvent::ReadMarkerReceived {
            network_id,
            target,
            marker: Some(echoed),
        });
        if let Some(in_flight) = self.read_markers.in_flight.get(&folded).copied() {
            if !read_marker_time_after(in_flight, echoed) {
                self.read_markers.in_flight.remove(&folded);
                if let Some(pending) = self.read_markers.pending.get(&folded) {
                    if !read_marker_time_after(pending.timestamp, echoed) {
                        self.read_markers.pending.remove(&folded);
                    }
                }
                self.flush_read_marker_set(&folded);
            }
        }
        true
    }

    /// Handles a `FAIL` that names a read-marker command (`handleReadMarkerFailLocked`).
    pub(crate) fn handle_read_marker_fail(&mut self, message: &IrcMessage) {
        let command = wire::param(message, 0);
        if !command.eq_ignore_ascii_case("MARKREAD") && !command.eq_ignore_ascii_case("READ") {
            return;
        }
        if message.params.len() < 4 {
            let folded: Vec<String> = self.read_markers.in_flight.keys().cloned().collect();
            for key in folded {
                self.read_markers.in_flight.remove(&key);
                self.flush_read_marker_set(&key);
            }
            return;
        }
        let target = wire::param(message, 2);
        let folded = self.fold_read_marker_target(&target);
        if folded.is_empty() {
            return;
        }
        self.read_markers.in_flight.remove(&folded);
        self.flush_read_marker_set(&folded);
    }

    /// Asks the server for the stored marker on `target`.
    pub(crate) fn request_read_marker_get(&mut self, target: &str) {
        if target.is_empty() {
            return;
        }
        let command = self.read_marker_command();
        if command.is_empty() {
            return;
        }
        self.send_command(&format!("{command} {target}"), "");
    }

    /// Records a newer marker to publish once the target is caught up
    /// (`queueReadMarkerSetLocked`).
    pub(crate) fn queue_read_marker_set(&mut self, target: &str, when: DateTime<Utc>) {
        if target.is_empty() || self.read_marker_command().is_empty() {
            return;
        }
        let folded = self.fold_read_marker_target(target);
        if folded.is_empty() {
            return;
        }
        let when = read_marker_time_millis(when);
        if let Some(in_flight) = self.read_markers.in_flight.get(&folded) {
            if !read_marker_time_after(when, *in_flight) {
                return;
            }
        }
        if let Some(pending) = self.read_markers.pending.get(&folded) {
            if !read_marker_time_after(when, pending.timestamp) {
                return;
            }
        }
        self.read_markers.pending.insert(
            folded.clone(),
            ReadMarkerQueue {
                target: target.to_owned(),
                timestamp: when,
            },
        );
        self.flush_read_marker_set(&folded);
    }

    /// Sends the queued marker for `folded` unless one is already in flight
    /// (`flushReadMarkerSetLocked`).
    fn flush_read_marker_set(&mut self, folded: &str) {
        if self.read_markers.in_flight.contains_key(folded) {
            return;
        }
        let Some(pending) = self.read_markers.pending.get(folded).cloned() else {
            return;
        };
        let command = self.read_marker_command();
        if command.is_empty() {
            self.read_markers.pending.remove(folded);
            return;
        }
        let line = format!(
            "{command} {} timestamp={}",
            pending.target,
            format_read_marker_time(pending.timestamp)
        );
        self.send_command(&line, "");
        self.read_markers.pending.remove(folded);
        self.read_markers
            .in_flight
            .insert(folded.to_owned(), pending.timestamp);
    }
}
