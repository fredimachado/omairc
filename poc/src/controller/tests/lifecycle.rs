//! Start and disconnect over the real demo, and the start the build refuses. Ports
//! `tui/internal/controller/disconnect_test.go` (`TestDisconnectStopsLiveSession`),
//! `controller_test.go` (`TestDisconnectLeavesOtherNetworkLive`,
//! `TestTwoSessionsStartTogether`, `TestQuitWhileReconnecting`) and the
//! transport-seam rule in `poc/src/controller/transports.rs`. The connection
//! states a session writes to Status follow the same demo, as `IrcStatusConsole`
//! records them in Qt.

use chrono::{TimeZone, Utc};

use crate::controller::transports::LIVE_IRC_UNAVAILABLE;
use crate::irc::statusentry::{LogSeverity, StatusEntry};

use super::fixture::{status_text, Fixture, OFTC, OMARCHY};

#[test]
fn disconnect_stops_one_network_and_leaves_the_other_live() {
    let mut fixture = Fixture::seeded();
    assert!(fixture.controller.disconnect(OFTC));
    assert_eq!(fixture.controller.connection_status_for(OFTC), "Offline");
    assert_eq!(fixture.controller.connection_status_for(OMARCHY), "Connected");
    assert!(!fixture.controller.disconnect(OFTC), "already idle");
}

#[test]
fn a_disconnected_network_starts_again_on_a_fresh_loopback() {
    let mut fixture = Fixture::seeded();
    assert!(fixture.controller.disconnect(OFTC));
    assert!(fixture.controller.start(OFTC));
    assert_eq!(fixture.controller.connection_status_for(OFTC), "Connected");
    assert_eq!(fixture.controller.connection_status_for(OMARCHY), "Connected");
    assert_eq!(fixture.controller.last_error_for(OFTC), None);
    assert!(fixture
        .controller
        .conversations()
        .iter()
        .any(|row| row.key.network_id == OFTC && row.target == "rio"));
}

#[test]
fn starting_a_live_network_is_a_no_op() {
    let mut fixture = Fixture::seeded();
    fixture.clear_frames(OMARCHY);
    assert!(fixture.controller.start(OMARCHY));
    assert_eq!(fixture.controller.connection_status_for(OMARCHY), "Connected");
    assert!(
        fixture.frames(OMARCHY).is_empty(),
        "a live session does not register again"
    );
}

#[test]
fn a_start_the_build_cannot_honour_is_refused_on_status() {
    let mut fixture = Fixture::seeded();
    assert!(!fixture.controller.start("freenode"));
    assert_eq!(
        fixture.controller.last_error_for("freenode"),
        Some(LIVE_IRC_UNAVAILABLE)
    );
    assert_eq!(fixture.controller.connection_status_for("freenode"), "Offline");
    assert!(status_text(&fixture.controller, "freenode")
        .iter()
        .any(|line| line == LIVE_IRC_UNAVAILABLE));
}

/// A factory that names its own reason, to show the seam takes the reason from it.
struct Refusing(&'static str);

impl crate::controller::transports::TransportFactory for Refusing {
    fn open(
        &self,
        _network_id: &str,
    ) -> Result<std::rc::Rc<dyn crate::session::transport::Transport>, String> {
        Err(self.0.to_owned())
    }
}

#[test]
fn the_transport_factory_supplies_the_reason_a_live_start_is_refused() {
    use crate::controller::controller::Controller;
    use crate::desktop::notifier::NullNotifier;
    use crate::session::clock::FakeClock;

    let clock = std::rc::Rc::new(FakeClock::new(
        Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap(),
    ));
    let mut controller = Controller::with_parts(
        clock,
        Box::new(NullNotifier),
        Box::new(Refusing("no route to the socket layer")),
    );
    assert!(!controller.start("freenode"));
    assert_eq!(
        controller.last_error_for("freenode"),
        Some("no route to the socket layer")
    );
}

#[test]
fn a_disconnect_of_an_unknown_network_is_a_no_op() {
    let mut fixture = Fixture::seeded();
    assert!(!fixture.controller.disconnect("ghost"));
    assert_eq!(fixture.controller.connection_status_for("ghost"), "Offline");
}

#[test]
fn a_disconnect_queues_the_footer_surface() {
    let mut fixture = Fixture::seeded();
    fixture.controller.take_notifications();
    assert!(fixture.controller.disconnect(OFTC));
    let surfaces = fixture.controller.take_notifications();
    assert!(surfaces.contains(&crate::irc::viewnotify::ViewNotify::Status));
}

/// The Status lines of a network, oldest first.
fn status_entries(fixture: &Fixture, network_id: &str) -> Vec<StatusEntry> {
    fixture
        .controller
        .status_lines(network_id)
        .into_iter()
        .cloned()
        .collect()
}

/// The index of the first Status line with this label and text.
fn index_of(lines: &[StatusEntry], label: &str, text: &str) -> usize {
    lines
        .iter()
        .position(|entry| entry.label == label && entry.text == text)
        .unwrap_or_else(|| panic!("no {label} line reads {text:?} in {lines:?}"))
}

#[test]
fn attaching_the_demo_writes_its_connection_states_to_status() {
    let fixture = Fixture::seeded();
    let lines = status_entries(&fixture, OMARCHY);
    let registering = index_of(&lines, "registering", "Registering");
    let welcome = index_of(&lines, "001", "Welcome to the demo network");
    let registered = index_of(&lines, "registered", "Registered");
    let named = index_of(&lines, "registered", "Registered as fred");
    assert!(registering < welcome, "registering comes before 001");
    assert!(
        welcome < registered && registered < named,
        "the welcome comes before the registered lines"
    );
    assert_eq!(lines[registering].severity, LogSeverity::Info);
    assert_eq!(lines[named].severity, LogSeverity::Info);
}

#[test]
fn a_server_error_writes_the_error_and_the_failed_state_as_alerts() {
    let mut fixture = Fixture::seeded();
    fixture.deliver(OMARCHY, ":server ERROR :Closing link: test");
    let lines = status_entries(&fixture, OMARCHY);
    let error = index_of(&lines, "error", "Closing link: test");
    let failed = index_of(&lines, "failed", "Connection failed");
    assert!(error < failed, "the error comes before the failed state");
    assert_eq!(lines[error].severity, LogSeverity::Alert);
    assert_eq!(lines[failed].severity, LogSeverity::Alert);
    assert_eq!(
        fixture.controller.last_error_for(OMARCHY),
        Some("Closing link: test")
    );
}

#[test]
fn disconnecting_then_starting_a_network_writes_each_state_to_status() {
    let mut fixture = Fixture::seeded();
    let before = status_entries(&fixture, OFTC).len();
    assert!(fixture.controller.disconnect(OFTC));
    let lines = status_entries(&fixture, OFTC);
    let disconnected = &lines[before..];
    let closing = index_of(disconnected, "closing", "Disconnecting");
    let offline = index_of(disconnected, "offline", "Disconnected");
    assert!(closing < offline, "closing comes before offline");

    let before = lines.len();
    assert!(fixture.controller.start(OFTC));
    let lines = status_entries(&fixture, OFTC);
    let started = &lines[before..];
    let connecting = index_of(started, "connecting", "Connecting");
    let registering = index_of(started, "registering", "Registering");
    let welcome = index_of(started, "001", "Welcome to the demo OFTC network");
    let named = index_of(started, "registered", "Registered as oak");
    assert!(
        connecting < registering && registering < welcome && welcome < named,
        "a restart walks the states in order: {started:?}"
    );
}
