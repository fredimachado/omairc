//! The left-channel state over the real demo. A PART, a KICK and a JOIN move a channel
//! between joined and left, and the row stays in the sidebar either way. The header's LEAVE
//! and JOIN are the intents `leave_selected_channel` and `join_selected_channel`, and CLOSE
//! is `close_direct` or `close_conversation`. Each test drives an intent, reads the frames
//! the loopback wrote, and checks the reads the header and the chords take. The demo echoes
//! PART, NICK and TOPIC as a server does, so the reducer sets the state from the echo.
//! feature map's `/part` then `/close` sequence (`slash-commands.md`) and

use crate::irc::command::ComposerSurface;
use crate::irc::event::ConversationKey;

use super::fixture::{bodies, Fixture, OMARCHY};

const CONVERSATION: ComposerSurface = ComposerSurface::Conversation;
const SEEDED_TOPIC: &str = "A cozy corner for Omarchy users and builders.";

fn key(fixture: &Fixture, target: &str) -> ConversationKey {
    fixture.controller.reducer.conversation_key(OMARCHY, target)
}

fn row_exists(fixture: &Fixture, target: &str) -> bool {
    fixture
        .controller
        .reducer
        .find(&key(fixture, target))
        .is_some()
}

fn joined_in_reducer(fixture: &Fixture, target: &str) -> Option<bool> {
    fixture
        .controller
        .reducer
        .find(&key(fixture, target))
        .and_then(|conversation| conversation.channel.as_ref())
        .map(|channel| channel.joined)
}

#[test]
fn a_part_writes_part_and_the_echo_leaves_the_row_in_place() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);

    assert!(fixture.controller.leave_selected_channel());
    assert_eq!(fixture.frames(OMARCHY), ["PART #omarchy\r\n"]);
    assert!(!fixture.controller.channel_joined());
    assert_eq!(joined_in_reducer(&fixture, "#omarchy"), Some(false));
    assert!(
        fixture.controller.can_close_selection(),
        "a left channel can close"
    );
    assert!(
        row_exists(&fixture, "#omarchy"),
        "a left channel keeps its row"
    );
    assert_eq!(
        fixture.controller.people_count(),
        0,
        "a left channel lists nobody"
    );
    assert_eq!(
        fixture.controller.topic(),
        SEEDED_TOPIC,
        "the topic stays with the row"
    );
    assert!(
        bodies(&fixture.controller)
            .iter()
            .any(|body| body == "fred left"),
        "the PART echo lands in the transcript"
    );
}

#[test]
fn a_join_of_a_left_channel_writes_join_and_the_seeded_topic_comes_back() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(fixture.controller.leave_selected_channel());
    fixture.clear_frames(OMARCHY);

    assert!(fixture.controller.join_selected_channel());
    assert_eq!(fixture.frames(OMARCHY), ["JOIN #omarchy\r\n"]);
    assert!(fixture.controller.channel_joined());
    assert_eq!(joined_in_reducer(&fixture, "#omarchy"), Some(true));
    assert!(
        !fixture.controller.can_close_selection(),
        "a joined channel cannot close"
    );
    assert_eq!(fixture.controller.selected_target(), "#omarchy");
    assert_eq!(
        fixture.controller.topic(),
        SEEDED_TOPIC,
        "the 332 restores the topic"
    );
}

#[test]
fn a_join_on_a_joined_channel_or_a_direct_message_writes_nothing() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert!(!fixture.controller.join_selected_channel());
    assert!(fixture.frames(OMARCHY).is_empty());

    fixture.controller.select_conversation(OMARCHY, "anna");
    assert!(!fixture.controller.join_selected_channel());
    assert!(fixture.frames(OMARCHY).is_empty());
}

#[test]
fn a_kick_of_ourselves_marks_the_channel_left_and_the_join_comes_back() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.deliver(OMARCHY, ":dax!u@h KICK #omarchy fred :out you go");

    assert!(
        !fixture.controller.channel_joined(),
        "a kick leaves the channel"
    );
    assert!(
        row_exists(&fixture, "#omarchy"),
        "a kicked channel keeps its row"
    );
    assert!(fixture.controller.can_close_selection());
    assert_eq!(fixture.controller.people_count(), 0);

    fixture.clear_frames(OMARCHY);
    assert!(fixture.controller.join_selected_channel());
    assert_eq!(fixture.frames(OMARCHY), ["JOIN #omarchy\r\n"]);
    assert!(fixture.controller.channel_joined());
}

#[test]
fn a_kick_of_someone_else_leaves_the_channel_joined() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.deliver(OMARCHY, ":dax!u@h KICK #omarchy anna :bye");
    assert!(fixture.controller.channel_joined());
}

#[test]
fn a_self_join_is_recognised_under_a_nick_changed_this_session() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(fixture.controller.submit(CONVERSATION, "/nick fred2"));
    assert!(fixture.controller.leave_selected_channel());
    assert_eq!(joined_in_reducer(&fixture, "#omarchy"), Some(false));

    fixture.clear_frames(OMARCHY);
    assert!(fixture.controller.join_selected_channel());
    assert_eq!(fixture.frames(OMARCHY), ["JOIN #omarchy\r\n"]);
    assert!(
        fixture.controller.channel_joined(),
        "the JOIN echo under fred2 is our own join"
    );
}

#[test]
fn close_drops_a_left_channel_and_refuses_a_joined_one() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(
        !fixture.controller.close_direct(),
        "a joined channel is refused"
    );
    assert!(row_exists(&fixture, "#omarchy"));

    assert!(fixture.controller.leave_selected_channel());
    assert!(fixture.controller.close_direct(), "a left channel closes");
    assert!(!row_exists(&fixture, "#omarchy"));
    assert_ne!(
        fixture.controller.selected_target(),
        "#omarchy",
        "the selection moves to a neighbour"
    );
}

#[test]
fn a_middle_click_close_takes_the_same_rule_as_close() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(
        !fixture.controller.close_conversation(OMARCHY, "#omarchy"),
        "a joined channel row does not close on a middle click"
    );
    assert!(row_exists(&fixture, "#omarchy"));

    assert!(fixture.controller.leave_selected_channel());
    assert!(fixture.controller.close_conversation(OMARCHY, "#omarchy"));
    assert!(!row_exists(&fixture, "#omarchy"));
}

#[test]
fn slash_close_closes_a_left_channel_and_leaves_a_joined_one_open() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(!fixture.controller.submit(CONVERSATION, "/close"));
    assert!(row_exists(&fixture, "#omarchy"));

    assert!(fixture.controller.submit(CONVERSATION, "/part"));
    assert!(fixture.controller.submit(CONVERSATION, "/close"));
    assert!(!row_exists(&fixture, "#omarchy"));
}
