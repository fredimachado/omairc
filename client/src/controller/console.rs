//! The Status console keeps each network's classified lines in a bounded ring:
//! the oldest line drops once a network passes its cap. The lines arrive already
//! redacted (`irc::statusentry` masks secrets before it builds an entry), so the
//! console stores them as they are.

use std::collections::{BTreeMap, VecDeque};

use crate::irc::statusentry::{LogSeverity, StatusEntry};
use crate::session::events::{SessionState, Severity};

/// The per-network Status cap (`IrcStatusConsole`'s default, 2000 lines).
pub const DEFAULT_CAPACITY: usize = 2000;

/// The word a connection-state line opens with (`stateLabel` in
/// `tui/internal/controller/console.go`). Capability discovery, capability requests and
/// SASL all read as `negotiating`.
pub(super) fn state_label(state: SessionState) -> &'static str {
    match state {
        SessionState::Idle => "offline",
        SessionState::Connecting | SessionState::StsUpgrading => "connecting",
        SessionState::CapLs | SessionState::CapReq | SessionState::Sasl => "negotiating",
        SessionState::Registering => "registering",
        SessionState::Registered => "registered",
        SessionState::Closing => "closing",
        SessionState::Reconnecting => "reconnecting",
        SessionState::Failed => "failed",
    }
}

/// The sentence a connection-state line says (`stateText` in
/// `tui/internal/controller/console.go`).
/// The Status severity for a session notice (`Severity` to `LogSeverity`).
pub(super) fn log_severity(severity: Severity) -> LogSeverity {
    match severity {
        Severity::Info => LogSeverity::Info,
        Severity::Alert => LogSeverity::Alert,
    }
}

pub(super) fn state_text(state: SessionState) -> &'static str {
    match state {
        SessionState::Idle => "Disconnected",
        SessionState::Connecting | SessionState::StsUpgrading => "Connecting",
        SessionState::CapLs => "Capability discovery",
        SessionState::CapReq => "Requesting capabilities",
        SessionState::Sasl => "Authenticating",
        SessionState::Registering => "Registering",
        SessionState::Registered => "Registered",
        SessionState::Closing => "Disconnecting",
        SessionState::Reconnecting => "Reconnecting",
        SessionState::Failed => "Connection failed",
    }
}

/// One network's Status ring and the sequence the Qt log uses for alerts.
#[derive(Debug, Default)]
struct NetworkLog {
    entries: VecDeque<StatusEntry>,
    next_sequence: u64,
    last_seen: u64,
}

impl NetworkLog {
    fn first_sequence(&self) -> u64 {
        self.next_sequence - self.entries.len() as u64
    }
}

/// The per-network ring buffers of Status lines.
#[derive(Debug)]
pub struct StatusConsole {
    capacity: usize,
    lines: BTreeMap<String, NetworkLog>,
}

impl StatusConsole {
    /// An empty console. A zero capacity falls back to `DEFAULT_CAPACITY`.
    pub fn new(capacity: usize) -> Self {
        let capacity = if capacity == 0 {
            DEFAULT_CAPACITY
        } else {
            capacity
        };
        Self {
            capacity,
            lines: BTreeMap::new(),
        }
    }

    /// Records one line on its network's buffer. An entry with no network id is
    /// dropped, as the Go console drops it.
    pub fn append(&mut self, entry: StatusEntry) {
        if entry.network_id.is_empty() {
            return;
        }
        let buffer = self.lines.entry(entry.network_id.clone()).or_default();
        if buffer.next_sequence == 0 {
            buffer.next_sequence = 1;
        }
        buffer.entries.push_back(entry);
        buffer.next_sequence += 1;
        while buffer.entries.len() > self.capacity {
            buffer.entries.pop_front();
        }
    }

    /// One network's lines, oldest first. A network with no lines returns empty.
    pub fn lines(&self, network_id: &str) -> Vec<&StatusEntry> {
        self.lines
            .get(network_id)
            .map(|buffer| buffer.entries.iter().collect())
            .unwrap_or_default()
    }

    /// Alert-severity lines not yet seen on Status (`IrcNetworkLog::alertsSinceSeen`).
    pub fn alerts_since_seen(&self, network_id: &str) -> usize {
        let Some(buffer) = self.lines.get(network_id) else {
            return 0;
        };
        if buffer.entries.is_empty() {
            return 0;
        }
        let first = buffer.first_sequence();
        buffer
            .entries
            .iter()
            .enumerate()
            .filter(|(index, entry)| {
                first + *index as u64 > buffer.last_seen
                    && entry.severity == LogSeverity::Alert
            })
            .count()
    }

    /// Marks every current line on `network_id` as seen (`markSeen`).
    pub fn mark_seen(&mut self, network_id: &str) {
        if let Some(buffer) = self.lines.get_mut(network_id) {
            if buffer.next_sequence > 0 {
                buffer.last_seen = buffer.next_sequence - 1;
            }
        }
    }

    /// The networks that hold at least one Status line, in id order.
    pub fn network_ids(&self) -> Vec<String> {
        self.lines.keys().cloned().collect()
    }

    /// The display text of each line, oldest first (`ConsoleText`).
    #[cfg(test)]
    pub fn text(&self, network_id: &str) -> Vec<String> {
        self.lines(network_id)
            .into_iter()
            .map(|entry| entry.text.clone())
            .collect()
    }

    /// Drops every line recorded for one network (`/clear` on Status).
    pub fn clear(&mut self, network_id: &str) {
        let Some(buffer) = self.lines.get_mut(network_id) else {
            return;
        };
        if buffer.entries.is_empty() {
            return;
        }
        buffer.entries.clear();
        if buffer.next_sequence > 0 {
            buffer.last_seen = buffer.next_sequence - 1;
        }
    }

    /// Drops a network's log entirely (`IrcNetworkLog::forget`).
    pub fn forget(&mut self, network_id: &str) {
        self.lines.remove(network_id);
    }
}

impl Default for StatusConsole {
    fn default() -> Self {
        Self::new(0)
    }
}
