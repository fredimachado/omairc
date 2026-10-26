//! Which surfaces an event dirties, read from the reducer after the fold.
//! of the three `ClassifyViewNotify` tests in `reducer_test.go`.

use super::*;
use crate::irc::event::Event;
use crate::irc::presence::{AVATAR_KEY, STATUS_KEY};
use crate::irc::reducer::classify_view_notify;
use crate::irc::viewnotify::ViewNotify;

fn metadata_notify(reducer: &EventReducer, nick: &str, key: &str) -> Vec<ViewNotify> {
    classify_view_notify(&Event::MemberMetadata(metadata(NETWORK_A, nick, key, "x")), reducer)
}

fn away_notify(reducer: &EventReducer, nick: &str, away_record: Option<crate::irc::presence::Away>) -> Vec<ViewNotify> {
    classify_view_notify(&Event::Away(away_event(NETWORK_A, nick, away_record)), reducer)
}

#[test]
fn metadata_notify_refreshes_only_affected_surfaces() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);

    let status_only = metadata_notify(&reducer, "Alice", STATUS_KEY);
    assert!(status_only.contains(&ViewNotify::MemberRow("alice".to_owned())), "status members");
    assert!(!status_only.contains(&ViewNotify::Conversations), "status conversations");
    assert!(!status_only.contains(&ViewNotify::Messages), "status messages");

    fold(&mut reducer, join(NETWORK_A, "#omarchy", "Alice"));
    let avatar_shared = metadata_notify(&reducer, "Alice", AVATAR_KEY);
    assert!(avatar_shared.contains(&ViewNotify::Messages), "shared avatar messages");
    assert!(!avatar_shared.contains(&ViewNotify::Conversations), "shared avatar conversations");

    let alice = reducer.conversation_key(NETWORK_A, "Alice");
    fold(&mut reducer, message(&alice, "Alice", "hi", "Alice"));
    let avatar_direct = metadata_notify(&reducer, "Alice", AVATAR_KEY);
    assert!(avatar_direct.contains(&ViewNotify::Messages), "direct avatar messages");
    assert!(avatar_direct.contains(&ViewNotify::Conversations), "direct avatar conversations");
}

#[test]
fn away_reloads_conversations_only_when_it_can_reach_a_direct_row() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);

    let orphan = away_notify(&reducer, "Alice", away("lunch"));
    assert!(!orphan.contains(&ViewNotify::Conversations), "orphan conversations");
    assert!(orphan.contains(&ViewNotify::MemberRow("alice".to_owned())), "orphan members");

    fold(&mut reducer, join(NETWORK_A, "#omarchy", "Alice"));
    let shared = away_notify(&reducer, "Alice", away("lunch"));
    assert!(shared.contains(&ViewNotify::Conversations), "shared conversations");

    fold(&mut reducer, quit(NETWORK_A, "Alice"));
    let alice = reducer.conversation_key(NETWORK_A, "Alice");
    fold(&mut reducer, message(&alice, "Alice", "hi", "Alice"));
    assert!(reducer.find(&alice).is_some(), "direct row present");
    let direct = away_notify(&reducer, "Alice", away("lunch"));
    assert!(direct.contains(&ViewNotify::Conversations), "direct conversations");

    fold(&mut reducer, away_event(NETWORK_A, "Alice", away("lunch")));
    let cleared = away_notify(&reducer, "Alice", None);
    assert!(cleared.contains(&ViewNotify::Conversations), "cleared conversations");
    assert!(cleared.contains(&ViewNotify::MemberRow("alice".to_owned())), "cleared members");
}

#[test]
fn account_change_refreshes_member_row_and_transcript() {
    let mut reducer = EventReducer::new();
    welcome_omairc(&mut reducer, NETWORK_A);
    fold(&mut reducer, join(NETWORK_A, "#room", "Alice"));

    let notify = classify_view_notify(&Event::Account(account_event(NETWORK_A, "Alice", "*")), &reducer);
    assert!(notify.contains(&ViewNotify::MemberRow("alice".to_owned())), "members");
    assert!(notify.contains(&ViewNotify::Messages), "messages");
    assert!(!notify.contains(&ViewNotify::Conversations), "conversations");
}
