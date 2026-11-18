//! Each conversation's unsent composer text, kept across a restart in `drafts.json`
//! beside `scrollplaces.ini`. Both clients read and write this one file.
//!
//! Wire format, version 1 (compact JSON, UTF-8, no trailing newline, keys sorted at
//! every level):
//! `{"drafts":{"<networkId>":{"<normalized target>":{"ms":<epoch ms>,"text":"..."}}},"version":1}`
//! `ms` is when the text last changed. An entry at least [`DRAFT_TTL_MS`] old is dropped
//! on load and pruned on every save. Empty text removes its entry. A file whose
//! `version` is newer than this client's is read as empty and never overwritten.
//! The caller normalizes the target under the network's case mapping and decides what
//! may be persisted at all (`irc::secretpolicy::allows_draft_persistence`).
//! Mirrors `tui/internal/storage/drafts.go`; keep the two byte for byte compatible.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde_json::Value;

use super::atomicwrite::write_atomic;
use super::paths::{self, DRAFTS_FILE};
use super::settings::Status;
use super::Backing;

/// Seven days, in milliseconds.
pub const DRAFT_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// The only format version this client writes.
pub const DRAFTS_VERSION: u32 = 1;
/// Longest text kept, in bytes. A longer draft is not persisted.
pub const MAX_DRAFT_BYTES: usize = 16 * 1024;
/// Most entries kept across every network. The oldest by `ms` go first.
pub const MAX_DRAFTS: usize = 200;

/// One stored draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredDraft {
    pub text: String,
    /// Wall-clock milliseconds since the epoch at which the text last changed.
    pub ms: i64,
}

/// Network id, then normalized target, then the draft.
pub type DraftMap = BTreeMap<String, BTreeMap<String, StoredDraft>>;

/// Reads and writes the drafts file.
#[derive(Debug)]
pub struct DraftFile {
    backing: Backing,
}

impl DraftFile {
    pub fn new(backing: Backing) -> Self {
        Self { backing }
    }

    pub fn for_settings(settings: &Backing) -> Self {
        Self::for_settings_with(
            settings,
            &paths::Env::from_process(),
            paths::Platform::current(),
        )
    }

    /// Like [`Self::for_settings`], with an explicit path environment (tests). The live
    /// user settings file maps to `drafts.json` under the state dir; any other settings
    /// path (a test conf) gets a sibling `drafts.json`. Ephemeral stays ephemeral.
    pub fn for_settings_with(
        settings: &Backing,
        env: &paths::Env,
        platform: paths::Platform,
    ) -> Self {
        match settings {
            Backing::Ephemeral => Self::new(Backing::Ephemeral),
            Backing::Disk(conf) => {
                let path = match paths::settings_path_for(env, platform) {
                    Some(user) if user == *conf => paths::drafts_path_for(env, platform),
                    _ => conf.with_file_name(DRAFTS_FILE),
                };
                Self::new(Backing::Disk(path))
            }
        }
    }

    pub fn is_ephemeral(&self) -> bool {
        self.backing.is_ephemeral()
    }

    /// The file path when this store is disk-backed.
    pub fn path(&self) -> Option<&Path> {
        match &self.backing {
            Backing::Disk(path) => Some(path.as_path()),
            Backing::Ephemeral => None,
        }
    }

    /// Every stored draft that has not expired. An ephemeral store, a missing, corrupt or
    /// newer-version file all read as empty.
    pub fn load(&self, now_ms: i64) -> DraftMap {
        let Backing::Disk(path) = &self.backing else {
            return DraftMap::new();
        };
        match read(path) {
            Read::Drafts(mut map) => {
                prune(&mut map, now_ms);
                map
            }
            _ => DraftMap::new(),
        }
    }

    /// Sets or removes one draft and rewrites the file. The file is re-read first, so
    /// another client's keys are kept. `text` of `None`, blank, or over
    /// [`MAX_DRAFT_BYTES`] removes the key. Nothing is written when the key's text would
    /// not change. A file from a newer version is left alone (`FormatError`).
    pub fn store(
        &self,
        network_id: &str,
        normalized: &str,
        text: Option<&str>,
        now_ms: i64,
    ) -> Status {
        let Backing::Disk(path) = &self.backing else {
            return Status::Written;
        };
        if network_id.is_empty() || normalized.is_empty() {
            return Status::Absent;
        }
        let mut map = match read(path) {
            Read::Drafts(map) => map,
            Read::Replaceable => DraftMap::new(),
            Read::Newer => return Status::FormatError,
            Read::Unreadable => return Status::AccessError,
        };
        prune(&mut map, now_ms);
        let wanted = text.filter(|t| !t.trim().is_empty() && t.len() <= MAX_DRAFT_BYTES);
        let current = map.get(network_id).and_then(|net| net.get(normalized));
        match (wanted, current) {
            (None, None) => return Status::Written,
            (Some(text), Some(have)) if have.text == text => return Status::Written,
            _ => {}
        }
        match wanted {
            Some(text) => {
                map.entry(network_id.to_owned()).or_default().insert(
                    normalized.to_owned(),
                    StoredDraft {
                        text: text.to_owned(),
                        ms: now_ms,
                    },
                );
            }
            None => {
                if let Some(net) = map.get_mut(network_id) {
                    net.remove(normalized);
                    if net.is_empty() {
                        map.remove(network_id);
                    }
                }
            }
        }
        cap(&mut map);
        match write_atomic(path, encode(&map).as_bytes()) {
            Ok(()) => Status::Written,
            Err(_) => Status::AccessError,
        }
    }
}

/// What the file held.
enum Read {
    Drafts(DraftMap),
    /// Missing or not a version-1 document: reads as empty and may be overwritten.
    Replaceable,
    /// A newer client wrote it: reads as empty and is never overwritten.
    Newer,
    Unreadable,
}

fn read(path: &Path) -> Read {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Read::Replaceable,
        Err(_) => return Read::Unreadable,
    };
    let Ok(Value::Object(root)) = serde_json::from_slice::<Value>(&bytes) else {
        return Read::Replaceable;
    };
    match root.get("version").and_then(Value::as_u64) {
        Some(version) if version > u64::from(DRAFTS_VERSION) => return Read::Newer,
        Some(version) if version == u64::from(DRAFTS_VERSION) => {}
        _ => return Read::Replaceable,
    }
    let mut map = DraftMap::new();
    let Some(Value::Object(networks)) = root.get("drafts") else {
        return Read::Drafts(map);
    };
    for (network, targets) in networks {
        let Value::Object(targets) = targets else {
            continue;
        };
        for (target, entry) in targets {
            let ms = entry.get("ms").and_then(Value::as_i64);
            let text = entry.get("text").and_then(Value::as_str);
            let (Some(ms), Some(text)) = (ms, text) else {
                continue;
            };
            if network.is_empty() || target.is_empty() || text.trim().is_empty() {
                continue;
            }
            map.entry(network.clone()).or_default().insert(
                target.clone(),
                StoredDraft {
                    text: text.to_owned(),
                    ms,
                },
            );
        }
    }
    Read::Drafts(map)
}

/// Drops every draft at least [`DRAFT_TTL_MS`] old. A stamp in the future is kept.
fn prune(map: &mut DraftMap, now_ms: i64) {
    for targets in map.values_mut() {
        targets.retain(|_, draft| now_ms.saturating_sub(draft.ms) < DRAFT_TTL_MS);
    }
    map.retain(|_, targets| !targets.is_empty());
}

/// Keeps the newest [`MAX_DRAFTS`] entries. Ties go by network then target.
fn cap(map: &mut DraftMap) {
    let total: usize = map.values().map(BTreeMap::len).sum();
    if total <= MAX_DRAFTS {
        return;
    }
    let mut order: Vec<(i64, String, String)> = map
        .iter()
        .flat_map(|(net, targets)| {
            targets
                .iter()
                .map(move |(target, draft)| (draft.ms, net.clone(), target.clone()))
        })
        .collect();
    order.sort();
    for (_, net, target) in order.into_iter().take(total - MAX_DRAFTS) {
        if let Some(targets) = map.get_mut(&net) {
            targets.remove(&target);
        }
    }
    map.retain(|_, targets| !targets.is_empty());
}

/// The version-1 bytes: compact, keys sorted (the maps are ordered), no trailing newline.
fn encode(map: &DraftMap) -> String {
    let quote = |text: &str| serde_json::to_string(text).expect("a string encodes");
    let mut out = String::from("{\"drafts\":{");
    for (index, (net, targets)) in map.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&quote(net));
        out.push_str(":{");
        for (inner, (target, draft)) in targets.iter().enumerate() {
            if inner > 0 {
                out.push(',');
            }
            out.push_str(&quote(target));
            out.push_str(&format!(":{{\"ms\":{},\"text\":{}}}", draft.ms, quote(&draft.text)));
        }
        out.push('}');
    }
    out.push_str(&format!("}},\"version\":{DRAFTS_VERSION}}}"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testdir::TempDir;
    use std::path::PathBuf;

    /// Keep identical to `tui/internal/storage/drafts_test.go`.
    const FIXTURE: &str =
        r##"{"drafts":{"libera":{"#a":{"ms":1780000000000,"text":"half typed"}}},"version":1}"##;
    const FIXTURE_MS: i64 = 1_780_000_000_000;
    const EMPTY: &str = r#"{"drafts":{},"version":1}"#;

    fn file(dir: &TempDir) -> (DraftFile, PathBuf) {
        let path = dir.path().join(DRAFTS_FILE);
        (DraftFile::new(Backing::Disk(path.clone())), path)
    }

    fn draft(text: &str, ms: i64) -> StoredDraft {
        StoredDraft {
            text: text.to_owned(),
            ms,
        }
    }

    #[test]
    fn a_draft_round_trips_and_survives_a_new_store() {
        let dir = TempDir::new("drafts-round-trip");
        let (store, path) = file(&dir);
        assert_eq!(
            store.store("libera", "#a", Some("half typed"), FIXTURE_MS),
            Status::Written
        );
        let reopened = DraftFile::new(Backing::Disk(path));
        let loaded = reopened.load(FIXTURE_MS + 1000);
        assert_eq!(loaded["libera"]["#a"], draft("half typed", FIXTURE_MS));
    }

    #[test]
    fn fixture_bytes_are_exact() {
        let dir = TempDir::new("drafts-fixture-bytes");
        let (store, path) = file(&dir);
        store.store("libera", "#a", Some("half typed"), FIXTURE_MS);
        assert_eq!(fs::read_to_string(path).unwrap(), FIXTURE);
    }

    #[test]
    fn reads_the_go_written_fixture() {
        let dir = TempDir::new("drafts-go-fixture");
        let (store, path) = file(&dir);
        fs::write(&path, FIXTURE).unwrap();
        let loaded = store.load(FIXTURE_MS + 1000);
        assert_eq!(loaded["libera"]["#a"], draft("half typed", FIXTURE_MS));
    }

    #[test]
    fn keys_are_sorted_at_every_level() {
        let dir = TempDir::new("drafts-sorted");
        let (store, path) = file(&dir);
        store.store("oftc", "#z", Some("z"), 1);
        store.store("libera", "#b", Some("b"), 2);
        store.store("libera", "#a", Some("a"), 3);
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            r##"{"drafts":{"libera":{"#a":{"ms":3,"text":"a"},"#b":{"ms":2,"text":"b"}},"oftc":{"#z":{"ms":1,"text":"z"}}},"version":1}"##
        );
    }

    #[test]
    fn text_is_escaped_and_read_back_exactly() {
        let dir = TempDir::new("drafts-escape");
        let (store, path) = file(&dir);
        let text = "say \"hi\" \\ <b>&amp;</b> tab\t \u{e9}\u{1f600}";
        store.store("libera", "#a", Some(text), 5);
        assert!(fs::read_to_string(&path).unwrap().contains("<b>&amp;</b>"));
        assert_eq!(store.load(6)["libera"]["#a"].text, text);
    }

    #[test]
    fn a_draft_older_than_seven_days_is_dropped_on_load() {
        let dir = TempDir::new("drafts-ttl");
        let (store, path) = file(&dir);
        fs::write(&path, FIXTURE).unwrap();
        assert!(store.load(FIXTURE_MS + DRAFT_TTL_MS).is_empty());
        let kept = store.load(FIXTURE_MS + DRAFT_TTL_MS - 1);
        assert_eq!(kept["libera"]["#a"].text, "half typed");
    }

    #[test]
    fn a_future_stamp_is_kept() {
        let dir = TempDir::new("drafts-future");
        let (store, path) = file(&dir);
        fs::write(&path, FIXTURE).unwrap();
        assert_eq!(store.load(FIXTURE_MS - 10_000)["libera"]["#a"].text, "half typed");
    }

    #[test]
    fn a_store_prunes_expired_entries_of_other_keys() {
        let dir = TempDir::new("drafts-prune");
        let (store, path) = file(&dir);
        fs::write(&path, FIXTURE).unwrap();
        let now = FIXTURE_MS + DRAFT_TTL_MS;
        store.store("oftc", "#b", Some("fresh"), now);
        let text = fs::read_to_string(path).unwrap();
        assert!(!text.contains("half typed"), "{text}");
        assert!(text.contains("fresh"));
    }

    #[test]
    fn whitespace_only_text_deletes_the_entry_and_empty_networks() {
        let dir = TempDir::new("drafts-delete");
        let (store, path) = file(&dir);
        store.store("libera", "#a", Some("x"), 10);
        store.store("oftc", "#b", Some("y"), 11);
        store.store("libera", "#a", Some("   "), 12);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            r##"{"drafts":{"oftc":{"#b":{"ms":11,"text":"y"}}},"version":1}"##
        );
        store.store("oftc", "#b", None, 13);
        assert_eq!(fs::read_to_string(&path).unwrap(), EMPTY);
    }

    #[test]
    fn unchanged_text_does_not_bump_ms_or_write() {
        let dir = TempDir::new("drafts-unchanged");
        let (store, path) = file(&dir);
        // A trailing newline is not what a save writes, so it proves no rewrite happened.
        let marked = format!("{FIXTURE}\n");
        fs::write(&path, &marked).unwrap();
        store.store("libera", "#a", Some("half typed"), FIXTURE_MS + 5000);
        assert_eq!(fs::read_to_string(&path).unwrap(), marked);
        store.store("libera", "#a", Some("changed"), FIXTURE_MS + 6000);
        assert_eq!(store.load(FIXTURE_MS + 7000)["libera"]["#a"].ms, FIXTURE_MS + 6000);
    }

    #[test]
    fn deleting_an_absent_key_writes_nothing() {
        let dir = TempDir::new("drafts-absent");
        let (store, path) = file(&dir);
        store.store("libera", "#a", None, 1);
        assert!(!path.exists());
    }

    #[test]
    fn changed_text_bumps_ms() {
        let dir = TempDir::new("drafts-bump");
        let (store, _) = file(&dir);
        store.store("libera", "#a", Some("one"), 100);
        store.store("libera", "#a", Some("two"), 200);
        assert_eq!(store.load(300)["libera"]["#a"], draft("two", 200));
    }

    #[test]
    fn a_newer_version_is_never_overwritten() {
        let dir = TempDir::new("drafts-newer");
        let (store, path) = file(&dir);
        let newer = r##"{"drafts":{"libera":{"#a":{"ms":1,"text":"x"}}},"version":2}"##;
        fs::write(&path, newer).unwrap();
        assert!(store.load(2).is_empty());
        assert_ne!(store.store("libera", "#b", Some("y"), 3), Status::Written);
        assert_eq!(fs::read_to_string(&path).unwrap(), newer);
    }

    #[test]
    fn a_corrupt_file_reads_empty_and_is_replaced() {
        let dir = TempDir::new("drafts-corrupt");
        let (store, path) = file(&dir);
        fs::write(&path, "{not json").unwrap();
        assert!(store.load(1).is_empty());
        assert_eq!(store.store("libera", "#a", Some("x"), 7), Status::Written);
        assert_eq!(store.load(8)["libera"]["#a"], draft("x", 7));
    }

    #[test]
    fn malformed_entries_are_skipped_and_unknown_fields_dropped_on_rewrite() {
        let dir = TempDir::new("drafts-skip");
        let (store, path) = file(&dir);
        fs::write(
            &path,
            r##"{"drafts":{"libera":{"#a":{"ms":5,"text":"keep","extra":true},"#b":{"ms":"x","text":"bad"},"#c":{"ms":5},"#d":{"ms":5,"text":"  "}},"oftc":3},"version":1}"##,
        )
        .unwrap();
        let loaded = store.load(6);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded["libera"].len(), 1);
        store.store("libera", "#e", Some("new"), 6);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            r##"{"drafts":{"libera":{"#a":{"ms":5,"text":"keep"},"#e":{"ms":6,"text":"new"}}},"version":1}"##
        );
    }

    #[test]
    fn oversize_text_is_not_written_and_clears_the_earlier_draft() {
        let dir = TempDir::new("drafts-oversize");
        let (store, path) = file(&dir);
        store.store("libera", "#a", Some("small"), 1);
        let big = "x".repeat(MAX_DRAFT_BYTES + 1);
        store.store("libera", "#a", Some(&big), 2);
        assert_eq!(fs::read_to_string(&path).unwrap(), EMPTY);
        let exact = "x".repeat(MAX_DRAFT_BYTES);
        store.store("libera", "#a", Some(&exact), 3);
        assert_eq!(store.load(4)["libera"]["#a"].text, exact);
    }

    #[test]
    fn the_oldest_entries_go_past_the_cap() {
        let dir = TempDir::new("drafts-cap");
        let (store, _) = file(&dir);
        for index in 0..(MAX_DRAFTS as i64 + 3) {
            store.store("libera", &format!("#c{index:03}"), Some("t"), 1000 + index);
        }
        let loaded = store.load(5000);
        assert_eq!(loaded["libera"].len(), MAX_DRAFTS);
        assert!(!loaded["libera"].contains_key("#c000"));
        assert!(!loaded["libera"].contains_key("#c002"));
        assert!(loaded["libera"].contains_key("#c003"));
        assert!(loaded["libera"].contains_key(&format!("#c{:03}", MAX_DRAFTS + 2)));
    }

    #[test]
    fn ephemeral_writes_nothing_and_loads_empty() {
        let store = DraftFile::new(Backing::Ephemeral);
        assert_eq!(store.store("libera", "#a", Some("x"), 1), Status::Written);
        assert!(store.load(2).is_empty());
        assert!(store.is_ephemeral());
        assert!(store.path().is_none());
    }

    #[test]
    fn the_state_file_follows_the_user_settings_and_sits_beside_a_test_conf() {
        let env = paths::Env {
            xdg_state_home: Some(PathBuf::from("st")),
            xdg_config_home: Some(PathBuf::from("cfg")),
            temp_dir: PathBuf::from("tmp"),
            ..paths::Env::default()
        };
        let platform = paths::Platform::Linux;
        let user = paths::settings_path_for(&env, platform).unwrap();
        let live = DraftFile::for_settings_with(&Backing::Disk(user), &env, platform);
        assert_eq!(
            live.path().unwrap(),
            paths::drafts_path_for(&env, platform).as_path()
        );
        let conf = PathBuf::from("somewhere").join("omairc.conf");
        let beside = DraftFile::for_settings_with(&Backing::Disk(conf), &env, platform);
        assert_eq!(
            beside.path().unwrap(),
            PathBuf::from("somewhere").join(DRAFTS_FILE).as_path()
        );
        let none = DraftFile::for_settings_with(&Backing::Ephemeral, &env, platform);
        assert!(none.is_ephemeral());
    }
}
