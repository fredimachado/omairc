//! On-disk and IPC product identity for this build.
//!
//! Every folder name, settings file name, and local endpoint leaf used for persistence
//! or IPC lives here. The `product` feature selects the same names as an installed
//! client. It is off by default so `cargo run` / `cargo test` never write under a real
//! install's tree or claim its socket.

/// Folder under config and state roots for this build.
#[cfg(feature = "product")]
pub const APP_FOLDER: &str = "omairc";

/// Folder under config and state roots for this build.
#[cfg(not(feature = "product"))]
pub const APP_FOLDER: &str = "omairc-poc";

/// Local endpoint name: the named-pipe leaf on Windows, the socket file stem elsewhere.
/// Same string as [`APP_FOLDER`].
pub const APP_NAME: &str = APP_FOLDER;

/// QSettings IniFormat leaf on Linux (and this build's own Windows INI).
/// Darwin uses [`SETTINGS_FILE_MACOS`]. Windows Qt keeps main settings in the
/// registry (`HKCU\Software\omairc\omairc`), not this file.
pub const SETTINGS_FILE: &str = "omairc.conf";

/// QSettings IniFormat leaf on macOS (`settingsFileMissingOnDisk` in
/// `src/irc/ircprofilestore.cpp` always appends `.ini` on Darwin).
pub const SETTINGS_FILE_MACOS: &str = "omairc.ini";

/// Folder the installed Qt client and the TUI use. Migration reads from this tree;
/// this build only writes there when the `product` feature is on (then it matches
/// [`APP_FOLDER`]).
pub const QT_APP_FOLDER: &str = "omairc";

/// QSettings-compatible INI file name for `platform`: `.ini` on macOS, `.conf`
/// elsewhere. Product mode on macOS uses the same leaf as the Qt client so an
/// install can take over the file in place.
pub fn settings_file_name(macos: bool) -> &'static str {
    if macos {
        SETTINGS_FILE_MACOS
    } else {
        SETTINGS_FILE
    }
}
