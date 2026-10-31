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

use super::settings::{Settings, Status};
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
        let mut settings = Settings::open(&self.path);
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
        let settings = Settings::open(&self.path);
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
        let mut settings = Settings::open(&self.path);
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
fn recover_malformed_sts(path: &Path, data: &[u8]) -> io::Result<()> {
    let temp = write_sts_temp(path, data)?;
    if !link_sts_corrupt_backup(path) {
        let _ = fs::remove_file(&temp);
        return Err(io::Error::other("sts corrupt backup failed"));
    }
    #[cfg(test)]
    if FAIL_STS_INSTALL.with(std::cell::Cell::get) {
        let _ = fs::remove_file(&temp);
        return Err(io::Error::other("test: install after backup failed"));
    }
    match fs::rename(&temp, path) {
        Ok(()) => Ok(()),
        Err(err) => {
            let _ = fs::remove_file(&temp);
            Err(err)
        }
    }
}

/// Creates a same-directory temp file with `data`, fsyncs it, and returns its path.
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
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    file.set_permissions(fs::Permissions::from_mode(0o600))?;
                }
                file.write_all(data)?;
                file.sync_all()?;
                return Ok(candidate);
            }
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => last_error = Some(err),
            Err(err) => return Err(err),
        }
    }
    Err(last_error.unwrap_or_else(|| io::Error::other("no free sts temp name")))
}

/// Hard-links `path` to `path.corrupt` / `path.corrupt.N`. Retries only on "already
/// exists"; falls back to an exclusive-create copy. Fails closed on other errors.
fn link_sts_corrupt_backup(path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    let Some(name) = path.file_name().map(|n| n.to_string_lossy()) else {
        return false;
    };
    for n in 0..MAX_STS_CORRUPT_BACKUPS {
        let candidate = if n == 0 {
            parent.join(format!("{name}.corrupt"))
        } else {
            parent.join(format!("{name}.corrupt.{n}"))
        };
        match fs::hard_link(path, &candidate) {
            Ok(()) => return true,
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(_) => match copy_sts_exclusive(path, &candidate) {
                Ok(()) => return true,
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(_) => return false,
            },
        }
    }
    false
}

/// Copies `src` to a new `dst` created with `create_new`, then fsyncs.
fn copy_sts_exclusive(src: &Path, dst: &Path) -> io::Result<()> {
    let data = fs::read(src)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(&data)?;
    file.sync_all()?;
    Ok(())
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
thread_local! {
    /// When true, recovery stops after the backup step and removes the temp file.
    static FAIL_STS_INSTALL: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
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

        assert_eq!(
            store.lookup("irc.example", at(1_700_000_000)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 1_700_000_060,
            }),
            "invalid existing port must not win over a valid policy"
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

    #[cfg(unix)]
    #[test]
    fn a_failed_backup_keeps_the_policy_in_memory() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("sts-rename-fail");
        let (store, path) = store_in(&dir);
        let garbage = "this line has no equals sign\n";
        fs::write(&path, garbage).expect("write");
        // A read-only parent blocks backup creation, so recovery must fail closed.
        let mut perms = fs::metadata(dir.path()).unwrap().permissions();
        perms.set_mode(0o555);
        fs::set_permissions(dir.path(), perms).unwrap();

        store.store("irc.example", 6697, 60, at(0));

        let mut perms = fs::metadata(dir.path()).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(dir.path(), perms).unwrap();

        assert_eq!(fs::read_to_string(&path).expect("still there"), garbage);
        assert_eq!(
            store.lookup("irc.example", at(1)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 60,
            }),
            "failed backup must keep the policy in memory"
        );
    }

    #[test]
    fn a_failed_write_after_backup_leaves_sts_intact() {
        let dir = TempDir::new("sts-after-backup-fail");
        let (store, path) = store_in(&dir);
        let garbage = "this line has no equals sign\n";
        fs::write(&path, garbage).expect("write");

        FAIL_STS_INSTALL.with(|flag| flag.set(true));
        store.store("irc.example", 6697, 60, at(0));
        FAIL_STS_INSTALL.with(|flag| flag.set(false));

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
        for entry in fs::read_dir(dir.path()).unwrap() {
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
}
