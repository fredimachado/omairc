//! Newest server-time per target per network (`IrcPlaybackTimeStore`). Group
//! `playbackTimes/<networkId>`, key `times`.

use std::cell::RefCell;
use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::irc::casemapping::IrcCaseMapping;

use super::settings::Settings;
use super::Backing;

const PLAYBACK_TIMES_GROUP: &str = "playbackTimes";
const PLAYBACK_TIMES_KEY: &str = "times";

/// One target and its newest parseable server-time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaybackTargetTime {
    pub target: String,
    pub when: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PlaybackEntry {
    target: String,
    epoch_ms: i64,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct PlaybackJsonRow {
    ms: String,
    target: String,
}

/// Persisted playback stamps per network. Ephemeral runs keep rows in memory only.
#[derive(Clone, Debug)]
pub struct PlaybackTimeStore {
    backing: Backing,
    ephemeral: bool,
    cache: RefCell<HashMap<String, Vec<PlaybackEntry>>>,
}

impl PlaybackTimeStore {
    /// An empty, disk-backed store.
    pub fn new(backing: &Backing) -> Self {
        Self {
            backing: backing.clone(),
            ephemeral: false,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Switches between disk and an in-memory-only cache. Turning it on clears the cache.
    #[cfg(test)]
    pub fn set_ephemeral(&mut self, ephemeral: bool) {
        self.ephemeral = ephemeral;
        if ephemeral {
            self.cache.borrow_mut().clear();
        }
    }

    /// The maximum stamp across all targets, or `None` when the network holds none.
    #[cfg(test)]
    pub fn newest(&self, network_id: &str) -> Option<DateTime<Utc>> {
        let rows = self.playback_entries(network_id);
        rows.iter()
            .map(|row| row.epoch_ms)
            .max()
            .map(|ms| DateTime::from_timestamp_millis(ms).unwrap_or_default())
    }

    /// Every target/stamp pair in insertion order.
    pub fn targets(&self, network_id: &str) -> Vec<PlaybackTargetTime> {
        self.playback_entries(network_id)
            .into_iter()
            .map(|row| PlaybackTargetTime {
                target: row.target,
                when: DateTime::from_timestamp_millis(row.epoch_ms).unwrap_or_default(),
            })
            .collect()
    }

    /// The stored stamp for `target`, or `None` when unknown.
    pub fn noted(
        &self,
        network_id: &str,
        target: &str,
        mapping: IrcCaseMapping,
    ) -> Option<DateTime<Utc>> {
        if network_id.is_empty() || target.is_empty() {
            return None;
        }
        let rows = self.playback_entries(network_id);
        let index = playback_index_of_target(&rows, target, mapping)?;
        Some(DateTime::from_timestamp_millis(rows[index].epoch_ms).unwrap_or_default())
    }

    /// Records `when` for `target`, only when strictly newer than the stored one.
    pub fn note(
        &mut self,
        network_id: &str,
        target: &str,
        when: DateTime<Utc>,
        mapping: IrcCaseMapping,
    ) -> bool {
        if network_id.is_empty() || target.is_empty() || when == DateTime::<Utc>::MIN_UTC {
            return false;
        }
        let ms = when.timestamp_millis();
        let mut rows = self.playback_entries(network_id);
        if let Some(index) = playback_index_of_target(&rows, target, mapping) {
            if ms <= rows[index].epoch_ms {
                return false;
            }
            rows[index].epoch_ms = ms;
        } else {
            rows.push(PlaybackEntry {
                target: target.to_owned(),
                epoch_ms: ms,
            });
        }
        self.cache
            .borrow_mut()
            .insert(network_id.to_owned(), rows);
        self.playback_save(network_id);
        true
    }

    /// Renames `old_target` to `new_target`, merging stamps when needed.
    pub fn rekey(
        &mut self,
        network_id: &str,
        old_target: &str,
        new_target: &str,
        mapping: IrcCaseMapping,
    ) -> bool {
        if network_id.is_empty() || old_target.is_empty() || new_target.is_empty() {
            return false;
        }
        let mut rows = self.playback_entries(network_id);
        let old_index = match playback_index_of_target(&rows, old_target, mapping) {
            Some(index) => index,
            None => return false,
        };
        let ms = rows[old_index].epoch_ms;
        if mapping.equals(old_target, new_target) {
            if rows[old_index].target == new_target {
                return false;
            }
            rows[old_index].target = new_target.to_owned();
        } else {
            rows.remove(old_index);
            if let Some(existing) = playback_index_of_target(&rows, new_target, mapping) {
                if ms > rows[existing].epoch_ms {
                    rows[existing].epoch_ms = ms;
                }
            } else {
                rows.push(PlaybackEntry {
                    target: new_target.to_owned(),
                    epoch_ms: ms,
                });
            }
        }
        self.cache
            .borrow_mut()
            .insert(network_id.to_owned(), rows);
        self.playback_save(network_id);
        true
    }

    /// Drops the cache entry and removes the persisted group.
    pub fn forget(&mut self, network_id: &str) {
        if network_id.is_empty() {
            return;
        }
        self.cache.borrow_mut().remove(network_id);
        self.playback_save(network_id);
    }

    fn playback_load(&self, network_id: &str) -> Vec<PlaybackEntry> {
        if network_id.is_empty() {
            return Vec::new();
        }
        if self.ephemeral {
            return self
                .cache
                .borrow()
                .get(network_id)
                .cloned()
                .unwrap_or_default();
        }
        let settings = self.backing.settings();
        let stored = settings.value(&format!("{}/{}", PLAYBACK_TIMES_GROUP, network_id), PLAYBACK_TIMES_KEY);
        if stored.is_empty() {
            return Vec::new();
        }
        let raw_rows: Vec<serde_json::Value> = match serde_json::from_str(&stored) {
            Ok(rows) => rows,
            Err(_) => return Vec::new(),
        };
        let mut rows = Vec::new();
        for raw in raw_rows {
            let object = match raw.as_object() {
                Some(object) => object,
                None => continue,
            };
            let target = object
                .get("target")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            if target.is_empty() {
                continue;
            }
            let ms_text = object
                .get("ms")
                .and_then(|value| value.as_str())
                .unwrap_or_default();
            let ms = match ms_text.parse::<i64>() {
                Ok(ms) => ms,
                Err(_) => continue,
            };
            rows.push(PlaybackEntry {
                target: target.to_owned(),
                epoch_ms: ms,
            });
        }
        rows
    }

    fn playback_save(&self, network_id: &str) {
        if network_id.is_empty() {
            return;
        }
        let rows = self
            .cache
            .borrow()
            .get(network_id)
            .cloned()
            .unwrap_or_default();
        if self.ephemeral {
            if rows.is_empty() {
                self.cache.borrow_mut().remove(network_id);
            }
            return;
        }
        let group = format!("{}/{}", PLAYBACK_TIMES_GROUP, network_id);
        if rows.is_empty() {
            let _ = self.backing.edit(|settings: &mut Settings| {
                settings.remove_group(&group);
            });
            return;
        }
        let json_rows: Vec<PlaybackJsonRow> = rows
            .iter()
            .map(|row| PlaybackJsonRow {
                ms: row.epoch_ms.to_string(),
                target: row.target.clone(),
            })
            .collect();
        let encoded = serde_json::to_string(&json_rows).unwrap_or_default();
        let _ = self.backing.edit(|settings: &mut Settings| {
            settings.set_value(&group, PLAYBACK_TIMES_KEY, &encoded);
        });
    }

    fn playback_entries(&self, network_id: &str) -> Vec<PlaybackEntry> {
        if network_id.is_empty() {
            return Vec::new();
        }
        if let Some(found) = self.cache.borrow().get(network_id) {
            return found.clone();
        }
        let loaded = self.playback_load(network_id);
        self.cache
            .borrow_mut()
            .insert(network_id.to_owned(), loaded.clone());
        loaded
    }
}

fn playback_index_of_target(
    rows: &[PlaybackEntry],
    target: &str,
    mapping: IrcCaseMapping,
) -> Option<usize> {
    rows.iter().position(|row| mapping.equals(&row.target, target))
}

fn play_stamp_with_ok(when: Option<DateTime<Utc>>, ok: bool) -> Option<String> {
    if !ok {
        return Some("0".to_owned());
    }
    let when = when?;
    let ms = when.timestamp_millis();
    let negative = ms < 0;
    let magnitude = ms.abs();
    let seconds = magnitude / 1000;
    let fraction = format!("{:03}", magnitude % 1000);
    let text = format!("{seconds}.{fraction}");
    Some(if negative {
        format!("-{text}")
    } else {
        text
    })
}

/// Like Go `PlaybackPlayStamp(when, ok)`.
pub fn play_stamp_at(when: DateTime<Utc>, ok: bool) -> String {
    if !ok || when == DateTime::<Utc>::MIN_UTC {
        return "0".to_owned();
    }
    play_stamp_with_ok(Some(when), true).unwrap_or_else(|| "0".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    use crate::irc::casemapping::IrcCaseMappingKind;
    use crate::storage::settings::Settings;
    use crate::util::testdir::TempDir;

    fn rfc1459() -> IrcCaseMapping {
        IrcCaseMapping::new(IrcCaseMappingKind::Rfc1459)
    }

    fn disk_store(dir: &TempDir) -> PlaybackTimeStore {
        PlaybackTimeStore::new(&Backing::Disk(dir.path().join("omairc.conf")))
    }

    #[test]
    fn note_newest_targets_noted() {
        let dir = TempDir::new("playback-note");
        let mut store = disk_store(&dir);
        let mapping = rfc1459();
        let t1 = Utc.timestamp_millis_opt(1_700_000_000_123).unwrap();
        let t2 = Utc.timestamp_millis_opt(1_700_000_001_000).unwrap();

        assert!(store.newest("net-1").is_none());
        assert!(store.noted("net-1", "alice", mapping).is_none());
        assert!(store.note("net-1", "alice", t1, mapping));
        assert!(store.note("net-1", "bob", t2, mapping));
        assert_eq!(store.newest("net-1"), Some(t2));
        assert_eq!(
            store.targets("net-1"),
            vec![
                PlaybackTargetTime {
                    target: "alice".to_owned(),
                    when: t1,
                },
                PlaybackTargetTime {
                    target: "bob".to_owned(),
                    when: t2,
                },
            ]
        );
        assert_eq!(store.noted("net-1", "ALICE", mapping), Some(t1));
        assert!(store.noted("net-1", "carol", mapping).is_none());
        assert!(!store.note("", "alice", t1, mapping));
        assert!(!store.note("net-1", "", t1, mapping));
        assert!(!store.note("net-1", "alice", DateTime::<Utc>::MIN_UTC, mapping));
        assert!(store.noted("", "alice", mapping).is_none());
        assert!(store.noted("net-1", "", mapping).is_none());
    }

    #[test]
    fn note_rejects_stale() {
        let dir = TempDir::new("playback-stale");
        let mut store = disk_store(&dir);
        let mapping = rfc1459();
        let base = Utc.timestamp_millis_opt(1000).unwrap();
        assert!(store.note("net-1", "alice", base, mapping));
        assert!(!store.note("net-1", "alice", base, mapping));
        assert!(!store.note("net-1", "alice", Utc.timestamp_millis_opt(999).unwrap(), mapping));
        assert!(store.note("net-1", "alice", Utc.timestamp_millis_opt(1001).unwrap(), mapping));
        assert_eq!(
            store.noted("net-1", "alice", mapping).map(|t| t.timestamp_millis()),
            Some(1001)
        );
    }

    #[test]
    fn rekey_merges_and_rewrites() {
        let dir = TempDir::new("playback-rekey");
        let mut store = disk_store(&dir);
        let mapping = rfc1459();
        let t1 = Utc.timestamp_millis_opt(1000).unwrap();
        let t2 = Utc.timestamp_millis_opt(2000).unwrap();
        store.note("net-1", "alice", t1, mapping);
        assert!(!store.rekey("", "alice", "bob", mapping));
        assert!(!store.rekey("net-1", "", "bob", mapping));
        assert!(!store.rekey("net-1", "alice", "", mapping));
        assert!(!store.rekey("net-1", "carol", "dave", mapping));
        assert!(store.rekey("net-1", "alice", "Alice", mapping));
        assert_eq!(store.noted("net-1", "ALICE", mapping), Some(t1));
        assert!(!store.rekey("net-1", "Alice", "Alice", mapping));
        assert!(store.rekey("net-1", "Alice", "bob", mapping));
        assert_eq!(store.noted("net-1", "bob", mapping), Some(t1));
        store.note("net-1", "carol", t2, mapping);
        assert!(store.rekey("net-1", "carol", "bob", mapping));
        assert_eq!(
            store.targets("net-1"),
            vec![PlaybackTargetTime {
                target: "bob".to_owned(),
                when: t2,
            }]
        );
        store.note("net-1", "dave", t2, mapping);
        store.note("net-1", "erin", t1, mapping);
        assert!(store.rekey("net-1", "erin", "dave", mapping));
        assert_eq!(store.noted("net-1", "dave", mapping), Some(t2));
    }

    #[test]
    fn forget_clears_disk() {
        let dir = TempDir::new("playback-forget");
        let mut store = disk_store(&dir);
        let mapping = rfc1459();
        store.note("net-1", "alice", Utc.timestamp_millis_opt(1000).unwrap(), mapping);
        store.forget("net-1");
        assert!(store.newest("net-1").is_none());
        let fresh = disk_store(&dir);
        assert!(fresh.targets("net-1").is_empty());
        let settings = Backing::Disk(dir.path().join("omairc.conf")).settings();
        assert!(settings.value("playbackTimes/net-1", "times").is_empty());
    }

    #[test]
    fn persistence_and_naming() {
        let dir = TempDir::new("playback-persist");
        let mut store = disk_store(&dir);
        let mapping = rfc1459();
        let t1 = Utc.timestamp_millis_opt(1_700_000_000_123).unwrap();
        assert!(store.note("net-1", "alice", t1, mapping));
        let fresh = disk_store(&dir);
        assert_eq!(fresh.noted("net-1", "alice", mapping), Some(t1));
        let settings = Backing::Disk(dir.path().join("omairc.conf")).settings();
        let stored = settings.value("playbackTimes/net-1", "times");
        assert!(!stored.is_empty());
        let rows: Vec<PlaybackJsonRow> = serde_json::from_str(&stored).expect("json");
        assert_eq!(
            rows,
            vec![PlaybackJsonRow {
                ms: "1700000000123".to_owned(),
                target: "alice".to_owned(),
            }]
        );
        let raw = std::fs::read_to_string(dir.path().join("omairc.conf")).expect("read");
        assert!(raw.contains("[playbackTimes]"));
    }

    #[test]
    fn malformed_json_yields_empty() {
        let dir = TempDir::new("playback-bad-json");
        let path = dir.path().join("omairc.conf");
        let mut settings = Settings::open(path.clone());
        settings.set_value("playbackTimes/net-1", "times", "not json");
        settings.sync();
        let store = PlaybackTimeStore::new(&Backing::Disk(path));
        assert!(store.targets("net-1").is_empty());
    }

    #[test]
    fn skips_unparseable_rows() {
        let dir = TempDir::new("playback-skip");
        let path = dir.path().join("omairc.conf");
        let mut settings = Settings::open(path.clone());
        settings.set_value(
            "playbackTimes/net-1",
            "times",
            r#"[{"target":"alice","ms":"1000"},{"target":"","ms":"2000"},{"target":"bob","ms":"nope"},{"target":"carol","ms":3000},"string",123,{"target":"dave","ms":"4000"}]"#,
        );
        settings.sync();
        let store = PlaybackTimeStore::new(&Backing::Disk(path));
        assert_eq!(
            store.targets("net-1"),
            vec![
                PlaybackTargetTime {
                    target: "alice".to_owned(),
                    when: Utc.timestamp_millis_opt(1000).unwrap(),
                },
                PlaybackTargetTime {
                    target: "dave".to_owned(),
                    when: Utc.timestamp_millis_opt(4000).unwrap(),
                },
            ]
        );
    }

    #[test]
    fn ephemeral_never_writes() {
        let dir = TempDir::new("playback-eph");
        let path = dir.path().join("omairc.conf");
        let mut store = PlaybackTimeStore::new(&Backing::Disk(path.clone()));
        store.set_ephemeral(true);
        let mapping = rfc1459();
        let t1 = Utc.timestamp_millis_opt(1000).unwrap();
        assert!(store.note("net-1", "alice", t1, mapping));
        assert_eq!(store.noted("net-1", "alice", mapping), Some(t1));
        assert!(!path.exists());
        let fresh = PlaybackTimeStore::new(&Backing::Disk(path));
        assert!(fresh.targets("net-1").is_empty());
        store.set_ephemeral(true);
        assert!(store.targets("net-1").is_empty());
    }

    #[test]
    fn play_stamp_formats() {
        let cases = [
            (
                "milliseconds",
                Utc.timestamp_millis_opt(1_700_000_000_123).unwrap(),
                true,
                "1700000000.123",
            ),
            (
                "whole second",
                Utc.timestamp_millis_opt(1_700_000_000_000).unwrap(),
                true,
                "1700000000.000",
            ),
            (
                "single digit fraction",
                Utc.timestamp_millis_opt(1_700_000_000_007).unwrap(),
                true,
                "1700000000.007",
            ),
            (
                "epoch",
                Utc.timestamp_millis_opt(0).unwrap(),
                true,
                "0.000",
            ),
            (
                "zero value",
                DateTime::<Utc>::MIN_UTC,
                true,
                "0",
            ),
            (
                "not ok",
                Utc.timestamp_millis_opt(1_700_000_000_123).unwrap(),
                false,
                "0",
            ),
            (
                "negative fraction",
                Utc.timestamp_millis_opt(-123).unwrap(),
                true,
                "-0.123",
            ),
            (
                "negative seconds",
                Utc.timestamp_millis_opt(-1500).unwrap(),
                true,
                "-1.500",
            ),
        ];
        for (name, when, ok, want) in cases {
            let got = play_stamp_at(when, ok);
            assert_eq!(got, want, "{name}");
        }
    }
}
