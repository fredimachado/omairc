//! The connection chrome the UI draws, read from the controller: the identity footer's
//! presence, the sidebar row's status line, and the Status subtitle. Each test drives an
//! intent (a slash line, a start, a disconnect), reads the frames the loopback wrote, and
//! reads back what the footer and the rows show. The sessions here are the demo's, or a
//! lab session the test drives by hand over a loopback for the states a demo never enters.
//! The colours are the `presenceMarkColor` values in `client/src/model/style.rs`.

use std::rc::Rc;

use chrono::{TimeZone, Utc};

use crate::controller::controller::Controller;
use crate::controller::transports::{NoTransports, LIVE_IRC_UNAVAILABLE};
use crate::irc::command::ComposerSurface;
use crate::model::presence::{footer_presence, SelfPresence};
use crate::model::status::{connection_line, network_mark_online};
use crate::model::theme::Theme;
use crate::model::{Color, Style};
use crate::session::clock::{Clock, FakeClock};
use crate::session::config::SessionConfig;
use crate::session::loopback::LoopbackTransport;

use super::fixture::{status_text, Fixture, OFTC, OMARCHY};

const CONVERSATION: ComposerSurface = ComposerSurface::Conversation;

fn style() -> Style {
    Style::new(Theme::defaults_dark(), 1.0)
}

fn hex(value: &str) -> Color {
    Color::from_hex(value).expect("a valid colour")
}

/// The mark colour the footer paints for a presence.
fn mark_of(presence: SelfPresence) -> Color {
    style().presence_color(presence.mark())
}

fn fake_clock() -> Rc<dyn Clock> {
    Rc::new(FakeClock::new(
        Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap(),
    ))
}

/// A controller with no demo and no live transport, as a first run starts.
fn bare_controller() -> Controller {
    Controller::with_parts(fake_clock(), Box::new(NoTransports))
}

/// A session the test drives by hand on a loopback, named `lab`. It starts connecting, and
/// the caller moves it on with the loopback's controls.
fn lab_session(controller: &mut Controller, reconnect: bool) -> Rc<LoopbackTransport> {
    let transport = Rc::new(LoopbackTransport::new());
    let mut config = SessionConfig::with_defaults("lab", "", "irc.example", "tester");
    config.reconnect_enabled = reconnect;
    let session = controller
        .manager
        .create(config, transport.clone(), controller.clock.clone())
        .expect("the lab session is new");
    session.start();
    controller.pump();
    transport
}

/// The rows of one network as (target, is a direct message), in sidebar order.
fn rows_of(controller: &Controller, network_id: &str) -> Vec<(String, bool)> {
    controller
        .conversations()
        .into_iter()
        .filter(|row| row.key.network_id == network_id)
        .map(|row| (row.target, row.direct))
        .collect()
}

#[test]
fn away_and_back_move_the_footer_through_amber_and_green() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);

    let available = footer_presence(&fixture.controller);
    assert_eq!(
        available,
        SelfPresence::Available,
        "a connected network is available"
    );
    assert_eq!(mark_of(available), hex("#69b978"));

    assert!(fixture.controller.submit(CONVERSATION, "/away brb"));
    assert!(fixture
        .frames(OMARCHY)
        .contains(&"AWAY :brb\r\n".to_owned()));
    let away = footer_presence(&fixture.controller);
    assert_eq!(
        away,
        SelfPresence::Away,
        "the 306 numeric marks the footer away"
    );
    assert_eq!(away.word(), "away");
    assert_eq!(mark_of(away), hex("#d6a552"), "amber while away");

    assert!(fixture.controller.submit(CONVERSATION, "/back"));
    assert!(fixture.frames(OMARCHY).contains(&"AWAY\r\n".to_owned()));
    let back = footer_presence(&fixture.controller);
    assert_eq!(back, SelfPresence::Available, "the 305 numeric clears it");
    assert_eq!(back.word(), "available");
    assert_eq!(mark_of(back), hex("#69b978"), "green again");
}

#[test]
fn a_stale_away_does_not_show_once_the_network_is_offline() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OFTC, "#lab");
    assert!(fixture.controller.submit(CONVERSATION, "/away brb"));
    assert_eq!(footer_presence(&fixture.controller), SelfPresence::Away);

    assert!(fixture.controller.disconnect(OFTC));
    assert_eq!(
        footer_presence(&fixture.controller),
        SelfPresence::Offline,
        "offline, even if the away fact remains"
    );
    assert_eq!(mark_of(SelfPresence::Offline), style().muted_color());
}

#[test]
fn disconnect_writes_quit_and_leaves_that_network_offline() {
    let mut fixture = Fixture::seeded();
    fixture.clear_frames(OFTC);
    assert!(fixture.controller.can_disconnect(OFTC));

    assert!(fixture.controller.disconnect(OFTC));
    assert_eq!(
        fixture.frames(OFTC),
        ["QUIT\r\n"],
        "one QUIT, and nothing else"
    );
    assert_eq!(fixture.controller.connection_status_for(OFTC), "Offline");
    assert_eq!(connection_line(&fixture.controller, OFTC), "Offline");
    assert!(
        !fixture.controller.can_disconnect(OFTC),
        "nothing left to stop"
    );

    assert_eq!(
        fixture.controller.connection_status_for(OMARCHY),
        "Connected",
        "the other network stays connected"
    );
    assert!(fixture.controller.can_disconnect(OMARCHY));
}

#[test]
fn a_start_after_disconnect_reseeds_the_network_without_duplicating_it() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OFTC, "#lab");
    let rows_before = rows_of(&fixture.controller, OFTC);
    let people_before = fixture.controller.people_count();
    let members_before = fixture.controller.members().len();
    let status_before = status_text(&fixture.controller, OFTC).len();

    assert!(fixture.controller.disconnect(OFTC));
    assert!(
        fixture.controller.start(OFTC),
        "a seeded network starts again"
    );

    assert_eq!(fixture.controller.connection_status_for(OFTC), "Connected");
    assert!(fixture.controller.can_disconnect(OFTC));
    assert_eq!(fixture.controller.last_error_for(OFTC), None);
    assert!(
        fixture
            .frames(OFTC)
            .iter()
            .any(|frame| frame.starts_with("CAP LS")),
        "the fresh session registers on its own loopback"
    );
    assert_eq!(
        rows_of(&fixture.controller, OFTC),
        rows_before,
        "the same rows, in the same order, with no second copy"
    );
    assert_eq!(fixture.controller.people_count(), people_before);
    assert_eq!(
        fixture.controller.members().len(),
        members_before,
        "NAMES does not double"
    );
    assert!(
        status_text(&fixture.controller, OFTC).len() > status_before,
        "the new session logs its handshake on Status"
    );
    assert_eq!(connection_line(&fixture.controller, OFTC), "Connected");
}

/// The nicks of the selected channel's members, sorted.
fn member_nicks(controller: &Controller) -> Vec<String> {
    let mut nicks: Vec<String> = controller
        .members()
        .into_iter()
        .map(|member| member.nick)
        .collect();
    nicks.sort();
    nicks
}

#[test]
fn a_restart_lists_the_late_joiners_the_transcript_brought_in() {
    // The seed brings some members into #omarchy with a JOIN in its transcript, not in its
    // NAMES. A reconnect lists the members as they are now, so the same people come back.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    let before = member_nicks(&fixture.controller);
    assert_eq!(fixture.controller.people_count(), 12, "the seeded count");

    assert!(fixture.controller.disconnect(OMARCHY));
    assert!(fixture.controller.start(OMARCHY));

    assert_eq!(
        member_nicks(&fixture.controller),
        before,
        "the same members"
    );
    assert_eq!(fixture.controller.people_count(), 12);
    assert!(fixture.controller.can_disconnect(OMARCHY));
}

#[test]
fn a_refused_start_is_the_row_status_and_the_status_line() {
    let mut fixture = Fixture::seeded();
    assert!(!fixture.controller.start("freenode"));

    assert_eq!(
        fixture.controller.last_error_for("freenode"),
        Some(LIVE_IRC_UNAVAILABLE)
    );
    assert_eq!(
        fixture.controller.connection_status_for("freenode"),
        "Offline"
    );
    let line = connection_line(&fixture.controller, "freenode");
    assert_eq!(
        line, LIVE_IRC_UNAVAILABLE,
        "the row's status line is the error"
    );
    assert!(!network_mark_online(&line), "an error mutes the mark");
    assert!(status_text(&fixture.controller, "freenode")
        .iter()
        .any(|text| text == LIVE_IRC_UNAVAILABLE));
    assert!(
        fixture
            .controller
            .network_order()
            .iter()
            .any(|id| id == "freenode"),
        "the network has a row to show its error on"
    );
    assert_eq!(fixture.controller.network_name("freenode"), "freenode");
    assert!(!fixture.controller.can_disconnect("freenode"));

    assert!(fixture.controller.forget_network("freenode"));
    assert!(!fixture
        .controller
        .network_order()
        .iter()
        .any(|id| id == "freenode"));
    assert_eq!(fixture.controller.last_error_for("freenode"), None);
}

#[test]
fn an_empty_network_id_starts_nothing_and_adds_no_row() {
    let mut controller = bare_controller();
    assert!(!controller.start(""));
    assert!(controller.network_order().is_empty());
    assert_eq!(controller.last_error_for(""), None);
}

#[test]
fn a_refused_first_start_brings_its_status_up_when_nothing_is_selected() {
    let mut controller = bare_controller();
    assert!(!controller.start("network-1"));
    assert_eq!(controller.focused_network_id(), "network-1");
    assert!(
        controller.shows_status(),
        "the window shows that network's Status"
    );
    assert_eq!(
        connection_line(&controller, "network-1"),
        LIVE_IRC_UNAVAILABLE
    );
    assert_eq!(footer_presence(&controller), SelfPresence::Offline);
}

#[test]
fn a_connecting_session_shows_connecting_and_can_be_disconnected() {
    let mut controller = bare_controller();
    let _transport = lab_session(&mut controller, false);

    assert_eq!(controller.connection_status_for("lab"), "Connecting");
    assert_eq!(connection_line(&controller, "lab"), "Connecting");
    assert!(!network_mark_online("Connecting"));
    assert!(
        controller.can_disconnect("lab"),
        "Disconnect shows while connecting"
    );

    assert!(controller.disconnect("lab"));
    assert_eq!(connection_line(&controller, "lab"), "Offline");
}

#[test]
fn a_dropped_session_that_retries_is_reconnecting_until_disconnect() {
    let mut controller = bare_controller();
    let transport = lab_session(&mut controller, true);
    transport.complete_connect();
    controller.pump();
    transport.remote_close();
    controller.pump();

    assert_eq!(controller.connection_status_for("lab"), "Reconnecting");
    assert!(
        controller.can_disconnect("lab"),
        "Disconnect shows while reconnecting"
    );
    assert_eq!(
        connection_line(&controller, "lab"),
        "Connection closed by the server",
        "the last error leads the row's status line, as Qt's liveStatus does"
    );

    assert!(controller.disconnect("lab"), "Disconnect stops the retry");
    assert_eq!(controller.connection_status_for("lab"), "Offline");
    assert!(!controller.can_disconnect("lab"));
}

#[test]
fn a_server_hang_up_without_retries_is_offline_with_the_reason() {
    let mut controller = bare_controller();
    let transport = lab_session(&mut controller, false);
    transport.complete_connect();
    controller.pump();
    transport.remote_close();
    controller.pump();

    assert_eq!(controller.connection_status_for("lab"), "Offline");
    assert_eq!(
        controller.last_error_for("lab"),
        Some("Connection closed by the server")
    );
    assert!(!controller.can_disconnect("lab"));
    assert!(!network_mark_online(&connection_line(&controller, "lab")));
}

#[test]
fn a_failed_connect_is_offline_with_its_error_and_no_disconnect() {
    let mut controller = bare_controller();
    let transport = lab_session(&mut controller, false);
    transport.fail_connect("no route to host");
    controller.pump();

    assert_eq!(controller.connection_status_for("lab"), "Offline");
    assert_eq!(controller.last_error_for("lab"), Some("no route to host"));
    assert_eq!(connection_line(&controller, "lab"), "no route to host");
    assert!(!controller.can_disconnect("lab"));
}

#[test]
fn a_connected_demo_network_reads_connected_on_its_row_and_status() {
    let fixture = Fixture::seeded();
    assert_eq!(connection_line(&fixture.controller, OMARCHY), "Connected");
    assert!(network_mark_online(&connection_line(
        &fixture.controller,
        OMARCHY
    )));
}
