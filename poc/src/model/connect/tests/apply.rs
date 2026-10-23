//! Apply and the saved networks: what it writes, what it refuses, and the first run.

use super::*;

#[test]
fn apply_writes_the_name_and_nick_into_the_world() {
    let (_, mut state) = seeded();
    state.set_text(ConnectField::Nick, "fred2");
    let profile = state.apply().expect("complete profile applies");
    assert_eq!(profile.network_id, "omarchy");
    assert_eq!(profile.nick, "fred2");
    let saved = state
        .saved()
        .iter()
        .find(|saved| saved.network_id == "omarchy")
        .expect("omarchy is saved");
    assert_eq!(saved.nick, "fred2");
}

#[test]
fn a_rename_that_clashes_adds_the_nick_and_a_clash_removed_drops_it() {
    let (_, mut state) = seeded();
    let labels = |state: &ConnectState| -> Vec<String> {
        state.rows().into_iter().map(|row| row.label).collect()
    };
    assert!(state.select("oftc"));
    state.set_text(ConnectField::Name, "libera");
    state.apply().expect("oftc applies");
    assert_eq!(labels(&state), ["irc.example", "libera"]);

    assert!(state.select("omarchy"));
    state.set_text(ConnectField::Name, "libera");
    state.apply().expect("omarchy applies");
    assert_eq!(labels(&state), ["libera · fred", "libera · oak"]);

    state.set_text(ConnectField::Name, "");
    state.apply().expect("omarchy clears its name");
    assert_eq!(labels(&state), ["irc.example", "libera"]);
}

#[test]
fn an_incomplete_draft_is_refused_and_changes_nothing() {
    let (_, mut state) = seeded();
    let before = state.saved().to_vec();
    state.set_text(ConnectField::Nick, "");
    assert_eq!(state.apply(), Err(Problem::MissingNick));
    assert_eq!(state.saved(), before.as_slice());
}

#[test]
fn apply_keeps_the_secret_text_until_the_sheet_closes() {
    let (_populated, mut state) = seeded();
    state.set_text(ConnectField::ServerPassword, "hunter2");
    assert_eq!(
        state
            .secret_edit(Secret::Password)
            .as_ref()
            .map(SecretText::expose),
        Some("hunter2")
    );
    state.apply().unwrap();
    // The saved network never holds the secret, and the sheet keeps the typed text.
    assert_eq!(state.text(ConnectField::ServerPassword), "hunter2");
    assert!(state.saved()[0].password.is_empty());
    assert_eq!(
        state.secret_edit(Secret::Password),
        None,
        "Apply took the edit"
    );
    state.clear_secret_text();
    assert_eq!(state.text(ConnectField::ServerPassword), "");
}

#[test]
fn first_run_is_the_suggested_freenode_draft_and_it_is_required() {
    let state = ConnectState::first_run();
    assert!(state.setup_required());
    assert_eq!(state.text(ConnectField::Name), "chat.freenode.net");
    assert_eq!(state.text(ConnectField::Host), "chat.freenode.net");
    assert_eq!(state.text(ConnectField::Port), "6697");
    assert!(state.tls());
    assert_eq!(state.text(ConnectField::Autojoin), "#omarchy");
    assert!(!state.connect_on_startup());
    assert_eq!(state.text(ConnectField::Nick), "", "no nick is suggested");
    assert_eq!(state.problem(), Some(Problem::MissingNick));
    assert_eq!(Problem::MissingNick.text(), "Nick is required");
    assert_eq!(
        state.draft_name(state.selected()).as_deref(),
        Some("chat.freenode.net"),
        "Status shows the draft's name before anything is saved"
    );
    assert!(state.saved().is_empty());
    assert_eq!(state.credential_status(), "");
    assert_eq!(state.persistence_status(), "");
    assert_eq!(state.initial_focus(), Stop::Field(ConnectField::Nick));
}

#[test]
fn a_saved_and_started_network_ends_the_requirement_and_closes_the_sheet() {
    let mut state = ConnectState::first_run();
    state.set_text(ConnectField::Nick, "fred");
    let id = state.selected().to_owned();
    let profile = state.apply().expect("a complete draft applies");
    assert_eq!(profile.network_id, id);
    assert!(!state.setup_required());
    assert!(state.record_apply(
        Status::Written,
        CredentialState::Missing,
        CredentialState::Missing,
        true
    ));
    assert_eq!(state.persistence_status(), "");
    assert_eq!(
        state.draft_name(&id),
        None,
        "the saved network is named by the controller"
    );
}

#[test]
fn a_saved_network_that_did_not_start_keeps_the_sheet_and_says_so() {
    let mut state = ConnectState::first_run();
    state.set_text(ConnectField::Nick, "fred");
    state.apply().expect("complete");
    let close = state.record_apply(
        Status::Written,
        CredentialState::Missing,
        CredentialState::Missing,
        false,
    );
    assert!(!close, "the sheet stays up with the sentence");
    assert_eq!(
        state.persistence_status(),
        "These settings were saved successfully."
    );
}

#[test]
fn a_failed_settings_write_is_said_and_a_started_session_is_noted() {
    let mut state = ConnectState::first_run();
    state.set_text(ConnectField::Nick, "fred");
    state.apply().expect("complete");
    state.record_apply(
        Status::AccessError,
        CredentialState::Missing,
        CredentialState::Missing,
        false,
    );
    assert_eq!(
        state.persistence_status(),
        "The settings file could not be written."
    );
    state.record_apply(
        Status::FormatError,
        CredentialState::Missing,
        CredentialState::Missing,
        true,
    );
    assert_eq!(
        state.persistence_status(),
        "Connected using these settings. The settings file could not be read."
    );
}

#[test]
fn from_stored_selects_the_first_complete_network() {
    let incomplete = stored("network-1", "a.example", "");
    let complete = stored("network-2", "b.example", "fred");
    let state = ConnectState::from_stored(&[incomplete, complete]);
    assert_eq!(state.selected(), "network-2");
    assert!(!state.setup_required());
    assert_eq!(state.text(ConnectField::Host), "b.example");
}

#[test]
fn saved_networks_with_none_complete_still_require_setup() {
    let state = ConnectState::from_stored(&[stored("network-1", "a.example", "")]);
    assert!(state.setup_required());
    assert_eq!(state.selected(), "network-1");
}

#[test]
fn saving_keeps_the_fields_the_sheet_does_not_edit() {
    let existing = StoredProfile {
        account: "fred-acct".to_owned(),
        bouncer_network: "bnc".to_owned(),
        avatar_url: "https://omairc.app/omairc128.png".to_owned(),
        autojoin_channels: vec!["#lab".to_owned(), "#omarchy".to_owned()],
        autojoin_keys: BTreeMap::from([
            ("#lab".to_owned(), "pw".to_owned()),
            ("#gone".to_owned(), "old".to_owned()),
        ]),
        ..stored("network-1", "irc.libera.chat", "fred")
    };
    let saved = Profile::from_stored(&existing).to_stored(Some(&existing));
    assert_eq!(saved.account, "fred-acct");
    assert_eq!(saved.bouncer_network, "bnc");
    assert_eq!(saved.avatar_url, "https://omairc.app/omairc128.png");
    assert_eq!(saved.autojoin_channels, ["#lab", "#omarchy"]);
    assert_eq!(
        saved.autojoin_keys,
        BTreeMap::from([("#lab".to_owned(), "pw".to_owned())]),
        "a key stays only while its channel is in the list"
    );
}

#[test]
fn apply_never_changes_the_connection_state_of_the_world() {
    let (populated, mut state) = seeded();
    assert!(state.apply().is_ok());
    assert_eq!(populated, ["omarchy", "oftc"]);
}
