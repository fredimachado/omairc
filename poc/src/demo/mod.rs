//! The in-process demo server, behind `--demo-server`. It mirrors
//! `tui/internal/demo/` (Go) and `src/irc/ircdemoserver.cpp` (C++). It is only the
//! peer at the far end of a loopback transport: it answers frames like a server
//! and never writes `World` or the controller.
//!
//! - `seed`: the two demo networks' world data (`seed.go`).
//! - `wire`: the IRC lines the seed and the answers send, and the bench channel.
//! - `answers`: the replies to the client's own frames (`answers.go`).
//! - `server`: `DemoServer`, which attaches the peers to sessions and plays the
//!   seed in Qt's order (`demo.go`).

// The controller wires the demo in during Phase 3. Until then only the tests
// reach these items, so the dead-code allow covers this module only, and it comes
// off when `--demo-server` calls `DemoServer::attach` and `seed`.
#![allow(dead_code)]

pub mod answers;
pub mod seed;
pub mod server;
pub mod wire;

#[cfg(test)]
mod tests;
