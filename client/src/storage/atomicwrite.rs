//! Atomic file replacement: write a temp sibling, sync, then rename over the target.
//! Same approach as Qt `QSaveFile` and the INI settings writer.
//!
//! When the destination path is a symlink (including stow-managed dotfiles), the temp
//! file is created beside the resolved target and the rename replaces that target,
//! so the symlink stays a symlink. Relative link targets are joined onto each link's
//! parent without collapsing `..`. After a symlink chain, parent directories of a
//! missing target are not created; a dangling link whose target directory is missing
//! fails without writing.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Maximum symlink hops when resolving an atomic-write destination.
const MAX_SYMLINK_HOPS: u32 = 32;

/// Resolved path for an atomic rename plus whether any symlink was followed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AtomicWriteTarget {
    pub destination: PathBuf,
    pub followed_symlink: bool,
}

/// Follows symlink chains starting at `path`. Relative targets are joined onto each
/// link's parent without normalizing `..`.
pub(crate) fn atomic_write_target(path: &Path) -> io::Result<AtomicWriteTarget> {
    let mut resolved = path.to_path_buf();
    let mut followed_symlink = false;
    for _ in 0..MAX_SYMLINK_HOPS {
        match fs::symlink_metadata(&resolved) {
            Ok(meta) if meta.file_type().is_symlink() => {
                followed_symlink = true;
                let link = fs::read_link(&resolved)?;
                resolved = if link.is_absolute() {
                    link
                } else {
                    let parent = resolved.parent().unwrap_or_else(|| Path::new("."));
                    parent.join(link)
                };
            }
            Ok(_) => {
                return Ok(AtomicWriteTarget {
                    destination: resolved,
                    followed_symlink,
                });
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok(AtomicWriteTarget {
                    destination: resolved,
                    followed_symlink,
                });
            }
            #[cfg(unix)]
            Err(err) if err.raw_os_error() == Some(40) => return Err(err),
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        "too many symlinks resolving atomic write destination",
    ))
}

/// Ensures the destination's parent directory exists when allowed to create it.
pub(crate) fn ensure_atomic_write_parent(target: &AtomicWriteTarget) -> io::Result<()> {
    let dir = match target.destination.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => return Ok(()),
    };
    if target.followed_symlink {
        match fs::symlink_metadata(dir) {
            Ok(meta) if meta.is_dir() => Ok(()),
            Ok(_) => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "atomic write target directory missing",
            )),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "atomic write target directory missing",
            )),
            Err(err) => Err(err),
        }
    } else {
        create_private_dir(dir)
    }
}

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
    let target = atomic_write_target(path)?;
    ensure_atomic_write_parent(&target)?;

    let dir = match target.destination.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };

    let (temp, mut file) = create_temp_file(dir)?;
    let written = (|| -> io::Result<()> {
        set_mode_on_temp(&file, &target.destination, preserve_mode)?;
        file.write_all(data)?;
        file.sync_all()
    })();
    drop(file);

    let result = written.and_then(|()| fs::rename(&temp, &target.destination));
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

    #[cfg(unix)]
    mod symlink_atomic_write_3dba {
        use super::*;
        use std::os::unix::fs::{symlink, PermissionsExt};

        fn assert_is_symlink(path: &Path, want_target: &Path) {
            let meta = fs::symlink_metadata(path).expect("symlink metadata");
            assert!(meta.file_type().is_symlink(), "expected a symlink at {}", path.display());
            let target = fs::read_link(path).expect("read_link");
            assert_eq!(target, want_target);
        }

        #[test]
        fn write_atomic_through_symlink_updates_target_and_keeps_link() {
            let dir = TempDir::new("atomic-symlink-3dba");
            let cfg_dir = dir.path().join("cfg").join("omairc");
            fs::create_dir_all(&cfg_dir).unwrap();
            let target = cfg_dir.join("real").join("omairc.conf");
            fs::create_dir_all(target.parent().unwrap()).unwrap();
            fs::write(&target, b"[preferences]\ndirects=false\n").unwrap();
            fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
            let link = cfg_dir.join("omairc.conf");
            symlink("real/omairc.conf", &link).unwrap();

            write_atomic_preserve_mode(&link, b"[preferences]\ndirects=true\n").unwrap();

            assert_is_symlink(&link, Path::new("real/omairc.conf"));
            assert_eq!(
                fs::read(&target).unwrap(),
                b"[preferences]\ndirects=true\n"
            );
            let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o644);
            assert!(!fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_file());
        }

        #[test]
        fn write_atomic_through_symlinked_parent_and_dotdot_in_target() {
            let dir = TempDir::new("atomic-symlink-parent-dotdot-3dba");
            let real_store = dir.path().join("real").join("store");
            let real_cfg = dir.path().join("real").join("cfg").join("omairc");
            fs::create_dir_all(&real_store).unwrap();
            fs::create_dir_all(&real_cfg).unwrap();
            let real_file = real_store.join("omairc.conf");
            fs::write(&real_file, b"old\n").unwrap();
            fs::set_permissions(&real_file, fs::Permissions::from_mode(0o644)).unwrap();
            let cfg_link = dir.path().join("cfg");
            symlink(
                dir.path().join("real").join("cfg"),
                &cfg_link,
            )
            .unwrap();
            let link = cfg_link.join("omairc").join("omairc.conf");
            symlink("../../store/omairc.conf", &link).unwrap();

            write_atomic_preserve_mode(&link, b"new\n").unwrap();

            assert_is_symlink(&link, Path::new("../../store/omairc.conf"));
            assert_eq!(fs::read(&real_file).unwrap(), b"new\n");
        }

        #[test]
        fn write_atomic_follows_relative_symlink_and_two_hop_chain() {
            let dir = TempDir::new("atomic-symlink-chain-3dba");
            let store = dir.path().join("store");
            let links = dir.path().join("links");
            fs::create_dir_all(&store).unwrap();
            fs::create_dir_all(&links).unwrap();
            let target = store.join("omairc.conf");
            fs::write(&target, b"old\n").unwrap();
            fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap();
            let hop1 = links.join("hop1.conf");
            let hop2 = links.join("hop2.conf");
            symlink("../store/omairc.conf", &hop1).unwrap();
            symlink("hop1.conf", &hop2).unwrap();

            write_atomic_preserve_mode(&hop2, b"new\n").unwrap();

            assert_is_symlink(&hop2, Path::new("hop1.conf"));
            assert_is_symlink(&hop1, Path::new("../store/omairc.conf"));
            assert_eq!(fs::read(&target).unwrap(), b"new\n");
        }

        #[test]
        fn write_atomic_creates_dangling_symlink_target_when_parent_exists() {
            let dir = TempDir::new("atomic-symlink-dangling-3dba");
            let cfg_dir = dir.path().join("cfg").join("omairc");
            fs::create_dir_all(&cfg_dir).unwrap();
            fs::create_dir_all(cfg_dir.join("real")).unwrap();
            let link = cfg_dir.join("omairc.conf");
            symlink("real/omairc.conf", &link).unwrap();

            write_atomic(&link, b"[preferences]\ndirects=true\n").unwrap();

            let target = cfg_dir.join("real").join("omairc.conf");
            assert!(target.is_file(), "dangling target must be created");
            assert_is_symlink(&link, Path::new("real/omairc.conf"));
            assert_eq!(
                fs::read(&target).unwrap(),
                b"[preferences]\ndirects=true\n"
            );
            let mode = fs::metadata(&target).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        #[test]
        fn write_atomic_errors_when_dangling_target_directory_missing() {
            let dir = TempDir::new("atomic-symlink-missing-dir-3dba");
            let cfg_dir = dir.path().join("cfg").join("omairc");
            fs::create_dir_all(&cfg_dir).unwrap();
            let link = cfg_dir.join("omairc.conf");
            symlink("missing/deep/omairc.conf", &link).unwrap();
            let missing_dir = cfg_dir.join("missing");

            let err = write_atomic(&link, b"nope\n");
            assert!(err.is_err(), "expected missing target directory to fail");
            assert!(!missing_dir.exists());
        }
    }
}
