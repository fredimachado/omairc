//! The in-process demo server, behind `--demo-server`. It mirrors
//! `tui/internal/demo/` (Go, `demo.go` holds `DemoServer::Attach`) and
//! `src/irc/ircdemoserver.cpp` (C++). It is only the peer at the far end of a
//! loopback transport: it answers frames like a server and never writes `World`.

pub mod answers;
pub mod seed;
pub mod wire;
