//! Desktop integration: the notifier seam, the file log, the single-instance
//! guard with its IPC, and the installer update. Go twin: `tui/internal/notify`
//! (the notifier seam only). C++: `src/backend.cpp`, `src/omaircfilelog.cpp`,
//! `src/singleinstance.cpp`, `src/omaircipc.cpp`, `src/omaircupdatecheck.cpp`.

pub mod filelog;
pub mod installer;
pub mod ipc;
pub mod notifier;
pub mod singleinstance;
pub mod systemtheme;
pub mod windowstate;
