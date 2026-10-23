//! Port of `tui/internal/irc/reducer.go` (Go) and `src/irc/irceventreducer.{h,cpp}`
//! (C++). The reducer folds the translated events in `irc/event.rs` into
//! conversation state and reports which surfaces each event dirtied.
//!
//! The state is owned here, in one `EventReducer`: a `Store` of conversations
//! (`BTreeMap` keyed by network then normalized target, the order the C++
//! `std::map` and the Go sort both use) and a `Facts` struct of per-network
//! facts (features, nicks, presence, selection, log). A handler borrows one
//! conversation and the facts together by disjoint field, so the fold never
//! reads through `&self` while a row is borrowed.
//!
//! Time is an argument: `apply` takes the caller's `now`, which stands in for
//! the Go `now` and the C++ wall-clock reads. Nothing here reads the clock, and
//! nothing touches a socket, a file or a thread.
//!
//! The submodules follow the Go file split: `state` (the types), `facts` (the
//! per-network facts and identity queries), `transcript` (rows, cap, log, live
//! chat), `splice` (replay batches and the pending queue), `handlers` (one fold
//! per event), `conversations` (creation, drop, close, selection, reads),
//! `readmarker`, `notify` (`ClassifyViewNotify`) and `sidebar` (`SidebarOrder`).

mod conversations;
mod facts;
mod handlers;
mod log;
mod notify;
mod readmarker;
mod sidebar;
mod splice;
mod state;
mod transcript;

#[cfg(test)]
mod tests;

pub use log::{ConversationLog, TranscriptLine};
pub use notify::classify_view_notify;
pub use readmarker::latest_server_message_time;
pub use state::{zero_time, MemberView, PeerPresence, Store};
#[cfg(test)]
pub use state::{ConversationState, MAX_MESSAGES};

use chrono::{DateTime, Utc};

use crate::irc::event::Event;
use crate::irc::viewnotify::ViewNotify;

use facts::Facts;

/// Folds translated events into conversation state (`IrcEventReducer`).
pub struct EventReducer {
    conversations: Store,
    facts: Facts,
}

impl EventReducer {
    /// An empty reducer with the window focused, as a default-constructed
    /// `IrcEventReducer` is.
    pub fn new() -> Self {
        Self {
            conversations: Store::new(),
            facts: Facts::new(),
        }
    }

    /// Folds one translated event in and returns the surfaces it dirtied, read
    /// after the fold (`apply` plus `ClassifyViewNotify`). An empty list means
    /// nothing visible changed. `now` is the caller's clock.
    pub fn apply(&mut self, event: Event, now: DateTime<Utc>) -> Vec<ViewNotify> {
        self.facts.mention_arrival = None;
        self.facts.inbox_arrival = None;
        self.reduce(&event, now);
        classify_view_notify(&event, self)
    }
}

impl Default for EventReducer {
    fn default() -> Self {
        Self::new()
    }
}
