//! The keyboard walk through the sheet's tabs and stops, and where focus lands.

use super::*;

#[test]
fn ctrl_tab_steps_the_tab_and_asks_for_its_button() {
    let (_, mut state) = seeded();
    assert_eq!(state.cycle_tab(1), ConnectTab::Preferences);
    assert_eq!(
        state.take_focus_request(),
        Some(Stop::Tab(ConnectTab::Preferences))
    );
    assert_eq!(state.cycle_tab(1), ConnectTab::Connection, "wraps");
    assert_eq!(
        state.take_focus_request(),
        Some(Stop::Tab(ConnectTab::Connection))
    );
    state.set_tab(ConnectTab::Preferences);
    assert_eq!(state.tab(), ConnectTab::Preferences);
    assert_eq!(state.take_focus_request(), None, "clicks do not move focus");
}

#[test]
fn the_connection_walk_follows_the_qt_focus_chain() {
    let (populated, state) = seeded();
    let stops = state.stops(&populated, false);
    assert_eq!(
        stops,
        vec![
            Stop::Tab(ConnectTab::Connection),
            Stop::Tab(ConnectTab::Preferences),
            Stop::Network("omarchy".into()),
            Stop::Network("oftc".into()),
            Stop::AddNetwork,
            Stop::ShortcutsHint,
            Stop::Field(ConnectField::Name),
            Stop::Field(ConnectField::Host),
            Stop::Field(ConnectField::Port),
            Stop::Tls,
            Stop::Field(ConnectField::Nick),
            Stop::Field(ConnectField::Username),
            Stop::Field(ConnectField::RealName),
            Stop::Field(ConnectField::Autojoin),
            Stop::ConnectOnStartup,
            Stop::FieldHelp(ConnectField::ServerPassword),
            Stop::Field(ConnectField::ServerPassword),
            Stop::FieldHelp(ConnectField::NickServPassword),
            Stop::Field(ConnectField::NickServPassword),
            Stop::Remove,
            Stop::Discard,
            Stop::Apply,
        ]
    );
}

#[test]
fn the_walk_drops_add_while_dirty_and_remove_when_not_allowed() {
    let (mut populated, mut state) = seeded();
    state.set_text(ConnectField::Name, "edited");
    let stops = state.stops(&populated, false);
    assert!(!stops.contains(&Stop::AddNetwork));
    populated.retain(|id| id != "oftc");
    assert!(!state.stops(&populated, false).contains(&Stop::Remove));
}

#[test]
fn the_preferences_walk_is_its_rows_and_their_marks() {
    let (populated, mut state) = seeded();
    state.set_tab(ConnectTab::Preferences);
    assert_eq!(
        state.stops(&populated, false),
        vec![
            Stop::Tab(ConnectTab::Connection),
            Stop::Tab(ConnectTab::Preferences),
            Stop::Preference(PrefName::Directs),
            Stop::PreferenceHelp(PrefName::Directs),
            Stop::Preference(PrefName::Avatars),
            Stop::PreferenceHelp(PrefName::Avatars),
            Stop::Preference(PrefName::Unread),
            Stop::PreferenceHelp(PrefName::Unread),
            Stop::Preference(PrefName::Colors),
            Stop::PreferenceHelp(PrefName::Colors),
            Stop::Preference(PrefName::Palette),
            Stop::PreferenceHelp(PrefName::Palette),
        ]
    );
}

#[test]
fn enter_and_tab_walk_forward_and_shift_tab_walks_back_with_wrap() {
    let (populated, state) = seeded();
    let step =
        |from: Stop, backwards: bool| state.step_stop(&populated, false, Some(&from), backwards);
    assert_eq!(
        step(Stop::Field(ConnectField::Name), false),
        Some(Stop::Field(ConnectField::Host))
    );
    assert_eq!(
        step(Stop::Field(ConnectField::Autojoin), false),
        Some(Stop::ConnectOnStartup)
    );
    assert_eq!(
        step(Stop::Apply, false),
        Some(Stop::Tab(ConnectTab::Connection)),
        "forward wraps to the first stop"
    );
    assert_eq!(
        step(Stop::Tab(ConnectTab::Connection), true),
        Some(Stop::Apply),
        "back wraps to the last stop"
    );
    assert_eq!(
        step(Stop::Field(ConnectField::Host), true),
        Some(Stop::Field(ConnectField::Name))
    );
}

#[test]
fn a_stop_off_the_showing_tab_falls_back_to_the_ends() {
    let (populated, state) = seeded();
    let off = Stop::Preference(PrefName::Directs);
    assert_eq!(
        state.step_stop(&populated, false, Some(&off), false),
        Some(Stop::Tab(ConnectTab::Connection))
    );
    assert_eq!(
        state.step_stop(&populated, false, Some(&off), true),
        Some(Stop::Apply)
    );
}

#[test]
fn initial_focus_is_nick_when_it_is_empty_else_name() {
    let (_, mut state) = seeded();
    assert_eq!(state.initial_focus(), Stop::Field(ConnectField::Name));
    state.set_text(ConnectField::Nick, "");
    assert_eq!(state.initial_focus(), Stop::Field(ConnectField::Nick));
}

#[test]
fn restore_focus_returns_to_the_last_reported_stop() {
    let (_, mut state) = seeded();
    state.restore_focus();
    assert_eq!(
        state.take_focus_request(),
        Some(Stop::Field(ConnectField::Name))
    );
    state.set_focus(Some(Stop::Field(ConnectField::Autojoin)));
    state.restore_focus();
    assert_eq!(
        state.take_focus_request(),
        Some(Stop::Field(ConnectField::Autojoin))
    );
    assert_eq!(state.take_focus_request(), None, "a request is taken once");
}

#[test]
fn request_focus_is_taken_once() {
    let (_, mut state) = seeded();
    state.request_focus(Stop::Apply);
    assert_eq!(state.take_focus_request(), Some(Stop::Apply));
    assert_eq!(state.take_focus_request(), None);
}

#[test]
fn step_stop_without_a_focused_stop_starts_at_the_ends() {
    let (populated, state) = seeded();
    assert_eq!(
        state.step_stop(&populated, false, None, false),
        Some(Stop::Tab(ConnectTab::Connection))
    );
    assert_eq!(
        state.step_stop(&populated, false, None, true),
        Some(Stop::Apply)
    );
}

#[test]
fn step_stop_walks_from_a_stop_on_the_tab_both_ways() {
    let (populated, state) = seeded();
    let from = Stop::Field(ConnectField::Host);
    assert_eq!(
        state.step_stop(&populated, false, Some(&from), false),
        Some(Stop::Field(ConnectField::Port))
    );
    assert_eq!(
        state.step_stop(&populated, false, Some(&from), true),
        Some(Stop::Field(ConnectField::Name))
    );
}

#[test]
fn disconnect_joins_the_walk_only_while_the_session_can_stop() {
    let (populated, state) = seeded();
    let before_apply = |stops: &[Stop]| {
        let at = stops.iter().position(|stop| *stop == Stop::Apply).unwrap();
        stops[at - 1].clone()
    };

    let live = state.stops(&populated, true);
    assert_eq!(
        before_apply(&live),
        Stop::Disconnect,
        "Disconnect sits next to Apply"
    );
    assert_eq!(
        live[live.len() - 3],
        Stop::Discard,
        "after Discard, before Apply"
    );
    assert!(!state.stops(&populated, false).contains(&Stop::Disconnect));

    assert_eq!(
        state.step_stop(&populated, true, Some(&Stop::Discard), false),
        Some(Stop::Disconnect)
    );
    assert_eq!(
        state.step_stop(&populated, false, Some(&Stop::Discard), false),
        Some(Stop::Apply),
        "without a live session Tab goes from Discard to Apply"
    );
    assert_eq!(
        state.step_stop(&populated, true, Some(&Stop::Apply), false),
        Some(Stop::Tab(ConnectTab::Connection)),
        "the walk still wraps"
    );
}
