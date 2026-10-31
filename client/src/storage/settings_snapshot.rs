//! On-disk byte snapshot helpers for [`super::settings::Settings`].

use std::fs;
use std::path::Path;

pub(super) fn read_ini_bytes(path: &Path) -> Option<Vec<u8>> {
    fs::read(path).ok()
}

pub(super) fn encoded_matches_disk(path: &Path, on_disk: Option<&[u8]>, encoded: &[u8]) -> bool {
    on_disk.is_some_and(|bytes| bytes == encoded)
        || (on_disk.is_none() && !path.exists() && encoded.is_empty())
}
