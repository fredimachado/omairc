//! The Status ring and the bench path. Ports `tui/internal/controller/console_test.go`
//! (`TestConsoleText`), the capacity rule in `console.go`, and the `--lines` bench
//! channel the shell used to build from the mock world.

use chrono::{TimeZone, Utc};

use crate::controller::console::{StatusConsole, DEFAULT_CAPACITY};
use crate::irc::statusentry::{lifecycle, LogSeverity};

use super::fixture::{Fixture, OMARCHY};

fn line(network: &str, text: &str) -> crate::irc::statusentry::StatusEntry {
    let at = Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap();
    lifecycle(network, LogSeverity::Info, "test", text, at)
}

#[test]
fn the_console_keeps_each_network_in_arrival_order() {
    let mut console = StatusConsole::new(0);
    assert_eq!(console.text("omarchy"), Vec::<String>::new());
    console.append(line("omarchy", "one"));
    console.append(line("oftc", "elsewhere"));
    console.append(line("omarchy", "two"));
    assert_eq!(console.text("omarchy"), ["one", "two"]);
    assert_eq!(console.text("oftc"), ["elsewhere"]);
}

#[test]
fn the_oldest_line_drops_once_a_network_passes_its_cap() {
    let mut console = StatusConsole::new(2);
    for text in ["a", "b", "c"] {
        console.append(line("omarchy", text));
    }
    assert_eq!(console.text("omarchy"), ["b", "c"]);
    assert_eq!(StatusConsole::new(0).text(""), Vec::<String>::new());
    assert_eq!(DEFAULT_CAPACITY, 2000);
}

#[test]
fn a_line_with_no_network_is_dropped() {
    let mut console = StatusConsole::default();
    console.append(line("", "nowhere"));
    assert!(console.lines("").is_empty());
}

#[test]
fn clear_empties_one_network_only() {
    let mut console = StatusConsole::default();
    console.append(line("omarchy", "gone"));
    console.append(line("oftc", "kept"));
    console.clear("omarchy");
    assert!(console.lines("omarchy").is_empty());
    assert_eq!(console.text("oftc"), ["kept"]);
}

#[test]
fn the_bench_channel_is_injected_as_frames_and_selected() {
    let mut fixture = Fixture::seeded();
    assert!(fixture.controller.bench_channel(60));
    assert_eq!(fixture.controller.selected_network_id(), Some(OMARCHY));
    assert_eq!(fixture.controller.selected_target(), "#bench");
    assert!(fixture.controller.is_channel());
    assert!(fixture.controller.channel_joined());
    assert!(
        fixture.controller.messages().len() >= 40,
        "the bench lines land as rows: {}",
        fixture.controller.messages().len()
    );
}

#[test]
fn the_bench_channel_is_refused_without_a_demo() {
    let clock = std::rc::Rc::new(crate::session::clock::FakeClock::new(
        Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap(),
    ));
    let mut controller = crate::controller::controller::Controller::new(clock);
    assert!(!controller.bench_channel(10));
}

#[test]
fn the_connection_states_read_as_the_qt_status_console_words() {
    use crate::controller::console::{state_label, state_text};
    use crate::session::events::SessionState;

    let words = [
        (SessionState::Idle, "offline", "Disconnected"),
        (SessionState::Connecting, "connecting", "Connecting"),
        (SessionState::CapLs, "negotiating", "Capability discovery"),
        (
            SessionState::CapReq,
            "negotiating",
            "Requesting capabilities",
        ),
        (SessionState::Sasl, "negotiating", "Authenticating"),
        (SessionState::Registering, "registering", "Registering"),
        (SessionState::Registered, "registered", "Registered"),
        (SessionState::Closing, "closing", "Disconnecting"),
        (SessionState::Reconnecting, "reconnecting", "Reconnecting"),
        (SessionState::Failed, "failed", "Connection failed"),
    ];
    for (state, label, text) in words {
        assert_eq!(state_label(state), label, "{state:?}");
        assert_eq!(state_text(state), text, "{state:?}");
    }
}
