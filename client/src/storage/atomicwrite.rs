//! Atomic file replacement: write a temp sibling, sync, then rename over the target.
//! Same approach as Qt `QSaveFile` and the INI settings writer.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Writes through a temporary file in the same folder and renames it over the
/// target, so an interrupted write cannot leave a half-written or truncated file.
/// The folder is created owner-only, and the file is owner read/write only.
pub(crate) fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    write_atomic_with_mode(path, data, false)
}

/// Like [`write_atomic`], but when `path` already exists the temp file takes
/// that file's permission bits before rename. A new file is still owner-only (0600).
pub(crate) fn write_atomic_preserve_mode(path: &Path, data: &[u8]) -> io::Result<()> {
    write_atomic_with_mode(path, data, true)
}

fn write_atomic_with_mode(path: &Path, data: &[u8], preserve_mode: bool) -> io::Result<()> {
    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    create_private_dir(dir)?;

    let (temp, mut file) = create_temp_file(dir)?;
    let written = (|| -> io::Result<()> {
        set_mode_on_temp(&file, path, preserve_mode)?;
        file.write_all(data)?;
        file.sync_all()
    })();
    drop(file);

    let result = written.and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn create_private_dir(dir: &Path) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(dir)
}

fn create_temp_file(dir: &Path) -> io::Result<(PathBuf, fs::File)> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut last_error = None;
    for _ in 0..16 {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = dir.join(format!(".omairc-write-{}-{n}.tmp", std::process::id()));
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => last_error = Some(err),
            Err(err) => return Err(err),
        }
    }
    Err(last_error.unwrap_or_else(|| io::Error::other("no free temporary name")))
}

fn set_mode_on_temp(file: &fs::File, target: &Path, preserve_mode: bool) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = if preserve_mode {
            fs::metadata(target)
                .map(|meta| meta.permissions().mode() & 0o777)
                .unwrap_or(0o600)
        } else {
            0o600
        };
        file.set_permissions(fs::Permissions::from_mode(mode))
    }
    #[cfg(not(unix))]
    {
        let _ = (file, target, preserve_mode);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testdir::TempDir;

    #[test]
    fn write_atomic_replaces_the_target() {
        let dir = TempDir::new("atomic-replace");
        let path = dir.path().join("transcript");
        fs::write(&path, b"old\n").unwrap();
        write_atomic(&path, b"new\n").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new\n");
        assert_no_tmp_siblings(dir.path());
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_leaves_the_target_when_the_temp_cannot_be_created() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new("atomic-preserve");
        let path = dir.path().join("transcript");
        fs::write(&path, b"keep me\n").unwrap();
        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();

        let err = write_atomic(&path, b"should not land\n");
        assert!(err.is_err(), "expected create in a read-only folder to fail");
        assert_eq!(fs::read(&path).unwrap(), b"keep me\n");

        fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        assert_no_tmp_siblings(dir.path());
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_preserves_existing_target_mode() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new("atomic-preserve-mode");
        let path = dir.path().join("config");
        fs::write(&path, b"old\n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        write_atomic_preserve_mode(&path, b"new\n").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"new\n");
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o644);
        assert_no_tmp_siblings(dir.path());
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_preserve_mode_uses_owner_only_for_new_file() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new("atomic-preserve-new");
        let path = dir.path().join("config");
        write_atomic_preserve_mode(&path, b"new\n").unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    fn assert_no_tmp_siblings(dir: &Path) {
        for entry in fs::read_dir(dir).unwrap() {
            let name = entry.unwrap().file_name();
            let name = name.to_string_lossy();
            assert!(
                !name.ends_with(".tmp"),
                "leftover temporary file: {name}"
            );
        }
    }
}
