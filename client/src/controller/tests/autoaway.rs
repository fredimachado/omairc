//! Auto-away and the manual away intents over the real demo. The clock is the
//! fixture's `FakeClock`: advancing it fires the idle and grace timers, and the
//! controller runs their expiries on the next tick.
//! `tui/internal/controller/autoawayruntime_test.go`. The per-nick `/autoaway` routing
//! to a conversation or Status is the dispatcher's, so those cases are not here.

use std::time::Duration;

use crate::irc::autoaway::parse_autoaway_argument;
use crate::irc::command::{spec_for, Verb};
use crate::irc::event::{Event, SelfAwayEvent};

use super::fixture::{status_text, Fixture, OFTC, OMARCHY};

/// Runs one `/autoaway` argument and returns the reply text.
fn autoaway(fixture: &mut Fixture, argument: &str) -> String {
    fixture
        .controller
        .autoaway_command(parse_autoaway_argument(argument))
}

/// The `/autoaway` usage line, the command catalog's row.
fn usage() -> &'static str {
    spec_for(Verb::Autoaway).usage
}

/// Fires the idle timer for `seconds` of idleness, then the grace timer.
fn idle_then_grace(fixture: &mut Fixture, idle_seconds: u64, grace_seconds: u64) {
    fixture.clock.advance(Duration::from_secs(idle_seconds));
    fixture.controller.tick();
    fixture.clock.advance(Duration::from_secs(grace_seconds));
    fixture.controller.tick();
}

/// The last Status line of `network_id`.
fn last_status(fixture: &Fixture, network_id: &str) -> String {
    status_text(&fixture.controller, network_id)
        .last()
        .cloned()
        .unwrap_or_default()
}

#[test]
fn the_query_and_each_change_return_their_confirmation() {
    let mut fixture = Fixture::seeded();
    assert_eq!(autoaway(&mut fixture, ""), "Auto-away off");

    assert_eq!(autoaway(&mut fixture, "5m"), "Auto-away 5 minutes");
    assert!(fixture.controller.autoaway.idle_scheduled());
    assert_eq!(
        fixture.controller.autoaway.idle_interval,
        Duration::from_secs(300),
        "5m arms the idle timer for five minutes"
    );

    assert_eq!(
        autoaway(&mut fixture, "reason lunch"),
        "Auto-away 5 minutes, reason: lunch"
    );
    assert_eq!(
        autoaway(&mut fixture, "on"),
        "Auto-away 5 minutes, reason: lunch"
    );

    assert_eq!(
        autoaway(&mut fixture, "off"),
        "Auto-away off, reason: lunch"
    );
    assert!(!fixture.controller.autoaway.idle_scheduled(), "off stops the idle timer");

    assert_eq!(autoaway(&mut fixture, "reason"), "Auto-away off");
    assert_eq!(autoaway(&mut fixture, "bogus"), usage());
}

#[test]
fn on_without_a_timeout_gives_the_usage_line() {
    let mut fixture = Fixture::seeded();
    assert_eq!(autoaway(&mut fixture, "on"), usage());
}

#[test]
fn idle_then_grace_trips_away_through_the_clock() {
    let mut fixture = Fixture::seeded();
    autoaway(&mut fixture, "5m");
    fixture.clear_frames(OMARCHY);
    fixture.clear_frames(OFTC);

    fixture.clock.advance(Duration::from_secs(300));
    fixture.controller.tick();
    assert!(!fixture.controller.autoaway.idle_scheduled(), "the idle timer fired");
    assert!(fixture.controller.autoaway.grace_scheduled(), "grace opened");
    assert_eq!(
        fixture.controller.autoaway.grace_interval,
        Duration::from_secs(10),
        "300 / 20 clamps to the 10 second ceiling"
    );
    assert!(!fixture.controller.autoaway.tripped(), "not tripped during grace");

    fixture.clock.advance(Duration::from_secs(10));
    fixture.controller.tick();
    assert!(fixture.controller.autoaway.tripped());
    assert!(fixture.frames(OMARCHY).iter().any(|frame| frame == "AWAY :\r\n"));
    assert_eq!(last_status(&fixture, OMARCHY), "Auto-away triggered.");
}

#[test]
fn a_short_timeout_uses_the_five_second_grace_floor() {
    let mut fixture = Fixture::seeded();
    autoaway(&mut fixture, "30s");
    fixture.clock.advance(Duration::from_secs(30));
    fixture.controller.tick();
    assert_eq!(
        fixture.controller.autoaway.grace_interval,
        Duration::from_secs(5),
        "30 / 20 = 1, clamped to the 5 second floor"
    );
    assert!(fixture.controller.autoaway.grace_scheduled());
}

#[test]
fn a_manually_away_network_is_left_alone_when_auto_away_trips() {
    let mut fixture = Fixture::seeded();
    fixture.controller.note_manual_away(OMARCHY);
    autoaway(&mut fixture, "5m");
    fixture.clear_frames(OMARCHY);
    fixture.clear_frames(OFTC);

    idle_then_grace(&mut fixture, 300, 10);

    assert!(!fixture.frames(OMARCHY).iter().any(|f| f.starts_with("AWAY")));
    assert!(
        fixture.frames(OFTC).iter().any(|frame| frame == "AWAY :\r\n"),
        "the other network is marked away"
    );
    assert_eq!(
        status_text(&fixture.controller, OMARCHY)
            .iter()
            .filter(|line| line.starts_with("Auto-away triggered"))
            .count(),
        0,
        "no trip line for the manual network"
    );
}

#[test]
fn an_already_away_network_is_left_alone_when_auto_away_trips() {
    let mut fixture = Fixture::seeded();
    fixture.controller.apply(Event::SelfAway(SelfAwayEvent {
        network_id: OFTC.to_owned(),
        away: true,
    }));
    autoaway(&mut fixture, "5m");
    fixture.clear_frames(OFTC);

    idle_then_grace(&mut fixture, 300, 10);

    assert!(
        !fixture.frames(OFTC).iter().any(|f| f.starts_with("AWAY")),
        "a network already away is not marked again"
    );
}

#[test]
fn local_activity_clears_the_trip_and_rearms_the_idle_timer() {
    let mut fixture = Fixture::seeded();
    autoaway(&mut fixture, "5m");
    idle_then_grace(&mut fixture, 300, 10);
    assert!(fixture.controller.autoaway.tripped());
    fixture.clear_frames(OMARCHY);

    fixture.controller.note_local_activity();

    assert!(!fixture.controller.autoaway.tripped(), "activity clears the trip");
    assert!(
        fixture.frames(OMARCHY).iter().any(|frame| frame == "AWAY\r\n"),
        "the clear is written"
    );
    assert!(
        !fixture.controller.autoaway.unaway_was_sent(OMARCHY),
        "the demo's 305 lifts the unaway latch"
    );
    assert_eq!(last_status(&fixture, OMARCHY), "Auto-away cleared — back online.");
    assert!(fixture.controller.autoaway.idle_scheduled(), "the idle timer re-arms");
    assert_eq!(
        fixture.controller.autoaway.idle_interval,
        Duration::from_secs(300)
    );
}

#[test]
fn a_reason_change_while_tripped_sends_away_again_with_the_new_reason() {
    let mut fixture = Fixture::seeded();
    autoaway(&mut fixture, "5m");
    idle_then_grace(&mut fixture, 300, 10);
    fixture.clear_frames(OMARCHY);

    autoaway(&mut fixture, "10m lunch");
    assert!(fixture.frames(OMARCHY).iter().any(|frame| frame == "AWAY :lunch\r\n"));
}

#[test]
fn a_reconnect_while_tripped_marks_the_new_session_away_on_registration() {
    let mut fixture = Fixture::seeded();
    autoaway(&mut fixture, "5m");
    idle_then_grace(&mut fixture, 300, 10);
    assert!(fixture.controller.autoaway.tripped());

    assert!(fixture.controller.disconnect(OMARCHY));
    assert!(fixture.controller.start(OMARCHY));

    assert!(
        fixture.frames(OMARCHY).iter().any(|frame| frame == "AWAY :\r\n"),
        "the registered session is marked away again"
    );
}

#[test]
fn a_one_shot_reason_lasts_until_the_last_auto_away_network_comes_back() {
    let mut fixture = Fixture::seeded();
    autoaway(&mut fixture, "5m lunch");
    idle_then_grace(&mut fixture, 300, 10);
    assert_eq!(
        autoaway(&mut fixture, ""),
        "Auto-away in 5 minutes: lunch (this time only)"
    );

    fixture.controller.note_away_cleared(OMARCHY);
    assert_eq!(
        autoaway(&mut fixture, ""),
        "Auto-away in 5 minutes: lunch (this time only)",
        "one network still away keeps the reason"
    );

    fixture.controller.note_away_cleared(OFTC);
    assert_eq!(autoaway(&mut fixture, ""), "Auto-away 5 minutes");
}

#[test]
fn forgetting_a_network_drops_its_manual_mark() {
    let mut fixture = Fixture::seeded();
    fixture.controller.note_manual_away(OMARCHY);
    fixture.controller.autoaway_forget_network(OMARCHY);
    autoaway(&mut fixture, "5m");
    fixture.clear_frames(OMARCHY);

    idle_then_grace(&mut fixture, 300, 10);
    assert!(
        fixture.frames(OMARCHY).iter().any(|frame| frame == "AWAY :\r\n"),
        "without the manual mark the network trips"
    );
}

#[test]
fn away_with_a_reason_is_manual_and_back_clears_it() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);

    assert!(fixture.controller.away(OMARCHY, "brb"));
    assert!(fixture.frames(OMARCHY).iter().any(|frame| frame == "AWAY :brb\r\n"));
    assert!(fixture.controller.self_away(), "the 306 reply marks the user away");
    assert!(fixture.controller.autoaway.is_manual_away(OMARCHY));

    assert!(fixture.controller.back(OMARCHY));
    assert!(fixture.frames(OMARCHY).iter().any(|frame| frame == "AWAY\r\n"));
    assert!(!fixture.controller.self_away(), "the 305 reply clears it");
    assert!(!fixture.controller.autoaway.is_manual_away(OMARCHY));
}

#[test]
fn a_chat_line_while_away_brings_the_user_back_once() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.controller.away(OMARCHY, "brb");
    fixture.clear_frames(OMARCHY);

    fixture.controller.note_chat_sent(OMARCHY);
    assert!(fixture.frames(OMARCHY).iter().any(|frame| frame == "AWAY\r\n"));
    assert!(!fixture.controller.self_away());
    assert!(
        !fixture.controller.autoaway.unaway_was_sent(OMARCHY),
        "the 305 that answers the unaway lifts the latch"
    );

    // Chatting again while not away sends nothing.
    fixture.clear_frames(OMARCHY);
    fixture.controller.note_chat_sent(OMARCHY);
    assert!(fixture.frames(OMARCHY).is_empty(), "not away, so no unaway");

    // A fresh away (a 306) lifts the latch, so the next chat brings the user back.
    fixture.controller.away(OMARCHY, "lunch");
    fixture.clear_frames(OMARCHY);
    fixture.controller.note_chat_sent(OMARCHY);
    assert!(fixture.frames(OMARCHY).iter().any(|frame| frame == "AWAY\r\n"));
}
