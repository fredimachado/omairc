//! `/status` and `/avatar` over the real demo, and the replies that answer them.
//! cases in `tui/internal/controller/replyrouter_test.go` (`TestReplyOwnMetadataFail`
//! `TestReplyOwnMetadataReply`, `TestReplyOwnMetadataError`, `TestReplyDispatchStatusAndAvatar`).
//! The demo answers a write with 761 or 766 at once. A test that needs the write unanswered
//! clears the demo's answer hook, so the watch stays armed for the FAIL or the numeric it injects.

use crate::irc::capability::IrcCapability;
use crate::irc::command::{parse, CommandOutcome, ComposerSurface};
use crate::irc::statusentry::{self, LogSeverity};
use crate::irc::world::MessageKind;
use crate::session::clock::Clock;

use super::fixture::{Fixture, OMARCHY};

const CONVERSATION: ComposerSurface = ComposerSurface::Conversation;
const STATUS: ComposerSurface = ComposerSurface::Status;

/// Dispatches one typed line on `surface` and returns its outcome.
fn run(fixture: &mut Fixture, surface: ComposerSurface, line: &str) -> CommandOutcome {
    fixture.controller.dispatch(surface, &parse(line))
}

/// Whether the transcript has a whois-style row with exactly `body`.
fn has_row(fixture: &Fixture, body: &str) -> bool {
    fixture
        .controller
        .messages()
        .iter()
        .any(|row| row.kind == MessageKind::Whois && row.body == body)
}

/// Stops the demo from answering the next write, so its watch stays armed.
fn silence_demo(fixture: &Fixture) {
    fixture
        .controller
        .demo()
        .and_then(|demo| demo.transport(OMARCHY))
        .expect("the omarchy loopback")
        .clear_frame_written_hook();
}

#[test]
fn status_set_is_confirmed_into_the_selected_conversation() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/status Heads down"), CommandOutcome::Sent);
    assert_eq!(
        fixture.frames(OMARCHY),
        ["METADATA * SET status :Heads down\r\n"]
    );
    assert!(has_row(&fixture, "Standing status set to Heads down."));
    assert_eq!(
        fixture.controller.reducer.nick_presence(OMARCHY, "fred").status(),
        "Heads down",
        "the 761 reached the reducer too"
    );
}

#[test]
fn status_clear_writes_the_bare_set_and_the_unset_is_confirmed() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert_eq!(run(&mut fixture, CONVERSATION, "/status Heads down"), CommandOutcome::Sent);
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/status clear"), CommandOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["METADATA * SET status\r\n"]);
    assert!(has_row(&fixture, "Standing status cleared."));
}

#[test]
fn status_without_an_argument_inspects_the_stored_value_or_says_there_is_none() {
    // The seed gives `fred` a standing status, so the first inspect reads it back.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert_eq!(run(&mut fixture, CONVERSATION, "/status"), CommandOutcome::Sent);
    assert!(has_row(&fixture, "Standing status: building Omairc"));

    assert_eq!(run(&mut fixture, CONVERSATION, "/status clear"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/status"), CommandOutcome::Sent);
    assert!(has_row(
        &fixture,
        "No standing status. Use /status <text> or /status clear."
    ));

    assert_eq!(run(&mut fixture, CONVERSATION, "/status Heads down"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/status"), CommandOutcome::Sent);
    assert!(has_row(&fixture, "Standing status: Heads down"));
}

#[test]
fn avatar_url_is_set_and_confirmed() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(
        run(&mut fixture, CONVERSATION, "/avatar https://omairc.app/omairc128.png"),
        CommandOutcome::Sent
    );
    assert_eq!(
        fixture.frames(OMARCHY),
        ["METADATA * SET avatar :https://omairc.app/omairc128.png\r\n"]
    );
    assert!(has_row(
        &fixture,
        "Avatar set to https://omairc.app/omairc128.png."
    ));
}

#[test]
fn an_email_avatar_is_stored_as_a_gravatar_url() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/avatar Fred@Example.com"), CommandOutcome::Sent);
    let frame = fixture.frames(OMARCHY).pop().expect("the set was written");
    assert!(
        frame.starts_with("METADATA * SET avatar :https://www.gravatar.com/avatar/"),
        "{frame}"
    );
    assert!(fixture
        .controller
        .messages()
        .iter()
        .any(|row| row.kind == MessageKind::Whois
            && row.body.starts_with("Avatar set to https://www.gravatar.com/avatar/")));
}

#[test]
fn avatar_clear_unsets_and_plain_http_is_refused_without_a_frame() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert_eq!(
        run(&mut fixture, CONVERSATION, "/avatar https://omairc.app/omairc128.png"),
        CommandOutcome::Sent
    );
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/avatar clear"), CommandOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["METADATA * SET avatar\r\n"]);
    assert!(has_row(&fixture, "Avatar cleared."));

    fixture.clear_frames(OMARCHY);
    assert_eq!(
        run(&mut fixture, CONVERSATION, "/avatar http://insecure.example/a.png"),
        CommandOutcome::Sent
    );
    assert!(fixture.frames(OMARCHY).is_empty());
    assert!(has_row(&fixture, "Avatar must be an HTTPS URL or an email address."));
}

#[test]
fn a_status_the_server_refuses_reports_the_reason_on_the_watch() {
    // Go `TestReplyOwnMetadataFail`, the generic fail on a set watch.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    silence_demo(&fixture);
    assert_eq!(run(&mut fixture, CONVERSATION, "/status busy"), CommandOutcome::Sent);
    fixture.deliver(OMARCHY, ":server FAIL METADATA VALUE_INVALID status :Too long");
    assert!(has_row(&fixture, "Could not set standing status: VALUE_INVALID Too long"));
    assert!(fixture.controller.reply_watches.own_metadata_count() == 0);
}

#[test]
fn a_clear_the_server_says_was_not_set_reads_as_already_clear() {
    // Go `TestReplyOwnMetadataFail`, KEY_NOT_SET on a clear watch.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    silence_demo(&fixture);
    assert_eq!(run(&mut fixture, CONVERSATION, "/avatar clear"), CommandOutcome::Sent);
    fixture.deliver(
        OMARCHY,
        ":server FAIL METADATA KEY_NOT_SET avatar :The key was not set",
    );
    assert!(has_row(&fixture, "Avatar cleared."));
    assert_eq!(fixture.controller.reply_watches.own_metadata_count(), 0);
}

#[test]
fn an_unknown_fail_code_is_ignored_and_the_watch_stays() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    silence_demo(&fixture);
    assert_eq!(run(&mut fixture, CONVERSATION, "/status busy"), CommandOutcome::Sent);
    fixture.deliver(OMARCHY, ":server FAIL METADATA SOMETHING_ELSE status :why");
    assert!(!has_row(&fixture, "Could not set standing status: SOMETHING_ELSE why"));
    fixture.deliver(OMARCHY, ":server 761 fred fred status * :busy");
    assert!(has_row(&fixture, "Standing status set to busy."), "the watch was kept for the 761");
}

#[test]
fn a_reply_from_another_nick_does_not_answer_the_write() {
    // Go `TestReplyOwnMetadataReply`, the self and the exact value.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    silence_demo(&fixture);
    assert_eq!(run(&mut fixture, CONVERSATION, "/status busy"), CommandOutcome::Sent);

    fixture.deliver(OMARCHY, ":server 761 fred anna status * :busy");
    fixture.deliver(OMARCHY, ":server 761 fred fred status * :wrong");
    assert!(!has_row(&fixture, "Standing status set to busy."));
    assert_eq!(fixture.controller.reply_watches.own_metadata_count(), 1, "a reply that does not match keeps the watch");

    fixture.deliver(OMARCHY, ":server 761 fred FRED status * :busy");
    assert!(has_row(&fixture, "Standing status set to busy."));
}

#[test]
fn a_numeric_refusal_names_the_key_and_ends_the_watch() {
    // Go `TestReplyOwnMetadataError`: the 764 reason is the text, key token included.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    silence_demo(&fixture);
    assert_eq!(run(&mut fixture, CONVERSATION, "/status busy"), CommandOutcome::Sent);
    let entry = statusentry::lifecycle(
        OMARCHY,
        LogSeverity::Alert,
        "764",
        "status :You may not set that",
        fixture.clock.now(),
    );
    fixture.controller.status_entry(entry);
    assert!(has_row(
        &fixture,
        "Could not set standing status: status :You may not set that"
    ));
    assert_eq!(fixture.controller.reply_watches.own_metadata_count(), 0);
}

#[test]
fn status_and_avatar_on_a_network_without_member_metadata_say_so() {
    // Go `TestReplyDispatchStatusAndAvatar`, the missing-capability text.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    let mut caps = fixture
        .controller
        .capabilities
        .get(OMARCHY)
        .copied()
        .expect("omarchy negotiated caps");
    caps.remove(IrcCapability::MemberMetadata);
    fixture.controller.capabilities.insert(OMARCHY.to_owned(), caps);
    fixture.clear_frames(OMARCHY);

    assert_eq!(run(&mut fixture, CONVERSATION, "/status busy"), CommandOutcome::Sent);
    assert!(has_row(&fixture, "This network does not support standing status."));
    assert_eq!(run(&mut fixture, CONVERSATION, "/avatar clear"), CommandOutcome::Sent);
    assert!(has_row(&fixture, "This network does not support avatars."));
    assert!(fixture.frames(OMARCHY).is_empty());
}

#[test]
fn status_refuses_with_no_session_and_no_selection_and_is_not_connected_on_status() {
    // Go `TestReplyDispatchStatusAndAvatar`, the session and selection branches.
    let mut fixture = Fixture::seeded();
    fixture.controller.clear_selection();
    assert_eq!(run(&mut fixture, CONVERSATION, "/status hi"), CommandOutcome::Refused);
    fixture.controller.open_status(OMARCHY);
    assert_eq!(run(&mut fixture, STATUS, "/status hi"), CommandOutcome::Sent);

    assert!(fixture.controller.disconnect(OMARCHY));
    assert_eq!(run(&mut fixture, STATUS, "/status hi"), CommandOutcome::NotConnected);
    assert_eq!(run(&mut fixture, STATUS, "/avatar hi"), CommandOutcome::NotConnected);
}
