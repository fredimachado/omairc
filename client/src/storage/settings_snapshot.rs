//! On-disk byte snapshot helpers for [`super::Settings`].

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::{IniValue, Settings};

pub(super) fn read_ini_bytes(path: &Path) -> Option<Vec<u8>> {
    fs::read(path).ok()
}

pub(super) fn encoded_matches_disk(path: &Path, on_disk: Option<&[u8]>, encoded: &[u8]) -> bool {
    on_disk.is_some_and(|bytes| bytes == encoded)
        || (on_disk.is_none() && !path.exists() && encoded.is_empty())
}

pub(super) fn finish_load(
    path: PathBuf,
    values: BTreeMap<String, IniValue>,
    file_read: bool,
    preserve_file_mode: bool,
) -> Settings {
    let mut settings = Settings {
        path,
        values,
        dirty: false,
        on_disk: None,
        preserve_file_mode,
    };
    if file_read {
        settings.on_disk = Some(settings.encode());
    }
    settings
}

impl Settings {
    /// Loads `path`. A missing or unreadable file gives an empty store, and
    /// [`Settings::sync`] still refuses to overwrite a malformed or unreadable file.
    pub fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let file_bytes = read_ini_bytes(&path);
        let values = file_bytes
            .as_ref()
            .map(|raw| super::parse_settings_ini(raw))
            .unwrap_or_default();
        finish_load(path, values, file_bytes.is_some(), true)
    }

    /// Loads an STS policy file. Same as [`Settings::open`], but after a bad line
    /// inside a group, later keys for that group are kept when not already set, and
    /// a key that is already set is not overwritten by a leaked line. Shared config
    /// reads stay on [`Settings::open`].
    pub(in crate::storage) fn open_sts(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let file_bytes = read_ini_bytes(&path);
        let values = file_bytes
            .as_ref()
            .map(|raw| super::parse_sts_ini(raw))
            .unwrap_or_default();
        finish_load(path, values, file_bytes.is_some(), false)
    }
}
