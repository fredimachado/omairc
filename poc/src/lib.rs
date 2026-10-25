//! Omairc client library. The binary calls [`run`]; live daemon tests and other
//! integration crates link this crate for the controller and net transport.

pub mod assets;
pub mod controller;
pub mod demo;
pub mod desktop;
pub mod identity;
pub mod irc;
pub mod model;
pub mod net;
pub mod session;
pub mod storage;
pub mod ui;
pub mod util;
pub mod version;

/// Live ircd harness (env, client, raw peer). Used by `tests/live`.
pub mod live;

mod app;

/// Starts the desktop application (the binary's `main`).
pub use app::run;
