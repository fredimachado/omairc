//! Port of `tui/internal/storage/scrollplace.go` (Go) and `src/irc/ircscrollplace.cpp`
//! (C++): where each conversation's transcript was last left, stored per network as one
//! JSON array in the `scrollPlaces/<network>` group, under the `places` key.
//!
//! A place is either "follow the end" or an anchor on the first visible message. The
//! anchor is the message's msgid when the server gave one, else its author, kind, body
//! and server time (`ms`). The JSON keys are sorted, as a `QJsonObject` writes them, so
//! the two clients read each other's files.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

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
    /// The time the line showed on screen, when the anchor has no epoch time. The PoC's
    /// transcript keeps the time as text. Qt ignores this key.
    pub shown_time: String,
}

/// The places of one network, keyed by target in the order they were first saved.
pub type PlaceRows = Vec<(String, ScrollPlace)>;

/// Reads and writes the scroll places of every network.
#[derive(Debug)]
pub struct ScrollPlaceFile {
    backing: Backing,
}

impl ScrollPlaceFile {
    pub fn new(backing: Backing) -> Self {
        Self { backing }
    }

    /// Every network's places, keyed by network id. Entries that do not parse are skipped.
    pub fn load_all(&self) -> BTreeMap<String, PlaceRows> {
        let settings = self.backing.settings();
        settings
            .child_groups(SCROLL_PLACES_GROUP)
            .into_iter()
            .map(|network_id| {
                let rows = read_rows(&settings, &network_id);
                (network_id, rows)
            })
            .collect()
    }

    /// Replaces one network's places (`save`). An empty list removes the network's group.
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
    use crate::util::testdir::TempDir;

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

    #[test]
    fn places_round_trip_in_their_saved_order() {
        let dir = TempDir::new("scroll");
        let file = ScrollPlaceFile::new(Backing::Disk(dir.path().join("omairc.conf")));
        let rows: PlaceRows = vec![
            ("#omarchy".to_owned(), ScrollPlace { follow_end: true, ..ScrollPlace::default() }),
            ("anna".to_owned(), anchor("anna", "see you")),
            (
                "#lab".to_owned(),
                ScrollPlace { msg_id: "m-42".to_owned(), ..ScrollPlace::default() },
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
        let dir = TempDir::new("scroll-empty");
        let file = ScrollPlaceFile::new(Backing::Disk(dir.path().join("omairc.conf")));
        file.save("net", &vec![("anna".to_owned(), anchor("anna", "x"))]);
        file.save("net", &Vec::new());
        assert!(file.load_all().is_empty());
    }

    #[test]
    fn an_ephemeral_store_keeps_nothing() {
        let file = ScrollPlaceFile::new(Backing::Ephemeral);
        file.save("net", &vec![("anna".to_owned(), anchor("anna", "x"))]);
        assert!(file.load_all().is_empty());
    }
}
