//! The STS policy cache on disk: the `sts` INI file beside the settings file, in the format
//! `IrcStsStore` writes in Qt (`src/irc/ircsts.cpp`). Each host is one group, named by its
//! `sts_group_key`, holding `port`, `duration` and `expiry` (epoch seconds). Qt and the PoC read
//! and write the same entries.
//!
//! The file goes through `Settings`, so a malformed or unreadable file is never overwritten. Each
//! write then restricts the file to its owner, as `restrictPermissions` does.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use super::settings::Settings;
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
        settings.sync();
        restrict_permissions(&self.path);
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
        settings.set_uint(&key, PORT_KEY, u64::from(port));
        settings.set_value(&key, DURATION_KEY, &duration_seconds.to_string());
        settings.set_value(&key, EXPIRY_KEY, &expiry.to_string());
        settings.sync();
        restrict_permissions(&self.path);
    }

    fn clear(&self, host: &str) {
        if let Some(key) = sts_group_key(host) {
            self.remove(&key);
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
mod tests {
    use super::*;
    use crate::util::testdir::TempDir;
    use chrono::TimeZone;
    use std::fs;

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
    fn a_malformed_file_is_left_alone() {
        let dir = TempDir::new("sts-malformed");
        let (store, path) = store_in(&dir);
        let garbage = "this line has no equals sign\n";
        fs::write(&path, garbage).expect("write the fixture");

        store.store("irc.example", 6697, 60, at(0));
        assert_eq!(store.lookup("irc.example", at(1)), None);
        assert_eq!(fs::read_to_string(&path).expect("read back"), garbage);
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
