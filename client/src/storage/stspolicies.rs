//! The STS policy cache on disk: the `sts` INI file beside the settings file, in the format
//! `IrcStsStore` writes in Qt (`tui/internal/session/sts.go`). Each host is one group, named by its
//! `sts_group_key`, holding `port`, `duration` and `expiry` (epoch seconds). Qt and this client read
//! and write the same entries.
//! A well-formed file is updated in place through `Settings`. When the file is malformed, this
//! store writes a clean temp file, hard-links `sts` to a non-clobbering `sts.corrupt` backup
//! (then `sts.corrupt.1`, `.2`, …), and renames the temp over `sts`. A failure at any step leaves
//! `sts` untouched and keeps the new policy in a small in-memory layer that lookups consult and
//! recovery merges. An unreadable or unwritable file is still left alone. Each successful write
//! then restricts the file to its owner, as `restrictPermissions` does.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use super::settings::{file_not_writable, Settings, Status};
use crate::session::sts::{sts_group_key, StsCached, StsStore};

/// The value names of one host's group, as Qt writes them.
const PORT_KEY: &str = "port";
const DURATION_KEY: &str = "duration";
const EXPIRY_KEY: &str = "expiry";

/// Cap on `sts.corrupt` / `sts.corrupt.N` attempts so a persistent error cannot spin forever.
const MAX_STS_CORRUPT_BACKUPS: u32 = 10_000;

/// The on-disk STS cache at `path`, plus a small in-memory layer for policies that must
/// survive a failed backup or write. Lookups consult memory first; recovery merges
/// unexpired memory entries into the clean file. Normal well-formed writes only persist
/// the mutation, so a host cleared on disk with `duration=0` is not brought back.
#[derive(Debug)]
pub struct IniStsStore {
    path: PathBuf,
    memory: RefCell<BTreeMap<String, StsCached>>,
}

impl IniStsStore {
    /// A cache in the file at `path`. The file and its folder are created on the first write.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            memory: RefCell::new(BTreeMap::new()),
        }
    }

    /// Removes the group `key` from memory and, when present, from the file.
    fn remove(&self, key: &str, now_unix: i64) {
        self.memory.borrow_mut().remove(key);
        let mut settings = Settings::open_sts(&self.path);
        if !settings.child_groups("").iter().any(|group| group == key) {
            return;
        }
        settings.remove_group(key);
        let _ = persist_sts_settings(&mut settings, &self.path, &self.memory, now_unix);
    }

    /// Remembers `entry` under `key` so a failed persist still serves it on lookup
    /// and a later recovery can merge it.
    fn remember(&self, key: &str, entry: StsCached) {
        self.memory.borrow_mut().insert(key.to_string(), entry);
    }

    /// Drops an expired memory entry for `key`, if any.
    fn forget_expired_memory(&self, key: &str, now_unix: i64) {
        let mut memory = self.memory.borrow_mut();
        if memory
            .get(key)
            .is_some_and(|entry| entry.expiry_epoch_seconds <= now_unix)
        {
            memory.remove(key);
        }
    }
}

impl StsStore for IniStsStore {
    fn lookup(&self, host: &str, now: DateTime<Utc>) -> Option<StsCached> {
        let key = sts_group_key(host)?;
        let now_unix = now.timestamp();
        self.forget_expired_memory(&key, now_unix);
        if let Some(cached) = self.memory.borrow().get(&key).copied() {
            if cached.port >= 1 && cached.expiry_epoch_seconds > now_unix {
                return Some(cached);
            }
        }
        let settings = Settings::open_sts(&self.path);
        if !settings.contains(&key, PORT_KEY) || !settings.contains(&key, EXPIRY_KEY) {
            return None;
        }
        let port = settings.uint_value(&key, PORT_KEY, 0);
        let duration_seconds = settings.int_value(&key, DURATION_KEY, 0);
        let expiry_epoch_seconds = settings.int_value(&key, EXPIRY_KEY, 0);

        let Some(port) = u16::try_from(port).ok().filter(|&port| port >= 1) else {
            self.remove(&key, now_unix);
            return None;
        };
        if expiry_epoch_seconds <= now_unix {
            self.remove(&key, now_unix);
            return None;
        }
        let cached = StsCached {
            port,
            duration_seconds,
            expiry_epoch_seconds,
        };
        self.remember(&key, cached);
        Some(cached)
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
        let incoming = StsCached {
            port,
            duration_seconds,
            expiry_epoch_seconds: expiry,
        };
        let mut settings = Settings::open_sts(&self.path);
        merge_sts_policy(&mut settings, &key, incoming, now.timestamp());
        // Remember the merge winner so a failed persist cannot drop it, and so
        // memory never holds a weaker policy than the file map.
        let winner = sts_cached_from_settings(&settings, &key).unwrap_or(incoming);
        self.remember(&key, winner);
        let _ = persist_sts_settings(&mut settings, &self.path, &self.memory, now.timestamp());
    }

    fn clear(&self, host: &str) {
        if let Some(key) = sts_group_key(host) {
            // duration=0: drop memory even when the disk write fails, matching the TUI.
            self.remove(&key, Utc::now().timestamp());
        }
    }
}

/// Reads one host's STS entry from `settings`, or `None` when the port is invalid.
fn sts_cached_from_settings(settings: &Settings, key: &str) -> Option<StsCached> {
    if !settings.contains(key, PORT_KEY) || !settings.contains(key, EXPIRY_KEY) {
        return None;
    }
    let port = u16::try_from(settings.uint_value(key, PORT_KEY, 0))
        .ok()
        .filter(|&port| port >= 1)?;
    Some(StsCached {
        port,
        duration_seconds: settings.int_value(key, DURATION_KEY, 0),
        expiry_epoch_seconds: settings.int_value(key, EXPIRY_KEY, 0),
    })
}

/// Keeps the stronger unexpired policy when the same host already has one.
/// A longer remaining expiry wins; on a tie the incoming policy wins. An existing
/// entry with an invalid port never wins. `duration=0` clears go through
/// [`IniStsStore::clear`] and never call this.
fn merge_sts_policy(settings: &mut Settings, key: &str, incoming: StsCached, now_unix: i64) {
    if settings.contains(key, PORT_KEY) && settings.contains(key, EXPIRY_KEY) {
        let existing_port = settings.uint_value(key, PORT_KEY, 0);
        let existing_expiry = settings.int_value(key, EXPIRY_KEY, 0);
        let port_ok = u16::try_from(existing_port)
            .ok()
            .filter(|&port| port >= 1)
            .is_some();
        if port_ok && existing_expiry > now_unix && existing_expiry > incoming.expiry_epoch_seconds
        {
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

/// Writes `settings` to `path`. On a malformed file, merges unexpired memory, writes a
/// clean temp file, hard-links a backup, then renames the temp over `sts`. A failure at
/// any step leaves `sts` untouched. Access errors leave the file alone.
fn persist_sts_settings(
    settings: &mut Settings,
    path: &Path,
    memory: &RefCell<BTreeMap<String, StsCached>>,
    now_unix: i64,
) -> Status {
    match settings.write_blocked() {
        Status::AccessError => Status::AccessError,
        Status::FormatError => {
            if file_not_writable(path) {
                return Status::AccessError;
            }
            merge_memory_into_settings(settings, memory, now_unix);
            match recover_malformed_sts(path, &settings.encode()) {
                Ok(()) => {
                    restrict_permissions(path);
                    Status::Written
                }
                Err(_) => Status::FormatError,
            }
        }
        Status::Written | Status::Absent => match settings.sync() {
            Status::Written => {
                restrict_permissions(path);
                Status::Written
            }
            other => other,
        },
    }
}

/// Merges every unexpired in-memory policy into `settings` for malformed-file recovery.
fn merge_memory_into_settings(
    settings: &mut Settings,
    memory: &RefCell<BTreeMap<String, StsCached>>,
    now_unix: i64,
) {
    for (key, entry) in memory.borrow().iter() {
        if entry.port < 1 || entry.expiry_epoch_seconds <= now_unix {
            continue;
        }
        merge_sts_policy(settings, key, *entry, now_unix);
    }
}

/// Writes clean `data` to a temp sibling, hard-links `path` to a backup, then renames
/// the temp over `path`. On any failure, leaves `path` untouched and removes the temp.
/// After a successful rename, the backup is restricted to owner-only (0600) so a
/// hard-linked copy of a looser-mode original is not left world-readable.
fn recover_malformed_sts(path: &Path, data: &[u8]) -> io::Result<()> {
    let destination = super::atomicwrite::atomic_write_destination(path)?;
    let temp = write_sts_temp(&destination, data)?;
    let Some(backup) = link_sts_corrupt_backup(path) else {
        let _ = fs::remove_file(&temp);
        return Err(io::Error::other("sts corrupt backup failed"));
    };
    #[cfg(test)]
    if STS_TEST_HOOK.with(|hook| matches!(hook.get(), StsTestHook::FailInstall)) {
        let _ = fs::remove_file(&temp);
        return Err(io::Error::other("test: install after backup failed"));
    }
    match fs::rename(&temp, &destination) {
        Ok(()) => {
            // Chmod after rename so the hard link no longer shares an inode with `sts`.
            restrict_permissions(&backup);
            Ok(())
        }
        Err(err) => {
            let _ = fs::remove_file(&temp);
            Err(err)
        }
    }
}

/// Creates a same-directory temp file with `data`, fsyncs it, and returns its path.
/// Removes the temp on every failure after create.
fn write_sts_temp(path: &Path, data: &[u8]) -> io::Result<PathBuf> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(dir)?;
    let mut last_error = None;
    for n in 0..16u32 {
        let candidate = dir.join(format!(
            ".omairc-sts-{}-{n}.tmp",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(mut file) => {
                let write_result = (|| -> io::Result<()> {
                    #[cfg(test)]
                    if STS_TEST_HOOK.with(|hook| matches!(hook.get(), StsTestHook::FailTempAfterCreate)) {
                        return Err(io::Error::other("test: temp write failed"));
                    }
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        file.set_permissions(fs::Permissions::from_mode(0o600))?;
                    }
                    file.write_all(data)?;
                    file.sync_all()?;
                    Ok(())
                })();
                drop(file);
                match write_result {
                    Ok(()) => return Ok(candidate),
                    Err(err) => {
                        let _ = fs::remove_file(&candidate);
                        return Err(err);
                    }
                }
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => last_error = Some(err),
            Err(err) => return Err(err),
        }
    }
    Err(last_error.unwrap_or_else(|| io::Error::other("no free sts temp name")))
}

fn sts_corrupt_backup_limit() -> u32 {
    #[cfg(test)]
    if let StsTestHook::MaxBackups(limit) = STS_TEST_HOOK.with(std::cell::Cell::get) {
        return limit.max(1);
    }
    MAX_STS_CORRUPT_BACKUPS
}

/// Hard-links `path` to `path.corrupt` / `path.corrupt.N`. Retries only on "already
/// exists"; falls back to an exclusive-create copy. Fails closed on other errors.
/// Returns the backup path on success.
fn link_sts_corrupt_backup(path: &Path) -> Option<PathBuf> {
    #[cfg(test)]
    match STS_TEST_HOOK.with(std::cell::Cell::get) {
        StsTestHook::FailBackup => return None,
        StsTestHook::FailCopyAfterCreate => {
            // Exercise the copy fallback, then fail it (and require cleanup).
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            let name = path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
            let candidate = parent.join(format!("{name}.corrupt"));
            return match copy_sts_exclusive(path, &candidate) {
                Ok(()) => Some(candidate),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => None,
                Err(_) => None,
            };
        }
        _ => {}
    }
    let parent = path.parent()?;
    let name = path.file_name()?.to_string_lossy();
    for n in 0..sts_corrupt_backup_limit() {
        let candidate = if n == 0 {
            parent.join(format!("{name}.corrupt"))
        } else {
            parent.join(format!("{name}.corrupt.{n}"))
        };
        match fs::hard_link(path, &candidate) {
            Ok(()) => return Some(candidate),
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(_) => match copy_sts_exclusive(path, &candidate) {
                Ok(()) => return Some(candidate),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(_) => return None,
            },
        }
    }
    None
}

/// Copies `src` to a new `dst` created with `create_new`, then fsyncs.
/// Removes a partial `dst` on any failure after create.
fn copy_sts_exclusive(src: &Path, dst: &Path) -> io::Result<()> {
    let data = fs::read(src)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst)?;
    let write_result = (|| -> io::Result<()> {
        #[cfg(test)]
        if STS_TEST_HOOK.with(|hook| matches!(hook.get(), StsTestHook::FailCopyAfterCreate)) {
            // Leave a truncated file that the cleanup path must remove.
            let _ = file.write_all(b"partial");
            return Err(io::Error::other("test: copy write failed"));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(&data)?;
        file.sync_all()?;
        Ok(())
    })();
    drop(file);
    match write_result {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(dst);
            Err(err)
        }
    }
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum StsTestHook {
    #[default]
    None,
    /// Recovery stops after the backup step and removes the temp file.
    FailInstall,
    /// Backup step itself fails (sts untouched, policy kept in memory).
    FailBackup,
    /// Copy fallback creates a partial file then fails; partial must be removed.
    FailCopyAfterCreate,
    /// Temp create succeeds then write fails; temp must be removed.
    FailTempAfterCreate,
    /// Caps the corrupt-backup name loop so exhaustion is testable.
    MaxBackups(u32),
}

#[cfg(test)]
thread_local! {
    static STS_TEST_HOOK: std::cell::Cell<StsTestHook> =
        const { std::cell::Cell::new(StsTestHook::None) };
}

#[cfg(test)]
fn with_sts_test_hook<T>(hook: StsTestHook, f: impl FnOnce() -> T) -> T {
    STS_TEST_HOOK.with(|cell| cell.set(hook));
    let result = f();
    STS_TEST_HOOK.with(|cell| cell.set(StsTestHook::None));
    result
}

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
    fn an_existing_backup_name_is_never_clobbered() {
        let dir = TempDir::new("sts-corrupt-preexist");
        let (store, path) = store_in(&dir);
        let preexisting = "preexisting backup bytes\n";
        fs::write(dir.path().join("sts.corrupt"), preexisting).unwrap();
        let garbage = "fresh garbage without equals\n";
        fs::write(&path, garbage).unwrap();

        store.store("irc.example", 6697, 60, at(0));

        assert_eq!(
            fs::read_to_string(dir.path().join("sts.corrupt")).unwrap(),
            preexisting,
            "existing backup must not be overwritten"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("sts.corrupt.1")).unwrap(),
            garbage
        );
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

    #[test]
    fn an_invalid_port_entry_does_not_beat_a_valid_policy() {
        let dir = TempDir::new("sts-bad-port-merge");
        let (store, path) = store_in(&dir);
        let junk = format!(
            "[{IRC_EXAMPLE_GROUP}]\nport=junk\nduration=60\nexpiry=4102444800\nbad line\n"
        );
        fs::write(&path, junk).expect("write");

        store.store("irc.example", 6697, 60, at(1_700_000_000));

        // Reload from disk so memory cannot mask a merge regression.
        let restarted = IniStsStore::new(path);
        assert_eq!(
            restarted.lookup("irc.example", at(1_700_000_000)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 1_700_000_060,
            }),
            "invalid existing port must not win over a valid policy on disk"
        );
    }

    #[test]
    fn an_unclosed_header_does_not_merge_into_a_complete_group() {
        let dir = TempDir::new("sts-unclosed-complete");
        let (store, path) = store_in(&dir);
        let other = other_example_group();
        // Unclosed [other — like main, opens other's section; A's keys stay A's.
        let mixed = format!(
            "[{IRC_EXAMPLE_GROUP}]\nport=6697\nduration=60\nexpiry=4102444800\n[{other}\nport=7000\nduration=100\nexpiry=100\n"
        );
        fs::write(&path, &mixed).expect("write");

        assert_eq!(
            store.lookup("irc.example", at(1_700_000_000)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 4_102_444_800,
            }),
            "A must stay exactly as advertised"
        );
        assert_ne!(
            store.lookup("irc.example", at(1_700_000_000)).map(|e| e.port),
            Some(7000),
            "B's port must not merge into A"
        );

        store.store("third.example", 6699, 10, at(1_700_000_000));
        let restarted = IniStsStore::new(path.clone());
        assert_eq!(
            restarted.lookup("irc.example", at(1_700_000_000)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 4_102_444_800,
            }),
            "A must survive recovery and reload"
        );
        let text = fs::read_to_string(&path).unwrap();
        let a_block = sts_group_block(&text, IRC_EXAMPLE_GROUP);
        assert!(
            a_block.contains("port=6697"),
            "A's port must remain under A: {text}"
        );
        assert!(
            !a_block.contains("port=7000"),
            "B's port must not land under A after recovery: {text}"
        );
    }

    #[test]
    fn an_unclosed_header_does_not_merge_into_a_partial_group() {
        let dir = TempDir::new("sts-unclosed-partial");
        let (store, path) = store_in(&dir);
        let other = other_example_group();
        // A only advertised port; B's duration/expiry/port must not fill A.
        let mixed = format!(
            "[{IRC_EXAMPLE_GROUP}]\nport=6697\n[{other}\nport=7000\nduration=100\nexpiry=100\n"
        );
        fs::write(&path, &mixed).expect("write");

        assert_eq!(
            store.lookup("irc.example", at(1_700_000_000)),
            None,
            "partial A without expiry must not become a full policy from B's keys"
        );

        store.store("third.example", 6699, 10, at(1_700_000_000));
        let text = fs::read_to_string(&path).unwrap();
        let a_block = sts_group_block(&text, IRC_EXAMPLE_GROUP);
        assert!(
            a_block.contains("port=6697"),
            "A's advertised port must remain: {text}"
        );
        assert!(
            !a_block.contains("port=7000"),
            "B's port must not merge into A: {text}"
        );
        assert!(
            !a_block.contains("duration=100"),
            "B's duration must not merge into A: {text}"
        );
        assert!(
            !a_block.contains("expiry=100"),
            "B's expiry must not merge into A: {text}"
        );
        let restarted = IniStsStore::new(path);
        assert_eq!(
            restarted.lookup("irc.example", at(1_700_000_000)),
            None,
            "partial A must not become enforceable after recovery"
        );
    }

    fn sts_group_block<'a>(text: &'a str, group: &str) -> &'a str {
        text.split(&format!("[{group}]"))
            .nth(1)
            .unwrap_or("")
            .split('[')
            .next()
            .unwrap_or("")
    }

    #[test]
    fn a_bad_line_inside_a_valid_group_keeps_the_policy() {
        let dir = TempDir::new("sts-bad-inside-group");
        let (store, path) = store_in(&dir);
        // Bad line after port must not drop duration/expiry on load or recovery.
        let mixed = format!(
            "[{IRC_EXAMPLE_GROUP}]\nport=6697\nbad line\nduration=60\nexpiry=4102444800\n"
        );
        fs::write(&path, &mixed).expect("write");

        assert_eq!(
            store.lookup("irc.example", at(1_700_000_000)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 4_102_444_800,
            }),
            "policy must stay intact on load"
        );

        store.store("other.example", 6698, 30, at(1_700_000_000));

        let restarted = IniStsStore::new(path.clone());
        assert_eq!(
            restarted.lookup("irc.example", at(1_700_000_000)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 4_102_444_800,
            }),
            "policy must survive recovery unchanged"
        );
        assert_eq!(
            fs::read_to_string(path.with_file_name("sts.corrupt")).unwrap(),
            mixed
        );
    }

    #[test]
    fn keys_under_a_garbage_header_are_not_saved() {
        let dir = TempDir::new("sts-garbage-header");
        let (store, path) = store_in(&dir);
        let other = other_example_group();
        // A non-header garbage line would otherwise let host B's keys leak into A.
        let mixed = format!(
            "[{IRC_EXAMPLE_GROUP}]\nport=6697\nduration=60\nexpiry=4102444800\ngarbage header line\nport=7000\nduration=100\nexpiry=100\n[{other}]\nport=6698\nduration=30\nexpiry=4102444800\n"
        );
        fs::write(&path, &mixed).expect("write");

        store.store("third.example", 6699, 10, at(1_700_000_000));

        assert_eq!(
            store.lookup("irc.example", at(1_700_000_000)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 4_102_444_800,
            }),
            "leaked keys after garbage must not weaken A"
        );
        assert_eq!(
            store.lookup("other.example", at(1_700_000_000)),
            Some(StsCached {
                port: 6698,
                duration_seconds: 30,
                expiry_epoch_seconds: 4_102_444_800,
            })
        );
        let text = fs::read_to_string(&path).expect("clean file");
        assert!(
            !text.contains("port=7000"),
            "leaked port must not be saved: {text}"
        );
        assert!(
            !text.contains("expiry=100\n") && !text.contains("expiry=100\r"),
            "leaked short expiry must not be saved: {text}"
        );
    }

    #[test]
    fn a_host_cleared_on_disk_is_not_brought_back_by_a_normal_write() {
        let dir = TempDir::new("sts-no-resurrect");
        let (store, path) = store_in(&dir);
        store.store("keep.example", 6697, 60, at(100));
        store.store("drop.example", 6697, 60, at(100));
        // Simulate another client clearing drop.example on disk with duration=0,
        // while this process still holds it in memory from the earlier store.
        let keep_key = sts_group_key("keep.example").unwrap();
        let drop_key = sts_group_key("drop.example").unwrap();
        let only_keep = format!("[{keep_key}]\nduration=60\nexpiry=160\nport=6697\n");
        fs::write(&path, only_keep).unwrap();
        assert!(
            store.memory.borrow().contains_key(&drop_key),
            "precondition: drop.example still in memory"
        );

        store.store("other.example", 6697, 30, at(110));

        let text = fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains(&drop_key),
            "cleared host must not return on a normal write: {text}"
        );
        assert!(text.contains(&keep_key), "{text}");
        assert!(
            text.contains(&sts_group_key("other.example").unwrap()),
            "{text}"
        );
    }

    #[test]
    fn a_failed_backup_keeps_the_policy_in_memory() {
        let dir = TempDir::new("sts-backup-fail");
        let (store, path) = store_in(&dir);
        let garbage = "this line has no equals sign\n";
        fs::write(&path, garbage).expect("write");

        with_sts_test_hook(StsTestHook::FailBackup, || {
            store.store("irc.example", 6697, 60, at(0));
        });

        assert_eq!(fs::read_to_string(&path).expect("still there"), garbage);
        assert!(
            !path.with_file_name("sts.corrupt").exists(),
            "backup step failed before creating a backup"
        );
        assert_eq!(
            store.lookup("irc.example", at(1)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 60,
            }),
            "failed backup must keep the policy in memory"
        );
        assert_no_sts_temps(dir.path());
    }

    #[test]
    fn a_failed_copy_fallback_leaves_sts_intact_and_removes_partial() {
        let dir = TempDir::new("sts-copy-fail");
        let (store, path) = store_in(&dir);
        let garbage = "this line has no equals sign\n";
        fs::write(&path, garbage).expect("write");

        with_sts_test_hook(StsTestHook::FailCopyAfterCreate, || {
            store.store("irc.example", 6697, 60, at(0));
        });

        assert_eq!(fs::read_to_string(&path).expect("still there"), garbage);
        assert!(
            !path.with_file_name("sts.corrupt").exists(),
            "partial copy backup must be removed"
        );
        assert_eq!(
            store.lookup("irc.example", at(1)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 60,
            }),
            "failed copy fallback must keep the policy in memory"
        );
        assert_no_sts_temps(dir.path());
    }

    #[test]
    fn exhausting_corrupt_backup_names_fails_closed() {
        let dir = TempDir::new("sts-backup-cap");
        let (store, path) = store_in(&dir);
        let garbage = "this line has no equals sign\n";
        fs::write(&path, garbage).expect("write");
        fs::write(dir.path().join("sts.corrupt"), "taken0").unwrap();
        fs::write(dir.path().join("sts.corrupt.1"), "taken1").unwrap();

        with_sts_test_hook(StsTestHook::MaxBackups(2), || {
            store.store("irc.example", 6697, 60, at(0));
        });

        assert_eq!(fs::read_to_string(&path).expect("still there"), garbage);
        assert_eq!(
            fs::read_to_string(dir.path().join("sts.corrupt")).unwrap(),
            "taken0"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("sts.corrupt.1")).unwrap(),
            "taken1"
        );
        assert!(
            !dir.path().join("sts.corrupt.2").exists(),
            "must not invent a name past the cap"
        );
        assert_eq!(
            store.lookup("irc.example", at(1)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 60,
            }),
            "cap exit must keep the policy in memory"
        );
        assert_no_sts_temps(dir.path());
    }

    #[test]
    fn a_failed_temp_write_leaves_no_temp_behind() {
        let dir = TempDir::new("sts-temp-fail");
        let (store, path) = store_in(&dir);
        let garbage = "this line has no equals sign\n";
        fs::write(&path, garbage).expect("write");

        with_sts_test_hook(StsTestHook::FailTempAfterCreate, || {
            store.store("irc.example", 6697, 60, at(0));
        });

        assert_eq!(fs::read_to_string(&path).expect("still there"), garbage);
        assert_eq!(
            store.lookup("irc.example", at(1)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 60,
            })
        );
        assert_no_sts_temps(dir.path());
    }

    #[test]
    fn a_failed_write_after_backup_leaves_sts_intact() {
        let dir = TempDir::new("sts-after-backup-fail");
        let (store, path) = store_in(&dir);
        let garbage = "this line has no equals sign\n";
        fs::write(&path, garbage).expect("write");

        with_sts_test_hook(StsTestHook::FailInstall, || {
            store.store("irc.example", 6697, 60, at(0));
        });

        assert_eq!(
            fs::read_to_string(&path).expect("sts untouched"),
            garbage,
            "sts must stay intact when install after backup fails"
        );
        assert_eq!(
            fs::read_to_string(path.with_file_name("sts.corrupt")).unwrap(),
            garbage,
            "backup was created before the failed install"
        );
        assert_eq!(
            store.lookup("irc.example", at(1)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 60,
            }),
            "policy stays in memory"
        );
        assert_no_sts_temps(dir.path());
    }

    fn assert_no_sts_temps(dir: &Path) {
        for entry in fs::read_dir(dir).unwrap() {
            let name = entry.unwrap().file_name();
            let name = name.to_string_lossy();
            assert!(
                !name.ends_with(".tmp"),
                "leftover temp file: {name}"
            );
        }
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

    #[cfg(unix)]
    #[test]
    fn a_read_only_malformed_file_is_left_alone() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("sts-readonly-malformed");
        let (store, path) = store_in(&dir);
        let garbage = "this line has no equals sign\n";
        fs::write(&path, garbage).expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        if fs::OpenOptions::new().write(true).open(&path).is_ok() {
            // Running as root: the mode does not stop the write.
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
            return;
        }

        store.store("irc.example", 6697, 60, at(0));

        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(fs::read_to_string(&path).expect("still there"), garbage);
        assert!(
            !path.with_file_name("sts.corrupt").exists(),
            "read-only malformed sts must not be backed up"
        );
        assert_eq!(
            store.lookup("irc.example", at(1)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 60,
            }),
            "policy stays in memory"
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_corrupt_backup_is_owner_only_after_recovery() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("sts-backup-mode");
        let (store, path) = store_in(&dir);
        let garbage = "this line has no equals sign\n";
        fs::write(&path, garbage).expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        store.store("irc.example", 6697, 60, at(0));

        let backup = path.with_file_name("sts.corrupt");
        let mode = fs::metadata(&backup).expect("backup").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "backup must be owner-only after recovery");
        let sts_mode = fs::metadata(&path).expect("sts").permissions().mode() & 0o777;
        assert_eq!(sts_mode, 0o600);
    }
}
