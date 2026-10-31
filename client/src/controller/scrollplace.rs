//! Where each conversation's transcript was last left, as a message anchor.
//! The anchor is the first visible message, named by its msgid when the server gave one
//! and otherwise by its kind, author, body and server time. An anchor is found again
//! after the transcript changes, so a place survives a reload. A place that pins the
//! bottom is stored as `follow_end`. The Status console has no place.
//! The places live in `<state>/scrollplaces.ini` under `scrollPlaces/<network>`, the
//! group Qt used in the conf (`storage::scrollplace`). Each network's list is loaded
//! when the store is built and written again after each change. A missing state file
//! still reads the legacy conf group once; saves never touch the conf.
//! The shell resumes a transcript by its own rows (`model::scroll_place`) and stores the
//! result through `remember_place` and `place_for`. The message-anchor reads below are the
//! Qt-format path over the controller's messages; the GUI uses row msgids through

#[cfg(test)]
use chrono::{DateTime, Utc};

use crate::irc::casemapping::IrcCaseMapping;
#[cfg(test)]
use crate::irc::reducer::zero_time;
#[cfg(test)]
use crate::irc::world::Message;
use crate::storage::scrollplace::{PlaceRows, ScrollPlace, ScrollPlaceFile};

use super::controller::Controller;
#[cfg(test)]
use super::snapshots::message_kind_name;

/// The selected conversation's saved place, resolved against the transcript
/// (`ScrollView`). `known` is false when the conversation has no place. `row` is `None`
/// when the anchor is not in the loaded transcript yet, and `known` stays true, so the
/// caller does not fall through to opening at unread.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg(test)]
pub struct ScrollView {
    pub known: bool,
    pub follow: bool,
    pub row: Option<usize>,
}

/// The saved places of every network, one list per network in the order they were first
/// saved (`ScrollPlaceStore`). A target matches under the network's case mapping.
#[derive(Debug)]
pub struct ScrollPlaceStore {
    file: ScrollPlaceFile,
    places: std::collections::BTreeMap<String, PlaceRows>,
}

impl ScrollPlaceStore {
    /// The store over `file`, holding every place an earlier run saved.
    pub fn new(file: ScrollPlaceFile) -> Self {
        let places = file.load_all();
        Self { file, places }
    }

    /// Stores the place for `target` on `network_id`, replacing a place that names it
    /// under `mapping`, and saves the network's list.
    pub fn remember(
        &mut self,
        network_id: &str,
        target: &str,
        place: ScrollPlace,
        mapping: IrcCaseMapping,
    ) {
        if network_id.is_empty() || target.is_empty() {
            return;
        }
        let rows = self.places.entry(network_id.to_owned()).or_default();
        match rows.iter().position(|(stored, _)| mapping.equals(stored, target)) {
            Some(index) => rows[index] = (target.to_owned(), place),
            None => rows.push((target.to_owned(), place)),
        }
        self.save(network_id);
    }

    /// The saved place for `target` on `network_id`, if any.
    pub fn place(&self, network_id: &str, target: &str, mapping: IrcCaseMapping) -> Option<&ScrollPlace> {
        self.places
            .get(network_id)?
            .iter()
            .find(|(stored, _)| mapping.equals(stored, target))
            .map(|(_, place)| place)
    }

    /// Follows a NICK: the place of `old` moves to `new` (`Rekey`). A `new` that already
    /// has a place takes this one. Returns whether anything moved.
    pub fn rekey(&mut self, network_id: &str, old: &str, new: &str, mapping: IrcCaseMapping) -> bool {
        let Some(rows) = self.places.get_mut(network_id) else {
            return false;
        };
        let Some(index) = rows.iter().position(|(stored, _)| mapping.equals(stored, old)) else {
            return false;
        };
        let (_, kept) = rows.remove(index);
        match rows.iter().position(|(stored, _)| mapping.equals(stored, new)) {
            Some(existing) => rows[existing] = (new.to_owned(), kept),
            None => rows.push((new.to_owned(), kept)),
        }
        self.save(network_id);
        true
    }

    /// Drops every place of `network_id` (`Forget`).
    pub fn forget(&mut self, network_id: &str) {
        if self.places.remove(network_id).is_some() {
            self.file.save(network_id, &PlaceRows::new());
        }
    }

    fn save(&self, network_id: &str) {
        let rows = self.places.get(network_id).cloned().unwrap_or_default();
        self.file.save(network_id, &rows);
    }
}

impl Controller {
    /// Records where the selected conversation's transcript was left
    /// (`RememberScrollPlace`). `follow_end` pins the bottom. Otherwise `anchor_row` is
    /// the first visible message. Nothing is saved for the Status console, with no
    /// selection, or for an anchor outside the transcript.
    #[cfg(test)]
    pub fn remember_scroll_place(&mut self, follow_end: bool, anchor_row: Option<usize>) {
        if self.console_open {
            return;
        }
        let Some(key) = self.selected.clone() else {
            return;
        };
        let Some(conversation) = self.reducer.find(&key) else {
            return;
        };
        let mut place = ScrollPlace {
            follow_end,
            ..ScrollPlace::default()
        };
        if !follow_end {
            let Some(message) = anchor_row.and_then(|row| conversation.messages.get(row)) else {
                return;
            };
            place = anchor_of(message);
        }
        self.remember_place(&key.network_id, &self.selected_target.clone(), place);
    }

    /// Stores a place for a conversation, from whatever anchor the caller holds. The shell
    /// uses it for the rows it shows, which are not the controller's messages.
    pub fn remember_place(&mut self, network_id: &str, target: &str, place: ScrollPlace) {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        self.scroll_places.remember(network_id, target, place, mapping);
    }

    /// The saved place for a conversation, as the caller last stored it.
    pub fn place_for(&self, network_id: &str, target: &str) -> Option<ScrollPlace> {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        self.scroll_places.place(network_id, target, mapping).cloned()
    }

    /// The selected conversation's saved place, resolved against its transcript
    /// (`CurrentScrollPlace`).
    #[cfg(test)]
    pub fn current_scroll_place(&self) -> ScrollView {
        let mut view = ScrollView::default();
        if self.console_open {
            return view;
        }
        let Some(key) = self.selected.as_ref() else {
            return view;
        };
        let mapping = self.reducer.server_features(&key.network_id).case_mapping();
        let Some(place) = self
            .scroll_places
            .place(&key.network_id, &self.selected_target, mapping)
        else {
            return view;
        };
        view.known = true;
        if place.follow_end {
            view.follow = true;
            return view;
        }
        let Some(conversation) = self.reducer.find(key) else {
            return view;
        };
        view.row = conversation
            .messages
            .iter()
            .position(|message| anchor_matches(place, message));
        view
    }
}

/// The anchor a message names: its msgid, or its kind, author, body and time
/// (`scrollPlaceFor`).
#[cfg(test)]
fn anchor_of(message: &Message) -> ScrollPlace {
    let mut place = ScrollPlace {
        msg_id: message.msgid.value.clone(),
        author: message.author.clone(),
        kind: message_kind_name(message.kind).to_owned(),
        body: message.body.clone(),
        ..ScrollPlace::default()
    };
    if message.time != zero_time() {
        place.epoch_ms = millis(message.time);
        place.has_time = true;
    }
    place
}

/// Whether `message` is the anchor `place` names (`scrollPlaceMatches`). A msgid is the
/// whole test when the place has one. Otherwise the kind, author and body must match
/// and the time when the place recorded one.
#[cfg(test)]
fn anchor_matches(place: &ScrollPlace, message: &Message) -> bool {
    if !place.msg_id.is_empty() {
        return message.msgid.value == place.msg_id;
    }
    if message_kind_name(message.kind) != place.kind
        || message.author != place.author
        || message.body != place.body
    {
        return false;
    }
    if !place.has_time {
        return true;
    }
    if message.time == zero_time() {
        return false;
    }
    millis(message.time) == place.epoch_ms
}

/// A time as milliseconds since the epoch, in UTC.
#[cfg(test)]
fn millis(time: DateTime<Utc>) -> i64 {
    time.timestamp_millis()
}
