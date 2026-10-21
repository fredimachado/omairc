//! Port of `tui/internal/session/session_test.go`: registration, capability
//! negotiation, and the upload credential.

use super::fixture::{assert_frames, test_config, Fixture};
use crate::irc::capability::IrcCapability;
use crate::session::events::SessionState;

#[test]
fn registers_and_autojoins() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    assert_eq!(fx.session.state(), SessionState::CapLs);
    assert_frames(&fx.frames(), &["CAP LS 302\r\n"]);

    fx.inject(":server CAP omairc LS :multi-prefix chghost cap-notify echo-message\r\n");
    assert_eq!(fx.session.state(), SessionState::Registering);
    assert_frames(
        &fx.written_since(1),
        &[
            "CAP REQ :multi-prefix chghost cap-notify echo-message\r\n",
            "NICK omairc\r\n",
            "USER omairc 8 * :Omairc User\r\n",
        ],
    );
    assert!(fx.session.capabilities().is_empty(), "capabilities must be empty before the ACK");

    fx.inject(":server CAP omairc ACK :multi-prefix chghost cap-notify echo-message\r\n");
    assert!(fx.wrote("CAP END\r\n"), "CAP END must follow the ACK");
    let enabled = fx.session.capabilities();
    for capability in [
        IrcCapability::MultiPrefix,
        IrcCapability::Chghost,
        IrcCapability::CapNotify,
        IrcCapability::EchoMessage,
    ] {
        assert!(enabled.contains(capability), "capabilities missing {capability:?}");
    }

    fx.inject(":server 001 omairc :Welcome\r\n");
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert_eq!(fx.session.nick(), "omairc");
    assert_eq!(fx.registered_count(), 1);
    assert_frames(&fx.written_since(5), &["JOIN #omarchy\r\n", "JOIN &local\r\n"]);
}

#[test]
fn negotiates_presence_capabilities() {
    let mut config = test_config();
    config.password = "secret".to_owned();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS * :sasl=PLAIN away-notify\r\n\
         :server CAP omairc LS :batch draft/metadata-2 multi-prefix\r\n",
    );
    assert_frames(
        &fx.frames(),
        &[
            "CAP LS 302\r\n",
            "CAP REQ :sasl\r\n",
            "CAP REQ :away-notify batch draft/metadata-2 multi-prefix\r\n",
            "NICK omairc\r\n",
            "USER omairc 8 * :Omairc User\r\n",
        ],
    );

    fx.inject(":server CAP omairc ACK :away-notify batch draft/metadata-2 multi-prefix\r\n");
    assert!(!fx.wrote("CAP END\r\n"), "CAP END must wait for the SASL ACK");

    fx.inject(
        ":server CAP omairc ACK :sasl\r\n\
         AUTHENTICATE +\r\n\
         :server 903 omairc :SASL successful\r\n",
    );
    assert!(fx.wrote("CAP END\r\n"), "CAP END must follow the SASL 903");

    fx.inject(":server 001 omairc :Welcome\r\n");
    assert!(
        fx.wrote("METADATA * SUB status avatar bot display-name pronouns homepage color\r\n"),
        "member metadata subscription missing"
    );

    fx.inject(":server 366 omairc #omarchy :End of /NAMES\r\n");
    assert!(fx.wrote("WHO #omarchy\r\n"), "away-notify probe missing");

    let enabled = fx.session.capabilities();
    for capability in [
        IrcCapability::AwayNotify,
        IrcCapability::Batch,
        IrcCapability::MemberMetadata,
    ] {
        assert!(enabled.contains(capability), "capabilities missing {capability:?}");
    }
    assert!(!fx.capability_changes().is_empty(), "capabilitiesChanged must have fired");
}

#[test]
fn negotiates_message_tags_on_own_line() {
    let mut config = test_config();
    config.password = "secret".to_owned();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :sasl=PLAIN message-tags away-notify batch draft/metadata-2\r\n",
    );
    assert_frames(
        &fx.frames(),
        &[
            "CAP LS 302\r\n",
            "CAP REQ :sasl\r\n",
            "CAP REQ :message-tags\r\n",
            "CAP REQ :away-notify batch draft/metadata-2\r\n",
            "NICK omairc\r\n",
            "USER omairc 8 * :Omairc User\r\n",
        ],
    );
}

#[test]
fn refused_presence_capabilities_stay_off_without_failing() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :away-notify batch draft/metadata-2\r\n");
    assert_frames(
        &fx.written_since(1),
        &[
            "CAP REQ :away-notify batch draft/metadata-2\r\n",
            "NICK omairc\r\n",
            "USER omairc 8 * :Omairc User\r\n",
        ],
    );
    fx.inject(":server CAP omairc NAK :away-notify batch draft/metadata-2\r\n");
    assert!(fx.wrote("CAP END\r\n"), "CAP END must follow the NAK");
    assert!(fx.errors().is_empty(), "a refused capability is not a failure");
    assert!(fx.session.capabilities().is_empty(), "refused capabilities must stay off");

    fx.inject(":server 001 omairc :Welcome\r\n:server 366 omairc #omarchy :End of /NAMES\r\n");
    assert_eq!(fx.session.state(), SessionState::Registered);
    assert!(
        !fx.wrote("METADATA * SUB status avatar bot display-name pronouns homepage color\r\n"),
        "metadata subscription must not be sent"
    );
    assert!(!fx.wrote("WHO #omarchy\r\n"), "away probe must not be sent");
}

#[test]
fn refused_message_tags_stay_off_without_failing() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :message-tags away-notify\r\n");
    assert_frames(
        &fx.written_since(1),
        &[
            "CAP REQ :message-tags\r\n",
            "CAP REQ :away-notify\r\n",
            "NICK omairc\r\n",
            "USER omairc 8 * :Omairc User\r\n",
        ],
    );
    fx.inject(":server CAP omairc NAK :message-tags\r\n");
    assert!(!fx.wrote("CAP END\r\n"), "CAP END must wait for the outstanding away-notify");
    assert!(fx.errors().is_empty());
    assert!(!fx.session.capabilities().contains(IrcCapability::MessageTags));

    fx.inject(":server CAP omairc ACK :away-notify\r\n");
    assert!(fx.wrote("CAP END\r\n"), "CAP END must follow the away-notify ACK");
    fx.inject(":server 001 omairc :Welcome\r\n");
    assert!(fx.session.capabilities().contains(IrcCapability::AwayNotify));
    assert!(!fx.session.capabilities().contains(IrcCapability::MessageTags));
}

#[test]
fn unanswered_presence_request_still_registers() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :away-notify\r\n");
    assert!(fx.clock.pending() > 0, "capability timer must be pending");
    assert!(!fx.wrote("CAP END\r\n"), "CAP END must wait for the ACK");

    fx.clock.advance(std::time::Duration::from_millis(10000));
    assert!(fx.wrote("CAP END\r\n"), "CAP END must follow the capability timeout");
    assert!(fx.session.capabilities().is_empty(), "unanswered capabilities must stay off");
}

#[test]
fn late_cap_ack_after_timeout_is_ignored() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :away-notify\r\n");
    fx.clock.advance(std::time::Duration::from_millis(10000));
    assert!(fx.session.capabilities().is_empty());
    fx.inject(":server CAP omairc ACK :away-notify\r\n:server 001 omairc :Welcome\r\n");
    assert!(
        !fx.session.capabilities().contains(IrcCapability::AwayNotify),
        "a late ACK must not re-enable away-notify"
    );
}

#[test]
fn late_cap_nak_after_timeout_is_ignored() {
    let mut config = test_config();
    config.password = "s3cret".to_owned();
    let fx = Fixture::new(config);
    fx.connect_tls();
    fx.inject(":server CAP omairc LS :sasl\r\n");
    fx.clock.advance(std::time::Duration::from_millis(10000));
    fx.inject(":server CAP omairc NAK :sasl\r\n");
    assert!(fx.errors().is_empty(), "a late NAK must not fail the session");
}

#[test]
fn cap_nak_before_cap_list_is_ignored() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server CAP omairc NAK :away-notify\r\n");
    assert!(!fx.wrote("CAP END\r\n"), "a NAK before CAP LS must not register");
    fx.inject(":server CAP omairc LS :away-notify\r\n:server CAP omairc ACK :away-notify\r\n");
    assert!(fx.wrote("CAP END\r\n"), "CAP END must follow the real ACK");
    assert!(fx.session.capabilities().contains(IrcCapability::AwayNotify));
}

#[test]
fn registers_when_cap_is_unsupported() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(":server 421 omairc CAP :Unknown command\r\n");
    assert_frames(
        &fx.written_since(1),
        &["NICK omairc\r\n", "USER omairc 8 * :Omairc User\r\n"],
    );
    fx.inject(":server 001 omairc :Welcome\r\n");
    assert_eq!(fx.session.state(), SessionState::Registered);
}

#[test]
fn withdrawn_capability_is_published() {
    let fx = Fixture::new(test_config());
    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :away-notify\r\n\
         :server CAP omairc ACK :away-notify\r\n\
         :server 001 omairc :Welcome\r\n\
         :server 366 omairc #omarchy :End of /NAMES\r\n",
    );
    assert!(fx.wrote("WHO #omarchy\r\n"), "away probe missing");
    assert!(fx.session.capabilities().contains(IrcCapability::AwayNotify));

    let changes_before = fx.capability_changes().len();
    fx.inject(
        ":server CAP omairc DEL :away-notify\r\n\
         :server 366 omairc #desktop :End of /NAMES\r\n",
    );
    assert_eq!(fx.capability_changes().len() - changes_before, 1, "one capabilitiesChanged after DEL");
    assert!(!fx.session.capabilities().contains(IrcCapability::AwayNotify));
    assert!(!fx.wrote("WHO #desktop\r\n"), "withdrawn away-notify must not probe");
}

#[test]
fn upload_credential_uses_the_server_password_until_sasl() {
    let mut config = test_config();
    config.password = "server-secret".to_owned();
    config.nickserv_password = "nick-secret".to_owned();
    config.sasl_account = "acct".to_owned();
    let fx = Fixture::new(config);

    let (user, secret, _, _) = fx.session.upload_credential();
    assert_eq!((user.as_str(), secret.as_str()), ("acct", "server-secret"));

    fx.connect_tls();
    fx.inject(
        ":server CAP omairc LS :sasl=PLAIN\r\n\
         :server CAP omairc ACK :sasl\r\n\
         AUTHENTICATE +\r\n\
         :server 903 omairc :SASL successful\r\n",
    );
    let (user, secret, _, _) = fx.session.upload_credential();
    assert_eq!((user.as_str(), secret.as_str()), ("acct", "nick-secret"));
}
