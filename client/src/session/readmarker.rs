//! The session publishes a read marker only when the server acknowledged
//! `draft/read-marker` (`MARKREAD`) or `soju.im/read` (`READ`). One marker per
//! target is in flight at a time (`SetSlot`); a newer marker waits behind it, and
//! the echo of the in-flight marker releases the next. A failed `FAIL` clears only the
//! target it names.
//! The timestamp and verb helpers are wire vocabulary and live in
//! `irc/readmarker`; they are re-exported here for the session's callers.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

pub(crate) use crate::irc::readmarker::{
    format_read_marker_time, parse_read_marker_parameter, read_marker_time_after,
    read_marker_time_millis,
};

use super::events::SessionEvent;
use super::session::Core;
use crate::irc::message::IrcMessage;

/// A SET line a slot transition wants on the wire now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SendSet {
    pub(crate) target: String,
    pub(crate) timestamp: DateTime<Utc>,
}

/// One SET slot per folded target: idle, one in-flight stamp, or one in-flight
/// stamp plus one newer queued stamp. Two in-flight stamps for one target cannot
/// be stored. All stamps are millisecond-truncated. Mirrors the Go `setSlot`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) enum SetSlot {
    #[default]
    Idle,
    InFlight {
        sent: DateTime<Utc>,
    },
    /// `target` is the wire spelling of the newest queued call.
    Queued {
        sent: DateTime<Utc>,
        next: DateTime<Utc>,
        target: String,
    },
}

impl SetSlot {
    /// Applies an offer. `last_known` is the forward-only high-water mark; an
    /// equal or older stamp is ignored. Returns the new slot and the SET that
    /// must go out now, if any.
    pub(crate) fn offer(
        self,
        when: DateTime<Utc>,
        target: &str,
        last_known: Option<DateTime<Utc>>,
    ) -> (SetSlot, Option<SendSet>) {
        if last_known.is_some_and(|known| !read_marker_time_after(when, known)) {
            return (self, None);
        }
        match self {
            SetSlot::Idle => (
                SetSlot::InFlight { sent: when },
                Some(SendSet {
                    target: target.to_owned(),
                    timestamp: when,
                }),
            ),
            SetSlot::InFlight { sent } if read_marker_time_after(when, sent) => (
                SetSlot::Queued {
                    sent,
                    next: when,
                    target: target.to_owned(),
                },
                None,
            ),
            SetSlot::Queued { sent, next, .. } if read_marker_time_after(when, next) => (
                SetSlot::Queued {
                    sent,
                    next: when,
                    target: target.to_owned(),
                },
                None,
            ),
            unchanged => (unchanged, None),
        }
    }

    /// Applies the echo of marker `echoed`: releases the in-flight stamp when it
    /// is not newer than the echo, drops a queued stamp the echo covers, and
    /// returns a still-newer queued stamp to send now.
    pub(crate) fn echoed(self, echoed: DateTime<Utc>) -> (SetSlot, Option<SendSet>) {
        match self {
            SetSlot::InFlight { sent } if !read_marker_time_after(sent, echoed) => {
                (SetSlot::Idle, None)
            }
            SetSlot::Queued { sent, next, target } if !read_marker_time_after(sent, echoed) => {
                if read_marker_time_after(next, echoed) {
                    (
                        SetSlot::InFlight { sent: next },
                        Some(SendSet {
                            target,
                            timestamp: next,
                        }),
                    )
                } else {
                    (SetSlot::Idle, None)
                }
            }
            unchanged => (unchanged, None),
        }
    }

    /// A FAIL or an unparseable echo: the in-flight stamp is released (the same
    /// stamp may retry) and a queued stamp is sent now.
    pub(crate) fn released(self) -> (SetSlot, Option<SendSet>) {
        match self {
            SetSlot::Queued { next, target, .. } => (
                SetSlot::InFlight { sent: next },
                Some(SendSet {
                    target,
                    timestamp: next,
                }),
            ),
            _ => (SetSlot::Idle, None),
        }
    }
}

/// One SET slot per folded target. `last_known` is the high-water mark from a
/// received marker only; it only moves forward so an echo that releases a slot
/// cannot re-send an equal or older timestamp, while a FAIL can still retry the
/// same stamp.
#[derive(Clone, Debug, Default)]
pub(crate) struct ReadMarkerLedger {
    pub(crate) slots: BTreeMap<String, SetSlot>,
    pub(crate) last_known: BTreeMap<String, DateTime<Utc>>,
}

impl Core {
    /// The read-marker verb for the acknowledged token: `MARKREAD` for
    /// `draft/read-marker`, `READ` for `soju.im/read`, empty when neither.
    pub(crate) fn read_marker_command(&self) -> &'static str {
        crate::irc::readmarker::read_marker_command(self.read_marker_token)
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
        let target = message.text_param(0);
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
        let Some(parsed) = parse_read_marker_parameter(&message.text_param(1)) else {
            self.release_read_marker_slot(&folded);
            // Unparseable replies still release a controller get-hold.
            self.emit(SessionEvent::ReadMarkerReceived {
                network_id,
                target,
                marker: None,
            });
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
        self.advance_last_known(&folded, echoed);
        self.emit(SessionEvent::ReadMarkerReceived {
            network_id,
            target,
            marker: Some(echoed),
        });
        let (slot, send) = self.read_marker_slot(&folded).echoed(echoed);
        self.settle_read_marker_slot(&folded, slot, send);
        true
    }

    /// Handles a `FAIL` that names a read-marker command (`handleReadMarkerFailLocked`).
    pub(crate) fn handle_read_marker_fail(&mut self, message: &IrcMessage) {
        let command = message.text_param(0);
        if !command.eq_ignore_ascii_case("MARKREAD") && !command.eq_ignore_ascii_case("READ") {
            return;
        }
        let network_id = self.network_id();
        if message.params.len() < 4 {
            let folded: Vec<String> = self.read_markers.slots.keys().cloned().collect();
            for key in folded {
                self.release_read_marker_slot(&key);
            }
            // Empty target clears every awaiting get on this network.
            self.emit(SessionEvent::ReadMarkerReceived {
                network_id,
                target: String::new(),
                marker: None,
            });
            return;
        }
        let target = message.text_param(2);
        let folded = self.fold_read_marker_target(&target);
        if folded.is_empty() {
            return;
        }
        self.release_read_marker_slot(&folded);
        self.emit(SessionEvent::ReadMarkerReceived {
            network_id,
            target,
            marker: None,
        });
    }

    /// Asks the server for the stored marker on `target`. Returns whether the
    /// command was sent (registered and read-marker cap enabled).
    pub(crate) fn request_read_marker_get(&mut self, target: &str) -> bool {
        if target.is_empty() {
            return false;
        }
        let command = self.read_marker_command();
        if command.is_empty() {
            return false;
        }
        self.send_command(&format!("{command} {target}"), "")
    }

    /// Offers a marker to the target's SET slot (`queueReadMarkerSetLocked`). The
    /// controller's `publish_read_marker` is the only caller that decides to.
    pub(crate) fn queue_read_marker_set(&mut self, target: &str, when: DateTime<Utc>) {
        if target.is_empty() || self.read_marker_command().is_empty() {
            return;
        }
        let folded = self.fold_read_marker_target(target);
        if folded.is_empty() {
            return;
        }
        let when = read_marker_time_millis(when);
        let last_known = self.read_markers.last_known.get(&folded).copied();
        let (slot, send) = self.read_marker_slot(&folded).offer(when, target, last_known);
        self.settle_read_marker_slot(&folded, slot, send);
    }

    fn read_marker_slot(&mut self, folded: &str) -> SetSlot {
        self.read_markers.slots.remove(folded).unwrap_or_default()
    }

    /// Releases the slot after a FAIL or an unparseable echo.
    fn release_read_marker_slot(&mut self, folded: &str) {
        let (slot, send) = self.read_marker_slot(folded).released();
        self.settle_read_marker_slot(folded, slot, send);
    }

    /// Stores `slot` and puts `send` on the wire. A refused send leaves the slot
    /// idle: nothing is in flight.
    fn settle_read_marker_slot(&mut self, folded: &str, slot: SetSlot, send: Option<SendSet>) {
        let slot = match send {
            Some(set) if !self.send_read_marker_set(&set) => SetSlot::Idle,
            _ => slot,
        };
        if slot != SetSlot::Idle {
            self.read_markers.slots.insert(folded.to_owned(), slot);
        }
    }

    fn send_read_marker_set(&mut self, set: &SendSet) -> bool {
        let command = self.read_marker_command();
        if command.is_empty() {
            return false;
        }
        let line = format!(
            "{command} {} timestamp={}",
            set.target,
            format_read_marker_time(set.timestamp)
        );
        self.send_command(&line, "")
    }

    fn advance_last_known(&mut self, folded: &str, when: DateTime<Utc>) {
        if let Some(known) = self.read_markers.last_known.get(folded) {
            if !read_marker_time_after(when, *known) {
                return;
            }
        }
        self.read_markers
            .last_known
            .insert(folded.to_owned(), when);
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeDelta, TimeZone};

    use super::*;

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 6, 1, 12, 0, 0).single().unwrap() + TimeDelta::seconds(seconds)
    }

    fn in_flight(seconds: i64) -> SetSlot {
        SetSlot::InFlight { sent: at(seconds) }
    }

    fn queued(sent: i64, next: i64, target: &str) -> SetSlot {
        SetSlot::Queued {
            sent: at(sent),
            next: at(next),
            target: target.to_owned(),
        }
    }

    fn send(target: &str, seconds: i64) -> Option<SendSet> {
        Some(SendSet {
            target: target.to_owned(),
            timestamp: at(seconds),
        })
    }

    #[test]
    fn offer_transitions() {
        let known = Some(at(5));
        let cases = [
            ("idle sends", SetSlot::Idle, 10, None, in_flight(10), send("#a", 10)),
            ("idle ignores not after last known", SetSlot::Idle, 5, known, SetSlot::Idle, None),
            ("idle sends after last known", SetSlot::Idle, 6, known, in_flight(6), send("#a", 6)),
            ("in flight queues newer", in_flight(10), 20, None, queued(10, 20, "#a"), None),
            ("in flight ignores equal", in_flight(10), 10, None, in_flight(10), None),
            ("in flight ignores older", in_flight(10), 9, None, in_flight(10), None),
            ("queued replaces next", queued(10, 20, "#old"), 30, None, queued(10, 30, "#a"), None),
            ("queued ignores equal next", queued(10, 20, "#old"), 20, None, queued(10, 20, "#old"), None),
            ("queued ignores older next", queued(10, 20, "#old"), 15, None, queued(10, 20, "#old"), None),
        ];
        for (name, slot, when, last_known, want_slot, want_send) in cases {
            let (got, sent) = slot.offer(at(when), "#a", last_known);
            assert_eq!(got, want_slot, "{name}");
            assert_eq!(sent, want_send, "{name}");
        }
    }

    #[test]
    fn echoed_transitions() {
        let cases = [
            ("idle stays idle", SetSlot::Idle, 10, SetSlot::Idle, None),
            ("equal echo releases", in_flight(10), 10, SetSlot::Idle, None),
            ("newer echo releases", in_flight(10), 11, SetSlot::Idle, None),
            ("older echo keeps in flight", in_flight(10), 9, in_flight(10), None),
            ("older echo keeps queued", queued(10, 20, "#a"), 9, queued(10, 20, "#a"), None),
            ("echo of sent promotes next", queued(10, 20, "#a"), 10, in_flight(20), send("#a", 20)),
            ("echo equal to next drops it", queued(10, 20, "#a"), 20, SetSlot::Idle, None),
            ("echo past next drops it", queued(10, 20, "#a"), 25, SetSlot::Idle, None),
        ];
        for (name, slot, echoed, want_slot, want_send) in cases {
            let (got, sent) = slot.echoed(at(echoed));
            assert_eq!(got, want_slot, "{name}");
            assert_eq!(sent, want_send, "{name}");
        }
    }

    #[test]
    fn released_transitions_and_same_stamp_retry() {
        assert_eq!(SetSlot::Idle.released(), (SetSlot::Idle, None));
        assert_eq!(in_flight(10).released(), (SetSlot::Idle, None));
        assert_eq!(
            queued(10, 20, "#a").released(),
            (in_flight(20), send("#a", 20))
        );
        let (slot, _) = in_flight(10).released();
        assert_eq!(slot.offer(at(10), "#a", None), (in_flight(10), send("#a", 10)));
    }
}
