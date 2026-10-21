//! Persistence under the PoC's own `omairc-poc` folder, so it never touches a
//! real install. It mirrors `tui/internal/storage/` (Go) and `src/omaircpaths.cpp`
//! plus the stores in `src/irc/` (C++). The formats match Qt and the TUI: a
//! QSettings-compatible INI file and JSON where Qt uses JSON.

pub mod credentialstore;
pub mod geometry;
pub mod opendirect;
pub mod paths;
pub mod profilestore;
pub mod settings;
