//! The session layer. It mirrors `tui/internal/session/` (Go) and the session
//! parts of `src/irc/` (C++: `ircsession`, `ircsessionmanager`,
//! `ircloopbacktransport`). The socket transport (`transport_net.go`), SCRAM
//! (`saslscram.go`) and STS (`sts.go`) are out of scope until Phase 11.

pub mod clock;
pub mod config;
pub mod loopback;
pub mod readmarker;
pub mod session;
pub mod sessionmanager;
pub mod transport;
