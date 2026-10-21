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
//! `console` and `snapshots` (the Status ring and the row shapes). The slash path is
//! `commanddispatcher` (every verb's decision, and the composer's submit rule), with the
//! modules it calls: `stores` (ignore, mute, monitor and highlight lists),
//! `monitorcoordinator` (MONITOR), `autoawayruntime` (idle and grace timers),
//! `composertyping` (the typing hint), `channellist` (`/list` through the wire), and
//! `replyrouter` (WHOIS, CTCP, and the user's own `/status` and `/avatar` replies). The
//! `scrollplace` module records where a transcript was left. Timers run on the `Clock`
//! seam, and `pump` runs what they queued on the UI thread.

pub mod autoawayruntime;
pub mod channellist;
pub mod cli;
pub mod commanddispatcher;
pub mod composertyping;
pub mod console;
pub mod controller;
pub mod demo;
pub mod lines;
pub mod monitorcoordinator;
pub mod networks;
pub mod reads;
pub mod replyrouter;
// The shell still keeps its scroll places in `model/scroll_place.rs`. The rewire to this
// store lands with the shell's transcript work, and the allow comes off then.
#[allow(dead_code)]
pub mod scrollplace;
pub mod selection;
pub mod send;
pub mod snapshots;
pub mod stores;
pub mod transports;

#[cfg(test)]
mod tests;

