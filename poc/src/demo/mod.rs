//! The in-process demo server, behind `--demo-server`. It mirrors
//! `tui/internal/demo/` (Go) and `src/irc/ircdemoserver.cpp` (C++). It is only the
//! peer at the far end of a loopback transport: it answers frames like a server
//! and never writes `World` or the controller.

// The controller wires the demo in during Phase 3. Until then only the tests
// reach these items, so the dead-code allow covers this module only, and it comes
// off when `--demo-server` calls `DemoServer::attach` and `seed`.
#![allow(dead_code)]

pub mod seed;
pub mod wire;
