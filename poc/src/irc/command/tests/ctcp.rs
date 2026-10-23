//! CTCP targets: which nick a /version or similar asks, and when it is refused.

use super::*;

#[test]
fn ctcp_verbs_without_a_nick_say_name_a_nick_on_a_channel() {
    for line in ["/whois", "/ping", "/time", "/version"] {
        let command = parse(line);
        assert_eq!(
            command_outcome_text(CommandOutcome::WrongScope, &command),
            "Name a nick",
            "{line}"
        );
    }
}

#[test]
fn version_on_a_direct_message_asks_the_peer() {
    let target = ctcp_target(
        &Words::default(),
        CONVERSATION,
        Some(SelectedView::Direct("anna")),
        CHANTYPES,
    )
    .expect("a DM has a peer");
    assert_eq!(
        target,
        CtcpTarget {
            nick: "anna".into(),
            in_conversation: true,
        }
    );
}

#[test]
fn version_from_status_stays_on_status() {
    let target = ctcp_target(
        &Words::default(),
        STATUS,
        Some(SelectedView::Direct("anna")),
        CHANTYPES,
    )
    .expect("the selected DM is the target");
    assert!(!target.in_conversation);
    assert_eq!(
        ctcp_target(&words("bob", ""), STATUS, None, CHANTYPES),
        Ok(CtcpTarget {
            nick: "bob".into(),
            in_conversation: false,
        })
    );
}

#[test]
fn version_on_a_channel_needs_a_nick() {
    assert_eq!(
        ctcp_target(
            &Words::default(),
            CONVERSATION,
            Some(SelectedView::Channel),
            CHANTYPES
        ),
        Err(CommandOutcome::WrongScope)
    );
    assert_eq!(
        command_outcome_text(CommandOutcome::WrongScope, &parse("/version")),
        "Name a nick"
    );
}

#[test]
fn version_with_nothing_selected_is_refused_when_empty() {
    assert_eq!(
        ctcp_target(&Words::default(), CONVERSATION, None, CHANTYPES),
        Err(CommandOutcome::Refused)
    );
}

#[test]
fn a_named_nick_is_asked_in_the_selected_conversation() {
    let target = ctcp_target(
        &words("bob", ""),
        CONVERSATION,
        Some(SelectedView::Channel),
        CHANTYPES,
    )
    .expect("a nick is a valid target");
    assert_eq!(target.nick, "bob");
    assert!(target.in_conversation);
}

#[test]
fn a_channel_target_is_refused() {
    assert_eq!(
        ctcp_target(
            &words("#linux", ""),
            CONVERSATION,
            Some(SelectedView::Direct("anna")),
            CHANTYPES
        ),
        Err(CommandOutcome::Refused)
    );
    assert_eq!(
        ctcp_target(&words("&local", ""), STATUS, None, CHANTYPES),
        Err(CommandOutcome::Refused)
    );
}

#[test]
fn extra_words_refuse_the_command() {
    assert_eq!(
        ctcp_target(
            &words("anna", "bob"),
            CONVERSATION,
            Some(SelectedView::Channel),
            CHANTYPES
        ),
        Err(CommandOutcome::Refused)
    );
    assert_eq!(
        ctcp_target(
            &Words::split("anna  "),
            CONVERSATION,
            Some(SelectedView::Channel),
            CHANTYPES
        )
        .map(|target| target.nick),
        Ok("anna".to_string())
    );
}

#[test]
fn a_nick_with_no_selection_on_a_conversation_is_wrong_scope() {
    assert_eq!(
        ctcp_target(&words("anna", ""), CONVERSATION, None, CHANTYPES),
        Err(CommandOutcome::WrongScope)
    );
}

#[test]
fn the_version_reply_text_matches_qt() {
    assert_eq!(
        version_reply_text("anna"),
        "VERSION reply from anna: https://omairc.app"
    );
}
