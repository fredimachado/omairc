//! Where each conversation's transcript was last left, stored per network as one
//! JSON array in the `scrollPlaces/<network>` group, under the `places` key.
//! A place is either "follow the end" or an anchor on the first visible message. The
//! anchor is the message's msgid when the server gave one, else its author, kind, body
//! and server time (`ms`). The JSON keys are sorted, as a `QJsonObject` writes them, so
//! the two clients read each other's files.
//!
//! Places live in `<state>/scrollplaces.ini` (next to `window.ini`). When that file is
//! missing, a one-time read falls back to the legacy `[scrollPlaces]` group in the
//! settings file. Saves never write to or strip the conf.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::identity::{SETTINGS_FILE, SETTINGS_FILE_MACOS};

use super::atomicwrite::write_atomic;
use super::paths::{self, SCROLL_PLACES_FILE};
use super::settings::{probe_ini, IniProbe, Settings, Status};
use super::Backing;

/// The QSettings group that holds every network's places.
pub const SCROLL_PLACES_GROUP: &str = "scrollPlaces";
/// The key under each network that holds its JSON array.
pub const PLACES_KEY: &str = "places";

/// Where one conversation's transcript was left (`ScrollPlace`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScrollPlace {
    /// The viewport was pinned to the bottom.
    pub follow_end: bool,
    pub msg_id: String,
    pub author: String,
    pub kind: String,
    pub body: String,
    pub epoch_ms: i64,
    pub has_time: bool,
    /// The time the line showed on screen, when the anchor has no epoch time. The client's
    /// transcript keeps the time as text. Qt ignores this key.
    pub shown_time: String,
}

/// The places of one network, keyed by target in the order they were first saved.
pub type PlaceRows = Vec<(String, ScrollPlace)>;

/// Reads and writes the scroll places of every network.
#[derive(Debug)]
pub struct ScrollPlaceFile {
    /// Where places are written (`scrollplaces.ini`) or ephemeral.
    backing: Backing,
    /// Legacy `omairc.conf` path for a one-time read when the state file is missing.
    conf_path: Option<PathBuf>,
}

impl ScrollPlaceFile {
    /// An ephemeral store, or a disk store over `backing` with no conf fallback.
    /// Prefer [`Self::for_settings`] for the live controller path.
    pub fn new(backing: Backing) -> Self {
        Self {
            backing,
            conf_path: None,
        }
    }

    /// Builds the store for a settings `Backing`. Ephemeral stays ephemeral. A disk
    /// settings path writes to `scrollplaces.ini` under the state dir when it is the
    /// live user settings file; otherwise (tests) the state file sits beside the
    /// settings file. The settings path is kept only for the legacy conf fallback.
    pub fn for_settings(settings: &Backing) -> Self {
        Self::for_settings_with(
            settings,
            &paths::Env::from_process(),
            paths::Platform::current(),
        )
    }

    /// Like [`Self::for_settings`], with an explicit path environment (tests).
    pub fn for_settings_with(
        settings: &Backing,
        env: &paths::Env,
        platform: paths::Platform,
    ) -> Self {
        match settings {
            Backing::Ephemeral => Self::new(Backing::Ephemeral),
            Backing::Disk(conf) => {
                let state = scroll_state_path_for_settings(conf, env, platform);
                Self {
                    backing: Backing::Disk(state),
                    conf_path: Some(conf.clone()),
                }
            }
        }
    }

    /// Explicit state file and optional legacy conf path (tests and helpers).
    pub fn with_paths(state_path: PathBuf, conf_path: Option<PathBuf>) -> Self {
        Self {
            backing: Backing::Disk(state_path),
            conf_path,
        }
    }

    /// Whether this store writes nothing between runs.
    pub fn is_ephemeral(&self) -> bool {
        self.backing.is_ephemeral()
    }

    /// The state-file path when this store is disk-backed.
    pub fn state_path(&self) -> Option<&Path> {
        match &self.backing {
            Backing::Disk(path) => Some(path.as_path()),
            Backing::Ephemeral => None,
        }
    }

    /// The legacy conf path kept for a one-time fallback read.
    pub fn conf_path(&self) -> Option<&Path> {
        self.conf_path.as_deref()
    }

    /// Every network's places, keyed by network id. Entries that do not parse are skipped.
    /// Prefers `scrollplaces.ini` when it exists; otherwise reads the legacy conf groups
    /// without writing or stripping them. On macOS both `omairc.ini` and `omairc.conf`
    /// are merged (`ini` wins per network).
    pub fn load_all(&self) -> BTreeMap<String, PlaceRows> {
        if self.backing.is_ephemeral() {
            return BTreeMap::new();
        }
        if let Backing::Disk(state) = &self.backing {
            if state.exists() {
                return load_all_from(&Settings::open(state));
            }
        }
        load_all_from_legacy(self.conf_path.as_deref())
    }

    /// Replaces one network's places (`save`). An empty list removes the network's group.
    /// When the state file is missing, every legacy `scrollPlaces/*` group from the conf
    /// is copied into the same write first, so other networks are not lost. The conf is
    /// never touched. If a legacy conf exists but cannot be read, the save is refused and
    /// no state file is created. When the seed copied anything and the write ends empty,
    /// the empty state file is still created so a later read does not fall back to conf.
    pub fn save(&self, network_id: &str, rows: &PlaceRows) -> Status {
        if network_id.is_empty() {
            return Status::Absent;
        }
        if self.backing.is_ephemeral() {
            return Status::Written;
        }
        let Backing::Disk(state) = &self.backing else {
            return Status::Written;
        };
        let missing = !state.exists();
        let mut settings = Settings::open(state);
        let mut seeded = false;
        if missing {
            match seed_legacy_into(&mut settings, self.conf_path.as_deref()) {
                SeedOutcome::Unreadable => return Status::AccessError,
                SeedOutcome::Copied => seeded = true,
                SeedOutcome::Empty => {}
            }
        }
        apply_network(&mut settings, network_id, rows);
        let status = settings.sync();
        if status != Status::Written {
            return status;
        }
        // Empty encode + missing file matches disk, so sync skips the create. An empty
        // state file still records that migration happened once anything was seeded.
        if missing && seeded && !state.exists() {
            return match write_atomic(state, b"") {
                Ok(()) => Status::Written,
                Err(_) => Status::AccessError,
            };
        }
        status
    }
}

/// State path for a settings file: the live user path uses the state dir; a
/// test/temp conf gets a sibling `scrollplaces.ini`.
fn scroll_state_path_for_settings(
    conf: &Path,
    env: &paths::Env,
    platform: paths::Platform,
) -> PathBuf {
    match paths::settings_path_for(env, platform) {
        Some(user) if user == conf => paths::scroll_places_path_for(env, platform),
        _ => conf.with_file_name(SCROLL_PLACES_FILE),
    }
}

/// Result of copying legacy conf groups into a new state file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SeedOutcome {
    /// No legacy conf existed, or none held places.
    Empty,
    /// At least one places group was copied.
    Copied,
    /// A legacy conf path exists but cannot be read; refuse the save.
    Unreadable,
}

/// Legacy conf paths beside the settings file: `omairc.ini` first (wins per network),
/// then `omairc.conf` fills networks that are not already present. Both clients share
/// one state file on macOS but historically wrote different conf names.
fn legacy_scroll_conf_paths(conf: Option<&Path>) -> Vec<PathBuf> {
    let Some(conf) = conf else {
        return Vec::new();
    };
    let Some(dir) = conf.parent() else {
        return vec![conf.to_path_buf()];
    };
    vec![dir.join(SETTINGS_FILE_MACOS), dir.join(SETTINGS_FILE)]
}

/// Copies every legacy `scrollPlaces/*` group from the macOS/desktop and TUI conf
/// files into `dest`. `omairc.ini` wins per network; `omairc.conf` fills the rest.
/// The confs are only read. Used when creating `scrollplaces.ini` for the first time.
fn seed_legacy_into(dest: &mut Settings, conf: Option<&Path>) -> SeedOutcome {
    let mut copied = false;
    for path in legacy_scroll_conf_paths(conf) {
        match probe_ini(&path) {
            IniProbe::Ok if !path.exists() => continue,
            IniProbe::Unreadable => return SeedOutcome::Unreadable,
            IniProbe::Malformed | IniProbe::Ok => {}
        }
        let legacy = Settings::open(&path);
        for network_id in legacy.child_groups(SCROLL_PLACES_GROUP) {
            let group = format!("{SCROLL_PLACES_GROUP}/{network_id}");
            if !legacy.contains(&group, PLACES_KEY) {
                continue;
            }
            if dest.contains(&group, PLACES_KEY) {
                continue;
            }
            dest.set_value(&group, PLACES_KEY, &legacy.value(&group, PLACES_KEY));
            copied = true;
        }
    }
    if copied {
        SeedOutcome::Copied
    } else {
        SeedOutcome::Empty
    }
}

/// Legacy places from both conf names, `omairc.ini` winning per network.
fn load_all_from_legacy(conf: Option<&Path>) -> BTreeMap<String, PlaceRows> {
    let mut out = BTreeMap::new();
    for path in legacy_scroll_conf_paths(conf) {
        if !path.exists() || probe_ini(&path) == IniProbe::Unreadable {
            continue;
        }
        for (network_id, rows) in load_all_from(&Settings::open(&path)) {
            out.entry(network_id).or_insert(rows);
        }
    }
    out
}

fn apply_network(settings: &mut Settings, network_id: &str, rows: &PlaceRows) {
    let group = format!("{SCROLL_PLACES_GROUP}/{network_id}");
    settings.remove_group(&group);
    if !rows.is_empty() {
        let array: Vec<Value> = rows
            .iter()
            .map(|(target, place)| place_json(target, place))
            .collect();
        settings.set_value(&group, PLACES_KEY, &Value::Array(array).to_string());
    }
}

/// Every network's places in `settings`. Used by the Qt migration probe, which still
/// finds places in a Qt install's conf.
pub fn load_all_from(settings: &Settings) -> BTreeMap<String, PlaceRows> {
    settings
        .child_groups(SCROLL_PLACES_GROUP)
        .into_iter()
        .map(|network_id| {
            let rows = read_rows(settings, &network_id);
            (network_id, rows)
        })
        .collect()
}

fn read_rows(settings: &Settings, network_id: &str) -> PlaceRows {
    let group = format!("{SCROLL_PLACES_GROUP}/{network_id}");
    let stored = settings.value(&group, PLACES_KEY);
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(&stored) else {
        return Vec::new();
    };
    items.iter().filter_map(place_from_json).collect()
}

/// One place as its JSON object. The keys are sorted; a follow place carries only its
/// target, and an anchor carries its msgid, or its author, body, kind and optional time.
fn place_json(target: &str, place: &ScrollPlace) -> Value {
    if place.follow_end {
        return json!({ "follow": true, "target": target });
    }
    if !place.msg_id.is_empty() {
        return json!({ "follow": false, "msgid": place.msg_id, "target": target });
    }
    // Inserted in key order, so the text is sorted whichever map type serde_json was built with.
    let mut fields: BTreeMap<&str, Value> = BTreeMap::new();
    fields.insert("author", Value::String(place.author.clone()));
    fields.insert("body", Value::String(place.body.clone()));
    fields.insert("follow", Value::Bool(false));
    fields.insert("kind", Value::String(place.kind.clone()));
    if place.has_time {
        fields.insert("ms", Value::String(place.epoch_ms.to_string()));
    }
    fields.insert("target", Value::String(target.to_owned()));
    if !place.shown_time.is_empty() {
        fields.insert("time", Value::String(place.shown_time.clone()));
    }
    let mut object = Map::new();
    for (key, value) in fields {
        object.insert(key.to_owned(), value);
    }
    Value::Object(object)
}

/// Parses one place. An object without a target or a `follow` flag is skipped, and so is
/// a time that is not a whole number, as the Qt reader does.
fn place_from_json(value: &Value) -> Option<(String, ScrollPlace)> {
    let object = value.as_object()?;
    let target = object.get("target")?.as_str()?;
    if target.is_empty() {
        return None;
    }
    let follow_end = object.get("follow")?.as_bool()?;
    let mut place = ScrollPlace {
        follow_end,
        ..ScrollPlace::default()
    };
    if !follow_end {
        place.msg_id = text_of(object, "msgid");
        if place.msg_id.is_empty() {
            place.author = text_of(object, "author");
            place.body = text_of(object, "body");
            place.kind = text_of(object, "kind");
            let time = text_of(object, "ms");
            if !time.is_empty() {
                place.epoch_ms = time.parse().ok()?;
                place.has_time = true;
            }
            place.shown_time = text_of(object, "time");
        }
    }
    Some((target.to_owned(), place))
}

fn text_of(object: &Map<String, Value>, key: &str) -> String {
    object
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::paths::{self, Env, Platform, SCROLL_PLACES_FILE};
    use crate::util::testdir::TempDir;
    use std::fs;
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;

    fn anchor(author: &str, body: &str) -> ScrollPlace {
        ScrollPlace {
            author: author.to_owned(),
            body: body.to_owned(),
            kind: "message".to_owned(),
            epoch_ms: 1_789_000_000_000,
            has_time: true,
            ..ScrollPlace::default()
        }
    }

    fn agent_dir(label: &str) -> TempDir {
        TempDir::new(&format!("{label}-1d06"))
    }

    fn sleep_for_mtime() {
        thread::sleep(Duration::from_millis(1100));
    }

    #[test]
    fn places_round_trip_in_their_saved_order() {
        let dir = agent_dir("scroll");
        let state = dir.path().join(SCROLL_PLACES_FILE);
        let file = ScrollPlaceFile::with_paths(state, None);
        let rows: PlaceRows = vec![
            (
                "#omarchy".to_owned(),
                ScrollPlace {
                    follow_end: true,
                    ..ScrollPlace::default()
                },
            ),
            ("anna".to_owned(), anchor("anna", "see you")),
            (
                "#lab".to_owned(),
                ScrollPlace {
                    msg_id: "m-42".to_owned(),
                    ..ScrollPlace::default()
                },
            ),
        ];
        assert_eq!(file.save("net", &rows), Status::Written);
        assert_eq!(file.load_all()["net"], rows);
    }

    #[test]
    fn the_json_keys_are_sorted_and_the_time_is_a_string() {
        let value = place_json("anna", &anchor("anna", "hi"));
        let text = value.to_string();
        let author = text.find("\"author\"").expect("author");
        let body = text.find("\"body\"").expect("body");
        let follow = text.find("\"follow\"").expect("follow");
        assert!(author < body && body < follow, "{text}");
        assert_eq!(value["ms"], "1789000000000");
    }

    #[test]
    fn a_time_that_is_not_a_number_skips_the_place() {
        let bad = json!({ "follow": false, "target": "anna", "author": "a", "body": "b", "kind": "k", "ms": "soon" });
        assert!(place_from_json(&bad).is_none());
        let no_follow = json!({ "target": "anna" });
        assert!(place_from_json(&no_follow).is_none());
    }

    #[test]
    fn an_empty_list_removes_the_network_group() {
        let dir = agent_dir("scroll-empty");
        let state = dir.path().join(SCROLL_PLACES_FILE);
        let file = ScrollPlaceFile::with_paths(state.clone(), None);
        file.save("net", &vec![("anna".to_owned(), anchor("anna", "x"))]);
        file.save("net", &Vec::new());
        assert!(file.load_all().is_empty());
        let text = fs::read_to_string(&state).unwrap_or_default();
        assert!(!text.contains("anna"), "{text}");
    }

    #[test]
    fn an_ephemeral_store_keeps_nothing() {
        let file = ScrollPlaceFile::new(Backing::Ephemeral);
        file.save("net", &vec![("anna".to_owned(), anchor("anna", "x"))]);
        assert!(file.load_all().is_empty());
        assert!(file.is_ephemeral());
    }

    #[test]
    fn save_goes_to_state_file_and_leaves_conf_bytes_alone() {
        let dir = agent_dir("scroll-conf-untouched");
        let conf = dir.path().join("omairc.conf");
        let state = dir.path().join(SCROLL_PLACES_FILE);
        fs::write(
            &conf,
            "[preferences]\ndirects=true\n[scrollPlaces]\nnet1\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#qa\\\"}]\"\n",
        )
        .unwrap();
        sleep_for_mtime();
        let before = fs::read(&conf).unwrap();
        let before_mtime = fs::metadata(&conf).unwrap().modified().unwrap();

        let file = ScrollPlaceFile::with_paths(state.clone(), Some(conf.clone()));
        assert_eq!(
            file.save(
                "net1",
                &vec![(
                    "#qa".to_owned(),
                    ScrollPlace {
                        follow_end: false,
                        msg_id: "m-1".to_owned(),
                        ..ScrollPlace::default()
                    }
                )]
            ),
            Status::Written
        );
        assert_eq!(
            fs::read(&conf).unwrap(),
            before,
            "conf bytes must not change"
        );
        assert_eq!(
            fs::metadata(&conf).unwrap().modified().unwrap(),
            before_mtime,
            "conf mtime must not change"
        );
        assert!(state.exists(), "state file must be created");
        let state_text = fs::read_to_string(&state).unwrap();
        assert!(state_text.contains("m-1"), "{state_text}");
        assert!(
            !fs::read_to_string(&conf).unwrap().contains("m-1"),
            "save must not write the new place into omairc.conf"
        );
    }

    #[test]
    fn legacy_conf_is_read_once_left_alone_and_state_file_is_created() {
        let dir = agent_dir("scroll-legacy-fallback");
        let conf = dir.path().join("omairc.conf");
        let state = dir.path().join(SCROLL_PLACES_FILE);
        fs::write(
            &conf,
            "[scrollPlaces]\nnet1\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#qa\\\"}]\"\n",
        )
        .unwrap();
        let conf_bytes = fs::read(&conf).unwrap();

        let file = ScrollPlaceFile::with_paths(state.clone(), Some(conf.clone()));
        assert!(!state.exists());
        let loaded = file.load_all();
        assert_eq!(
            loaded.get("net1").map(|rows| rows.as_slice()),
            Some(
                [(
                    "#qa".to_owned(),
                    ScrollPlace {
                        follow_end: true,
                        ..ScrollPlace::default()
                    }
                )]
                .as_slice()
            )
        );
        assert_eq!(fs::read(&conf).unwrap(), conf_bytes);
        assert!(
            !state.exists(),
            "a legacy read must not create the state file"
        );

        assert_eq!(
            file.save(
                "net1",
                &vec![(
                    "#qa".to_owned(),
                    ScrollPlace {
                        follow_end: true,
                        ..ScrollPlace::default()
                    }
                )]
            ),
            Status::Written
        );
        assert_eq!(fs::read(&conf).unwrap(), conf_bytes, "legacy lines stay");
        assert!(state.exists());
        assert_eq!(
            ScrollPlaceFile::with_paths(state.clone(), Some(conf.clone())).load_all()["net1"][0].0,
            "#qa"
        );
    }

    #[test]
    fn state_file_wins_over_legacy_conf() {
        let dir = agent_dir("scroll-state-wins");
        let conf = dir.path().join("omairc.conf");
        let state = dir.path().join(SCROLL_PLACES_FILE);
        fs::write(
            &conf,
            "[scrollPlaces]\nnet1\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#old\\\"}]\"\n",
        )
        .unwrap();
        let file = ScrollPlaceFile::with_paths(state, Some(conf));
        file.save(
            "net1",
            &vec![(
                "#new".to_owned(),
                ScrollPlace {
                    follow_end: true,
                    ..ScrollPlace::default()
                },
            )],
        );
        let loaded = file.load_all();
        assert_eq!(loaded["net1"][0].0, "#new");
    }

    #[test]
    fn demo_ephemeral_writes_neither_conf_nor_state() {
        let dir = agent_dir("scroll-demo");
        let conf = dir.path().join("omairc.conf");
        let state = dir.path().join(SCROLL_PLACES_FILE);
        fs::write(&conf, "[preferences]\ndirects=true\n").unwrap();
        let before = fs::read(&conf).unwrap();

        let file = ScrollPlaceFile::for_settings(&Backing::Ephemeral);
        file.save(
            "net1",
            &vec![(
                "#qa".to_owned(),
                ScrollPlace {
                    follow_end: true,
                    ..ScrollPlace::default()
                },
            )],
        );
        assert!(file.load_all().is_empty());
        assert_eq!(fs::read(&conf).unwrap(), before);
        assert!(!state.exists());
        assert!(fs::read_dir(dir.path())
            .unwrap()
            .all(|entry| { entry.unwrap().file_name() != *SCROLL_PLACES_FILE }));
    }

    #[test]
    fn for_settings_disk_uses_sibling_state_file_in_temp_dirs() {
        let dir = agent_dir("scroll-for-settings");
        let conf = dir.path().join("omairc.conf");
        let file = ScrollPlaceFile::for_settings(&Backing::Disk(conf.clone()));
        assert_eq!(
            file.state_path(),
            Some(dir.path().join(SCROLL_PLACES_FILE).as_path())
        );
        file.save(
            "net",
            &vec![(
                "#omarchy".to_owned(),
                ScrollPlace {
                    follow_end: true,
                    ..ScrollPlace::default()
                },
            )],
        );
        assert!(dir.path().join(SCROLL_PLACES_FILE).exists());
        assert!(
            !conf.exists()
                || !fs::read_to_string(&conf)
                    .unwrap_or_default()
                    .contains("scrollPlaces")
        );
    }

    #[test]
    fn scrollplaces_ini_honours_xdg_state_home_and_dev_home() {
        let home = PathBuf::from("home-1d06");
        let xdg = agent_dir("scroll-xdg");
        let env = Env {
            xdg_state_home: Some(xdg.path().to_path_buf()),
            home: Some(home.clone()),
            temp_dir: PathBuf::from("tmp"),
            ..Env::default()
        };
        let state = paths::scroll_places_path_for(&env, Platform::Linux);
        let file = ScrollPlaceFile::with_paths(state.clone(), None);
        assert_eq!(
            file.save(
                "net",
                &vec![(
                    "#qa".to_owned(),
                    ScrollPlace {
                        follow_end: true,
                        ..ScrollPlace::default()
                    }
                )]
            ),
            Status::Written
        );
        assert!(state.starts_with(xdg.path()));
        assert_eq!(file.load_all()["net"][0].0, "#qa");

        let dev = agent_dir("scroll-dev");
        let dev_env = Env {
            dev_home: Some(dev.path().to_path_buf()),
            xdg_state_home: Some(PathBuf::from("ignored")),
            home: Some(home),
            temp_dir: PathBuf::from("tmp"),
            ..Env::default()
        };
        let dev_state = paths::scroll_places_path_for(&dev_env, Platform::Linux);
        assert_eq!(dev_state, dev.path().join("state").join(SCROLL_PLACES_FILE));
    }

    #[test]
    fn cross_client_fixture_round_trips_the_shared_ini_shape() {
        let dir = agent_dir("scroll-cross-client");
        let state = dir.path().join(SCROLL_PLACES_FILE);
        // Bytes both clients must accept: Qt/TUI sorted JSON inside a QSettings group.
        fs::write(
            &state,
            "[scrollPlaces]\nnet1\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#qa\\\"},{\\\"follow\\\":false,\\\"msgid\\\":\\\"m-9\\\",\\\"target\\\":\\\"anna\\\"}]\"\n",
        )
        .unwrap();
        let file = ScrollPlaceFile::with_paths(state.clone(), None);
        let rows = &file.load_all()["net1"];
        assert_eq!(rows.len(), 2);
        assert!(rows[0].1.follow_end && rows[0].0 == "#qa");
        assert_eq!(rows[1].1.msg_id, "m-9");
        assert_eq!(rows[1].0, "anna");

        file.save("net1", rows);
        let again = ScrollPlaceFile::with_paths(state, None).load_all();
        assert_eq!(again["net1"], *rows);
    }

    #[test]
    fn save_must_target_scrollplaces_ini_not_conf() {
        let dir = agent_dir("scroll-mutation-guard");
        let conf = dir.path().join("omairc.conf");
        let state = dir.path().join(SCROLL_PLACES_FILE);
        fs::write(&conf, "[preferences]\nkeep=yes\n").unwrap();
        sleep_for_mtime();
        let before = fs::read(&conf).unwrap();

        let file = ScrollPlaceFile::with_paths(state.clone(), Some(conf.clone()));
        assert_eq!(
            file.save(
                "net1",
                &vec![(
                    "#qa".to_owned(),
                    ScrollPlace {
                        follow_end: true,
                        ..ScrollPlace::default()
                    }
                )]
            ),
            Status::Written
        );
        assert_eq!(fs::read(&conf).unwrap(), before);
        assert!(state.exists());
        assert!(
            !fs::read_to_string(&conf)
                .unwrap()
                .contains("[scrollPlaces]"),
            "scroll place save must not add a [scrollPlaces] group to omairc.conf"
        );
    }

    fn two_network_legacy_conf(conf: &Path) {
        fs::write(
            conf,
            "[scrollPlaces]\n\
             net1\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#qa\\\"},{\\\"follow\\\":true,\\\"target\\\":\\\"#qb\\\"}]\"\n\
             net2\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#dev\\\"}]\"\n",
        )
        .unwrap();
    }

    #[test]
    fn first_save_keeps_other_networks_from_legacy_conf() {
        let dir = agent_dir("scroll-two-net-save");
        let conf = dir.path().join("omairc.conf");
        let state = dir.path().join(SCROLL_PLACES_FILE);
        two_network_legacy_conf(&conf);
        let conf_bytes = fs::read(&conf).unwrap();

        let file = ScrollPlaceFile::with_paths(state.clone(), Some(conf.clone()));
        assert_eq!(
            file.save(
                "net1",
                &vec![
                    (
                        "#qa".to_owned(),
                        ScrollPlace {
                            follow_end: false,
                            msg_id: "m-qa".to_owned(),
                            ..ScrollPlace::default()
                        },
                    ),
                    (
                        "#qb".to_owned(),
                        ScrollPlace {
                            follow_end: true,
                            ..ScrollPlace::default()
                        },
                    ),
                ]
            ),
            Status::Written
        );
        assert_eq!(fs::read(&conf).unwrap(), conf_bytes);

        let relaunched = ScrollPlaceFile::with_paths(state, Some(conf)).load_all();
        assert_eq!(relaunched["net1"][0].1.msg_id, "m-qa");
        assert!(
            relaunched.get("net2").is_some_and(|rows| rows
                == &[(
                    "#dev".to_owned(),
                    ScrollPlace {
                        follow_end: true,
                        ..ScrollPlace::default()
                    }
                )]),
            "net2#dev must survive the first save: {relaunched:?}"
        );
    }

    #[test]
    fn forget_seeds_legacy_networks_before_removing_one() {
        let dir = agent_dir("scroll-two-net-forget");
        let conf = dir.path().join("omairc.conf");
        let state = dir.path().join(SCROLL_PLACES_FILE);
        two_network_legacy_conf(&conf);
        let conf_bytes = fs::read(&conf).unwrap();

        let file = ScrollPlaceFile::with_paths(state.clone(), Some(conf.clone()));
        assert!(!state.exists());
        assert_eq!(file.save("net1", &PlaceRows::new()), Status::Written);
        assert_eq!(fs::read(&conf).unwrap(), conf_bytes);

        let relaunched = ScrollPlaceFile::with_paths(state, Some(conf)).load_all();
        assert!(
            !relaunched.contains_key("net1"),
            "forgotten net1 must be gone: {relaunched:?}"
        );
        assert!(
            relaunched
                .get("net2")
                .is_some_and(|rows| rows.iter().any(|(target, _)| target == "#dev")),
            "net2#dev must survive forget when the state file was missing: {relaunched:?}"
        );
    }

    #[test]
    fn for_settings_live_path_uses_state_dir_and_keeps_legacy_conf() {
        let dir = agent_dir("scroll-live-wiring");
        let env = paths::Env {
            xdg_config_home: Some(dir.path().join("config")),
            xdg_state_home: Some(dir.path().join("state")),
            home: Some(PathBuf::from("home-1d06")),
            temp_dir: dir.path().join("tmp"),
            ..paths::Env::default()
        };
        let platform = Platform::Linux;
        let conf = paths::settings_path_for(&env, platform).expect("config root");
        let state = paths::scroll_places_path_for(&env, platform);
        fs::create_dir_all(conf.parent().expect("parent")).unwrap();
        two_network_legacy_conf(&conf);
        let conf_bytes = fs::read(&conf).unwrap();

        let file = ScrollPlaceFile::for_settings_with(&Backing::Disk(conf.clone()), &env, platform);
        assert_eq!(file.conf_path(), Some(conf.as_path()));
        assert_eq!(file.state_path(), Some(state.as_path()));
        assert!(
            !state.starts_with(conf.parent().unwrap()),
            "live state must not sit in the conf folder: state={} conf={}",
            state.display(),
            conf.display()
        );
        assert!(
            file.load_all().contains_key("net2"),
            "live wiring must keep the legacy conf fallback"
        );

        assert_eq!(
            file.save(
                "net1",
                &vec![(
                    "#qa".to_owned(),
                    ScrollPlace {
                        follow_end: true,
                        ..ScrollPlace::default()
                    }
                )]
            ),
            Status::Written
        );
        assert_eq!(fs::read(&conf).unwrap(), conf_bytes);
        assert!(state.exists());
        assert!(!conf.parent().unwrap().join(SCROLL_PLACES_FILE).exists());
        let relaunched =
            ScrollPlaceFile::for_settings_with(&Backing::Disk(conf), &env, platform).load_all();
        assert!(relaunched.contains_key("net2"), "{relaunched:?}");
    }

    #[test]
    fn for_settings_macos_uses_ini_conf_and_state_dir() {
        let dir = agent_dir("scroll-macos-ini");
        let env = paths::Env {
            home: Some(dir.path().join("home")),
            temp_dir: dir.path().join("tmp"),
            ..paths::Env::default()
        };
        let conf = paths::settings_path_for(&env, Platform::MacOs).expect("macos conf");
        assert_eq!(
            conf.file_name().and_then(|name| name.to_str()),
            Some("omairc.ini"),
            "macOS settings stay on omairc.ini"
        );
        let file =
            ScrollPlaceFile::for_settings_with(&Backing::Disk(conf.clone()), &env, Platform::MacOs);
        assert_eq!(file.conf_path(), Some(conf.as_path()));
        assert_eq!(
            file.state_path(),
            Some(paths::scroll_places_path_for(&env, Platform::MacOs).as_path())
        );
    }

    #[test]
    fn forgetting_the_only_legacy_network_writes_empty_state() {
        let dir = agent_dir("scroll-one-net-forget");
        let conf = dir.path().join("omairc.conf");
        let state = dir.path().join(SCROLL_PLACES_FILE);
        fs::write(
            &conf,
            "[scrollPlaces]\nnet1\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#qa\\\"}]\"\n",
        )
        .unwrap();
        let conf_bytes = fs::read(&conf).unwrap();

        let file = ScrollPlaceFile::with_paths(state.clone(), Some(conf.clone()));
        assert!(!state.exists());
        assert_eq!(file.save("net1", &PlaceRows::new()), Status::Written);
        assert_eq!(fs::read(&conf).unwrap(), conf_bytes);
        assert!(
            state.exists(),
            "forgetting the only legacy network must create an empty state file"
        );

        let relaunched = ScrollPlaceFile::with_paths(state, Some(conf)).load_all();
        assert!(
            relaunched.is_empty(),
            "forgotten sole legacy network must stay gone: {relaunched:?}"
        );
    }

    #[test]
    fn seed_reads_both_macos_conf_names_with_ini_winning() {
        let dir = agent_dir("scroll-dual-conf");
        let ini = dir.path().join("omairc.ini");
        let conf = dir.path().join("omairc.conf");
        let state = dir.path().join(SCROLL_PLACES_FILE);
        fs::write(
            &ini,
            "[scrollPlaces]\nnet_ini\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#from-ini\\\"}]\"\n\
             net_both\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#ini-wins\\\"}]\"\n",
        )
        .unwrap();
        fs::write(
            &conf,
            "[scrollPlaces]\nnet_conf\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#from-conf\\\"}]\"\n\
             net_both\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#conf-loses\\\"}]\"\n",
        )
        .unwrap();

        // Desktop-shaped primary (omairc.ini): first save must also pick up conf-only nets.
        let file = ScrollPlaceFile::with_paths(state.clone(), Some(ini.clone()));
        assert_eq!(
            file.save(
                "net_ini",
                &vec![(
                    "#from-ini".to_owned(),
                    ScrollPlace {
                        follow_end: true,
                        ..ScrollPlace::default()
                    }
                )]
            ),
            Status::Written
        );
        let loaded = ScrollPlaceFile::with_paths(state.clone(), Some(ini)).load_all();
        assert_eq!(loaded["net_ini"][0].0, "#from-ini");
        assert_eq!(
            loaded["net_conf"][0].0, "#from-conf",
            "places only in omairc.conf must be seeded"
        );
        assert_eq!(
            loaded["net_both"][0].0, "#ini-wins",
            "omairc.ini must win per network over omairc.conf"
        );

        // TUI-shaped primary (omairc.conf): places only in omairc.ini must still seed.
        let dir2 = agent_dir("scroll-dual-conf-tui");
        let ini2 = dir2.path().join("omairc.ini");
        let conf2 = dir2.path().join("omairc.conf");
        let state2 = dir2.path().join(SCROLL_PLACES_FILE);
        fs::write(
            &ini2,
            "[scrollPlaces]\nnet_ini_only\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#ini-only\\\"}]\"\n",
        )
        .unwrap();
        fs::write(&conf2, "[preferences]\ndirects=true\n").unwrap();
        let tui_shaped = ScrollPlaceFile::with_paths(state2.clone(), Some(conf2));
        assert_eq!(
            tui_shaped.save(
                "other",
                &vec![(
                    "#x".to_owned(),
                    ScrollPlace {
                        follow_end: true,
                        ..ScrollPlace::default()
                    }
                )]
            ),
            Status::Written
        );
        let again = ScrollPlaceFile::with_paths(state2, None).load_all();
        assert_eq!(
            again["net_ini_only"][0].0, "#ini-only",
            "places only in omairc.ini must seed when primary is omairc.conf"
        );
    }

    #[test]
    fn unreadable_legacy_conf_refuses_save_and_creates_no_state() {
        let dir = agent_dir("scroll-unreadable-conf");
        let conf = dir.path().join("omairc.conf");
        let state = dir.path().join(SCROLL_PLACES_FILE);
        fs::write(
            &conf,
            "[scrollPlaces]\nnet1\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#qa\\\"}]\"\n\
             net2\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#dev\\\"}]\"\n",
        )
        .unwrap();
        #[cfg(not(unix))]
        {
            let _ = (conf, state);
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let restore = fs::Permissions::from_mode(0o600);
            fs::set_permissions(&conf, fs::Permissions::from_mode(0o000)).unwrap();
            if fs::read(&conf).is_ok() {
                fs::set_permissions(&conf, restore).unwrap();
                return;
            }
            let file = ScrollPlaceFile::with_paths(state.clone(), Some(conf.clone()));
            assert_eq!(
                file.save(
                    "net1",
                    &vec![(
                        "#qa".to_owned(),
                        ScrollPlace {
                            follow_end: true,
                            ..ScrollPlace::default()
                        }
                    )]
                ),
                Status::AccessError
            );
            assert!(
                !state.exists(),
                "unreadable legacy conf must not create an unseeded state file"
            );
            fs::set_permissions(&conf, restore).unwrap();
        }
    }

    #[test]
    fn seed_runs_only_when_state_file_is_missing() {
        let dir = agent_dir("scroll-seed-once");
        let conf = dir.path().join("omairc.conf");
        let state = dir.path().join(SCROLL_PLACES_FILE);
        fs::write(
            &conf,
            "[scrollPlaces]\nnet2\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#dev\\\"}]\"\n",
        )
        .unwrap();
        // State exists without net2 (migration already happened).
        fs::write(
            &state,
            "[scrollPlaces]\nnet1\\places=\"[{\\\"follow\\\":true,\\\"target\\\":\\\"#qa\\\"}]\"\n",
        )
        .unwrap();

        let file = ScrollPlaceFile::with_paths(state.clone(), Some(conf));
        assert_eq!(
            file.save(
                "net1",
                &vec![(
                    "#qa".to_owned(),
                    ScrollPlace {
                        follow_end: false,
                        msg_id: "m-1".to_owned(),
                        ..ScrollPlace::default()
                    }
                )]
            ),
            Status::Written
        );
        let loaded = ScrollPlaceFile::with_paths(state, None).load_all();
        assert_eq!(loaded["net1"][0].1.msg_id, "m-1");
        assert!(
            !loaded.contains_key("net2"),
            "existing state file must not re-seed net2 from conf: {loaded:?}"
        );
    }
}
