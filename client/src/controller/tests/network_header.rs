//! Sidebar network header fields the PoC reads from `build_network_headers`
//! (`liveAlerts` and `showEdit` in `NetworkSection`).

use crate::irc::statusentry::{lifecycle, LogSeverity};
use crate::model::connect::ConnectState;
use crate::session::clock::Clock;
use crate::model::sidebar::NetworkHeader;
use crate::ui::shell::view::build_network_headers;

use super::fixture::{Fixture, OMARCHY};

fn network_section_headers(
    controller: &crate::controller::controller::Controller,
) -> Vec<NetworkHeader> {
    let rows = controller.conversations();
    let connect = ConnectState::seed(&[]);
    build_network_headers(controller, &connect, &rows)
}

#[test]
fn network_section_header_alerts_follow_alerts_for() {
    let mut fixture = Fixture::seeded();
    let before = network_section_headers(&fixture.controller)
        .into_iter()
        .find(|header| header.id == OMARCHY)
        .expect("omarchy header")
        .alerts;
    assert_eq!(before, 0);

    fixture.controller.status_entry(lifecycle(
        OMARCHY,
        LogSeverity::Alert,
        "error",
        "refused",
        fixture.clock.now(),
    ));
    assert_eq!(fixture.controller.alerts_for(OMARCHY), 1);

    let header = network_section_headers(&fixture.controller)
        .into_iter()
        .find(|header| header.id == OMARCHY)
        .expect("omarchy header");
    assert_eq!(header.alerts, fixture.controller.alerts_for(OMARCHY));
    assert!(header.alerts > 0);
}

#[test]
fn network_section_show_edit_follows_live_session() {
    let mut fixture = Fixture::seeded();
    assert!(!fixture.controller.start("freenode"));
    assert!(fixture.controller.session("freenode").is_none());

    let headers = network_section_headers(&fixture.controller);
    let omarchy = headers
        .iter()
        .find(|header| header.id == OMARCHY)
        .expect("omarchy header");
    let freenode = headers
        .iter()
        .find(|header| header.id == "freenode")
        .expect("freenode header");

    assert!(omarchy.show_edit, "the demo session is live");
    assert!(!freenode.show_edit, "a refused start has no session");
}
