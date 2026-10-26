//! The reply router over the real demo: a WHOIS and a CTCP query write their frames, the
//! labeled replies the test reads back from those frames copy to the destination, and the
//! watch ends with the reply.
//! `tui/internal/controller/replyrouter_test.go`. The demo answers CTCP but not WHOIS, so
//! the WHOIS replies are injected with the label the frame carried, the way a labeled
//! server answers.

use crate::irc::event::ConversationKey;
use crate::irc::world::MessageKind;

use super::fixture::{Fixture, OMARCHY};

/// The `label` tag of a frame, if it carries one.
fn label_of(frame: &str) -> Option<String> {
    let tags = frame.strip_prefix('@')?;
    let end = tags.find(' ')?;
    tags[..end]
        .split(';')
        .find_map(|tag| tag.strip_prefix("label="))
        .map(str::to_owned)
}

/// The key of a channel on the seeded network, as a destination.
fn channel(fixture: &Fixture, name: &str) -> ConversationKey {
    fixture.controller.reducer.conversation_key(OMARCHY, name)
}

fn last_frame(fixture: &Fixture) -> String {
    fixture.frames(OMARCHY).last().cloned().expect("a frame was written")
}

#[test]
fn a_whois_is_labeled_and_its_replies_reach_the_destination() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    let destination = channel(&fixture, "#omarchy");
    fixture.clear_frames(OMARCHY);

    assert!(fixture.controller.whois(OMARCHY, "anna", Some(destination)));
    let frame = last_frame(&fixture);
    assert!(frame.ends_with("WHOIS anna anna\r\n"), "the nick twice asks for idle time too: {frame:?}");
    let label = label_of(&frame).expect("labeled-response is negotiated, so the WHOIS is labeled");
    assert_eq!(fixture.controller.reply_watches.labeled_count(), 1);

    // A WHOIS answers in a labeled-response batch, so the request stays open until the
    // batch closes and the label is not finished by its first detail line.
    fixture.deliver(OMARCHY, &format!("@label={label} :server BATCH +wb labeled-response"));
    fixture.deliver(
        OMARCHY,
        "@batch=wb :server 311 fred anna anna host * :Anna Example",
    );
    assert!(
        fixture
            .controller
            .messages()
            .iter()
            .any(|row| row.kind == MessageKind::Whois),
        "the detail line reaches the destination transcript"
    );
    assert_eq!(
        fixture.controller.reply_watches.labeled_count(),
        1,
        "the watch waits for the end"
    );

    fixture.deliver(OMARCHY, "@batch=wb :server 318 fred anna :End of /WHOIS list");
    assert_eq!(
        fixture.controller.reply_watches.labeled_count(),
        0,
        "318 ends the labeled WHOIS"
    );
    fixture.deliver(OMARCHY, "BATCH -wb");
    assert_eq!(fixture.controller.reply_watches.labeled_count(), 0);
}

#[test]
fn a_whois_without_a_destination_leaves_the_transcript_alone() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    let rows = fixture.controller.messages().len();
    fixture.clear_frames(OMARCHY);

    assert!(fixture.controller.whois(OMARCHY, "anna", None));
    let label = label_of(&last_frame(&fixture)).expect("labeled");
    fixture.deliver(
        OMARCHY,
        &format!("@label={label} :server 311 fred anna anna host * :Anna Example"),
    );
    assert_eq!(fixture.controller.messages().len(), rows);
}

#[test]
fn a_request_label_finished_drops_its_labeled_watch() {
    let mut fixture = Fixture::seeded();
    fixture.controller.whois(OMARCHY, "anna", None);
    let label = label_of(&last_frame(&fixture)).expect("labeled");
    assert_eq!(fixture.controller.reply_watches.labeled_count(), 1);

    fixture.controller.request_label_finished(OMARCHY, &label);
    assert_eq!(fixture.controller.reply_watches.labeled_count(), 0);
}

#[test]
fn a_ctcp_ping_is_answered_into_the_destination_and_the_watch_ends() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    let destination = channel(&fixture, "#omarchy");
    fixture.clear_frames(OMARCHY);

    assert!(fixture
        .controller
        .ctcp_query(OMARCHY, "anna", "PING", "1234", Some(destination)));
    assert!(
        fixture.frames(OMARCHY).iter().any(|frame| frame.contains("PING 1234")),
        "the query is written: {:?}",
        fixture.frames(OMARCHY)
    );
    assert!(
        fixture
            .controller
            .messages()
            .iter()
            .any(|row| row.body.contains("anna")),
        "the demo's PING reply lands on the destination"
    );
    assert_eq!(fixture.controller.reply_watches.plain_count(), 0, "the watch ended with the reply");
}

#[test]
fn a_ctcp_query_to_an_unknown_network_is_refused() {
    let mut fixture = Fixture::seeded();
    assert!(!fixture.controller.ctcp_query("nowhere", "anna", "TIME", "", None));
    assert!(!fixture.controller.whois("nowhere", "anna", None));
}
