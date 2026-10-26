//! Desktop integration: the notifier seam, the file log, the single-instance
//! guard with its IPC, the installer update and the avatar fetch. Go twin:
//! `tui/internal/notify` (the notifier seam only) and `tui/internal/avatar/fetch.go`.
//! C++: `src/backend.cpp`, `omaircfilelog`, `singleinstance`,
//! `omaircipc`, `omaircupdatecheck`, `tui/internal/avatar/avatar.go`.

pub mod avatarhttp;
pub mod cli;
pub mod cli_cursor;
pub mod endpoint;
pub mod filelog;
pub mod fileupload;
pub mod httpfetch;
pub mod installer;
pub mod ipc;
pub mod ipchandler;
pub mod locale;
pub mod notifier;
pub mod reachability;
pub mod singleinstance;
pub mod systemtheme;
pub mod textscale;
pub mod windowstate;
pub mod windowvisibility;
