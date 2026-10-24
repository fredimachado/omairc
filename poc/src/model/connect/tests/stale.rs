//! Background JOIN/PART updates must reach the Connect draft before Apply.

use std::collections::BTreeMap;

use super::*;
use crate::irc::casemapping::{IrcCaseMapping, IrcCaseMappingKind};
use crate::model::connect::{retain_autojoin_keys, ConnectField};

#[test]
fn background_autojoin_sync_stops_apply_from_restoring_a_parted_channel() {
    let existing = StoredProfile {
        autojoin_channels: vec!["#omarchy".to_owned(), "#lab".to_owned()],
        ..stored("net-1", "irc.libera.chat", "fred")
    };
    let mut state = ConnectState::from_stored(&[existing.clone()]);
    let updated = StoredProfile {
        autojoin_channels: vec!["#omarchy".to_owned()],
        ..existing
    };
    state.sync_background_profile(&updated);
    state.set_text(ConnectField::Nick, "fred2");
    let applied = state.apply().expect("complete profile applies");
    let saved = applied.to_stored(Some(&updated), IrcCaseMapping::default());
    assert_eq!(saved.autojoin_channels, ["#omarchy"]);
}

#[test]
fn retain_autojoin_keys_matches_with_the_network_case_mapping() {
    let existing = StoredProfile {
        autojoin_channels: vec!["#Lab".to_owned()],
        autojoin_keys: BTreeMap::from([("#lab".to_owned(), "pw".to_owned())]),
        ..stored("net-1", "irc.libera.chat", "fred")
    };
    let profile = Profile::from_stored(&existing);
    let saved = profile.to_stored(Some(&existing), IrcCaseMapping::default());
    assert_eq!(
        saved.autojoin_keys.get("#Lab").map(String::as_str),
        Some("pw")
    );
}

#[test]
fn retain_autojoin_keys_keeps_channels_the_mapping_keeps_distinct() {
    let keys = BTreeMap::from([
        ("#nick^".to_owned(), "caret".to_owned()),
        ("#nick~".to_owned(), "tilde".to_owned()),
    ]);
    let channels = vec!["#nick^".to_owned(), "#nick~".to_owned()];
    let mapping = IrcCaseMapping::new(IrcCaseMappingKind::Rfc1459Strict);
    let retained = retain_autojoin_keys(&keys, &channels, mapping);
    assert_eq!(retained.get("#nick^").map(String::as_str), Some("caret"));
    assert_eq!(retained.get("#nick~").map(String::as_str), Some("tilde"));
}
