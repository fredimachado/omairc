//! Persistence under the PoC's own `omairc-poc` folder, so it never touches a
//! real install. It mirrors `tui/internal/storage/` (Go) and `src/omaircpaths.cpp`
//! plus the stores in `src/irc/` (C++). The formats match Qt and the TUI: a
//! QSettings-compatible INI file, with the JSON that Qt keeps inside a string value.
//!
//! Every store takes a [`Backing`]. `Disk` is the INI file under the config folder;
//! `Ephemeral` writes nothing, which is what `--demo-server` and the tests use. A
//! store never reads the real environment itself: the caller picks the backing.

use std::path::PathBuf;

use settings::{Settings, Status};

pub mod credentialstore;
pub mod geometry;
pub mod lists;
pub mod opendirect;
pub mod paths;
pub mod preferences;
pub mod profilestore;
pub mod scrollplace;
pub mod settings;

/// Where a store keeps its data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Backing {
    /// The QSettings-compatible INI file at this path.
    Disk(PathBuf),
    /// Nothing is kept between runs. A store keeps what it needs in memory.
    Ephemeral,
}

impl Backing {
    /// The user's settings file, or `Ephemeral` when the platform has no config root.
    pub fn user() -> Self {
        paths::settings_path().map_or(Backing::Ephemeral, Backing::Disk)
    }

    pub fn is_ephemeral(&self) -> bool {
        matches!(self, Backing::Ephemeral)
    }

    /// A fresh read of the store. An ephemeral backing reads an empty store.
    pub fn settings(&self) -> Settings {
        match self {
            Backing::Disk(path) => Settings::open(path.clone()),
            Backing::Ephemeral => Settings::open(PathBuf::new()),
        }
    }

    /// Reads the store, applies `edit`, and writes it back. The write refuses first
    /// when the file is malformed or read-only, so a bad file is left alone. An
    /// ephemeral backing applies the edit and reports success.
    pub fn edit(&self, edit: impl FnOnce(&mut Settings)) -> Status {
        let mut settings = self.settings();
        edit(&mut settings);
        if self.is_ephemeral() {
            return Status::Written;
        }
        settings.sync()
    }
}
