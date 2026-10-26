//! Inbound lines over the real demo: unread and mention counts, the notifier, and
//! the session inbox.
//! (`TestInboxMentionArrivalAppendsSnapshot`, `TestInboxConsumedOnSelect`
//! `TestInboxInviteAppendActivateJoins`) and `TestBackgroundChatBumpsConversationEpoch`.

use crate::controller::notify::Arrival;
use crate::irc::inbox::InboxKind;
use crate::irc::viewnotify::ViewNotify;
use crate::session::clock::Clock;

use super::fixture::{Fixture, OMARCHY};

fn mention_arrivals(fixture: &mut Fixture) -> Vec<crate::irc::event::MentionArrival> {
    fixture
        .controller
        .take_arrivals()
        .into_iter()
        .filter_map(|arrival| match arrival {
            Arrival::Mention(mention) => Some(mention),
            Arrival::Monitor { .. } => None,
        })
        .collect()
}

fn unread(fixture: &Fixture, target: &str) -> (u32, bool) {
    fixture
        .controller
        .conversations()
        .into_iter()
        .find(|row| row.target == target)
        .map(|row| (row.unread, row.mention))
        .expect("conversation row")
}

#[test]
fn a_background_line_bumps_the_unread_count_without_a_mention() {
    let mut fixture = Fixture::seeded();
    assert_eq!(unread(&fixture, "#help"), (0, false));
    fixture.deliver(
        OMARCHY,
        ":kai!u@h PRIVMSG #help :anyone around for a quick question",
    );
    assert_eq!(unread(&fixture, "#help"), (1, false));
    assert!(mention_arrivals(&mut fixture).is_empty());
}

#[test]
fn a_line_naming_me_raises_a_mention_and_notifies_once() {
    let mut fixture = Fixture::seeded();
    fixture.deliver(OMARCHY, ":kai!u@h PRIVMSG #help :fred, can you look?");
    let (count, mention) = unread(&fixture, "#help");
    assert_eq!(count, 1);
    assert!(mention);
    let mentions = mention_arrivals(&mut fixture);
    assert_eq!(mentions.len(), 1, "one arrival per mention");
    assert_eq!(mentions[0].body, "fred, can you look?");
    assert_eq!(mentions[0].target, "#help");
    assert_eq!(mentions[0].author, "kai");
}

#[test]
fn a_direct_message_from_a_peer_bumps_its_row() {
    let mut fixture = Fixture::seeded();
    assert_eq!(unread(&fixture, "dax"), (0, false));
    fixture.deliver(OMARCHY, ":dax!u@h PRIVMSG fred :are you on the stream?");
    assert_eq!(unread(&fixture, "dax"), (1, false));
}

#[test]
fn an_inbound_line_raises_the_surfaces_the_shell_repaints() {
    let mut fixture = Fixture::seeded();
    fixture.controller.take_notifications();
    fixture.deliver(OMARCHY, ":kai!u@h PRIVMSG #help :hello");
    let surfaces = fixture.controller.take_notifications();
    assert!(surfaces.contains(&ViewNotify::Conversations), "{surfaces:?}");
    assert!(surfaces.contains(&ViewNotify::Messages), "{surfaces:?}");
    assert!(
        !surfaces.contains(&ViewNotify::Status),
        "a chat line is not a Status line: {surfaces:?}"
    );
}

#[test]
fn a_mention_row_lands_in_the_inbox_and_selecting_its_channel_consumes_it() {
    let mut fixture = Fixture::seeded();
    fixture.deliver(OMARCHY, ":kai!u@h PRIVMSG #help :fred ping");
    let kinds: Vec<_> = fixture
        .controller
        .inbox_items()
        .iter()
        .filter(|item| item.target == "#help")
        .map(|item| item.kind)
        .collect();
    assert_eq!(kinds, [InboxKind::Mention]);

    fixture.controller.select_conversation(OMARCHY, "#help");
    assert!(fixture
        .controller
        .inbox_items()
        .iter()
        .all(|item| item.target != "#help"));
}

#[test]
fn an_invite_lands_in_the_inbox_with_its_inviter_and_channel() {
    let mut fixture = Fixture::seeded();
    let before = fixture.controller.inbox_items().len();
    fixture.deliver(OMARCHY, ":anna!u@h INVITE fred :#newroom");
    let items = fixture.controller.inbox_items();
    assert_eq!(items.len(), before + 1);
    let invite = &items[0];
    assert_eq!(invite.kind, InboxKind::Invite);
    assert_eq!(invite.actor, "anna");
    assert_eq!(invite.target, "#newroom");
    assert_eq!(invite.network_id, OMARCHY);
}

#[test]
fn the_inbox_rows_carry_the_arrival_instant() {
    let mut fixture = Fixture::seeded();
    fixture.deliver(OMARCHY, ":anna!u@h INVITE fred :#stamped");
    let invite = fixture
        .controller
        .inbox_items()
        .iter()
        .find(|item| item.target == "#stamped")
        .expect("the invite row");
    assert_eq!(invite.timestamp, Clock::now(fixture.clock.as_ref()));
}

#[test]
fn mark_all_read_leaves_the_inbox_rows_in_place() {
    let mut fixture = Fixture::seeded();
    let before = fixture.controller.inbox_items().len();
    fixture.controller.mark_all_read();
    assert_eq!(fixture.controller.inbox_items().len(), before);
}
