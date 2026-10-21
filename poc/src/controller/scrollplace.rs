//! Port of `tui/internal/controller/scrollplace.go` (Go) and `src/irc/ircscrollplace.cpp`
//! (C++): where each conversation's transcript was last left, as a message anchor.
//!
//! The anchor is the first visible message, named by its msgid when the server gave one
//! and otherwise by its kind, author, body and server time. An anchor is found again
//! after the transcript changes, so a place survives a reload. A place that pins the
//! bottom is stored as `follow_end`. The Status console has no place.
//!
//! The store is in memory for now. Phase 8 binds it to the settings file under
//! `scrollPlaces/<network>`, with the group and key names of the Qt client.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::irc::casemapping::IrcCaseMapping;
use crate::irc::reducer::zero_time;
use crate::irc::world::Message;

use super::controller::Controller;
use super::snapshots::message_kind_name;

/// One conversation's saved place (`storage::ScrollPlace`). `follow_end` pins the
/// bottom. Otherwise the anchor fields name the first visible message.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScrollPlace {
    pub follow_end: bool,
    pub msg_id: String,
    pub author: String,
    pub kind: String,
    pub body: String,
    pub epoch_ms: i64,
    pub has_time: bool,
}

/// The selected conversation's saved place, resolved against the transcript
/// (`ScrollView`). `known` is false when the conversation has no place. `row` is `None`
/// when the anchor is not in the loaded transcript yet, and `known` stays true, so the
/// caller does not fall through to opening at unread.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScrollView {
    pub known: bool,
    pub follow: bool,
    pub row: Option<usize>,
}

/// The saved places, one per conversation (`ScrollPlaceStore`). Keys are the network id
/// and the target folded under the network's case mapping.
#[derive(Debug, Default)]
pub struct ScrollPlaceStore {
    places: BTreeMap<String, BTreeMap<String, ScrollPlace>>,
}

impl ScrollPlaceStore {
    /// Stores the place for `target` on `network_id`.
    pub fn remember(
        &mut self,
        network_id: &str,
        target: &str,
        place: ScrollPlace,
        mapping: IrcCaseMapping,
    ) {
        if network_id.is_empty() {
            return;
        }
        self.places
            .entry(network_id.to_owned())
            .or_default()
            .insert(mapping.normalize(target), place);
    }

    /// The saved place for `target` on `network_id`, if any.
    pub fn place(&self, network_id: &str, target: &str, mapping: IrcCaseMapping) -> Option<&ScrollPlace> {
        self.places
            .get(network_id)
            .and_then(|targets| targets.get(&mapping.normalize(target)))
    }

    /// Drops every place of `network_id` (`Forget`).
    pub fn forget(&mut self, network_id: &str) {
        self.places.remove(network_id);
    }
}

impl Controller {
    /// Records where the selected conversation's transcript was left
    /// (`RememberScrollPlace`). `follow_end` pins the bottom. Otherwise `anchor_row` is
    /// the first visible message. Nothing is saved for the Status console, with no
    /// selection, or for an anchor outside the transcript.
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
        let mapping = self.reducer.server_features(&key.network_id).case_mapping();
        let target = self.selected_target.clone();
        self.scroll_places
            .remember(&key.network_id, &target, place, mapping);
    }

    /// The selected conversation's saved place, resolved against its transcript
    /// (`CurrentScrollPlace`).
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
/// whole test when the place has one. Otherwise the kind, author and body must match,
/// and the time when the place recorded one.
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
fn millis(time: DateTime<Utc>) -> i64 {
    time.timestamp_millis()
}

