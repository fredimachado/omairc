//! Minimized and hidden windows count as away for unread and mention notify.
//! controller-level parts of:
//! `focusWhileMinimizedDoesNotConsumeUnread`, `restoreWhileInactiveKeepsCatchUpOff`
//! `minimizedActiveWindowNotifiesMention`, `minimizedChannelFollowEndLandsOnMark`
//! `minimizedDirectFollowEndLandsOnMark`, and `hiddenWindowFollowEndLandsOnMark`.

use crate::controller::notify::Arrival;
use crate::desktop::windowvisibility::{TranscriptActivity, WindowVisibility};
use crate::irc::plaintext::plain_irc_text;
use crate::model::title::note_title_mark;

use super::fixture::{bodies, Fixture, OMARCHY};

fn apply_focus(activity: &mut TranscriptActivity, fixture: &mut Fixture, active: bool) {
    if let Some(effective) = activity.set_focus(active) {
        fixture.controller.set_window_active(effective);
    }
}

fn apply_visibility(
    activity: &mut TranscriptActivity,
    fixture: &mut Fixture,
    visibility: WindowVisibility,
) {
    if let Some(effective) = activity.set_visibility(visibility) {
        fixture.controller.set_window_active(effective);
    }
}

fn unread(fixture: &Fixture, target: &str) -> u32 {
    fixture
        .controller
        .conversations()
        .into_iter()
        .find(|row| row.target == target)
        .map(|row| row.unread)
        .expect("conversation row")
}

fn unread_mark(fixture: &Fixture) -> Option<i64> {
    fixture.controller.unread_mark()
}

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

fn deliver_burst(fixture: &mut Fixture, nick: &str, target: &str, token: &str) {
    fixture.deliver(
        OMARCHY,
        &format!(":{nick}!u@h PRIVMSG {target} :{token}-first"),
    );
    for index in 0..28 {
        fixture.deliver(
            OMARCHY,
            &format!(":{nick}!u@h PRIVMSG {target} :{token} filler {index}"),
        );
    }
}

fn expect_suspended_burst_lands_on_mark(
    fixture: &mut Fixture,
    activity: &mut TranscriptActivity,
    nick: &str,
    wire_target: &str,
    conversation: &str,
    token: &str,
    suspended: WindowVisibility,
) {
    assert!(activity.window_active);
    apply_visibility(activity, fixture, suspended);
    assert!(activity.window_active, "suspension must be able to leave OS focus true");
    assert!(!activity.effective());

    deliver_burst(fixture, nick, wire_target, token);
    let messages = bodies(&fixture.controller);
    let first = messages
        .iter()
        .position(|body| body == &format!("{token}-first"))
        .expect("first suspended line");
    assert!(first > 0, "the first suspended line should not lead the buffer");
    let mark = unread_mark(fixture).expect("a suspended window should plant the New messages mark");
    let first_seq = fixture.controller.messages()[first].sequence;
    assert_eq!(
        mark, first_seq,
        "the mark boundary is the first suspended arrival"
    );

    apply_visibility(activity, fixture, WindowVisibility::Visible);
    assert!(activity.effective());
    assert_eq!(
        unread(fixture, conversation),
        0,
        "restore while active consumes unread"
    );
    assert!(
        unread_mark(fixture).is_some(),
        "restore keeps the mark for the transcript to land on"
    );
}

#[test]
fn focus_while_minimized_does_not_consume_unread() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    let mut activity = TranscriptActivity::default();
    apply_visibility(&mut activity, &mut fixture, WindowVisibility::Minimized);
    assert!(activity.window_active);
    apply_focus(&mut activity, &mut fixture, true);

    deliver_burst(&mut fixture, "anna", "#omarchy", "focus-min");
    let mark = unread_mark(&fixture).expect("focus while minimized must not consume the backlog");
    let first_seq = fixture
        .controller
        .messages()
        .iter()
        .find(|message| message.body == "focus-min-first")
        .map(|message| message.sequence)
        .expect("first line");
    assert_eq!(mark, first_seq);
    let before = unread(&fixture, "#omarchy");
    assert!(before > 0);
    apply_focus(&mut activity, &mut fixture, true);
    assert_eq!(unread(&fixture, "#omarchy"), before);

    apply_visibility(&mut activity, &mut fixture, WindowVisibility::Visible);
    assert_eq!(unread(&fixture, "#omarchy"), 0);
    assert_eq!(unread_mark(&fixture), Some(mark));
}

#[test]
fn restore_while_inactive_keeps_catch_up_off() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    let mut activity = TranscriptActivity::default();
    apply_visibility(&mut activity, &mut fixture, WindowVisibility::Minimized);
    deliver_burst(&mut fixture, "anna", "#omarchy", "restore-away");
    let mark = unread_mark(&fixture).expect("mark");
    let unread_before = unread(&fixture, "#omarchy");
    assert!(unread_before > 0);

    // finishTranscriptSuspension(false): leave suspension without focus.
    activity.window_active = false;
    apply_visibility(&mut activity, &mut fixture, WindowVisibility::Visible);
    assert!(!activity.effective());
    fixture.deliver(OMARCHY, ":anna!u@h PRIVMSG #omarchy :restore-away-later");
    assert_eq!(unread(&fixture, "#omarchy"), unread_before + 1);

    apply_focus(&mut activity, &mut fixture, true);
    assert_eq!(unread(&fixture, "#omarchy"), 0);
    assert_eq!(unread_mark(&fixture), Some(mark));
}

#[test]
fn minimized_active_window_notifies_mention() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#ricing");
    let mut activity = TranscriptActivity::default();
    assert!(activity.effective());
    apply_visibility(&mut activity, &mut fixture, WindowVisibility::Minimized);
    assert!(activity.window_active);
    assert!(!activity.effective());
    fixture.controller.take_arrivals();

    fixture.deliver(OMARCHY, ":alice!u@h PRIVMSG #ricing :hey \x02fred");
    let mentions = mention_arrivals(&mut fixture);
    assert_eq!(mentions.len(), 1);
    assert_eq!(mentions[0].author, "alice");
    let mut title = None;
    note_title_mark(
        &mut title,
        activity.effective(),
        &mentions[0].author,
        &plain_irc_text(&mentions[0].body),
        &mentions[0].network_id,
        &mentions[0].target,
    );
    let mark = title.as_ref().expect("minimized window titles the mention");
    assert_eq!(mark.author, "alice");
    assert_eq!(mark.plain_body, "hey fred");

    fixture.deliver(OMARCHY, ":dax!u@h PRIVMSG fred :hello there");
    let directs = mention_arrivals(&mut fixture);
    assert_eq!(directs.len(), 1);
    assert_eq!(directs[0].author, "dax");
    note_title_mark(
        &mut title,
        activity.effective(),
        &directs[0].author,
        &plain_irc_text(&directs[0].body),
        &directs[0].network_id,
        &directs[0].target,
    );
    let mark = title.as_ref().expect("direct title");
    assert_eq!(mark.author, "dax");
    assert_eq!(mark.plain_body, "hello there");
}

#[test]
fn minimized_channel_follow_end_lands_on_mark() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    let mut activity = TranscriptActivity::default();
    expect_suspended_burst_lands_on_mark(
        &mut fixture,
        &mut activity,
        "anna",
        "#omarchy",
        "#omarchy",
        "channel-min",
        WindowVisibility::Minimized,
    );
}

#[test]
fn minimized_direct_follow_end_lands_on_mark() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "anna");
    let mut activity = TranscriptActivity::default();
    expect_suspended_burst_lands_on_mark(
        &mut fixture,
        &mut activity,
        "anna",
        "fred",
        "anna",
        "dm-min",
        WindowVisibility::Minimized,
    );
}

#[test]
fn hidden_window_follow_end_lands_on_mark() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    let mut activity = TranscriptActivity::default();
    expect_suspended_burst_lands_on_mark(
        &mut fixture,
        &mut activity,
        "anna",
        "#omarchy",
        "#omarchy",
        "channel-hide",
        WindowVisibility::Hidden,
    );
}
