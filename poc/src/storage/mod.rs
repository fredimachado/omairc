//! Persistence under this build's app folder ([`crate::identity::APP_FOLDER`]), so a
//! default build never touches a real install. It mirrors `tui/internal/storage/` (Go) and `src/omaircpaths.cpp`
//! plus the stores in `src/irc/` (C++). The formats match Qt and the TUI: a
//! QSettings-compatible INI file, with the JSON that Qt keeps inside a string value.
//!
//! Every store takes a [`Backing`]. `Disk` is the INI file under the config folder;
//! `Ephemeral` writes nothing, which is what `--demo-server` and the tests use. A
//! store never reads the real environment itself: the caller picks the backing.

use std::path::PathBuf;
use std::rc::Rc;

use settings::{Settings, Status};
use stspolicies::IniStsStore;

use crate::session::sts::{MemoryStsStore, StsStore};

pub mod atomicwrite;
pub mod conversationlog;
pub mod credentialstore;
pub mod geometry;
pub mod lists;
pub mod migrate;
pub mod opendirect;
pub mod playbacktime;
pub mod paths;
pub mod preferences;
pub mod profilestore;
pub mod scrollplace;
pub mod settings;
pub mod stspolicies;

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

    /// The STS policy cache for this backing: the `sts` file beside the settings file, or memory
    /// for an ephemeral backing. The controller builds one and shares it across its sessions.
    pub fn sts_store(&self) -> Rc<dyn StsStore> {
        match self {
            Backing::Disk(path) => Rc::new(IniStsStore::new(paths::sts_path_beside(path))),
            Backing::Ephemeral => Rc::new(MemoryStsStore::default()),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testdir::TempDir;
    use chrono::{TimeZone, Utc};

    #[test]
    fn a_disk_backing_keeps_its_sts_cache_beside_its_settings() {
        let dir = TempDir::new("sts-backing");
        let settings = dir.path().join("omairc.conf");
        let now = Utc.timestamp_opt(0, 0).single().unwrap();
        Backing::Disk(settings.clone())
            .sts_store()
            .store("irc.example", 6697, 60, now);

        let restarted = Backing::Disk(settings).sts_store();
        assert!(restarted.lookup("irc.example", now).is_some());
        assert!(dir.path().join(paths::STS_FILE).exists());
    }

    #[test]
    fn an_ephemeral_backing_keeps_the_sts_cache_in_memory() {
        let now = Utc.timestamp_opt(0, 0).single().unwrap();
        let store = Backing::Ephemeral.sts_store();
        store.store("irc.example", 6697, 60, now);
        assert!(store.lookup("irc.example", now).is_some());
    }
}
