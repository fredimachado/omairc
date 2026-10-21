//! The controller: one place for intents, state reads and view notifications.
//! It mirrors `tui/internal/controller/` (Go) and `src/irc/irccontroller.cpp`
//! (C++). The UI calls its intents and never writes world state.

pub mod autoawayruntime;
pub mod channellist;
pub mod commanddispatcher;
pub mod console;
pub mod controller;
pub mod monitorcoordinator;
pub mod replyrouter;
pub mod scrollplace;
pub mod snapshots;
pub mod stores;
