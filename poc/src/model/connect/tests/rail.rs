//! The network rail: selecting, adding, discarding and removing networks, and walking the rail.

use super::*;

#[test]
fn select_moves_to_a_saved_network_when_clean() {
    let (_, mut state) = seeded();
    assert!(state.select("oftc"));
    assert_eq!(state.selected(), "oftc");
    assert_eq!(state.draft().nick, "oak");
    assert!(!state.select("oftc"), "the same row is not a move");
    assert!(!state.select(""), "no id is not a move");
    assert!(!state.select("nowhere"));
}

#[test]
fn select_is_refused_while_the_draft_is_dirty_as_in_qt() {
    let (_, mut state) = seeded();
    state.set_text(ConnectField::Name, "edited");
    assert!(!state.select("oftc"));
    assert_eq!(state.selected(), "omarchy");
    assert_eq!(state.draft().name, "edited");
}

#[test]
fn selecting_resets_remove_armed() {
    let (populated, mut state) = seeded();
    assert_eq!(state.remove(&populated), RemoveOutcome::Armed);
    assert!(state.remove_armed());
    state.select("oftc");
    assert!(!state.remove_armed());
}

#[test]
fn alt_arrows_walk_the_rail_and_wrap() {
    let (_, mut state) = seeded();
    assert_eq!(state.walk_network(1).as_deref(), Some("oftc"));
    assert_eq!(state.selected(), "oftc");
    assert_eq!(
        state.walk_network(1).as_deref(),
        Some("omarchy"),
        "wraps forward"
    );
    assert_eq!(
        state.walk_network(-1).as_deref(),
        Some("oftc"),
        "wraps back"
    );
}

#[test]
fn walking_while_dirty_reports_the_row_but_keeps_the_selection() {
    let (_, mut state) = seeded();
    state.set_text(ConnectField::Host, "edited.example");
    assert_eq!(state.walk_network(1).as_deref(), Some("oftc"));
    assert_eq!(state.selected(), "omarchy");
}

#[test]
fn rail_enter_selects_and_arms_then_applies() {
    let (_, mut state) = seeded();
    assert_eq!(state.rail_enter("oftc"), RailEnter::Selected);
    assert_eq!(state.selected(), "oftc");
    assert_eq!(state.rail_enter("oftc"), RailEnter::Apply);
}

#[test]
fn leaving_the_rail_disarms_enter() {
    let (_, mut state) = seeded();
    state.rail_enter("oftc");
    state.rail_left();
    assert_eq!(state.rail_enter("oftc"), RailEnter::Selected);
}

#[test]
fn an_unselected_row_enter_arms_but_does_not_apply() {
    let (_, mut state) = seeded();
    assert_eq!(state.rail_enter("oftc"), RailEnter::Selected);
    assert_eq!(state.rail_enter("omarchy"), RailEnter::Selected);
}

#[test]
fn add_makes_an_unsaved_draft_with_qt_defaults() {
    let (_, mut state) = seeded();
    assert!(state.add());
    let draft = state.draft();
    assert_eq!(draft.host, "");
    assert_eq!(draft.port, "6697");
    assert!(draft.tls);
    assert_eq!(draft.icon_color, Some(0), "the first free colour");
    assert!(!state.is_saved(state.selected()));
    assert_eq!(stored_ids(&state), ["omarchy", "oftc"], "not saved yet");
    assert_eq!(
        state.take_focus_request(),
        Some(Stop::Field(ConnectField::Name))
    );
}

#[test]
fn the_draft_row_is_listed_last_as_new_network() {
    let (_, mut state) = seeded();
    state.add();
    let rows = state.rows();
    assert_eq!(rows.len(), 3);
    assert!(!rows[2].stored);
    assert!(rows[2].selected);
    assert_eq!(rows[2].label, "New network");
    assert!(!rows[0].selected);
}

#[test]
fn add_is_refused_while_dirty_or_on_an_unsaved_draft() {
    let (_, mut state) = seeded();
    state.set_text(ConnectField::Name, "edited");
    assert!(!state.add());
    assert_eq!(state.rows().len(), 2);
    state.discard();
    assert!(state.add());
    assert!(!state.add(), "a second add while the draft is unsaved");
}

#[test]
fn discard_of_an_unsaved_draft_returns_to_the_network_it_came_from() {
    let (_, mut state) = seeded();
    state.select("oftc");
    state.add();
    state.discard();
    assert_eq!(state.selected(), "oftc");
    assert_eq!(state.rows().len(), 2);
    assert!(!state.dirty());
}

#[test]
fn discard_of_a_saved_network_reverts_its_edits() {
    let (_, mut state) = seeded();
    state.set_text(ConnectField::Name, "edited");
    state.discard();
    assert_eq!(state.draft().name, "");
    assert!(!state.dirty());
    assert_eq!(state.selected(), "omarchy");
}

#[test]
fn discard_clears_remove_armed_and_secrets() {
    let (populated, mut state) = seeded();
    state.set_text(ConnectField::ServerPassword, "pw");
    state.remove(&populated);
    state.discard();
    assert!(!state.remove_armed());
    assert_eq!(state.text(ConnectField::ServerPassword), "");
}

#[test]
fn applying_a_new_network_adds_it_to_the_world_with_its_colour() {
    let (_, mut state) = seeded();
    assert!(state.add());
    state.set_text(ConnectField::Host, "irc.libera.example");
    state.set_text(ConnectField::Nick, "zed");
    let profile = state.apply().expect("complete");
    assert_eq!(state.saved().len(), 3);
    assert_eq!(
        profile.icon_color,
        Some(0),
        "the first colour no saved network uses"
    );
}

#[test]
fn a_new_network_with_the_demo_host_gets_the_nick_suffix() {
    let (_, mut state) = seeded();
    assert!(state.add());
    state.set_text(ConnectField::Host, "irc.example");
    state.set_text(ConnectField::Nick, "zed");
    state.apply().expect("complete");
    let labels: Vec<String> = state.rows().into_iter().map(|row| row.label).collect();
    assert_eq!(
        labels,
        [
            "irc.example · fred",
            "irc.example · oak",
            "irc.example · zed"
        ]
    );
}

#[test]
fn a_second_new_network_gets_the_next_free_colour_and_id() {
    let (_populated, mut state) = seeded();
    state.add();
    state.set_text(ConnectField::Host, "a.example");
    state.set_text(ConnectField::Nick, "fred");
    state.apply().unwrap();
    state.add();
    assert_eq!(state.selected(), "network-2");
    assert_eq!(state.draft().icon_color, Some(3));
}

#[test]
fn remove_arms_on_the_first_press_and_removes_on_the_second() {
    let (populated, mut state) = seeded();
    assert_eq!(state.remove(&populated), RemoveOutcome::Armed);
    assert_eq!(stored_ids(&state), ["omarchy", "oftc"]);
    assert_eq!(
        state.remove(&populated),
        RemoveOutcome::Removed("omarchy".to_string())
    );
    assert_eq!(stored_ids(&state), ["oftc"]);
    assert!(!state.is_saved("omarchy"));
    assert_eq!(state.selected(), "oftc");
    assert!(!state.remove_armed());
}

#[test]
fn removing_selects_the_network_now_at_the_same_position() {
    let (populated, mut state) = seeded();
    state.select("oftc");
    state.remove(&populated);
    assert_eq!(
        state.remove(&populated),
        RemoveOutcome::Removed("oftc".into())
    );
    assert_eq!(
        state.selected(),
        "omarchy",
        "the last network takes the place"
    );
    assert_eq!(state.draft().nick, "fred");
}

#[test]
fn remove_is_refused_when_no_other_network_has_a_conversation() {
    let (mut populated, mut state) = seeded();
    populated.retain(|id| id != "oftc");
    assert!(!state.can_remove(&populated));
    assert_eq!(state.remove(&populated), RemoveOutcome::Refused);
    assert!(!state.remove_armed());
    assert_eq!(stored_ids(&state).len(), 2);
}

#[test]
fn remove_is_refused_for_an_unsaved_draft() {
    let (populated, mut state) = seeded();
    state.add();
    assert!(!state.can_remove(&populated));
    assert_eq!(state.remove(&populated), RemoveOutcome::Refused);
}

#[test]
fn a_first_run_draft_takes_a_fresh_network_id_so_apply_has_a_network() {
    let mut state = ConnectState::seed(&[]);
    assert_eq!(state.selected(), "network-1");
    assert_eq!(state.draft().network_id, "network-1");
    state.set_text(ConnectField::Host, "irc.example");
    state.set_text(ConnectField::Nick, "fred");
    let profile = state.apply().expect("a complete draft applies");
    assert_eq!(profile.network_id, "network-1");
    assert!(state.is_saved("network-1"));
}

#[test]
fn rail_stops_are_the_rows_then_add_while_it_shows() {
    let (_, mut state) = seeded();
    assert_eq!(
        state.rail_stops(),
        vec![
            Stop::Network("omarchy".into()),
            Stop::Network("oftc".into()),
            Stop::AddNetwork,
        ]
    );
    state.add();
    // A dirty or unsaved draft cannot add again, so the Add row is hidden.
    assert_eq!(
        state.rail_stops().last(),
        Some(&Stop::Network("network-1".into()))
    );
}

#[test]
fn rail_up_and_down_clamp_at_the_ends_without_wrapping() {
    let (_, state) = seeded();
    let first = Stop::Network("omarchy".into());
    let add = Stop::AddNetwork;
    assert_eq!(state.rail_step(&first, -1), Some(first.clone()));
    assert_eq!(
        state.rail_step(&first, 1),
        Some(Stop::Network("oftc".into()))
    );
    assert_eq!(state.rail_step(&add, 1), Some(add.clone()));
    assert_eq!(state.rail_step(&Stop::Apply, 1), None, "not a rail stop");
}

#[test]
fn rail_home_and_end_are_its_first_and_last_stops() {
    let (_, state) = seeded();
    assert_eq!(
        state.rail_edge(false),
        Some(Stop::Network("omarchy".into()))
    );
    assert_eq!(state.rail_edge(true), Some(Stop::AddNetwork));
}

#[test]
fn a_refused_switch_raises_the_notice_until_the_draft_is_settled() {
    let (_populated, mut state) = seeded();
    assert!(!state.switch_refused());
    state.set_text(ConnectField::Host, "edited.example");
    assert!(!state.select("oftc"));
    assert!(state.switch_refused(), "a dirty draft refuses the switch");
    assert_eq!(
        SWITCH_REFUSED_TEXT,
        "Apply or discard your edits before switching networks."
    );
    state.discard();
    assert!(!state.switch_refused(), "Discard settles the draft");
    state.set_text(ConnectField::Host, "edited.example");
    assert!(!state.select("oftc"));
    state.apply().unwrap();
    assert!(!state.switch_refused(), "Apply settles the draft");
}

#[test]
fn a_switch_that_moves_clears_the_notice() {
    let (_, mut state) = seeded();
    state.set_text(ConnectField::Host, "edited.example");
    state.select("oftc");
    assert!(state.switch_refused());
    state.set_text(ConnectField::Host, "irc.example");
    assert!(!state.dirty());
    assert!(state.select("oftc"));
    assert!(!state.switch_refused());
}

#[test]
fn a_no_op_select_is_not_a_refusal() {
    let (_, mut state) = seeded();
    state.set_text(ConnectField::Name, "edited");
    assert!(!state.select("omarchy"), "the same row is not a move");
    assert!(!state.switch_refused(), "and it is not a refused switch");
    assert!(!state.select("nowhere"));
    assert!(
        !state.switch_refused(),
        "an unknown row is not refused either"
    );
}
