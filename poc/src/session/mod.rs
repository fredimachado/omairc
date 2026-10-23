//! The session layer. It mirrors `tui/internal/session/` (Go) and the session
//! parts of `src/irc/` (C++: `ircsession`, `ircsessionmanager`,
//! `ircloopbacktransport`). SCRAM-SHA-256 (`saslscram.go`) is in `saslscram`, and STS
//! (`sts.go`) is in `sts` and `stsflow`. The socket transport (`transport_net.go`) is
//! out of scope until Phase 11.
//!
//! Nothing here opens a socket or reads the wall clock directly: time comes
//! through `Clock`, bytes through `Transport`, and the owner drains
//! `SessionEvent`s. `tests/irc_portability.rs` keeps `std::net` out of `irc/`;
//! this module is the seam where the socket transport will plug in.

pub mod autojoin;
pub mod batch;
pub mod caps;
pub mod clock;
pub mod config;
pub mod connection;
pub mod ctcp;
pub mod dispatch;
pub mod events;
pub mod history;
pub mod labels;
pub mod loopback;
pub mod malformed;
pub mod metadata;
pub mod outbound;
pub mod readmarker;
pub mod registration;
pub mod saslscram;
pub mod send;
pub mod session;
pub mod sessionmanager;
pub mod sink;
pub mod sts;
pub mod stsflow;
pub mod timers;
pub mod transport;
pub mod typing;
pub mod watchdog;
pub mod wire;

#[cfg(test)]
pub(crate) mod conformance;
#[cfg(test)]
mod tests;
