//! Sidebar network order and collapse. Ports `tui/internal/controller/networkorder_test.go`
//! (`TestNetworkOrderRecordedOrderWins`, `TestNetworkOrderDeduplicates`,
//! `TestNetworkCollapsedRoundTrip`, `TestSetAllNetworksCollapsed`, `TestMoveNetwork`,
//! `TestCollapseDoesNotMutateConversationsOrder`).

use crate::irc::serverfeatures::IrcServerFeatures;
use crate::session::events::SessionState;

use super::fixture::{Fixture, OFTC, OMARCHY};

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn the_recorded_order_wins_and_unknown_networks_drop_out() {
    let mut fixture = Fixture::seeded();
    fixture
        .controller
        .set_network_order(&ids(&["ghost", OFTC, OFTC]));
    assert_eq!(fixture.controller.network_order(), [OFTC, OMARCHY]);
}

#[test]
fn the_sidebar_rows_follow_the_network_order() {
    let mut fixture = Fixture::seeded();
    fixture.controller.set_network_order(&ids(&[OFTC, OMARCHY]));
    let rows = fixture.controller.conversations();
    assert_eq!(rows.first().map(|row| row.key.network_id.as_str()), Some(OFTC));
    assert_eq!(rows.last().map(|row| row.key.network_id.as_str()), Some(OMARCHY));
}

#[test]
fn move_network_swaps_neighbours_and_refuses_to_leave_the_list() {
    let mut fixture = Fixture::seeded();
    assert!(!fixture.controller.move_network(OFTC, 1), "already last");
    assert!(fixture.controller.move_network(OFTC, -1));
    assert_eq!(fixture.controller.network_order(), [OFTC, OMARCHY]);
    assert!(!fixture.controller.move_network(OFTC, -1), "already first");
    assert!(!fixture.controller.move_network(OFTC, 0));
    assert!(!fixture.controller.move_network("ghost", 1));
}

#[test]
fn one_network_collapses_and_expands() {
    let mut fixture = Fixture::seeded();
    let rows_before = fixture.controller.conversations().len();
    fixture.controller.set_network_collapsed(OFTC, true);
    assert!(fixture.controller.is_network_collapsed(OFTC));
    assert!(!fixture.controller.is_network_collapsed(OMARCHY));
    assert_eq!(
        fixture.controller.conversations().len(),
        rows_before,
        "collapse folds the view and leaves the rows in place"
    );
    fixture.controller.set_network_collapsed(OFTC, false);
    assert!(!fixture.controller.is_network_collapsed(OFTC));
}

#[test]
fn every_network_collapses_at_once_and_expands_again() {
    let mut fixture = Fixture::seeded();
    fixture.controller.set_all_networks_collapsed(true);
    assert!(fixture.controller.is_network_collapsed(OMARCHY));
    assert!(fixture.controller.is_network_collapsed(OFTC));
    fixture.controller.set_all_networks_collapsed(false);
    assert!(!fixture.controller.is_network_collapsed(OMARCHY));
    assert!(!fixture.controller.is_network_collapsed(OFTC));
}

#[test]
fn forgetting_a_registered_network_clears_monitor_before_quit() {
    let mut fixture = Fixture::seeded();
    fixture.controller.monitor_add(OMARCHY, "anna");
    fixture.clear_frames(OMARCHY);

    assert!(fixture.controller.forget_network(OMARCHY));
    assert_eq!(
        fixture.frames(OMARCHY),
        ["MONITOR C\r\n", "QUIT\r\n"],
        "MONITOR C clears the server list before teardown"
    );
}

#[test]
fn forgetting_a_connecting_network_skips_monitor_c_and_quit() {
    let mut fixture = Fixture::seeded();
    fixture.controller.monitor_add(OMARCHY, "anna");
    let session = fixture.controller.session(OMARCHY).expect("demo network");
    session.locked(|core| core.set_state(SessionState::Connecting));
    fixture.clear_frames(OMARCHY);

    assert!(fixture.controller.forget_network(OMARCHY));
    assert!(
        fixture.frames(OMARCHY).is_empty(),
        "MONITOR C and QUIT run only while registered"
    );
}

#[test]
fn forgetting_a_network_without_monitor_skips_monitor_c() {
    let mut fixture = Fixture::seeded();
    fixture
        .controller
        .reducer
        .set_server_features(OMARCHY, IrcServerFeatures::new());
    fixture.clear_frames(OMARCHY);

    assert!(fixture.controller.forget_network(OMARCHY));
    assert_eq!(fixture.frames(OMARCHY), ["QUIT\r\n"]);
}

#[test]
fn an_unchanged_order_queues_nothing() {
    let mut fixture = Fixture::seeded();
    fixture.controller.take_notifications();
    let current = fixture.controller.network_order();
    fixture.controller.set_network_order(&current);
    assert!(fixture.controller.take_notifications().is_empty());
}
