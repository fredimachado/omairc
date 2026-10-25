//! The send intent over the real demo: the frame that reaches the loopback, the
//! echo that comes back, the local echo when the server does not echo, and the
//! refusals. Ports the Go `TestEchoMessage*` and `TestSelectedPrivmsg*` cases in
//! `tui/internal/controller/controller_test.go`.

use crate::irc::capability::IrcCapability;

use super::fixture::{bodies, status_text, Fixture, OFTC, OMARCHY};

#[test]
fn a_send_writes_one_privmsg_frame_and_the_server_echo_is_the_row() {
    let mut fixture = Fixture::seeded();
    fixture.clear_frames(OMARCHY);
    assert!(fixture
        .controller
        .send_to_target(OMARCHY, "#omarchy", "hello there"));

    assert_eq!(
        fixture.frames(OMARCHY),
        ["PRIVMSG #omarchy :hello there\r\n"]
    );
    let rows: Vec<_> = fixture
        .controller
        .messages()
        .iter()
        .filter(|message| message.body == "hello there")
        .collect();
    assert_eq!(rows.len(), 1, "echo-message replaces the local echo");
    assert_eq!(rows[0].author, "fred");
    assert_eq!(fixture.controller.last_error_for(OMARCHY), None);
}

#[test]
fn a_channel_message_stays_out_of_status_as_qt_keeps_it() {
    let mut fixture = Fixture::seeded();
    assert!(fixture
        .controller
        .send_to_target(OMARCHY, "#omarchy", "status check"));
    assert!(
        !status_text(&fixture.controller, OMARCHY)
            .iter()
            .any(|line| line.contains("status check")),
        "PRIVMSG is not a Status line"
    );
}

#[test]
fn a_kept_outgoing_line_reaches_status_as_the_wire_line() {
    let mut fixture = Fixture::seeded();
    let session = fixture.controller.session(OMARCHY).expect("session");
    assert!(session.send_raw("CAP REQ :away-notify"));
    fixture.controller.tick();
    assert!(status_text(&fixture.controller, OMARCHY)
        .iter()
        .any(|line| line.contains("away-notify")));
}

#[test]
fn without_echo_message_the_controller_echoes_the_send_locally() {
    let mut fixture = Fixture::seeded();
    // Stop the demo's own echo, and drop the capability, so the controller must
    // draw the row itself, as Qt does on a server that does not echo.
    fixture
        .controller
        .demo()
        .and_then(|demo| demo.transport(OMARCHY))
        .expect("the omarchy loopback")
        .clear_frame_written_hook();
    let mut caps = fixture
        .controller
        .capabilities
        .get(OMARCHY)
        .copied()
        .expect("omarchy negotiated caps");
    caps.remove(IrcCapability::EchoMessage);
    fixture.controller.capabilities.insert(OMARCHY.to_owned(), caps);

    assert!(fixture
        .controller
        .send_to_target(OMARCHY, "#omarchy", "local only"));
    let rows: Vec<_> = fixture
        .controller
        .messages()
        .iter()
        .filter(|message| message.body == "local only")
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].author, "fred");
}

#[test]
fn a_send_with_missing_input_is_refused_and_writes_nothing() {
    const MISSING: &str = "Missing network, target, or text";
    let mut fixture = Fixture::seeded();
    fixture.clear_frames(OMARCHY);
    assert!(!fixture.controller.send_to_target(OMARCHY, "#omarchy", ""));
    assert_eq!(fixture.controller.last_error_for(OMARCHY), Some(MISSING));
    assert!(!fixture.controller.send_to_target(OMARCHY, "", "hi"));
    assert!(!fixture.controller.send_to_target("", "#omarchy", "hi"));
    assert_eq!(fixture.controller.last_error_for(""), Some(MISSING));
    assert!(fixture.frames(OMARCHY).is_empty());
}

#[test]
fn a_send_to_an_unknown_network_names_the_problem() {
    let mut fixture = Fixture::seeded();
    assert!(!fixture.controller.send_to_target("ghost", "#x", "hi"));
    assert_eq!(
        fixture.controller.last_error_for("ghost"),
        Some("That network is not configured")
    );
    assert!(status_text(&fixture.controller, "ghost").is_empty());
}

#[test]
fn a_send_on_a_disconnected_network_is_refused_as_not_connected() {
    let mut fixture = Fixture::seeded();
    assert!(fixture.controller.disconnect(OFTC));
    fixture.clear_frames(OFTC);
    assert!(!fixture.controller.send_to_target(OFTC, "#lab", "hi"));
    assert_eq!(fixture.controller.last_error_for(OFTC), Some("Not connected"));
    assert!(fixture.frames(OFTC).is_empty());
}

#[test]
fn a_refused_send_keeps_the_last_error_until_the_next_good_send() {
    let mut fixture = Fixture::seeded();
    assert!(!fixture.controller.send_to_target("ghost", "#x", "hi"));
    assert!(fixture
        .controller
        .send_to_target(OMARCHY, "#omarchy", "back on track"));
    assert_eq!(fixture.controller.last_error_for(OMARCHY), None);
    assert!(bodies(&fixture.controller)
        .iter()
        .any(|body| body == "back on track"));
}

#[test]
fn a_direct_message_send_reaches_the_peer_and_echoes() {
    let mut fixture = Fixture::seeded();
    assert!(fixture.controller.open_direct("dax"));
    fixture.clear_frames(OMARCHY);
    assert!(fixture.controller.send_to_target(OMARCHY, "dax", "ping dax"));
    assert_eq!(fixture.frames(OMARCHY), ["PRIVMSG dax :ping dax\r\n"]);
    assert!(bodies(&fixture.controller)
        .iter()
        .any(|body| body == "ping dax"));
}

#[test]
fn outgoing_secrets_never_reach_status() {
    let mut fixture = Fixture::seeded();
    // PASS is the one outgoing verb Status keeps, so it is the one that could leak.
    // The raw write is the session's own; the Outbound event it raises is what the
    // controller must mask before Status shows it.
    let session = fixture.controller.session(OMARCHY).expect("session");
    assert!(session.send_raw("PASS hunter2-secret"));
    fixture.controller.tick();
    let lines = status_text(&fixture.controller, OMARCHY);
    assert!(
        lines.iter().any(|line| line.starts_with("PASS")),
        "PASS is kept, masked: {lines:?}"
    );
    assert!(
        !lines.iter().any(|line| line.contains("hunter2-secret")),
        "a password must not reach Status: {lines:?}"
    );
}
