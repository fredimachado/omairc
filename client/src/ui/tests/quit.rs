//! Quit proofs: every quit, Ctrl+Q included, stops the live sessions, so each one refreshes
//! its STS expiry on the way out.

use crate::session::events::SessionState;

use super::fixture;

#[gpui::test]
fn quitting_stops_every_live_session(cx: &mut gpui::TestAppContext) {
    let live = {
        let ui = fixture::open(cx);
        ui.shell.read_with(&*ui.cx, |shell, _| {
            shell
                .controller
                .network_order()
                .into_iter()
                .filter(|network_id| shell.controller.can_disconnect(network_id))
                .filter_map(|network_id| shell.controller.session(&network_id))
                .collect::<Vec<_>>()
        })
    };
    assert!(!live.is_empty(), "the demo seeds at least one live session");

    cx.quit();

    for session in &live {
        assert!(
            matches!(session.state(), SessionState::Closing | SessionState::Idle),
            "quit must stop the session, found {:?}",
            session.state()
        );
    }
}
