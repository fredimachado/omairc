//! The ignore, mute and highlight stores over the real demo: an ignored nick's
//! traffic never reaches the transcript, a mute flags the conversation, and a
//! highlight word raises a mention.
//! lists (`tui/internal/controller/stores.go`).

use crate::controller::notify::Arrival;

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

/// Whether the controller holds a conversation for `target` on the seeded network.
fn has_conversation(fixture: &Fixture, target: &str) -> bool {
    fixture
        .controller
        .conversations()
        .iter()
        .any(|row| row.target.eq_ignore_ascii_case(target))
}

#[test]
fn an_ignored_nicks_direct_message_never_opens_a_conversation() {
    let mut fixture = Fixture::seeded();
    assert!(fixture.controller.ignore_nick(OMARCHY, "spammer"));
    assert_eq!(fixture.controller.ignored_nicks(OMARCHY), ["spammer"]);

    fixture.deliver(OMARCHY, ":spammer!u@h PRIVMSG fred :buy now");
    assert!(!has_conversation(&fixture, "spammer"), "the DM is dropped before the reducer");

    assert!(fixture.controller.unignore_nick(OMARCHY, "spammer"));
    fixture.deliver(OMARCHY, ":spammer!u@h PRIVMSG fred :hello again");
    assert!(has_conversation(&fixture, "spammer"), "unignored, the DM arrives");
}

#[test]
fn an_ignored_nick_is_matched_under_the_network_case_mapping() {
    let mut fixture = Fixture::seeded();
    fixture.controller.ignore_nick(OMARCHY, "Spam[bot]");
    fixture.deliver(OMARCHY, ":spam{bot}!u@h PRIVMSG fred :hi");
    assert!(
        !has_conversation(&fixture, "spam{bot}"),
        "RFC 1459 folds the brackets, so the variant is ignored"
    );
}

#[test]
fn an_ignored_nicks_notice_is_dropped_even_in_a_channel() {
    let mut fixture = Fixture::seeded();
    fixture.controller.ignore_nick(OMARCHY, "spammer");
    let before = fixture.controller.messages().len();

    fixture.deliver(OMARCHY, ":spammer!u@h NOTICE #omarchy :promo");
    assert_eq!(fixture.controller.messages().len(), before, "the NOTICE is dropped");
}

#[test]
fn an_ignored_nicks_channel_message_still_arrives() {
    let mut fixture = Fixture::seeded();
    fixture.controller.ignore_nick(OMARCHY, "spammer");
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    let before = fixture.controller.messages().len();

    fixture.deliver(OMARCHY, ":spammer!u@h PRIVMSG #omarchy :hi everyone");
    assert_eq!(
        fixture.controller.messages().len(),
        before + 1,
        "the predicate only drops direct traffic for a channel line"
    );
}

#[test]
fn muting_a_channel_flags_it_and_unmuting_clears_the_flag() {
    let mut fixture = Fixture::seeded();
    assert!(fixture.controller.apply_mute(OMARCHY, "#omarchy", true));
    assert!(fixture.controller.is_muted_target(OMARCHY, "#omarchy"));
    let muted = fixture
        .controller
        .conversations()
        .into_iter()
        .find(|row| row.target == "#omarchy")
        .expect("the channel is listed");
    assert!(muted.muted, "the conversation is muted");

    assert!(fixture.controller.apply_mute(OMARCHY, "#omarchy", false));
    assert!(!fixture.controller.is_muted_target(OMARCHY, "#omarchy"));
    let open = fixture
        .controller
        .conversations()
        .into_iter()
        .find(|row| row.target == "#omarchy")
        .expect("the channel is listed");
    assert!(!open.muted);
}

#[test]
fn a_muted_channel_does_not_mention_but_an_unmuted_one_does() {
    let mut fixture = Fixture::seeded();
    fixture.controller.apply_mute(OMARCHY, "#omarchy", true);
    fixture.deliver(OMARCHY, ":anna!u@h PRIVMSG #omarchy :fred: ping");
    assert!(mention_arrivals(&mut fixture).is_empty(), "muted, no mention");

    fixture.controller.apply_mute(OMARCHY, "#omarchy", false);
    fixture.deliver(OMARCHY, ":anna!u@h PRIVMSG #omarchy :fred: ping again");
    let mentions = mention_arrivals(&mut fixture);
    assert_eq!(mentions.len(), 1, "unmuted, mentions again");
}

#[test]
fn a_highlight_word_mentions_like_a_nick_until_it_is_removed() {
    let mut fixture = Fixture::seeded();
    assert!(fixture.controller.add_highlight_word(OMARCHY, "chowder"));
    assert!(!fixture.controller.add_highlight_word(OMARCHY, "CHOWDER"), "case-mapped duplicate");
    assert_eq!(fixture.controller.highlight_words(OMARCHY), ["chowder"]);
    fixture.deliver(OMARCHY, ":anna!u@h PRIVMSG #omarchy :the chowder is ready");
    assert_eq!(mention_arrivals(&mut fixture).len(), 1, "the word mentions");

    assert!(fixture.controller.remove_highlight_word(OMARCHY, "chowder"));
    fixture.deliver(OMARCHY, ":anna!u@h PRIVMSG #omarchy :more chowder");
    assert!(mention_arrivals(&mut fixture).is_empty(), "removed, no mention");
}
