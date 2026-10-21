//! Port of `tui/internal/controller/console.go` (Go) and `src/irc/ircstatusconsole.cpp` (C++).
//!
//! The Status console keeps each network's classified lines in a bounded ring:
//! the oldest line drops once a network passes its cap. The lines arrive already
//! redacted (`irc::statusentry` masks secrets before it builds an entry), so the
//! console stores them as they are.

use std::collections::{BTreeMap, VecDeque};

use crate::irc::statusentry::StatusEntry;

/// The per-network Status cap (`IrcStatusConsole`'s default, 2000 lines).
pub const DEFAULT_CAPACITY: usize = 2000;

/// The per-network ring buffers of Status lines.
#[derive(Debug)]
pub struct StatusConsole {
    capacity: usize,
    lines: BTreeMap<String, VecDeque<StatusEntry>>,
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
        buffer.push_back(entry);
        while buffer.len() > self.capacity {
            buffer.pop_front();
        }
    }

    /// One network's lines, oldest first. A network with no lines returns empty.
    pub fn lines(&self, network_id: &str) -> Vec<&StatusEntry> {
        self.lines
            .get(network_id)
            .map(|buffer| buffer.iter().collect())
            .unwrap_or_default()
    }

    /// The display text of each line, oldest first (`ConsoleText`).
    pub fn text(&self, network_id: &str) -> Vec<String> {
        self.lines(network_id)
            .into_iter()
            .map(|entry| entry.text.clone())
            .collect()
    }

    /// Drops every line recorded for one network (`/clear` on Status).
    pub fn clear(&mut self, network_id: &str) {
        self.lines.remove(network_id);
    }
}

impl Default for StatusConsole {
    fn default() -> Self {
        Self::new(0)
    }
}
