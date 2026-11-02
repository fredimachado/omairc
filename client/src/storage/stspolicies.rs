//! The STS policy cache on disk: the `sts` INI file beside the settings file, in the format
//! `IrcStsStore` writes in Qt (`tui/internal/session/sts.go`). Each host is one group, named by its
//! `sts_group_key`, holding `port`, `duration` and `expiry` (epoch seconds). Qt and this client read
//! and write the same entries.
//! A well-formed file is updated in place through `Settings`. When the file is malformed, this
//! store writes a clean temp file, hard-links `sts` to a non-clobbering `sts.corrupt` backup
//! (then `sts.corrupt.1`, `.2`, …), and renames the temp over `sts`. A failure at any step leaves
//! `sts` untouched and keeps the new policy in memory as unsaved. Lookups prefer an unsaved policy
//! until the file moves on from the view it was recorded against (another client saved a different
//! live policy, or cleared the host), then keep it as saved memory. Every later successful write
//! merges the unsaved policies into the file. An unreadable or unwritable file is still left alone. Each successful write then
//! restricts the file to its owner, as `restrictPermissions` does.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use super::settings::{file_not_writable, probe_ini, IniProbe, Settings, Status};
use crate::session::sts::{is_sts_group_key, sts_group_key, StsCached, StsStore};

/// The value names of one host's group, as Qt writes them.
const PORT_KEY: &str = "port";
const DURATION_KEY: &str = "duration";
const EXPIRY_KEY: &str = "expiry";

/// Cap on `sts.corrupt` / `sts.corrupt.N` attempts so a persistent error cannot spin forever.
const MAX_STS_CORRUPT_BACKUPS: u32 = 10_000;

/// What the file held for one host when it was read. `Unreadable` and `Unknown` say nothing about
/// the host. `Unreadable` leaves a recorded baseline as it was, because the file may be unchanged.
/// `Unknown` ends the baseline: the file is missing or malformed, or its group for the host is not
/// a valid policy, so the next readable view is compared with nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
enum FileView {
    Unreadable,
    Unknown,
    /// A well-formed file with no group for the host.
    Clear,
    /// The host's valid policy group, lapsed or not.
    Policy(StsCached),
}

impl FileView {
    /// What the view says about the host: `Some(None)` for a clear, `Some(Some(policy))` for a
    /// policy, and `None` when the view says nothing.
    fn known(self) -> Option<Option<StsCached>> {
        match self {
            Self::Unreadable | Self::Unknown => None,
            Self::Clear => Some(None),
            Self::Policy(cached) => Some(Some(cached)),
        }
    }

    fn policy(self) -> Option<StsCached> {
        self.known().flatten()
    }
}

/// Reads `key` as the file at `path` holds it, given `settings` loaded from that file. Only a
/// well-formed file with no group for the host is a clear. A missing file, a malformed file, and a
/// group that is present but is not a valid policy are `Unknown`, unless the file is unreadable.
fn file_view(path: &Path, settings: &Settings, key: &str) -> FileView {
    let probe = probe_ini(path);
    if probe == IniProbe::Unreadable {
        return FileView::Unreadable;
    }
    if let Some(policy) = read_policy(settings, key) {
        return FileView::Policy(policy);
    }
    let missing_group = !settings.child_groups("").iter().any(|group| group == key);
    if probe == IniProbe::Ok && missing_group && path.exists() {
        FileView::Clear
    } else {
        FileView::Unknown
    }
}

/// Whether the file moved from `recorded` to `now`. Only a view that says something can move.
/// Another client saving a different live policy moves it, and so does clearing the host while
/// the recorded policy was live. A lapsed policy is not in force, so it moves nothing. A refresh
/// by another client keeps the port and duration, so it never moves.
fn file_moved(recorded: FileView, now: FileView, now_unix: i64) -> bool {
    let (Some(before), Some(after)) = (recorded.known(), now.known()) else {
        return false;
    };
    let identity = |policy: Option<StsCached>| policy.map(|c| (c.port, c.duration_seconds));
    if identity(before) == identity(after) {
        return false;
    }
    match after {
        Some(cached) => cached.expiry_epoch_seconds > now_unix,
        None => before.is_some_and(|cached| cached.expiry_epoch_seconds > now_unix),
    }
}

/// One host's policy in memory. `unsaved` marks a policy whose write failed: the file does
/// not hold it, so lookups prefer it over the file until the file moves on. It holds the view the
/// policy is compared with, which is `Unknown` or `Unreadable` when there is no readable view yet.
#[derive(Clone, Copy, Debug)]
struct Remembered {
    cached: StsCached,
    unsaved: Option<FileView>,
}

/// The on-disk STS cache at `path`, plus a small in-memory layer for policies that must
/// survive a failed backup or write. Lookups prefer an unsaved policy the file has not moved on
/// from, then read the file, then fall back to memory only when the file has no live policy for
/// the host. Every call opens the file through [`IniStsStore::open`], which settles the unsaved
/// policies first. Every successful write merges the kept unsaved policies into the file. Only
/// recovery also merges saved memory entries where the file has none. Normal well-formed writes
/// do not merge saved memory, so a host cleared on disk with `duration=0` is not brought back.
#[derive(Debug)]
pub struct IniStsStore {
    path: PathBuf,
    memory: RefCell<BTreeMap<String, Remembered>>,
}

impl IniStsStore {
    /// A cache in the file at `path`. The file and its folder are created on the first write.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            memory: RefCell::new(BTreeMap::new()),
        }
    }

    /// Opens the file for one call and settles every unsaved policy against the view it reads,
    /// before the call changes anything. An unreadable file keeps each baseline. An unknown view
    /// ends it. A readable view that moved from a live baseline marks the policy saved, so the
    /// file answers and nothing writes it back. Otherwise the view becomes the baseline.
    fn open(&self, now_unix: i64) -> Settings {
        let settings = Settings::open_sts(&self.path);
        let unsaved: Vec<String> = self
            .memory
            .borrow()
            .iter()
            .filter(|(_, held)| held.unsaved.is_some())
            .map(|(key, _)| key.clone())
            .collect();
        for key in unsaved {
            let file = file_view(&self.path, &settings, &key);
            let mut memory = self.memory.borrow_mut();
            let Some(held) = memory.get_mut(&key) else {
                continue;
            };
            let Some(recorded) = held.unsaved else {
                continue;
            };
            if file_moved(recorded, file, now_unix) {
                // The file moved on, so it must never get this policy back. It stays in memory
                // as a saved policy: the file answers first, and memory answers until expiry.
                held.unsaved = None;
            } else if file != FileView::Unreadable {
                // An unreadable file says nothing, so the baseline stays as it was.
                held.unsaved = Some(file);
            }
        }
        settings
    }

    /// Removes the group `key` from memory and, when present, from the file.
    fn remove(&self, key: &str, now_unix: i64) {
        self.memory.borrow_mut().remove(key);
        let mut settings = self.open(now_unix);
        if !settings.child_groups("").iter().any(|group| group == key) {
            return;
        }
        settings.remove_group(key);
        persist_sts_settings(&mut settings, &self.path, &self.memory, now_unix);
    }

    /// Remembers `cached` under `key`. `unsaved` is set when its write has not landed, with
    /// the view the policy is compared with.
    fn remember(&self, key: &str, cached: StsCached, unsaved: Option<FileView>) {
        self.memory
            .borrow_mut()
            .insert(key.to_string(), Remembered { cached, unsaved });
    }
}

impl StsStore for IniStsStore {
    /// An unsaved policy answers first while the file has not moved on from it, so the file never
    /// replaces it. Then the file answers when it holds a live policy for `host`, so a stale memory
    /// entry never beats a newer policy another client saved. Memory answers last, when the file
    /// has none, which covers a corrupt or unwritable file.
    fn lookup(&self, host: &str, now: DateTime<Utc>) -> Option<StsCached> {
        let key = sts_group_key(host)?;
        let now_unix = now.timestamp();
        let settings = self.open(now_unix);
        let unsaved = self
            .memory
            .borrow()
            .get(&key)
            .copied()
            .filter(|held| held.unsaved.is_some());
        if let Some(held) = unsaved.filter(|held| held.cached.expiry_epoch_seconds > now_unix) {
            return Some(held.cached);
        }
        let file = file_view(&self.path, &settings, &key);
        if let Some(cached) = file.policy() {
            if cached.expiry_epoch_seconds > now_unix {
                self.remember(&key, cached, None);
                return Some(cached);
            }
        }
        let remembered = self.memory.borrow().get(&key).copied();
        if let Some(held) = remembered.filter(|held| held.cached.expiry_epoch_seconds > now_unix) {
            return Some(held.cached);
        }
        if settings.contains(&key, PORT_KEY) && settings.contains(&key, EXPIRY_KEY) {
            // Expired or invalid on disk: drop it from the file.
            self.remove(&key, now_unix);
        }
        None
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
        let now_unix = now.timestamp();
        let incoming = StsCached {
            port,
            duration_seconds,
            expiry_epoch_seconds: now_unix.saturating_add(duration_seconds),
        };
        let mut settings = self.open(now_unix);
        // The file as it stands before this write. If the write fails, the file still holds it.
        let file = file_view(&self.path, &settings, &key);
        merge_sts_policy(&mut settings, &key, incoming);
        // Remember the written policy as unsaved before the write, so a failed persist keeps it
        // and the next successful write lands it.
        self.remember(&key, incoming, Some(file));
        persist_sts_settings(&mut settings, &self.path, &self.memory, now_unix);
    }

    fn refresh(&self, host: &str, duration_seconds: i64, now: DateTime<Utc>) {
        let Some(key) = sts_group_key(host) else {
            return;
        };
        let now_unix = now.timestamp();
        let mut settings = self.open(now_unix);
        let file = file_view(&self.path, &settings, &key);
        let unsaved = self
            .memory
            .borrow()
            .get(&key)
            .copied()
            .filter(|held| held.unsaved.is_some());
        // An unsaved policy is the one lookup serves, so a refresh extends it. Otherwise the
        // file's policy is extended.
        let Some(cached) = unsaved
            .map(|held| held.cached)
            .or_else(|| file.policy())
            .filter(|cached| cached.duration_seconds == duration_seconds)
        else {
            return;
        };
        let refreshed = StsCached {
            expiry_epoch_seconds: now_unix.saturating_add(duration_seconds),
            ..cached
        };
        merge_sts_policy(&mut settings, &key, refreshed);
        // An unsaved policy keeps the baseline that `open` settled. Otherwise the file's view is the
        // baseline, since this refresh extends that policy.
        let recorded = unsaved.and_then(|held| held.unsaved).unwrap_or(file);
        self.remember(&key, refreshed, Some(recorded));
        persist_sts_settings(&mut settings, &self.path, &self.memory, now_unix);
    }

    fn clear(&self, host: &str) {
        if let Some(key) = sts_group_key(host) {
            // duration=0: drop memory even when the disk write fails, matching the TUI.
            self.remove(&key, Utc::now().timestamp());
        }
    }
}

/// Reads one host's group as a policy: a port in 1..=65535, and a duration and expiry of at
/// least one second. `None` for a group that is missing or malformed.
fn read_policy(settings: &Settings, key: &str) -> Option<StsCached> {
    let port = u16::try_from(settings.uint_value(key, PORT_KEY, 0))
        .ok()
        .filter(|&port| port >= 1)?;
    let duration_seconds = settings.int_value(key, DURATION_KEY, 0);
    let expiry_epoch_seconds = settings.int_value(key, EXPIRY_KEY, 0);
    (duration_seconds >= 1 && expiry_epoch_seconds >= 1).then_some(StsCached {
        port,
        duration_seconds,
        expiry_epoch_seconds,
    })
}

/// Writes `incoming` as the current policy for `key`. Per IRCv3 STS, the latest
/// policy replaces the previous one. `duration=0` clears go through
/// [`IniStsStore::clear`] and never call this.
fn merge_sts_policy(settings: &mut Settings, key: &str, incoming: StsCached) {
    settings.set_uint(key, PORT_KEY, u64::from(incoming.port));
    settings.set_value(key, DURATION_KEY, &incoming.duration_seconds.to_string());
    settings.set_value(
        key,
        EXPIRY_KEY,
        &incoming.expiry_epoch_seconds.to_string(),
    );
}

/// Writes `settings` to `path`, with the memory policies merged in first. On a malformed file,
/// writes a clean temp file, hard-links a backup, then renames the temp over `sts`. A failure at
/// any step leaves `sts` untouched. Access errors leave the file alone. Each successful write
/// restricts the file to its owner and marks the memory policies it carried as saved. Every write
/// first drops the groups that are not policies, so the file keeps only real ones.
fn persist_sts_settings(
    settings: &mut Settings,
    path: &Path,
    memory: &RefCell<BTreeMap<String, Remembered>>,
    now_unix: i64,
) {
    drop_invalid_groups(settings);
    match settings.write_blocked() {
        Status::AccessError => {}
        Status::FormatError => {
            if file_not_writable(path) {
                return;
            }
            merge_memory_into_settings(settings, memory, now_unix, true);
            if recover_malformed_sts(path, &settings.encode()).is_ok() {
                restrict_permissions(path);
                settle_written(memory, now_unix);
            }
        }
        Status::Written | Status::Absent => {
            merge_memory_into_settings(settings, memory, now_unix, false);
            if settings.sync() == Status::Written {
                restrict_permissions(path);
                settle_written(memory, now_unix);
            }
        }
    }
}

/// Marks every unexpired memory policy as saved once a write has landed. Each one is in the file
/// now: an unsaved one was merged, and a saved one was there already or came from recovery.
fn settle_written(memory: &RefCell<BTreeMap<String, Remembered>>, now_unix: i64) {
    for held in memory.borrow_mut().values_mut() {
        if held.cached.expiry_epoch_seconds > now_unix {
            held.unsaved = None;
        }
    }
}

/// Drops every group that is not a policy: a name `sts_group_key` could not return, or a
/// port, duration or expiry that is missing or out of range.
fn drop_invalid_groups(settings: &mut Settings) {
    for group in settings.child_groups("") {
        if !is_sts_group_key(&group) || read_policy(settings, &group).is_none() {
            settings.remove_group(&group);
        }
    }
}

/// Merges the unexpired in-memory policies into `settings` for a write. An unsaved policy always
/// lands, since `open` has kept it. A saved one lands only in recovery, and only where the file
/// has no live policy for the host, since a live one is newer than memory.
fn merge_memory_into_settings(
    settings: &mut Settings,
    memory: &RefCell<BTreeMap<String, Remembered>>,
    now_unix: i64,
    recovery: bool,
) {
    for (key, held) in memory.borrow().iter() {
        let entry = held.cached;
        if entry.port < 1 || entry.expiry_epoch_seconds <= now_unix {
            continue;
        }
        if held.unsaved.is_none() {
            let disk_is_live =
                read_policy(settings, key).is_some_and(|disk| disk.expiry_epoch_seconds > now_unix);
            if !recovery || disk_is_live {
                continue;
            }
        }
        merge_sts_policy(settings, key, entry);
    }
}

/// Writes clean `data` to a temp sibling, hard-links `path` to a backup, then renames
/// the temp over `path`. On any failure, leaves `path` untouched and removes the temp.
/// After a successful rename, the backup is restricted to owner-only (0600) so a
/// hard-linked copy of a looser-mode original is not left world-readable.
fn recover_malformed_sts(path: &Path, data: &[u8]) -> io::Result<()> {
    let target = super::atomicwrite::atomic_write_target(path)?;
    super::atomicwrite::ensure_atomic_write_parent(&target)?;
    let temp = write_sts_temp(&target.destination, data)?;
    let Some(backup) = link_sts_corrupt_backup(&target.destination) else {
        let _ = fs::remove_file(&temp);
        return Err(io::Error::other("sts corrupt backup failed"));
    };
    #[cfg(test)]
    if STS_TEST_HOOK.with(|hook| matches!(hook.get(), StsTestHook::FailInstall)) {
        let _ = fs::remove_file(&temp);
        return Err(io::Error::other("test: install after backup failed"));
    }
    match fs::rename(&temp, &target.destination) {
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
    let dir = super::atomicwrite::atomic_write_dir(path).unwrap_or_else(|| PathBuf::from("."));
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
            let parent = super::atomicwrite::atomic_write_parent_dir(path);
            let base = super::atomicwrite::atomic_write_base(path);
            let name = base.to_string_lossy();
            let candidate = parent.join(format!("{name}.corrupt"));
            return match copy_sts_exclusive(path, &candidate) {
                Ok(()) => Some(candidate),
                Err(err) if err.kind() == io::ErrorKind::AlreadyExists => None,
                Err(_) => None,
            };
        }
        _ => {}
    }
    let parent = super::atomicwrite::atomic_write_parent_dir(path);
    let base = super::atomicwrite::atomic_write_base(path);
    let name = base.to_string_lossy();
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
    fn a_refresh_keeps_a_newer_policy_another_client_saved() {
        let dir = TempDir::new("sts-refresh-newer");
        let (ours, path) = store_in(&dir);
        ours.store("irc.example", 6697, 3600, at(0));
        IniStsStore::new(path.clone()).store("irc.example", 6697, 60, at(10));

        ours.refresh("irc.example", 3600, at(20));

        assert_eq!(
            IniStsStore::new(path).lookup("irc.example", at(20)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 70,
            }),
            "the newer 60s policy keeps its expiry"
        );
    }

    #[test]
    fn a_refresh_extends_the_expiry_of_the_same_policy() {
        let dir = TempDir::new("sts-refresh-same");
        let (store, path) = store_in(&dir);
        store.store("irc.example", 6697, 60, at(0));

        store.refresh("irc.example", 60, at(30));

        assert_eq!(
            IniStsStore::new(path).lookup("irc.example", at(31)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 90,
            })
        );
    }

    #[test]
    fn a_refresh_does_not_recreate_a_host_another_client_cleared() {
        let dir = TempDir::new("sts-refresh-cleared");
        let (ours, path) = store_in(&dir);
        ours.store("irc.example", 6697, 60, at(0));
        IniStsStore::new(path.clone()).store("irc.example", 6697, 0, at(10));

        ours.refresh("irc.example", 60, at(20));

        assert_eq!(
            IniStsStore::new(path.clone()).lookup("irc.example", at(20)),
            None
        );
        let text = fs::read_to_string(&path).expect("the file stays");
        assert!(!text.contains(IRC_EXAMPLE_GROUP), "{text}");
    }

    #[test]
    fn a_refresh_revives_an_expired_policy_of_the_same_duration() {
        let dir = TempDir::new("sts-refresh-lapsed");
        let (store, path) = store_in(&dir);
        store.store("irc.example", 6697, 60, at(0));

        store.refresh("irc.example", 60, at(100));

        assert_eq!(
            IniStsStore::new(path).lookup("irc.example", at(100)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 160,
            }),
            "a session that outlived its duration re-arms the policy it held"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_refresh_write_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("sts-refresh-mode");
        let (store, path) = store_in(&dir);
        store.store("irc.example", 6697, 60, at(0));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        store.refresh("irc.example", 60, at(30));

        let mode = fs::metadata(&path).expect("stat").permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn a_stale_memory_policy_does_not_beat_a_newer_one_on_disk() {
        let dir = TempDir::new("sts-memory-stale");
        let (ours, path) = store_in(&dir);
        ours.store("irc.example", 6697, 3600, at(0));
        IniStsStore::new(path).store("irc.example", 6697, 60, at(10));

        assert_eq!(
            ours.lookup("irc.example", at(20)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 70,
            }),
            "the policy another client saved is newer than this process's memory"
        );
    }

    #[test]
    fn a_recovery_keeps_the_newer_policy_on_disk_over_memory() {
        let dir = TempDir::new("sts-recovery-newer");
        let (ours, path) = store_in(&dir);
        ours.store("irc.example", 6697, 3600, at(0));
        IniStsStore::new(path.clone()).store("irc.example", 6697, 60, at(10));
        let mixed = format!(
            "this line has no equals sign\n[{IRC_EXAMPLE_GROUP}]\nport=6697\nduration=60\nexpiry=70\n"
        );
        fs::write(&path, mixed).expect("write");

        ours.store("third.example", 6699, 10, at(20));

        assert_eq!(
            IniStsStore::new(path).lookup("irc.example", at(20)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 70,
            }),
            "recovery must not write the stale 3600s memory over the 60s policy on disk"
        );
    }

    /// Makes `path` read-only. Returns false when the process can still write it (running as
    /// root), so the caller skips.
    #[cfg(unix)]
    fn make_read_only(path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o444)).unwrap();
        if fs::OpenOptions::new().write(true).open(path).is_ok() {
            fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
            return false;
        }
        true
    }

    #[cfg(unix)]
    fn make_writable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
    }

    /// Makes `path` unreadable. Returns false when the process can still read it (running as
    /// root), so the caller skips.
    #[cfg(unix)]
    fn make_unreadable(path: &Path) -> bool {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(path).is_ok() {
            make_writable(path);
            return false;
        }
        true
    }

    #[cfg(unix)]
    #[test]
    fn an_unwritable_store_keeps_its_longer_policy_over_the_file() {
        let dir = TempDir::new("sts-unwritable-longer");
        let (store, path) = store_in(&dir);
        IniStsStore::new(path.clone()).store("irc.example", 6697, 60, at(0));
        if !make_read_only(&path) {
            return;
        }

        store.store("irc.example", 6697, 86400, at(0));
        make_writable(&path);

        for seconds in [31, 61] {
            assert_eq!(
                store.lookup("irc.example", at(seconds)),
                Some(StsCached {
                    port: 6697,
                    duration_seconds: 86400,
                    expiry_epoch_seconds: 86400,
                }),
                "at {seconds}s the unwritten 86400s policy must hold over the file's 60s one"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_later_write_that_lands_clears_the_unwritten_mark() {
        let dir = TempDir::new("sts-unwritten-cleared");
        let (store, path) = store_in(&dir);
        IniStsStore::new(path.clone()).store("irc.example", 6697, 60, at(0));
        if !make_read_only(&path) {
            return;
        }
        store.store("irc.example", 6697, 86400, at(0));
        make_writable(&path);
        store.store("irc.example", 6697, 30, at(0));
        IniStsStore::new(path.clone()).store("irc.example", 6697, 45, at(10));

        assert_eq!(
            store.lookup("irc.example", at(11)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 45,
                expiry_epoch_seconds: 55,
            }),
            "once a write landed, the newer file policy wins again"
        );
    }

    /// Another client saves a different port for the host after this store's write failed, and
    /// the disk recovers. Lookup follows the file, and a refresh does not write 6697 back.
    #[cfg(unix)]
    #[test]
    fn a_replacement_another_client_saves_after_our_failed_write_wins() {
        let dir = TempDir::new("sts-moved-replaced");
        let (ours, path) = store_in(&dir);
        IniStsStore::new(path.clone()).store("irc.example", 6697, 3600, at(0));
        if !make_read_only(&path) {
            return;
        }
        ours.store("irc.example", 6697, 86400, at(10));
        make_writable(&path);
        IniStsStore::new(path.clone()).store("irc.example", 6698, 86400, at(20));

        assert_eq!(
            ours.lookup("irc.example", at(21)),
            Some(StsCached {
                port: 6698,
                duration_seconds: 86400,
                expiry_epoch_seconds: 86420,
            }),
            "the newer port on disk beats this store's unwritten 6697 policy"
        );

        ours.refresh("irc.example", 86400, at(30));
        assert_eq!(
            IniStsStore::new(path).lookup("irc.example", at(31)),
            Some(StsCached {
                port: 6698,
                duration_seconds: 86400,
                expiry_epoch_seconds: 86430,
            }),
            "a refresh must not write the unwritten 6697 policy back"
        );
    }

    /// Another client clears the host after this store's write failed, and the disk recovers.
    /// The policy is saved memory from then on, so lookup still answers it, and a refresh does not
    /// recreate the cleared group.
    #[cfg(unix)]
    #[test]
    fn a_clear_another_client_saves_after_our_failed_write_is_kept_as_saved_memory() {
        let dir = TempDir::new("sts-moved-cleared");
        let (ours, path) = store_in(&dir);
        IniStsStore::new(path.clone()).store("irc.example", 6697, 60, at(0));
        if !make_read_only(&path) {
            return;
        }
        ours.store("irc.example", 6697, 86400, at(0));
        make_writable(&path);
        IniStsStore::new(path.clone()).store("irc.example", 6697, 0, at(10));

        assert_eq!(
            ours.lookup("irc.example", at(11)),
            entry(6697, 86400, 86400),
            "the clear on disk is not written back, and memory keeps answering"
        );
        ours.refresh("irc.example", 86400, at(20));

        assert_eq!(
            IniStsStore::new(path.clone()).lookup("irc.example", at(20)),
            None
        );
        let text = fs::read_to_string(&path).expect("the file stays");
        assert!(
            !text.contains(IRC_EXAMPLE_GROUP),
            "a refresh must not recreate the cleared group: {text}"
        );
    }

    /// The 60s policy lapses, and another client's lookup then drops that group from the file.
    /// A lapsed group removed this way is not a clear, so the unwritten 86400 policy still holds.
    #[cfg(unix)]
    #[test]
    fn a_lapsed_group_dropped_from_disk_is_not_a_clear() {
        let dir = TempDir::new("sts-moved-lapsed");
        let (ours, path) = store_in(&dir);
        IniStsStore::new(path.clone()).store("irc.example", 6697, 60, at(0));
        if !make_read_only(&path) {
            return;
        }
        ours.store("irc.example", 6697, 86400, at(0));
        make_writable(&path);
        IniStsStore::new(path.clone()).lookup("irc.example", at(100));

        assert_eq!(
            ours.lookup("irc.example", at(100)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 86400,
                expiry_epoch_seconds: 86400,
            }),
            "a lapsed group another client dropped must not drop this store's unwritten policy"
        );
    }

    /// Another client refreshes the 60s policy after this store's write failed. The refresh moves
    /// only the expiry, which is not a change, so the unwritten 86400 policy keeps answering after
    /// the recorded 60s expiry has passed.
    #[cfg(unix)]
    #[test]
    fn an_expiry_only_refresh_by_another_client_keeps_the_unwritten_policy() {
        let dir = TempDir::new("sts-moved-expiry-only");
        let (ours, path) = store_in(&dir);
        IniStsStore::new(path.clone()).store("irc.example", 6697, 60, at(0));
        if !make_read_only(&path) {
            return;
        }
        ours.store("irc.example", 6697, 86400, at(0));
        make_writable(&path);
        IniStsStore::new(path.clone()).refresh("irc.example", 60, at(50));

        for seconds in [70, 120] {
            assert_eq!(
                ours.lookup("irc.example", at(seconds)),
                Some(StsCached {
                    port: 6697,
                    duration_seconds: 86400,
                    expiry_epoch_seconds: 86400,
                }),
                "at {seconds}s the unwritten 86400s policy must hold over the refreshed 60s one"
            );
        }
    }

    /// The file holds a 3600s policy when this store's write fails, then it is overwritten with
    /// garbage. A malformed file with no valid group for the host says nothing about it, so the
    /// unwritten 86400 policy still answers instead of the garbage reading as a clear.
    #[cfg(unix)]
    #[test]
    fn a_malformed_file_is_not_a_clear_for_an_unwritten_policy() {
        let dir = TempDir::new("sts-moved-malformed");
        let (ours, path) = store_in(&dir);
        IniStsStore::new(path.clone()).store("irc.example", 6697, 3600, at(0));
        if !make_read_only(&path) {
            return;
        }
        ours.store("irc.example", 6697, 86400, at(0));
        make_writable(&path);
        fs::write(&path, "this line has no equals sign\n").expect("write garbage");

        assert_eq!(
            ours.lookup("irc.example", at(1)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 86400,
                expiry_epoch_seconds: 86400,
            }),
            "a malformed file must not drop the unwritten 86400s policy"
        );
    }

    /// The file is unreadable when this store looks at it. Nothing can say the host moved, so
    /// the unwritten 86400 policy still answers.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_file_keeps_the_unwritten_policy() {
        let dir = TempDir::new("sts-moved-unreadable");
        let (ours, path) = store_in(&dir);
        IniStsStore::new(path.clone()).store("irc.example", 6697, 60, at(0));
        if !make_read_only(&path) {
            return;
        }
        ours.store("irc.example", 6697, 86400, at(0));
        if !make_unreadable(&path) {
            return;
        }

        let answered = ours.lookup("irc.example", at(1));
        make_writable(&path);
        assert_eq!(
            answered,
            Some(StsCached {
                port: 6697,
                duration_seconds: 86400,
                expiry_epoch_seconds: 86400,
            }),
            "an unreadable file must not drop the unwritten policy"
        );
    }

    /// The write fails while the file is unreadable, so nothing readable was recorded. Once the
    /// file reads again, its first view is the baseline, so a later save by another client is seen
    /// and a refresh does not write 6697 over it.
    #[cfg(unix)]
    #[test]
    fn a_first_readable_view_after_an_unreadable_write_is_the_baseline() {
        let dir = TempDir::new("sts-unreadable-baseline");
        let (ours, path) = store_in(&dir);
        IniStsStore::new(path.clone()).store("irc.example", 6697, 60, at(0));
        if !make_unreadable(&path) {
            return;
        }
        ours.store("irc.example", 6697, 86400, at(0));
        make_writable(&path);

        assert_eq!(
            ours.lookup("irc.example", at(1)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 86400,
                expiry_epoch_seconds: 86400,
            }),
            "the first readable lookup takes the baseline and still answers this store's policy"
        );
        IniStsStore::new(path.clone()).store("irc.example", 6698, 86400, at(2));
        assert_eq!(
            ours.lookup("irc.example", at(3)),
            Some(StsCached {
                port: 6698,
                duration_seconds: 86400,
                expiry_epoch_seconds: 86402,
            }),
            "the 6698 save after the baseline must win"
        );

        ours.refresh("irc.example", 86400, at(4));
        assert_eq!(
            IniStsStore::new(path).lookup("irc.example", at(5)),
            Some(StsCached {
                port: 6698,
                duration_seconds: 86400,
                expiry_epoch_seconds: 86404,
            }),
            "a refresh must not write 6697 over the 6698 save"
        );
    }

    /// The write fails while the file is readable, then a refresh runs while it is unreadable.
    /// The refresh cannot write, so it must keep the view the policy was recorded against. A later
    /// save by another client is then still seen once the file reads again.
    #[cfg(unix)]
    #[test]
    fn a_refresh_on_an_unreadable_file_keeps_the_view_it_was_recorded_against() {
        let dir = TempDir::new("sts-refresh-unreadable");
        let (ours, path) = store_in(&dir);
        IniStsStore::new(path.clone()).store("irc.example", 6697, 3600, at(0));
        if !make_read_only(&path) {
            return;
        }
        ours.store("irc.example", 6697, 86400, at(0));
        if !make_unreadable(&path) {
            return;
        }
        ours.refresh("irc.example", 86400, at(10));
        make_writable(&path);
        IniStsStore::new(path.clone()).store("irc.example", 6698, 3600, at(20));

        assert_eq!(
            ours.lookup("irc.example", at(21)),
            Some(StsCached {
                port: 6698,
                duration_seconds: 3600,
                expiry_epoch_seconds: 3620,
            }),
            "the 6698 save after the failed refresh must be seen as a change"
        );
    }

    /// Another client's 3600s policy is on disk when this store's 86400s write fails. A refresh
    /// that also cannot write must leave the file's view as it was, so the unchanged file still
    /// yields to the unwritten policy, and a later save by another client is still seen.
    #[cfg(unix)]
    #[test]
    fn a_refresh_that_cannot_write_keeps_the_file_view_it_checks_against() {
        let dir = TempDir::new("sts-refresh-snapshot");
        let (ours, path) = store_in(&dir);
        IniStsStore::new(path.clone()).store("irc.example", 6698, 3600, at(0));
        if !make_read_only(&path) {
            return;
        }
        ours.store("irc.example", 6697, 86400, at(0));
        ours.refresh("irc.example", 86400, at(10));
        make_writable(&path);

        assert_eq!(
            ours.lookup("irc.example", at(11)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 86400,
                expiry_epoch_seconds: 86410,
            }),
            "an unchanged file must keep the unwritten policy after a failed refresh"
        );
        IniStsStore::new(path.clone()).store("irc.example", 6699, 3600, at(12));
        assert_eq!(
            ours.lookup("irc.example", at(13)),
            Some(StsCached {
                port: 6699,
                duration_seconds: 3600,
                expiry_epoch_seconds: 3612,
            }),
            "a later save by another client must still win"
        );
    }

    #[test]
    fn a_recovery_writes_an_unwritten_policy_over_a_live_one_on_disk() {
        let dir = TempDir::new("sts-recovery-unwritten");
        let (ours, path) = store_in(&dir);
        let key = sts_group_key("irc.example").expect("host");
        let mixed =
            format!("this line has no equals sign\n[{key}]\nport=6697\nduration=60\nexpiry=70\n");
        fs::write(&path, &mixed).expect("write");
        with_sts_test_hook(StsTestHook::FailBackup, || {
            ours.store("irc.example", 6697, 86400, at(0));
        });

        ours.store("third.example", 6699, 10, at(20));

        assert_eq!(
            IniStsStore::new(path).lookup("irc.example", at(20)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 86400,
                expiry_epoch_seconds: 86400,
            }),
            "recovery lands the unwritten policy, which lookup already served over the 60s one"
        );
    }

    #[test]
    fn a_refresh_extends_a_policy_whose_write_failed() {
        let dir = TempDir::new("sts-refresh-unwritten");
        let (store, path) = store_in(&dir);
        fs::write(&path, "this line has no equals sign\n").expect("write");
        with_sts_test_hook(StsTestHook::FailBackup, || {
            store.store("irc.example", 6697, 86400, at(0));
            store.refresh("irc.example", 86400, at(1000));
        });

        assert_eq!(
            store.lookup("irc.example", at(86500)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 86400,
                expiry_epoch_seconds: 87400,
            }),
            "a refresh of the unwritten policy extends it in memory"
        );
    }

    fn entry(port: u16, duration_seconds: i64, expiry_epoch_seconds: i64) -> Option<StsCached> {
        Some(StsCached {
            port,
            duration_seconds,
            expiry_epoch_seconds,
        })
    }

    /// This store's 6697/86400 policy written over the file's 6697/3600 one. With `saved` the write
    /// lands, so the file holds 86400. Otherwise it fails and the policy stays unsaved, with the
    /// 3600s policy as its baseline. The file is writable again on return. `None` when the process
    /// can still write a read-only file (root), so the caller skips.
    #[cfg(unix)]
    fn ours_over_file_3600(dir: &TempDir, saved: bool) -> Option<(IniStsStore, PathBuf)> {
        let (ours, path) = store_in(dir);
        IniStsStore::new(path.clone()).store("irc.example", 6697, 3600, at(0));
        if !saved && !make_read_only(&path) {
            return None;
        }
        ours.store("irc.example", 6697, 86400, at(0));
        make_writable(&path);
        Some((ours, path))
    }

    /// Row: missing file. Missing says nothing, so the unsaved policy keeps answering. A refresh
    /// writes it back and the file is recreated. The saved policy answers from memory, and a
    /// refresh has no file policy to extend, so it writes nothing.
    #[cfg(unix)]
    #[test]
    fn a_missing_file_keeps_the_saved_and_unwritten_policies() {
        for saved in [true, false] {
            let dir = TempDir::new("sts-row-missing");
            let Some((ours, path)) = ours_over_file_3600(&dir, saved) else {
                return;
            };
            fs::remove_file(&path).expect("remove the file");

            assert_eq!(
                ours.lookup("irc.example", at(1)),
                entry(6697, 86400, 86400),
                "saved={saved}"
            );
            ours.refresh("irc.example", 86400, at(2));
            let after = IniStsStore::new(path).lookup("irc.example", at(3));
            let expected = if saved {
                None
            } else {
                entry(6697, 86400, 86402)
            };
            assert_eq!(
                after, expected,
                "saved={saved}: refresh writes only the unsaved one"
            );
        }
    }

    /// Row: unreadable. Unreadable says nothing, so both kinds of policy keep answering.
    #[cfg(unix)]
    #[test]
    fn an_unreadable_file_keeps_the_saved_and_unwritten_policies() {
        for saved in [true, false] {
            let dir = TempDir::new("sts-row-unreadable");
            let Some((ours, path)) = ours_over_file_3600(&dir, saved) else {
                return;
            };
            if !make_unreadable(&path) {
                return;
            }
            let answered = ours.lookup("irc.example", at(1));
            make_writable(&path);
            assert_eq!(answered, entry(6697, 86400, 86400), "saved={saved}");
        }
    }

    /// Row: malformed, with no valid group for the host. It says nothing about the host, so both
    /// kinds of policy keep answering. A refresh of the unsaved one recovers the file with it.
    #[cfg(unix)]
    #[test]
    fn a_malformed_file_keeps_the_saved_and_unwritten_policies() {
        for saved in [true, false] {
            let dir = TempDir::new("sts-row-malformed");
            let Some((ours, path)) = ours_over_file_3600(&dir, saved) else {
                return;
            };
            fs::write(&path, "this line has no equals sign\n").expect("write garbage");

            assert_eq!(
                ours.lookup("irc.example", at(1)),
                entry(6697, 86400, 86400),
                "saved={saved}"
            );
            if !saved {
                ours.refresh("irc.example", 86400, at(2));
                assert_eq!(
                    IniStsStore::new(path).lookup("irc.example", at(3)),
                    entry(6697, 86400, 86402),
                    "the refresh recovers the file with the unsaved policy"
                );
            }
        }
    }

    /// Row: invalid group. A well-formed file whose group for the host is present but is not a
    /// valid policy says nothing about the host, so it is not a clear: the unsaved policy keeps
    /// answering, and a refresh writes it over the invalid group. Finding 2 of round 5.
    #[cfg(unix)]
    #[test]
    fn an_invalid_group_is_not_a_clear_for_an_unwritten_policy() {
        for saved in [true, false] {
            let dir = TempDir::new("sts-row-invalid");
            let Some((ours, path)) = ours_over_file_3600(&dir, saved) else {
                return;
            };
            fs::write(
                &path,
                format!("[{IRC_EXAMPLE_GROUP}]\nport=0\nduration=3600\nexpiry=3600\n"),
            )
            .expect("write the invalid group");

            assert_eq!(
                ours.lookup("irc.example", at(1)),
                entry(6697, 86400, 86400),
                "saved={saved}"
            );
            if !saved {
                ours.refresh("irc.example", 86400, at(2));
                assert_eq!(
                    IniStsStore::new(path).lookup("irc.example", at(3)),
                    entry(6697, 86400, 86402),
                    "the refresh writes the unsaved policy over the invalid group"
                );
            }
        }
    }

    /// Finding 1, another client's rewrite. The file is damaged, and this store looks at it while
    /// damaged. Another client then rewrites it clean without this host. That clean view is the
    /// new baseline and the unsaved policy keeps answering. A later save by another client is then
    /// a change from that baseline, and the file answers.
    #[cfg(unix)]
    #[test]
    fn a_clean_rewrite_after_damage_keeps_an_unwritten_policy() {
        let other = other_example_group();
        for saved in [true, false] {
            let dir = TempDir::new("sts-clean-rewrite");
            let Some((ours, path)) = ours_over_file_3600(&dir, saved) else {
                return;
            };
            fs::write(&path, "this line has no equals sign\n").expect("write garbage");
            assert_eq!(
                ours.lookup("irc.example", at(1)),
                entry(6697, 86400, 86400),
                "saved={saved}: the damaged file keeps the policy"
            );

            fs::write(
                &path,
                format!("[{other}]\nport=6698\nduration=60\nexpiry=4102444800\n"),
            )
            .expect("write the clean rewrite");
            assert_eq!(
                ours.lookup("irc.example", at(2)),
                entry(6697, 86400, 86400),
                "saved={saved}: a clean view without the host keeps the policy"
            );

            IniStsStore::new(path.clone()).store("irc.example", 6698, 3600, at(3));
            assert_eq!(
                ours.lookup("irc.example", at(4)),
                entry(6698, 3600, 3603),
                "saved={saved}: a later save by another client wins"
            );
        }
    }

    /// The file is damaged, and this store never looks at it. Another client's recovery then
    /// rewrites it clean without this host. From this store's side that is a clear of a live
    /// baseline, so the unsaved policy becomes saved memory and keeps answering. Lookup never
    /// fails open.
    #[cfg(unix)]
    #[test]
    fn a_clean_rewrite_nobody_here_saw_damaged_does_not_fail_open() {
        for saved in [true, false] {
            let dir = TempDir::new("sts-unseen-damage");
            let Some((ours, path)) = ours_over_file_3600(&dir, saved) else {
                return;
            };
            fs::write(&path, "this line has no equals sign\n").expect("write garbage");
            IniStsStore::new(path).store("other.example", 6698, 60, at(2));

            assert_eq!(
                ours.lookup("irc.example", at(3)),
                entry(6697, 86400, 86400),
                "saved={saved}: the unseen rewrite must not fail open"
            );
        }
    }

    /// Row: newer policy, live, with another duration. The file's policy answers, and a refresh
    /// with this store's duration does not write the old policy back.
    #[cfg(unix)]
    #[test]
    fn a_newer_duration_another_client_saves_wins_over_an_unwritten_policy() {
        for saved in [true, false] {
            let dir = TempDir::new("sts-newer-duration");
            let Some((ours, path)) = ours_over_file_3600(&dir, saved) else {
                return;
            };
            IniStsStore::new(path.clone()).store("irc.example", 6697, 60, at(10));

            assert_eq!(
                ours.lookup("irc.example", at(11)),
                entry(6697, 60, 70),
                "saved={saved}"
            );
            ours.refresh("irc.example", 86400, at(12));
            assert_eq!(
                IniStsStore::new(path).lookup("irc.example", at(13)),
                entry(6697, 60, 70),
                "saved={saved}: a refresh must not write the 86400s policy back"
            );
        }
    }

    /// Row: newer policy, lapsed. A lapsed group another client saved is not in force, so it does
    /// not replace a live policy. The saved policy answers from memory, and the unsaved one keeps
    /// answering because a lapsed group is not a move.
    #[cfg(unix)]
    #[test]
    fn a_lapsed_newer_policy_on_disk_does_not_replace_an_unwritten_policy() {
        for saved in [true, false] {
            let dir = TempDir::new("sts-lapsed-newer");
            let Some((ours, path)) = ours_over_file_3600(&dir, saved) else {
                return;
            };
            IniStsStore::new(path).store("irc.example", 6698, 60, at(3000));

            assert_eq!(
                ours.lookup("irc.example", at(3100)),
                entry(6697, 86400, 86400),
                "saved={saved}: the lapsed 6698 group must not replace this store's policy"
            );
        }
    }

    /// Row: clear from a live baseline. A clear on disk makes an unsaved policy saved memory, like
    /// a saved one (known gap 4): both keep answering, and a refresh does not recreate the group.
    #[cfg(unix)]
    #[test]
    fn a_clear_on_disk_leaves_an_unwritten_policy_as_saved_memory() {
        for saved in [true, false] {
            let dir = TempDir::new("sts-clear-row");
            let Some((ours, path)) = ours_over_file_3600(&dir, saved) else {
                return;
            };
            IniStsStore::new(path.clone()).store("irc.example", 6697, 0, at(10));

            assert_eq!(
                ours.lookup("irc.example", at(11)),
                entry(6697, 86400, 86400),
                "saved={saved}"
            );
            ours.refresh("irc.example", 86400, at(20));
            let text = fs::read_to_string(&path).expect("the file stays");
            assert!(
                !text.contains(IRC_EXAMPLE_GROUP),
                "saved={saved}: a refresh must not recreate the cleared host: {text}"
            );
        }
    }

    /// Row: expiry-only refresh by another client. The same port and duration is not a change, so
    /// both kinds of policy keep answering, and the saved one reads the later expiry from the file.
    #[cfg(unix)]
    #[test]
    fn an_expiry_only_refresh_by_another_client_is_not_a_change() {
        for saved in [true, false] {
            let dir = TempDir::new("sts-expiry-row");
            let Some((ours, path)) = ours_over_file_3600(&dir, saved) else {
                return;
            };
            let duration = if saved { 86400 } else { 3600 };
            IniStsStore::new(path).refresh("irc.example", duration, at(50));

            let expected = if saved {
                entry(6697, 86400, 86450)
            } else {
                entry(6697, 86400, 86400)
            };
            for seconds in [70, 120] {
                assert_eq!(
                    ours.lookup("irc.example", at(seconds)),
                    expected,
                    "saved={saved} at {seconds}s"
                );
            }
        }
    }

    /// A store for another host. The unsaved policy for this host is still kept, so the write
    /// merges it into the file and marks it saved. A fresh reader then finds it there. Covers an
    /// invalid group (dropped by the write) and a malformed file (recovered by the write).
    #[cfg(unix)]
    #[test]
    fn a_store_for_another_host_keeps_an_unwritten_policy_over_damage() {
        for damage in [
            format!("[{IRC_EXAMPLE_GROUP}]\nport=0\nduration=3600\nexpiry=3600\n"),
            "this line has no equals sign\n".to_owned(),
        ] {
            let dir = TempDir::new("sts-store-other");
            let Some((ours, path)) = ours_over_file_3600(&dir, false) else {
                return;
            };
            fs::write(&path, damage).expect("write the damage");

            ours.store("other.example", 6699, 60, at(2));

            assert_eq!(
                IniStsStore::new(path.clone()).lookup("irc.example", at(3)),
                entry(6697, 86400, 86400),
                "the write for another host lands this store's unwritten policy"
            );
            assert_eq!(
                IniStsStore::new(path).lookup("other.example", at(3)),
                entry(6699, 60, 62),
                "the other host is written too"
            );
        }
    }

    /// A clear of a host the file does not hold writes nothing, so it must not mark an unwritten
    /// policy for another host as saved. The policy still answers after the file's own policy
    /// lapses, and the next write for another host still carries it. The clear runs on the wall
    /// clock, so the policies are live now.
    #[cfg(unix)]
    #[test]
    fn a_clear_of_an_absent_host_keeps_an_unwritten_policy_unsaved() {
        let base = Utc::now().timestamp();
        let dir = TempDir::new("sts-clear-absent");
        let (ours, path) = store_in(&dir);
        IniStsStore::new(path.clone()).store("irc.example", 6697, 3600, at(base));
        if !make_read_only(&path) {
            return;
        }
        ours.store("irc.example", 6697, 86400, at(base));
        make_writable(&path);

        ours.clear("absent.example");

        let held = entry(6697, 86400, base + 86400);
        assert_eq!(ours.lookup("irc.example", at(base + 3700)), held);
        ours.store("other.example", 6699, 60, at(base + 3701));
        assert_eq!(
            IniStsStore::new(path).lookup("irc.example", at(base + 3702)),
            held,
            "the next write for another host still carries the unwritten policy"
        );
    }

    /// Another client saves a newer port for the host after this store's write failed. A store for
    /// another host then must not write this store's yielded 6697 policy back over that save.
    #[cfg(unix)]
    #[test]
    fn a_write_for_another_host_does_not_write_back_a_yielded_policy() {
        let dir = TempDir::new("sts-no-write-back");
        let Some((ours, path)) = ours_over_file_3600(&dir, false) else {
            return;
        };
        IniStsStore::new(path.clone()).store("irc.example", 6698, 3600, at(10));

        ours.store("other.example", 6699, 60, at(20));

        assert_eq!(
            IniStsStore::new(path.clone()).lookup("irc.example", at(21)),
            entry(6698, 3600, 3610),
            "a store for another host must not write the yielded 6697 policy back"
        );
        assert_eq!(
            IniStsStore::new(path).lookup("other.example", at(21)),
            entry(6699, 60, 80),
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_junk_group_is_dropped_by_the_next_write() {
        use std::os::unix::fs::PermissionsExt;
        let other = other_example_group();
        let junk = [
            (
                "a name sts_group_key never returns",
                "[deadbeef]\nport=1\nduration=60\nexpiry=4102444800\n".to_owned(),
            ),
            ("no expiry", format!("[{other}]\nport=6698\nduration=60\n")),
            (
                "no duration",
                format!("[{other}]\nport=6698\nexpiry=4102444800\n"),
            ),
            (
                "port out of range",
                format!("[{other}]\nport=70000\nduration=60\nexpiry=4102444800\n"),
            ),
        ];
        for (name, group) in junk {
            let dir = TempDir::new("sts-junk");
            let (store, path) = store_in(&dir);
            store.store("irc.example", 6697, 60, at(1_700_000_000));
            let mut text = fs::read_to_string(&path).expect("seeded");
            text.push_str(&group);
            fs::write(&path, text).expect("append junk");
            assert!(
                store.lookup("irc.example", at(1_700_000_000)).is_some(),
                "{name}: the real policy loads beside junk"
            );

            store.store("irc.example", 6697, 60, at(1_700_000_000));

            let text = fs::read_to_string(&path).expect("file stays");
            assert!(
                !text.contains("deadbeef") && !text.contains(&other),
                "{name}: junk survived the write: {text}"
            );
            assert!(text.contains(IRC_EXAMPLE_GROUP), "{name}: {text}");
            let mode = fs::metadata(&path).expect("stat").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{name}");
        }
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
    fn a_shorter_duration_replaces_an_existing_expiry() {
        let dir = TempDir::new("sts-shorter-duration-605d");
        let (store, path) = store_in(&dir);
        let mixed = format!(
            "[{IRC_EXAMPLE_GROUP}]\nport=6697\nduration=60\nexpiry=5000\nbad line\n"
        );
        fs::write(&path, mixed).expect("write");

        store.store("irc.example", 7000, 10, at(100));

        assert_eq!(
            store.lookup("irc.example", at(100)),
            Some(StsCached {
                port: 7000,
                duration_seconds: 10,
                expiry_epoch_seconds: 110,
            }),
            "a shorter advertised duration must replace the stored expiry"
        );
    }

    #[test]
    fn a_longer_duration_replaces_an_existing_expiry() {
        let dir = TempDir::new("sts-longer-duration-605d");
        let (store, path) = store_in(&dir);
        let mixed = format!(
            "[{IRC_EXAMPLE_GROUP}]\nport=6697\nduration=10\nexpiry=200\nbad line\n"
        );
        fs::write(&path, mixed).expect("write");

        store.store("irc.example", 6697, 3600, at(100));

        assert_eq!(
            store.lookup("irc.example", at(100)),
            Some(StsCached {
                port: 6697,
                duration_seconds: 3600,
                expiry_epoch_seconds: 3700,
            }),
            "a longer advertised duration must replace the stored expiry"
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
        assert!(
            !text.contains(IRC_EXAMPLE_GROUP),
            "partial A has no expiry, so the next write drops it: {text}"
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

    #[cfg(unix)]
    #[test]
    fn malformed_sts_through_symlink_backs_up_target_and_keeps_link() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new("sts-symlink-recover-3dba");
        let real_sts = dir.path().join("real").join("sts");
        fs::create_dir_all(real_sts.parent().unwrap()).unwrap();
        let corrupt = b"this line has no equals sign\n";
        fs::write(&real_sts, corrupt).unwrap();
        let cfg_link = dir.path().join("cfg");
        fs::create_dir_all(dir.path().join("real").join("cfg")).unwrap();
        symlink(dir.path().join("real").join("cfg"), &cfg_link).unwrap();
        let link = cfg_link.join("sts");
        symlink("../sts", &link).unwrap();

        let store = IniStsStore::new(link.clone());
        store.store("irc.example", 6697, 60, at(0));

        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        let backup = real_sts.with_file_name("sts.corrupt");
        assert_eq!(fs::read(&backup).unwrap(), corrupt);
        let text = fs::read_to_string(&real_sts).unwrap();
        assert!(text.contains(IRC_EXAMPLE_GROUP), "{text}");
        assert!(store.lookup("irc.example", at(1)).is_some());
    }

    #[cfg(unix)]
    #[test]
    fn malformed_sts_through_symlinked_data_directory() {
        use std::os::unix::fs::symlink;

        let dir = TempDir::new("sts-symlink-data-dir-3dba");
        let data = dir.path().join("data");
        fs::create_dir_all(&data).unwrap();
        let real_sts = data.join("sts");
        let corrupt = b"this line has no equals sign\n";
        fs::write(&real_sts, corrupt).unwrap();
        let dotfiles = dir.path().join("dotfiles");
        symlink(&data, &dotfiles).unwrap();
        fs::create_dir_all(dir.path().join("cfg")).unwrap();
        let link = dir.path().join("cfg").join("sts");
        symlink("../dotfiles/sts", &link).unwrap();

        let store = IniStsStore::new(link.clone());
        store.store("irc.example", 6697, 60, at(0));

        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        let backup = data.join("sts.corrupt");
        assert_eq!(fs::read(&backup).unwrap(), corrupt);
    }
}
