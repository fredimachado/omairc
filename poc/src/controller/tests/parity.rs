//! The Phase 3 parity exit, read off the controller over the real demo. Each
//! assertion is a Qt observable the demo seed must reproduce: the titles' network
//! name, the sidebar counts, the member panel order and labels, the typing hint,
//! the inbox, and the Status `-AUTH-` line. The sidebar case ports
//! `TestDemo...` in `tui/internal/demo/demo_test.go`.

use crate::irc::inbox::InboxKind;
use crate::irc::reducer::MemberView;

use super::fixture::{status_text, Fixture, OFTC, OMARCHY};

/// The sidebar as `(network, target, unread, mention, typing, direct)`.
fn sidebar(fixture: &Fixture) -> Vec<(String, String, u32, bool, bool, bool)> {
    fixture
        .controller
        .conversations()
        .into_iter()
        .map(|row| {
            (
                row.key.network_id.clone(),
                row.target.clone(),
                row.unread,
                row.mention,
                row.typing,
                row.direct,
            )
        })
        .collect()
}

fn row(network: &str, target: &str, unread: u32, mention: bool, typing: bool, direct: bool)
    -> (String, String, u32, bool, bool, bool)
{
    (
        network.to_owned(),
        target.to_owned(),
        unread,
        mention,
        typing,
        direct,
    )
}

/// The member panel's rank label, one word per member, in panel order.
fn panel_labels(members: &[MemberView]) -> String {
    members
        .iter()
        .map(|member| member.label.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn both_seeded_networks_register_and_show_connected() {
    let fixture = Fixture::seeded();
    assert_eq!(fixture.controller.connection_status_for(OMARCHY), "Connected");
    assert_eq!(fixture.controller.connection_status_for(OFTC), "Connected");
    assert_eq!(fixture.controller.network_order(), [OMARCHY, OFTC]);
    assert_eq!(fixture.controller.network_name(OMARCHY), "irc.example");
    assert_eq!(fixture.controller.network_name(OFTC), "irc.example");
    assert_eq!(fixture.controller.selected_network_id(), Some(OMARCHY));
    assert_eq!(fixture.controller.selected_target(), "#omarchy");
    assert!(!fixture.controller.shows_status());
}

#[test]
fn sidebar_rows_and_unread_counts_match_qt() {
    let fixture = Fixture::seeded();
    assert_eq!(
        sidebar(&fixture),
        vec![
            row(OMARCHY, "#desktop", 3, false, false, false),
            row(OMARCHY, "#help", 0, false, false, false),
            row(OMARCHY, "#omarchy", 0, false, false, false),
            row(OMARCHY, "#ricing", 12, true, false, false),
            row(OMARCHY, "anna", 1, true, true, true),
            row(OMARCHY, "dax", 0, false, false, true),
            row(OFTC, "#build", 2, false, false, false),
            row(OFTC, "#lab", 0, false, false, false),
            row(OFTC, "#omarchy", 0, false, false, false),
            row(OFTC, "rio", 0, false, false, true),
        ]
    );
    assert_eq!(fixture.controller.unread_count_for(OMARCHY), 16);
    assert!(fixture.controller.mention_for(OMARCHY));
}

#[test]
fn member_panel_orders_by_prefix_rank_then_nick() {
    let fixture = Fixture::seeded();
    let members = fixture.controller.members();
    let nicks: Vec<&str> = members.iter().map(|member| member.nick.as_str()).collect();
    assert_eq!(
        nicks,
        ["fred", "anna", "dax", "mira", "kai", "teo", "ivy", "lena", "max", "nora", "sam", "sol"]
    );
    assert_eq!(fixture.controller.people_count(), 12);
    assert_eq!(
        panel_labels(&members),
        "~fred &anna @dax @mira %kai +teo ivy lena max nora sam sol"
    );
}

#[test]
fn anna_is_typing_in_the_selected_channel_and_the_sidebar_row() {
    let fixture = Fixture::seeded();
    assert_eq!(fixture.controller.typing_nicks(), ["anna"]);
    assert!(fixture.controller.conversations().iter().any(|row| {
        row.target == "anna" && row.typing
    }));
}

#[test]
fn the_inbox_holds_the_seeded_mentions_and_the_oftc_invite() {
    // Three rows, not the two the plan and features/inbox.md name. The seed's anna
    // direct is a live line that names fred, and the Go reference TUI (run against
    // this seed) and the Qt reducer both plant a Mention row for it. Newest first.
    let fixture = Fixture::seeded();
    let items = fixture.controller.inbox_items();
    assert_eq!(items.len(), 3, "rows: {items:?}");
    let anna = items
        .iter()
        .find(|item| item.kind == InboxKind::Mention && item.target == "anna")
        .expect("the anna direct's mention row");
    assert_eq!(anna.network_id, OMARCHY);
    assert_eq!(anna.actor, "anna");
    let mention = items
        .iter()
        .find(|item| item.kind == InboxKind::Mention && item.target == "#ricing")
        .expect("a mention row");
    assert_eq!(mention.network_id, OMARCHY);
    assert_eq!(mention.actor, "mira");
    let invite = items
        .iter()
        .find(|item| item.kind == InboxKind::Invite)
        .expect("an invite row");
    assert_eq!(invite.network_id, OFTC);
    assert_eq!(invite.target, "#debian");
    assert_eq!(invite.actor, "rio");
}

#[test]
fn status_carries_the_auth_notice_and_stays_off_the_sidebar() {
    let fixture = Fixture::seeded();
    let lines = status_text(&fixture.controller, OMARCHY);
    assert!(
        lines
            .iter()
            .any(|line| line == "-AUTH- *** Looking up your hostname..."),
        "status lines: {lines:?}"
    );
    assert!(
        !fixture
            .controller
            .conversations()
            .iter()
            .any(|row| row.target == "AUTH"),
        "AUTH is not a direct message"
    );
}

#[test]
fn the_seeded_transcript_reads_for_the_selected_channel() {
    let fixture = Fixture::seeded();
    assert!(!fixture.controller.messages().is_empty());
    assert!(!fixture.controller.topic().is_empty());
}

#[test]
fn the_seed_claims_the_selection_before_the_first_conversation_does() {
    let fixture = Fixture::seeded();
    assert!(!fixture.controller.shows_status());
    assert!(fixture.controller.channel_joined());
    assert!(fixture.controller.is_channel());
}

#[test]
fn the_seed_leaves_nothing_queued_for_the_first_tick() {
    let mut fixture = Fixture::seeded();
    let notifications = fixture.controller.take_notifications();
    assert!(
        !notifications.is_empty(),
        "the seed dirties the shell's surfaces"
    );
    fixture.controller.tick();
    assert!(fixture.controller.take_notifications().is_empty());
}
