//! The intents the shell calls for its own writes: `/me`, the join of an invite, the
//! local outcome and reply lines, the two `/clear` forms, closing any row, forgetting
//! a network, the clash-suffixed network name, and the inbox rows the shell takes.
//! Each one drives the real demo and reads the frames or the reads it changes.

use crate::irc::world::MessageKind;
use crate::irc::inbox::InboxKind;

use super::fixture::{status_text, Fixture, OFTC, OMARCHY};

#[test]
fn me_writes_a_ctcp_action_frame_and_the_echo_is_an_action_row() {
    let mut fixture = Fixture::seeded();
    fixture.clear_frames(OMARCHY);
    assert!(fixture
        .controller
        .send_action(OMARCHY, "#omarchy", "waves"));

    assert_eq!(
        fixture.frames(OMARCHY),
        ["PRIVMSG #omarchy :\u{1}ACTION waves\u{1}\r\n"]
    );
    let action = fixture
        .controller
        .messages()
        .iter()
        .find(|message| message.body == "waves")
        .expect("the echoed action is a row");
    assert_eq!(action.kind, MessageKind::Action);
    assert_eq!(action.author, "fred");
}

#[test]
fn me_refuses_an_unregistered_network_with_a_last_error() {
    let mut fixture = Fixture::seeded();
    assert!(!fixture.controller.send_action("ghost", "#omarchy", "waves"));
    assert_eq!(
        fixture.controller.last_error_for("ghost"),
        Some("That network is not configured")
    );
}

#[test]
fn joining_an_invite_writes_join_and_selects_the_channel() {
    let mut fixture = Fixture::seeded();
    fixture.clear_frames(OFTC);
    assert!(fixture.controller.join_channel(OFTC, "#debian"));

    assert_eq!(fixture.frames(OFTC), ["JOIN #debian\r\n"]);
    assert_eq!(fixture.controller.selected_network_id(), Some(OFTC));
    assert_eq!(fixture.controller.selected_target(), "#debian");
    assert!(!fixture.controller.shows_status());
}

#[test]
fn joining_clears_the_invite_row_for_that_channel() {
    let mut fixture = Fixture::seeded();
    let invites = fixture
        .controller
        .inbox_items()
        .iter()
        .filter(|item| item.kind == InboxKind::Invite)
        .count();
    assert_eq!(invites, 1, "the seed offers rio's invite to #debian");
    assert!(fixture.controller.join_channel(OFTC, "#debian"));
    assert!(fixture
        .controller
        .inbox_items()
        .iter()
        .all(|item| item.kind != InboxKind::Invite));
}

#[test]
fn an_outcome_line_lands_on_status_as_a_local_command_line() {
    let mut fixture = Fixture::seeded();
    fixture.controller.log_outcome(OMARCHY, "Command was refused");
    let line = fixture
        .controller
        .status_lines(OMARCHY)
        .last()
        .copied()
        .expect("a status line")
        .clone();
    assert_eq!(line.label, "command");
    assert_eq!(line.text, "Command was refused");
    assert_eq!(line.source, crate::irc::statusentry::LogSource::Local);
}

#[test]
fn a_ctcp_reply_line_lands_on_status_labelled_ctcp() {
    let mut fixture = Fixture::seeded();
    fixture
        .controller
        .log_ctcp_reply(OMARCHY, "VERSION reply from anna: https://omairc.app");
    assert!(fixture
        .controller
        .status_lines(OMARCHY)
        .iter()
        .any(|line| line.label == "CTCP" && line.text.contains("omairc.app")));
}

#[test]
fn a_reply_lands_in_the_selected_conversation_as_a_whois_row() {
    let mut fixture = Fixture::seeded();
    assert!(fixture.controller.reply_in_selection("Commands: /me, /help."));
    let last = fixture
        .controller
        .messages()
        .last()
        .cloned()
        .expect("a row");
    assert_eq!(last.kind, MessageKind::Whois);
    assert_eq!(last.body, "Commands: /me, /help.");
    assert!(last.author.is_empty());
}

#[test]
fn a_reply_with_status_selected_is_refused_so_the_caller_uses_status() {
    let mut fixture = Fixture::seeded();
    fixture.controller.open_status(OMARCHY);
    assert!(!fixture.controller.reply_in_selection("nope"));
}

#[test]
fn clear_empties_the_status_log_of_one_network_only() {
    let mut fixture = Fixture::seeded();
    assert!(!status_text(&fixture.controller, OMARCHY).is_empty());
    fixture.controller.clear_status(OMARCHY);
    assert!(status_text(&fixture.controller, OMARCHY).is_empty());
    assert!(!status_text(&fixture.controller, OFTC).is_empty());
}

#[test]
fn clear_empties_one_conversation_and_keeps_the_rest() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#desktop");
    assert!(!fixture.controller.messages().is_empty());
    fixture.controller.clear_conversation(OMARCHY, "#desktop");
    assert!(fixture.controller.messages().is_empty());

    fixture.controller.select_conversation(OMARCHY, "#ricing");
    assert!(!fixture.controller.messages().is_empty());
}

#[test]
fn closing_a_direct_message_by_name_keeps_the_selection_where_it_was() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(fixture.controller.close_conversation(OMARCHY, "anna"));
    assert!(fixture
        .controller
        .conversations()
        .iter()
        .all(|row| row.target != "anna"));
    assert_eq!(fixture.controller.selected_target(), "#omarchy");
}

#[test]
fn closing_an_unknown_or_joined_conversation_is_refused() {
    let mut fixture = Fixture::seeded();
    assert!(!fixture.controller.close_conversation(OMARCHY, "nobody"));
    assert!(!fixture.controller.close_conversation(OMARCHY, "#omarchy"));
}

#[test]
fn a_selection_clears_the_networks_last_error() {
    let mut fixture = Fixture::seeded();
    fixture.controller.set_last_error(OMARCHY, Some("Name a nick".to_owned()));
    fixture.controller.select_conversation(OMARCHY, "#desktop");
    assert_eq!(fixture.controller.last_error_for(OMARCHY), None);
}

#[test]
fn the_network_name_gains_the_nick_only_while_another_network_shares_the_host() {
    let mut fixture = Fixture::seeded();
    assert_eq!(fixture.controller.network_name(OMARCHY), "irc.example · fred");
    assert_eq!(fixture.controller.network_name(OFTC), "irc.example · oak");

    assert!(fixture.controller.forget_network(OFTC));
    assert_eq!(fixture.controller.network_name(OMARCHY), "irc.example");
    assert_eq!(fixture.controller.network_name(OFTC), "");
}

#[test]
fn forgetting_a_network_drops_its_rows_status_order_and_inbox_items() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OFTC, "#lab");
    assert!(fixture.controller.forget_network(OFTC));

    assert!(fixture
        .controller
        .conversations()
        .iter()
        .all(|row| row.key.network_id != OFTC));
    assert!(status_text(&fixture.controller, OFTC).is_empty());
    assert!(fixture.controller.session(OFTC).is_none());
    assert!(!fixture.controller.network_order().contains(&OFTC.to_owned()));
    assert!(fixture
        .controller
        .inbox_items()
        .iter()
        .all(|item| item.network_id != OFTC));
    assert!(fixture.controller.selected().is_none(), "the selection was on OFTC");
}

#[test]
fn forgetting_an_unknown_network_reports_it_was_never_known() {
    let mut fixture = Fixture::seeded();
    assert!(!fixture.controller.forget_network("ghost"));
    assert!(!fixture.controller.forget_network(""));
}

#[test]
fn taking_an_inbox_row_removes_it_and_hands_it_back() {
    let mut fixture = Fixture::seeded();
    let before = fixture.controller.inbox_items().len();
    let item = fixture
        .controller
        .take_inbox_item(0)
        .expect("the newest row");
    assert!(!item.network_id.is_empty());
    assert_eq!(fixture.controller.inbox_items().len(), before - 1);
    assert!(fixture.controller.take_inbox_item(99).is_none());
}
