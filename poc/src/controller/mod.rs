//! The controller: one place for intents, state reads and view notifications.
//! It mirrors `tui/internal/controller/` (Go) and `src/irc/irccontroller.cpp`
//! (C++). The UI calls its intents and never writes world state.
//!
//! The pipeline is one way: a `Transport` delivers bytes, its `Session` queues
//! `SessionEvent`s, `Controller::pump` folds them into the `EventReducer`, and the
//! `ViewNotify` surfaces wait in a queue the shell drains. Nothing here depends on
//! gpui, and the UI thread owns the whole tree.
//!
//! Submodules: `controller` (the struct, the pump and the session handlers),
//! `selection` (which conversation shows), `send` (the outbound intent), `networks`
//! (sidebar order and collapse), `reads` (what the shell draws), `demo` (the
//! `--demo-server` wiring and `--lines`), `transports` (the transport seam), and
//! `console` and `snapshots` (the Status ring and the row shapes).

// The Shell rewire is the first consumer outside the tests. Until it lands, nothing
// in the binary reaches the controller, so the allow covers the whole tree here and
// comes off with that change.
#![allow(dead_code)]

pub mod autoawayruntime;
pub mod channellist;
pub mod commanddispatcher;
pub mod console;
pub mod controller;
pub mod demo;
pub mod monitorcoordinator;
pub mod networks;
pub mod reads;
pub mod replyrouter;
pub mod scrollplace;
pub mod selection;
pub mod send;
pub mod snapshots;
pub mod stores;
pub mod transports;

#[cfg(test)]
mod tests;

