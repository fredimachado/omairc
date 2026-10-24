//! The credential line under the password fields: where a secret lives and whether it was kept.

use super::*;

#[test]
fn the_credential_line_says_where_a_typed_secret_lives() {
    let mut state = ConnectState::from_stored(&[stored("network-1", "irc.libera.chat", "fred")]);
    state.set_text(ConnectField::ServerPassword, "hunter2");
    assert_eq!(
        state.credential_status(),
        "password changed; apply to save securely"
    );
    assert_eq!(
        state
            .secret_edit(Secret::Password)
            .as_ref()
            .map(SecretText::expose),
        Some("hunter2")
    );
    state.apply().expect("complete");
    state.record_apply(
        Status::Written,
        CredentialState::SessionOnly,
        CredentialState::Missing,
        false,
    );
    assert_eq!(
        state.credential_status(),
        "secure storage unavailable; password is session-only"
    );
    state.set_text(ConnectField::NickServPassword, "nickpw");
    let line = state.credential_status();
    assert!(
        line.starts_with("secure storage unavailable; password is session-only; "),
        "{line}"
    );
    assert!(
        line.ends_with("NickServ changed; apply to save securely"),
        "{line}"
    );
}

#[test]
fn the_credential_line_says_when_the_keychain_holds_a_secret() {
    let mut state = ConnectState::from_stored(&[stored("network-1", "irc.libera.chat", "fred")]);
    state.set_text(ConnectField::ServerPassword, "hunter2");
    state.apply().expect("complete");
    state.record_apply(
        Status::Written,
        CredentialState::Available,
        CredentialState::Missing,
        true,
    );
    state.clear_secret_text();
    assert_eq!(state.credential_status(), "password saved securely");
    state.set_text(ConnectField::ServerPassword, "swordfish");
    assert_eq!(
        state.credential_status(),
        "password changed; apply to save securely"
    );
}

#[test]
fn a_refused_keychain_write_is_said_and_the_secret_is_not_called_saved() {
    let mut state = ConnectState::from_stored(&[stored("network-1", "irc.libera.chat", "fred")]);
    state.set_text(ConnectField::ServerPassword, "hunter2");
    state.apply().expect("complete");
    let refused = CredentialState::Error("Access is denied.".to_owned());
    state.record_apply(
        Status::Written,
        refused.clone(),
        CredentialState::Missing,
        false,
    );
    assert_eq!(
        state.credential_status(),
        "secure storage error; password is session-only"
    );
    state.clear_secret_text();
    assert_eq!(
        state.credential_status(),
        "secure storage error; password is not saved"
    );
}

#[test]
fn a_cleared_secret_leaves_no_line_behind() {
    let mut state = ConnectState::from_stored(&[stored("network-1", "irc.libera.chat", "fred")]);
    state.set_text(ConnectField::ServerPassword, "hunter2");
    state.apply().expect("complete");
    state.record_apply(
        Status::Written,
        CredentialState::Missing,
        CredentialState::Missing,
        false,
    );
    state.clear_secret_text();
    assert_eq!(state.credential_status(), "");
    assert_eq!(state.text(ConnectField::ServerPassword), "");
}

#[test]
fn can_forget_when_the_keychain_holds_a_secret_or_the_draft_does() {
    let mut state = ConnectState::from_stored(&[stored("network-1", "irc.libera.chat", "fred")]);
    assert!(!state.can_forget(Secret::Password));
    assert!(!state.can_forget(Secret::NickServ));

    state.set_text(ConnectField::ServerPassword, "hunter2");
    assert!(state.can_forget(Secret::Password), "held draft text");
    assert!(!state.can_forget(Secret::NickServ));

    state.apply().expect("complete");
    state.record_apply(
        Status::Written,
        CredentialState::Available,
        CredentialState::Missing,
        true,
    );
    state.clear_secret_text();
    assert!(
        state.can_forget(Secret::Password),
        "Available after Apply with empty draft"
    );
    assert!(!state.can_forget(Secret::NickServ));

    let mut first_run = ConnectState::first_run();
    first_run.set_text(ConnectField::ServerPassword, "hunter2");
    assert!(
        !first_run.can_forget(Secret::Password),
        "an unsaved draft never shows forget"
    );
}

#[test]
fn forget_clears_the_draft_and_keeps_the_slot_until_settled() {
    let mut state = ConnectState::from_stored(&[stored("network-1", "irc.libera.chat", "fred")]);
    state.set_text(ConnectField::ServerPassword, "hunter2");
    state.apply().expect("complete");
    state.record_apply(
        Status::Written,
        CredentialState::Available,
        CredentialState::Missing,
        true,
    );
    state.clear_secret_text();
    assert_eq!(state.credential_status(), "password saved securely");

    assert_eq!(
        state.forget(Secret::Password).as_deref(),
        Some("network-1")
    );
    assert_eq!(state.text(ConnectField::ServerPassword), "");
    assert_eq!(
        state.secret_states().password,
        CredentialState::Available,
        "slot stays Available until the store settle is applied"
    );
    assert!(
        state.can_forget(Secret::Password),
        "forget stays available until Missing settles"
    );
    assert_eq!(
        state.secret_edit(Secret::Password).as_ref().map(SecretText::expose),
        Some("")
    );

    state.apply_forget_result(Secret::Password, CredentialState::Missing);
    assert_eq!(state.secret_states().password, CredentialState::Missing);
    assert_eq!(state.credential_status(), "");
    assert!(!state.can_forget(Secret::Password));
    assert_eq!(
        state.forget(Secret::Password),
        None,
        "already empty and Missing is a no-op"
    );
}

#[test]
fn a_refused_forget_keeps_the_link_and_shows_the_error() {
    let mut profile = stored("network-1", "irc.libera.chat", "fred");
    profile.secret_saved = true;
    let mut state = ConnectState::from_stored(&[profile]);
    assert!(state.can_forget(Secret::Password));

    assert_eq!(
        state.forget(Secret::Password).as_deref(),
        Some("network-1")
    );
    state.apply_forget_result(
        Secret::Password,
        CredentialState::Error("remove failed".to_owned()),
    );
    assert!(
        state.can_forget(Secret::Password),
        "Error after refuse keeps mayBeStored-equivalent"
    );
    assert_eq!(
        state.credential_status(),
        "secure storage error; password is not saved"
    );
    assert_eq!(
        state.forget(Secret::Password).as_deref(),
        Some("network-1"),
        "retry is still offered"
    );
}

#[test]
fn emptying_a_held_secret_through_set_text_is_refused() {
    let mut state = ConnectState::from_stored(&[stored("network-1", "irc.libera.chat", "fred")]);
    state.set_text(ConnectField::ServerPassword, "hunter2");
    state.set_text(ConnectField::ServerPassword, "");
    assert_eq!(
        state.text(ConnectField::ServerPassword),
        "hunter2",
        "forget is the only clearing path"
    );
    assert!(state.secret_edit(Secret::Password).is_some());
}

#[test]
fn empty_to_empty_set_text_does_not_mark_edited_when_secret_is_available() {
    let mut profile = stored("network-1", "irc.libera.chat", "fred");
    profile.secret_saved = true;
    let mut state = ConnectState::from_stored(&[profile]);
    assert!(state.secret_edit(Secret::Password).is_none());
    assert_eq!(state.credential_status(), "password saved securely");

    state.set_text(ConnectField::ServerPassword, "");
    assert!(
        state.secret_edit(Secret::Password).is_none(),
        "empty→empty must not set password_edited"
    );
    assert_eq!(state.credential_status(), "password saved securely");
}

#[test]
fn forget_stops_sit_after_the_password_fields_when_visible() {
    let populated = vec!["network-1".to_string()];
    let mut state = ConnectState::from_stored(&[stored("network-1", "irc.libera.chat", "fred")]);
    state.set_text(ConnectField::ServerPassword, "hunter2");
    state.set_text(ConnectField::NickServPassword, "nickpw");
    let stops = state.stops(&populated, false);
    let password = stops
        .iter()
        .position(|stop| *stop == Stop::Field(ConnectField::ServerPassword))
        .expect("password");
    let nickserv = stops
        .iter()
        .position(|stop| *stop == Stop::Field(ConnectField::NickServPassword))
        .expect("nickserv");
    let forget_password = stops
        .iter()
        .position(|stop| *stop == Stop::ForgetPassword)
        .expect("forget password");
    let forget_nickserv = stops
        .iter()
        .position(|stop| *stop == Stop::ForgetNickServ)
        .expect("forget nickserv");
    assert!(password < nickserv);
    assert!(nickserv < forget_password);
    assert!(forget_password < forget_nickserv);
}

#[test]
fn from_stored_marks_saved_secrets_available() {
    let mut profile = stored("network-1", "irc.libera.chat", "fred");
    profile.secret_saved = true;
    profile.nickserv_saved = true;
    let state = ConnectState::from_stored(&[profile]);
    assert_eq!(state.secret_states().password, CredentialState::Available);
    assert_eq!(state.secret_states().nickserv, CredentialState::Available);
    assert!(state.can_forget(Secret::Password));
    assert!(state.can_forget(Secret::NickServ));
    assert_eq!(
        state.credential_status(),
        "password saved securely; NickServ saved securely"
    );
}
