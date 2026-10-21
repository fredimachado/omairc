//! Port of the autojoin bookkeeping in `tui/internal/session/session.go` (Go),
//! `recordAutojoinLocked` and friends, and the matching code in
//! `src/irc/ircsession.cpp` (C++).
//!
//! The session keeps the channels it should rejoin and their keys. A self JOIN
//! records the channel (and a key the user typed), a self PART or KICK forgets
//! them, and a 475 (`+k`, bad key) drops a key that no longer works. Every change
//! emits `AutojoinChannelsChanged`, and the owner persists the new list.

use std::collections::BTreeMap;

use super::events::SessionEvent;
use super::session::Core;
use crate::irc::casemapping::IrcCaseMapping;

/// The first stored key that names `channel` under `mapping`, in key order.
pub(crate) fn lookup_autojoin_key(
    keys: &BTreeMap<String, String>,
    mapping: IrcCaseMapping,
    channel: &str,
) -> Option<String> {
    keys.iter()
        .find(|(key, _)| mapping.equals(key, channel))
        .map(|(_, value)| value.clone())
}

/// Removes the first stored key that names `channel`, in key order. Returns
/// whether one was removed.
pub(crate) fn erase_autojoin_key(
    keys: &mut BTreeMap<String, String>,
    mapping: IrcCaseMapping,
    channel: &str,
) -> bool {
    let found = keys
        .keys()
        .find(|key| mapping.equals(key, channel))
        .cloned();
    match found {
        Some(key) => {
            keys.remove(&key);
            true
        }
        None => false,
    }
}

impl Core {
    /// Records a self JOIN or a self PART/KICK (`recordAutojoinLocked`).
    pub(crate) fn record_autojoin(&mut self, channel: &str, joined: bool) {
        if channel.is_empty() {
            return;
        }
        let mapping = self.features.case_mapping();
        let found_index = self
            .autojoin_channels
            .iter()
            .position(|existing| mapping.equals(existing, channel));

        if joined {
            let mut listed = channel.to_owned();
            let mut name_changed = false;
            match found_index {
                None => {
                    self.autojoin_channels.push(channel.to_owned());
                    name_changed = true;
                }
                Some(index) => listed = self.autojoin_channels[index].clone(),
            }

            let mut key_changed = false;
            let folded = self.fold_channel(channel);
            let pending = self.pending_join_keys.remove(&folded).unwrap_or_default();
            if !pending.is_empty() {
                let existing = lookup_autojoin_key(&self.autojoin_keys, mapping, &listed);
                if existing.as_deref() != Some(pending.as_str()) {
                    erase_autojoin_key(&mut self.autojoin_keys, mapping, &listed);
                    self.autojoin_keys.insert(listed, pending);
                    key_changed = true;
                }
            }
            if !name_changed && !key_changed {
                return;
            }
        } else {
            let had_name = found_index.is_some();
            let had_key = erase_autojoin_key(&mut self.autojoin_keys, mapping, channel);
            let folded = self.fold_channel(channel);
            self.pending_join_keys.remove(&folded);
            if !had_name && !had_key {
                return;
            }
            if let Some(index) = found_index {
                self.autojoin_channels.remove(index);
            }
        }
        self.emit_autojoin_changed();
    }

    /// Drops a stored key after a 475 (`dropStoredAutojoinKeyLocked`).
    pub(crate) fn drop_stored_autojoin_key(&mut self, channel: &str) {
        if channel.is_empty() {
            return;
        }
        let folded = self.fold_channel(channel);
        self.pending_join_keys.remove(&folded);
        let mapping = self.features.case_mapping();
        if !erase_autojoin_key(&mut self.autojoin_keys, mapping, channel) {
            return;
        }
        self.emit_autojoin_changed();
    }

    fn emit_autojoin_changed(&mut self) {
        let event = SessionEvent::AutojoinChannelsChanged {
            network_id: self.network_id(),
            channels: self.autojoin_channels.clone(),
            keys: self.autojoin_keys.clone(),
        };
        self.emit(event);
    }

    /// The key to send with a JOIN for `channel`, if one is stored.
    pub(crate) fn autojoin_key_for(&self, channel: &str) -> Option<String> {
        lookup_autojoin_key(&self.autojoin_keys, self.features.case_mapping(), channel)
    }
}
