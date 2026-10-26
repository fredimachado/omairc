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
//! `--demo-server` wiring and the test-only `#bench` seed), `transports` (the
//! transport seam), and
//! `console` and `snapshots` (the Status ring and the row shapes). The slash path is
//! `commanddispatcher` (every verb's decision, and the composer's submit rule), with the
//! modules it calls: `stores` (ignore, mute, monitor and highlight lists),
//! `monitorcoordinator` (MONITOR), `autoawayruntime` (idle and grace timers),
//! `composertyping` (the typing hint), `channellist` (`/list` through the wire), and
//! `replyrouter` (WHOIS, CTCP, and the user's own `/status` and `/avatar` replies). The
//! `history` (history batches and self-JOIN), `notify` (mention and monitor arrivals),
//! and `playback` (bouncer catch-up, Gap 5). The `scrollplace` module records where a
//! transcript was left. Timers run on the `Clock`
//! seam, and `pump` runs what they queued on the UI thread. `persistence` loads what the
//! controller keeps between runs (profiles, preferences, open queries, lists, and each complete
//! network's session), and saves each change through the stores in `storage/`.

pub mod autoawayruntime;
pub mod avatars;
pub mod channellist;
pub mod cli;
pub mod commanddispatcher;
pub mod composertyping;
pub mod console;
pub mod controller;
pub mod demo;
pub mod error;
pub mod fileupload;
pub mod history;
pub mod lines;
pub mod monitorcoordinator;
pub mod notify;
pub mod playback;
pub mod readmarker;
pub mod networks;
pub mod persistence;
pub mod reads;
pub mod replyrouter;
pub mod replytext;
pub mod scrollplace;
pub mod selection;
pub mod send;
pub mod sessionconnect;
pub mod snapshots;
pub mod stores;
pub mod transports;

#[cfg(test)]
mod tests;

