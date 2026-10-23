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
