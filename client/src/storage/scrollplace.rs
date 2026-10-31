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

use super::paths::{self, SCROLL_PLACES_FILE};
use super::settings::{Settings, Status};
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
        match settings {
            Backing::Ephemeral => Self::new(Backing::Ephemeral),
            Backing::Disk(conf) => {
                let state = scroll_state_path_for_settings(conf);
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

    /// Every network's places, keyed by network id. Entries that do not parse are skipped.
    /// Prefers `scrollplaces.ini` when it exists; otherwise reads the legacy conf group
    /// without writing or stripping it.
    pub fn load_all(&self) -> BTreeMap<String, PlaceRows> {
        if self.backing.is_ephemeral() {
            return BTreeMap::new();
        }
        if let Backing::Disk(state) = &self.backing {
            if state.exists() {
                return load_all_from(&Settings::open(state));
            }
        }
        self.conf_path
            .as_ref()
            .map(|path| load_all_from(&Settings::open(path)))
            .unwrap_or_default()
    }

    /// Replaces one network's places (`save`). An empty list removes the network's group.
    /// Writes only to the state file; the conf is never touched.
    pub fn save(&self, network_id: &str, rows: &PlaceRows) -> Status {
        if network_id.is_empty() {
            return Status::Absent;
        }
        let group = format!("{SCROLL_PLACES_GROUP}/{network_id}");
        self.backing.edit(|settings| {
            settings.remove_group(&group);
            if !rows.is_empty() {
                let array: Vec<Value> = rows
                    .iter()
                    .map(|(target, place)| place_json(target, place))
                    .collect();
                settings.set_value(&group, PLACES_KEY, &Value::Array(array).to_string());
            }
        })
    }
}

/// State path for a settings file: the live user path uses the state dir; a
/// test/temp conf gets a sibling `scrollplaces.ini`.
fn scroll_state_path_for_settings(conf: &Path) -> PathBuf {
    match paths::settings_path() {
        Some(user) if user == conf => paths::scroll_places_path(),
        _ => conf.with_file_name(SCROLL_PLACES_FILE),
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
}
