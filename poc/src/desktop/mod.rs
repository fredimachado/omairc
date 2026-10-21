//! Desktop integration: the notifier seam, the file log, the single-instance
//! guard with its IPC, the installer update and the avatar fetch. Go twin:
//! `tui/internal/notify` (the notifier seam only) and `tui/internal/avatar/fetch.go`.
//! C++: `src/backend.cpp`, `src/omaircfilelog.cpp`, `src/singleinstance.cpp`,
//! `src/omaircipc.cpp`, `src/omaircupdatecheck.cpp`, `src/irc/ircavatarstore.cpp`.

pub mod avatarhttp;
pub mod filelog;
pub mod httpfetch;
pub mod installer;
pub mod ipc;
pub mod notifier;
pub mod singleinstance;
pub mod systemtheme;
pub mod windowstate;
