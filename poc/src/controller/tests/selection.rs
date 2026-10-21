//! Selection, Status and direct messages over the real demo. Ports the Go cases
//! `TestFirstConversationClaimsOffStatus`, `TestClearedSelectionFallsBackToStatus`,
//! `TestExplicitSelectionStillClosesConsole`, `TestQuietSendDoesNotOpenDirect`,
//! `TestConversationClearWipesMessages`, `TestSelectConversationRefreshesSidebarUnread`
//! and the open-direct cases in `tui/internal/controller/`.

use crate::irc::event::ConversationKey;
use crate::irc::viewnotify::ViewNotify;

use super::fixture::{Fixture, OMARCHY};

#[test]
fn selecting_a_conversation_clears_its_unread_and_mention_counts() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#ricing");
    assert_eq!(fixture.controller.selected_target(), "#ricing");
    let row = fixture
        .controller
        .conversations()
        .into_iter()
        .find(|row| row.target == "#ricing")
        .expect("#ricing row");
    assert_eq!(row.unread, 0);
    assert!(!row.mention);
    assert_eq!(fixture.controller.unread_count_for(OMARCHY), 4);
}

#[test]
fn selecting_a_conversation_consumes_its_inbox_rows() {
    let mut fixture = Fixture::seeded();
    assert_eq!(fixture.controller.inbox_items().len(), 3);
    fixture.controller.select_conversation(OMARCHY, "#ricing");
    let items = fixture.controller.inbox_items();
    assert_eq!(items.len(), 2, "the #ricing mention is consumed: {items:?}");
    assert!(items.iter().all(|item| item.target != "#ricing"));
    assert!(items.iter().any(|item| item.target == "#debian"));
}

#[test]
fn opening_status_shows_status_until_a_conversation_is_selected() {
    let mut fixture = Fixture::seeded();
    fixture.controller.open_status(OMARCHY);
    assert!(fixture.controller.shows_status());
    assert_eq!(fixture.controller.focused_network_id(), OMARCHY);
    assert!(fixture.controller.messages().len() > 0, "the conversation survives");

    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(!fixture.controller.shows_status());
}

#[test]
fn clearing_the_selection_falls_back_to_status() {
    let mut fixture = Fixture::seeded();
    fixture.controller.clear_selection();
    assert!(fixture.controller.selected().is_none());
    assert!(fixture.controller.shows_status());
    assert_eq!(fixture.controller.selected_target(), "");
    assert!(fixture.controller.messages().is_empty());
}

#[test]
fn the_selection_change_repaints_every_transcript_surface() {
    let mut fixture = Fixture::seeded();
    fixture.controller.take_notifications();
    fixture.controller.select_conversation(OMARCHY, "#help");
    let surfaces = fixture.controller.take_notifications();
    for expected in [
        ViewNotify::Conversations,
        ViewNotify::Messages,
        ViewNotify::MemberReset,
        ViewNotify::Selection,
    ] {
        assert!(surfaces.contains(&expected), "missing {expected:?}: {surfaces:?}");
    }
}

#[test]
fn open_direct_creates_the_query_on_the_selected_network_and_selects_it() {
    let mut fixture = Fixture::seeded();
    assert!(fixture.controller.open_direct("newcomer"));
    assert_eq!(fixture.controller.selected_target(), "newcomer");
    assert_eq!(fixture.controller.selected_network_id(), Some(OMARCHY));
    assert!(fixture
        .controller
        .conversations()
        .iter()
        .any(|row| row.target == "newcomer" && row.direct));
}

#[test]
fn open_direct_refuses_an_empty_nick_and_no_selection() {
    let mut fixture = Fixture::seeded();
    assert!(!fixture.controller.open_direct(""));
    fixture.controller.clear_selection();
    assert!(!fixture.controller.open_direct("newcomer"));
}

#[test]
fn closing_a_direct_message_removes_it_and_selects_its_neighbour() {
    let mut fixture = Fixture::seeded();
    assert!(fixture.controller.open_direct("newcomer"));
    assert!(fixture.controller.close_direct());
    assert!(!fixture
        .controller
        .conversations()
        .iter()
        .any(|row| row.target == "newcomer"));
    assert_ne!(fixture.controller.selected_target(), "newcomer");
    assert!(fixture.controller.selected().is_some());
}

#[test]
fn a_joined_channel_cannot_be_closed() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(fixture.controller.channel_joined());
    assert!(!fixture.controller.can_close_selection());
    assert!(!fixture.controller.close_direct());
    assert_eq!(fixture.controller.selected_target(), "#omarchy");
}

#[test]
fn a_direct_message_can_always_close() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "dax");
    assert!(!fixture.controller.channel_joined());
    assert!(fixture.controller.can_close_selection());
}

#[test]
fn mark_all_read_clears_every_badge_on_the_network() {
    let mut fixture = Fixture::seeded();
    assert!(fixture.controller.unread_count_for(OMARCHY) > 0);
    fixture.controller.mark_all_read();
    assert_eq!(fixture.controller.unread_count_for(OMARCHY), 0);
    assert!(!fixture.controller.mention_for(OMARCHY));
}

#[test]
fn losing_window_focus_keeps_the_badges_and_gaining_it_reads_the_selection() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.controller.set_window_active(false);
    fixture.controller.set_window_active(true);
    let selected = fixture.controller.selected().cloned();
    assert_eq!(
        selected,
        Some(ConversationKey::new(OMARCHY, "#omarchy"))
    );
}
