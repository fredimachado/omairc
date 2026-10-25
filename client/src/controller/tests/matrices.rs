//! The feature matrices the verify skill walks, held at frame level. Each test names the
//! feature row it mirrors in `.cursor/skills/verify-omairc/features/slash-commands.md`,
//! `slash-complete.md`, or the conversation-cause rules (`.cursor/skills/conversation-cause`).
//! The typing frames are the composer's `+typing` TAGMSG. Frames come from the loopback, and
//! the demo answers the way a server would.

use std::time::Duration;

use crate::irc::command::ComposerSurface;
use crate::irc::event::{ConversationKey, IrcConversationCause};

use super::fixture::{Fixture, OMARCHY};

const CONVERSATION: ComposerSurface = ComposerSurface::Conversation;
const STATUS: ComposerSurface = ComposerSurface::Status;

/// Whether a conversation row exists for `target` on `network_id`.
fn has_conversation(fixture: &Fixture, network_id: &str, target: &str) -> bool {
    let key: ConversationKey = fixture.controller.reducer.conversation_key(network_id, target);
    fixture.controller.reducer.find(&key).is_some()
}

#[test]
fn join_then_close_refuses_on_the_channel_then_part_then_close_drops_it() {
    // The slash-commands feature's sequence: `/join #help` opens it, `/close` is refused while
    // the user is in it and keeps the composer text, `/part` keeps the row, and a `/close` after
    // the part drops it.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(fixture.controller.submit(CONVERSATION, "/join #help"));
    assert_eq!(fixture.controller.selected_target(), "#help");

    assert!(!fixture.controller.submit(CONVERSATION, "/close"));
    assert_eq!(fixture.controller.selected_target(), "#help");
    assert_eq!(
        fixture.controller.last_error_for(OMARCHY),
        Some("Close applies to direct messages and channels you have left")
    );

    assert!(fixture.controller.submit(CONVERSATION, "/part"));
    assert!(has_conversation(&fixture, OMARCHY, "#help"));
    assert!(fixture.controller.submit(CONVERSATION, "/close"));
    assert!(!has_conversation(&fixture, OMARCHY, "#help"));
}

#[test]
fn a_part_on_a_channel_you_have_left_sends_nothing_and_the_row_stays() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(fixture.controller.submit(CONVERSATION, "/part"));
    fixture.clear_frames(OMARCHY);
    assert!(fixture.controller.submit(CONVERSATION, "/part"));
    assert!(fixture.frames(OMARCHY).is_empty(), "a second part writes nothing");
    assert!(has_conversation(&fixture, OMARCHY, "#omarchy"));
}

#[test]
fn a_msg_to_a_nick_with_echo_message_does_not_open_a_direct_message() {
    // The conversation-cause matrix, case 1: `/msg` is a quiet send, so the server's echo of
    // the line is self-authored and never inserts the missing conversation.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(fixture.controller.submit(CONVERSATION, "/msg lena hello"));
    assert!(!has_conversation(&fixture, OMARCHY, "lena"));
    assert_eq!(fixture.controller.selected_target(), "#omarchy");
}

#[test]
fn a_nickserv_privmsg_does_not_open_a_direct_message() {
    // Case 2: an incoming service PRIVMSG never inserts a non-service direct message.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.deliver(
        OMARCHY,
        ":NickServ!NickServ@services PRIVMSG fred :This nickname is registered.",
    );
    assert!(!has_conversation(&fixture, OMARCHY, "NickServ"));
}

#[test]
fn a_human_privmsg_opens_a_direct_message() {
    // Case 3: an incoming PRIVMSG from a person opens the direct message it names.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.deliver(OMARCHY, ":lena!u@h PRIVMSG fred :hey there");
    assert!(has_conversation(&fixture, OMARCHY, "lena"));
}

#[test]
fn the_quiet_send_cause_never_inserts_and_user_open_does_for_a_direct_message() {
    // The conversation-cause table at the dispatcher: a quiet send leaves the row out, and
    // `/query` inserts it.
    let mut fixture = Fixture::seeded();
    let key = fixture.controller.reducer.conversation_key(OMARCHY, "milo");
    assert!(fixture
        .controller
        .reducer
        .ensure_conversation(&key, "milo", IrcConversationCause::QuietSend)
        .is_none());
    assert!(fixture
        .controller
        .reducer
        .ensure_conversation(&key, "milo", IrcConversationCause::UserOpen)
        .is_some());
}

#[test]
fn a_notice_to_a_nick_reaches_the_wire_without_opening_a_direct_message() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert!(fixture.controller.submit(CONVERSATION, "/notice milo hi"));
    assert_eq!(fixture.frames(OMARCHY), ["NOTICE milo :hi\r\n"]);
    assert!(!has_conversation(&fixture, OMARCHY, "milo"));
}

#[test]
fn a_refused_status_line_is_logged_and_the_composer_clears() {
    // The Status surface's rule: a refusal is a Status line and the line clears. The
    // conversation surface keeps the text instead.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.controller.open_status(OMARCHY);
    assert!(fixture.controller.submit(STATUS, "/close"));
    let last = fixture
        .controller
        .status_lines(OMARCHY)
        .last()
        .map(|entry| entry.text.clone())
        .unwrap_or_default();
    assert_eq!(
        last,
        "Close applies to direct messages and channels you have left"
    );
}

#[test]
fn an_empty_composer_line_is_never_submitted() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert!(!fixture.controller.submit(CONVERSATION, "   "));
    assert!(fixture.frames(OMARCHY).is_empty());
}

#[test]
fn typing_publishes_active_for_a_live_line_and_done_when_it_is_withdrawn() {
    // The `+typing` TAGMSG the composer sends, since the demo grants message-tags.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);

    fixture.controller.composer_text_changed("hi");
    assert_eq!(fixture.frames(OMARCHY), ["@+typing=active TAGMSG #omarchy\r\n"]);

    fixture.clock.advance(Duration::from_secs(3));
    fixture.controller.composer_text_changed("");
    assert_eq!(
        fixture.frames(OMARCHY),
        [
            "@+typing=active TAGMSG #omarchy\r\n",
            "@+typing=done TAGMSG #omarchy\r\n",
        ]
    );
    // A second withdrawal with nothing held publishes nothing.
    fixture.controller.composer_text_changed("");
    assert_eq!(fixture.frames(OMARCHY).len(), 2);
}

#[test]
fn a_me_line_is_live_and_a_command_line_withdraws_the_hint() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    fixture.controller.composer_text_changed("/me waves");
    fixture.clock.advance(Duration::from_secs(3));
    fixture.controller.composer_text_changed("/away brb");
    assert_eq!(
        fixture.frames(OMARCHY),
        [
            "@+typing=active TAGMSG #omarchy\r\n",
            "@+typing=done TAGMSG #omarchy\r\n",
        ]
    );
}

#[test]
fn typing_is_paced_to_one_active_per_interval() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    fixture.controller.composer_text_changed("h");
    fixture.controller.composer_text_changed("hi");
    assert_eq!(fixture.frames(OMARCHY).len(), 1, "the second keystroke is paced out");

    fixture.clock.advance(Duration::from_secs(4));
    fixture.controller.composer_text_changed("hi!");
    assert_eq!(fixture.frames(OMARCHY).len(), 2);
}

#[test]
fn typing_on_status_or_without_message_tags_writes_nothing() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.controller.open_status(OMARCHY);
    fixture.clear_frames(OMARCHY);
    fixture.controller.composer_text_changed("hi");
    assert!(fixture.frames(OMARCHY).is_empty(), "the Status console never publishes");

    fixture.controller.leave_status();
    let mut caps = fixture
        .controller
        .capabilities
        .get(OMARCHY)
        .copied()
        .expect("omarchy negotiated caps");
    caps.remove(crate::irc::capability::IrcCapability::MessageTags);
    fixture.controller.capabilities.insert(OMARCHY.to_owned(), caps);
    fixture.controller.composer_text_changed("hi again");
    assert!(fixture.frames(OMARCHY).is_empty(), "no message-tags, no hint");
}

#[test]
fn leaving_a_network_with_a_hint_open_withdraws_it_on_the_held_target() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.controller.composer_text_changed("typing");
    fixture.clear_frames(OMARCHY);
    fixture.clock.advance(Duration::from_secs(3));
    fixture.controller.select_conversation(OMARCHY, "anna");
    assert!(
        fixture.frames(OMARCHY).iter().any(|frame| frame == "@+typing=done TAGMSG #omarchy\r\n"),
        "switching away sends done on the target that held the hint"
    );
    // The held hint names the channel, not the new selection, so it cleared.
    assert!(fixture.controller.typing_target().is_none());
}

#[test]
fn without_echo_message_a_msg_echoes_locally_into_an_open_direct_and_never_opens_a_missing_one() {
    // The `/msg` fallback: with no `echo-message` the controller draws the line itself, and only
    // into a conversation that already exists (`EchoIfPresent`).
    let mut fixture = Fixture::seeded();
    let mut caps = fixture
        .controller
        .capabilities
        .get(OMARCHY)
        .copied()
        .expect("omarchy negotiated caps");
    caps.remove(crate::irc::capability::IrcCapability::EchoMessage);
    fixture.controller.capabilities.insert(OMARCHY.to_owned(), caps);
    fixture
        .controller
        .demo()
        .and_then(|demo| demo.transport(OMARCHY))
        .expect("the omarchy loopback")
        .clear_frame_written_hook();

    fixture.controller.select_conversation(OMARCHY, "anna");
    assert!(fixture.controller.submit(CONVERSATION, "/msg anna quiet hello"));
    let echoed = super::fixture::bodies(&fixture.controller)
        .iter()
        .filter(|body| body.as_str() == "quiet hello")
        .count();
    assert_eq!(echoed, 1, "one local echo, since the server did not echo");

    assert!(fixture.controller.submit(CONVERSATION, "/msg lena nobody sees this"));
    assert!(!has_conversation(&fixture, OMARCHY, "lena"));
}
