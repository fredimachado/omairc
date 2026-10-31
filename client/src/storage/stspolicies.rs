//! The STS policy cache on disk: the `sts` INI file beside the settings file, in the format
//! `IrcStsStore` writes in Qt (`tui/internal/session/sts.go`). Each host is one group, named by its
//! `sts_group_key`, holding `port`, `duration` and `expiry` (epoch seconds). Qt and this client read
//! and write the same entries.
//! A well-formed file is updated in place through `Settings`. When the file is malformed, this
//! store renames it to a non-clobbering `sts.corrupt` backup (then `sts.corrupt.1`, `.2`, …) and
//! writes a clean file from the parseable entries plus the new policy. A failed rename leaves the
//! file alone. An unreadable or unwritable file is still left alone. Each successful write then
//! restricts the file to its owner, as `restrictPermissions` does.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use super::settings::{Settings, Status};
use crate::session::sts::{sts_group_key, StsCached, StsStore};

/// The value names of one host's group, as Qt writes them.
const PORT_KEY: &str = "port";
const DURATION_KEY: &str = "duration";
const EXPIRY_KEY: &str = "expiry";

/// The on-disk STS cache at `path`. Every call reads or writes the file, so an entry another
/// client wrote is seen on the next call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IniStsStore {
    path: PathBuf,
}

impl IniStsStore {
    /// A cache in the file at `path`. The file and its folder are created on the first write.
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Removes the group `key`. Nothing is written when the group is not there.
    fn remove(&self, key: &str) {
        let mut settings = Settings::open(&self.path);
        if !settings.child_groups("").iter().any(|group| group == key) {
            return;
        }
        settings.remove_group(key);
        persist_sts_settings(&mut settings, &self.path);
    }
}

impl StsStore for IniStsStore {
    fn lookup(&self, host: &str, now: DateTime<Utc>) -> Option<StsCached> {
        let key = sts_group_key(host)?;
        let settings = Settings::open(&self.path);
        if !settings.contains(&key, PORT_KEY) || !settings.contains(&key, EXPIRY_KEY) {
            return None;
        }
        let port = settings.uint_value(&key, PORT_KEY, 0);
        let duration_seconds = settings.int_value(&key, DURATION_KEY, 0);
        let expiry_epoch_seconds = settings.int_value(&key, EXPIRY_KEY, 0);

        let Some(port) = u16::try_from(port).ok().filter(|&port| port >= 1) else {
            self.remove(&key);
            return None;
        };
        if expiry_epoch_seconds <= now.timestamp() {
            self.remove(&key);
            return None;
        }
        Some(StsCached {
            port,
            duration_seconds,
            expiry_epoch_seconds,
        })
    }

    fn store(&self, host: &str, port: u16, duration_seconds: i64, now: DateTime<Utc>) {
        if duration_seconds == 0 {
            self.clear(host);
            return;
        }
        let Some(key) = sts_group_key(host) else {
            return;
        };
        if port == 0 {
            return;
        }
        let expiry = now.timestamp().saturating_add(duration_seconds);
        let mut settings = Settings::open(&self.path);
        merge_sts_policy(
            &mut settings,
            &key,
            StsCached {
                port,
                duration_seconds,
                expiry_epoch_seconds: expiry,
            },
            now.timestamp(),
        );
        persist_sts_settings(&mut settings, &self.path);
    }

    fn clear(&self, host: &str) {
        if let Some(key) = sts_group_key(host) {
            self.remove(&key);
        }
    }
}

/// Keeps the stronger unexpired policy when the same host already has one.
/// A longer remaining expiry wins; on a tie the incoming policy wins. `duration=0`
/// clears go through [`IniStsStore::clear`] and never call this.
fn merge_sts_policy(settings: &mut Settings, key: &str, incoming: StsCached, now_unix: i64) {
    if settings.contains(key, PORT_KEY) && settings.contains(key, EXPIRY_KEY) {
        let existing_expiry = settings.int_value(key, EXPIRY_KEY, 0);
        if existing_expiry > now_unix && existing_expiry > incoming.expiry_epoch_seconds {
            return;
        }
    }
    settings.set_uint(key, PORT_KEY, u64::from(incoming.port));
    settings.set_value(key, DURATION_KEY, &incoming.duration_seconds.to_string());
    settings.set_value(
        key,
        EXPIRY_KEY,
        &incoming.expiry_epoch_seconds.to_string(),
    );
}

/// Writes `settings` to `path`. On a malformed file, renames it to a backup first;
/// a failed rename leaves the file alone. Access errors leave the file alone.
fn persist_sts_settings(settings: &mut Settings, path: &Path) {
    match settings.write_blocked() {
        Status::AccessError => {}
        Status::FormatError => {
            if !rename_sts_corrupt_backup(path) {
                return;
            }
            // Merge may have kept existing values (dirty=false). Still rewrite.
            settings.mark_dirty();
            let _ = settings.sync();
            restrict_permissions(path);
        }
        Status::Written | Status::Absent => {
            let _ = settings.sync();
            restrict_permissions(path);
        }
    }
}

/// Renames `path` to `path.corrupt`, or `path.corrupt.N` when that name is taken.
/// Never overwrites an existing backup. Returns false when the rename fails.
fn rename_sts_corrupt_backup(path: &Path) -> bool {
    let Some(backup) = next_sts_corrupt_path(path) else {
        return false;
    };
    fs::rename(path, backup).is_ok()
}

/// The next unused backup path beside `path`: `sts.corrupt`, then `sts.corrupt.1`, …
fn next_sts_corrupt_path(path: &Path) -> Option<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path.file_name()?.to_string_lossy();
    let primary = parent.join(format!("{name}.corrupt"));
    if !primary.exists() {
        return Some(primary);
    }
    for n in 1u64.. {
        let candidate = parent.join(format!("{name}.corrupt.{n}"));
        if !candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Owner read and write only (`IrcStsStore::restrictPermissions`). A missing file is left alone.
#[cfg(unix)]
fn restrict_permissions(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
fn restrict_permissions(_path: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testdir::TempDir;
    use chrono::TimeZone;

    /// The Qt group name for `irc.example`: the SHA-256 of the host, in lowercase hex.
    const IRC_EXAMPLE_GROUP: &str =
        "a84a91b2bc2ea526cb7d244872ddffe0770a9ca3737ee9ef355e9018a76767de";

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0).single().unwrap()
    }

    fn store_in(dir: &TempDir) -> (IniStsStore, PathBuf) {
        let path = dir.path().join("sts");
        (IniStsStore::new(path.clone()), path)
    }

    fn other_example_group() -> String {
        sts_group_key("other.example").expect("host")
    }

    #[test]
    fn a_policy_survives_a_restart_under_the_qt_group_layout() {
        let dir = TempDir::new("sts-round-trip");
        let path = dir.path().join("nested").join("sts");
        IniStsStore::new(path.clone()).store("IRC.example ", 6697, 60, at(1000));

        let restarted = IniStsStore::new(path.clone());
        assert_eq!(
            restarted.lookup("irc.example", at(1030)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 1060,
            })
        );

        let text = fs::read_to_string(&path).expect("the file is written");
        assert!(text.contains(&format!("[{IRC_EXAMPLE_GROUP}]")), "{text}");
        assert!(text.contains("port=6697"), "{text}");
        assert!(text.contains("duration=60"), "{text}");
        assert!(text.contains("expiry=1060"), "{text}");
    }

    #[test]
    fn a_file_written_by_qt_is_read() {
        let dir = TempDir::new("sts-qt");
        let (store, path) = store_in(&dir);
        let qt = format!("[{IRC_EXAMPLE_GROUP}]\nport=6697\nduration=60\nexpiry=4102444800\n");
        fs::write(&path, qt).expect("write the fixture");

        assert_eq!(
            store.lookup("irc.example", at(1_700_000_000)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 4_102_444_800,
            })
        );
    }

    #[test]
    fn an_expired_entry_is_dropped_from_the_file() {
        let dir = TempDir::new("sts-expiry");
        let (store, path) = store_in(&dir);
        store.store("irc.example", 6697, 60, at(0));

        assert_eq!(store.lookup("irc.example", at(60)), None);
        let text = fs::read_to_string(&path).expect("the file stays");
        assert!(
            !text.contains(IRC_EXAMPLE_GROUP),
            "the expired group is removed: {text}"
        );
    }

    #[test]
    fn a_bad_port_is_dropped_from_the_file() {
        for port in ["0", "70000", "junk"] {
            let dir = TempDir::new("sts-port");
            let (store, path) = store_in(&dir);
            let qt =
                format!("[{IRC_EXAMPLE_GROUP}]\nport={port}\nduration=60\nexpiry=4102444800\n");
            fs::write(&path, qt).expect("write the fixture");

            assert_eq!(store.lookup("irc.example", at(1)), None, "port {port}");
            let text = fs::read_to_string(&path).expect("the file stays");
            assert!(
                !text.contains(IRC_EXAMPLE_GROUP),
                "port {port} is removed: {text}"
            );
        }
    }

    #[test]
    fn a_zero_duration_clears_the_host() {
        let dir = TempDir::new("sts-zero");
        let (store, _) = store_in(&dir);
        store.store("irc.example", 6697, 60, at(0));
        store.store("irc.example", 6697, 0, at(0));
        assert_eq!(store.lookup("irc.example", at(1)), None);
    }

    #[test]
    fn a_lookup_or_clear_with_nothing_stored_writes_no_file() {
        let dir = TempDir::new("sts-empty");
        let (store, path) = store_in(&dir);
        assert_eq!(store.lookup("irc.example", at(1)), None);
        store.clear("irc.example");
        assert!(!path.exists());
    }

    #[test]
    fn a_malformed_file_is_backed_up_and_a_new_policy_survives() {
        let dir = TempDir::new("sts-malformed");
        let (store, path) = store_in(&dir);
        let garbage = "this line has no equals sign\n";
        fs::write(&path, garbage).expect("write the fixture");

        store.store("irc.example", 6697, 60, at(0));

        let backup = path.with_file_name("sts.corrupt");
        assert_eq!(
            fs::read_to_string(&backup).expect("backup"),
            garbage,
            "original bytes stay in the backup"
        );
        assert_eq!(
            store.lookup("irc.example", at(1)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 60,
            })
        );
        let restarted = IniStsStore::new(path.clone());
        assert_eq!(
            restarted.lookup("irc.example", at(1)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 60,
            }),
            "new policy survives a reload"
        );
    }

    #[test]
    fn a_second_malformed_write_does_not_clobber_the_first_backup() {
        let dir = TempDir::new("sts-corrupt-n");
        let (store, path) = store_in(&dir);
        let first = "first garbage without equals\n";
        fs::write(&path, first).expect("write");
        store.store("irc.example", 6697, 60, at(0));

        let second = "second garbage without equals\n";
        fs::write(&path, second).expect("re-corrupt");
        store.store("other.example", 6697, 30, at(10));

        assert_eq!(
            fs::read_to_string(path.with_file_name("sts.corrupt")).unwrap(),
            first,
            "first backup must stay untouched"
        );
        assert_eq!(
            fs::read_to_string(path.with_file_name("sts.corrupt.1")).unwrap(),
            second
        );
        assert!(store.lookup("other.example", at(11)).is_some());
    }

    #[test]
    fn valid_entries_in_a_partly_malformed_file_are_preserved() {
        let dir = TempDir::new("sts-partial");
        let (store, path) = store_in(&dir);
        let other = other_example_group();
        let mixed = format!(
            "[{IRC_EXAMPLE_GROUP}]\nport=6697\nduration=60\nexpiry=4102444800\nbad line\n[{other}]\nport=6698\nduration=30\nexpiry=4102444800\n"
        );
        fs::write(&path, &mixed).expect("write");

        store.store("third.example", 6699, 10, at(1_700_000_000));

        assert_eq!(fs::read_to_string(path.with_file_name("sts.corrupt")).unwrap(), mixed);
        assert_eq!(
            store.lookup("irc.example", at(1_700_000_000)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 4_102_444_800,
            })
        );
        assert_eq!(
            store.lookup("other.example", at(1_700_000_000)),
            Some(StsCached {
                port: 6698,
                duration_seconds: 30,
                expiry_epoch_seconds: 4_102_444_800,
            })
        );
        assert!(store.lookup("third.example", at(1_700_000_000)).is_some());
    }

    #[test]
    fn a_weaker_new_policy_does_not_shorten_an_existing_expiry() {
        let dir = TempDir::new("sts-no-weaken");
        let (store, path) = store_in(&dir);
        let mixed = format!(
            "[{IRC_EXAMPLE_GROUP}]\nport=6697\nduration=60\nexpiry=5000\nbad line\n"
        );
        fs::write(&path, mixed).expect("write");

        store.store("irc.example", 7000, 10, at(100));

        assert_eq!(
            store.lookup("irc.example", at(100)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 5000,
            }),
            "existing unexpired policy must not be shortened"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_backup_rename_leaves_the_malformed_file_alone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("sts-rename-fail");
        let (store, path) = store_in(&dir);
        let garbage = "this line has no equals sign\n";
        fs::write(&path, garbage).expect("write");
        // A read-only parent blocks rename, so recovery must fail closed.
        let mut perms = fs::metadata(dir.path()).unwrap().permissions();
        perms.set_mode(0o555);
        fs::set_permissions(dir.path(), perms).unwrap();

        store.store("irc.example", 6697, 60, at(0));

        let mut perms = fs::metadata(dir.path()).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(dir.path(), perms).unwrap();

        assert_eq!(fs::read_to_string(&path).expect("still there"), garbage);
        assert_eq!(store.lookup("irc.example", at(1)), None);
    }

    #[cfg(unix)]
    #[test]
    fn the_file_is_owner_only_after_a_write() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("sts-mode");
        let (store, path) = store_in(&dir);
        store.store("irc.example", 6697, 60, at(0));

        let mode = fs::metadata(&path).expect("stat").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn next_corrupt_path_skips_existing_backups() {
        let dir = TempDir::new("sts-next");
        let path = dir.path().join("sts");
        fs::write(&path, "x").unwrap();
        assert_eq!(
            next_sts_corrupt_path(&path).unwrap(),
            dir.path().join("sts.corrupt")
        );
        fs::write(dir.path().join("sts.corrupt"), "a").unwrap();
        assert_eq!(
            next_sts_corrupt_path(&path).unwrap(),
            dir.path().join("sts.corrupt.1")
        );
        fs::write(dir.path().join("sts.corrupt.1"), "b").unwrap();
        assert_eq!(
            next_sts_corrupt_path(&path).unwrap(),
            dir.path().join("sts.corrupt.2")
        );
    }
}
