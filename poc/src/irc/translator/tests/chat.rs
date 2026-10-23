//! NOTICE, PRIVMSG, CTCP ACTION, typing TAGMSG, and channel-context delivery.

use super::*;

#[test]
fn notice_updates_account_but_produces_no_transcript_event() {
    assert!(translate_line(":Alice!u@h NOTICE #room :hi").is_empty());

    let events = translate_line("@account=services :Alice!u@h NOTICE #room :hi");
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::Account(account) => {
            assert_eq!(account.nick, "Alice");
            assert_eq!(account.account, "services");
        }
        other => panic!("want AccountEvent, got {other:?}"),
    }
}

#[test]
fn privmsg_routes_channel_and_direct_display_targets() {
    let events = translate_line(":alice!u@h PRIVMSG #room :hello");
    assert_eq!(events.len(), 1);
    let channel = message_of(&events);
    assert_eq!(channel.conversation.normalized_target, "#room");
    assert_eq!(channel.author, "alice");
    assert_eq!(channel.body, "hello");
    assert_eq!(channel.target, "#room");

    let events = translate_line(":alice!u@h PRIVMSG omairc :hi");
    assert_eq!(events.len(), 1);
    let inbound = message_of(&events);
    assert_eq!(inbound.conversation.normalized_target, "alice");
    assert_eq!(inbound.target, "alice");

    let events = translate_line(":omairc!u@h PRIVMSG alice :hi");
    assert_eq!(events.len(), 1);
    let outbound = message_of(&events);
    assert_eq!(outbound.conversation.normalized_target, "alice");
    assert_eq!(outbound.target, "alice");
}

#[test]
fn ctcp_action_is_an_emote_and_other_ctcp_is_dropped() {
    let events = translate_line(":alice!u@h PRIVMSG #room :\u{1}ACTION waves\u{1}");
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::Action(action) => {
            assert_eq!(action.author, "alice");
            assert_eq!(action.body, "waves");
        }
        other => panic!("want ActionEvent, got {other:?}"),
    }
    assert!(translate_line(":alice!u@h PRIVMSG #room :\u{1}VERSION\u{1}").is_empty());
}

#[test]
fn typing_tagmsg_reads_the_phase_and_ignores_the_rest() {
    let events = translate_line("@+typing=active :alice!u@h TAGMSG #omarchy");
    assert_eq!(events.len(), 1);
    match &events[0] {
        Event::Typing(typing) => {
            assert_eq!(typing.nick, "alice");
            assert_eq!(typing.phase, TypingPhase::Active);
            assert_eq!(typing.conversation.normalized_target, "#omarchy");
        }
        other => panic!("want TypingEvent, got {other:?}"),
    }

    match translate_line("@+typing=PAUSED :alice!u@h TAGMSG omairc").first() {
        Some(Event::Typing(typing)) => {
            assert_eq!(typing.phase, TypingPhase::Paused);
            assert_eq!(typing.conversation.normalized_target, "alice");
        }
        other => panic!("want TypingEvent, got {other:?}"),
    }

    for line in [
        "@+typing=draft :alice!u@h TAGMSG #omarchy",
        ":alice!u@h TAGMSG #omarchy",
        "@+typing=active TAGMSG #omarchy",
    ] {
        assert!(
            translate_line(line).is_empty(),
            "{line} must produce nothing"
        );
    }

    let events = translate_line("@+typing=active :alice!u@h PRIVMSG #omarchy :here");
    assert_eq!(events.len(), 1);
    assert!(
        matches!(events[0], Event::Message(_)),
        "a typing tag on chat is still chat"
    );
}

#[test]
fn channel_context_private_line_is_only_visible_to_you() {
    let marked = translate_joined(
        "@+draft/channel-context=#room :bot!u@h PRIVMSG omairc :just you",
        &room_member_lookup,
    );
    assert_eq!(marked.len(), 1);
    let message = message_of(&marked);
    assert_eq!(message.conversation.normalized_target, "#room");
    assert_eq!(message.target, "#room");
    assert_eq!(message.body, "just you");
    assert!(message.only_visible_to_you);

    let unprefixed = translate_joined(
        "@draft/channel-context=#room :bot!u@h PRIVMSG omairc :also you",
        &room_member_lookup,
    );
    assert!(message_of(&unprefixed).only_visible_to_you);

    // The lookup's canonical spelling is the buffer name.
    let spelled = translate_joined(
        "@+draft/channel-context=#Room :bot!u@h PRIVMSG omairc :canonical",
        &room_member_lookup,
    );
    let canonical = message_of(&spelled);
    assert_eq!(canonical.target, "#room");
    assert!(canonical.only_visible_to_you);

    let ordinary = translate_joined(":alice!u@h PRIVMSG #room :everyone", &room_member_lookup);
    let ordinary = message_of(&ordinary);
    assert_eq!(ordinary.conversation.normalized_target, "#room");
    assert!(!ordinary.only_visible_to_you);

    let tagged_channel = translate_joined(
        "@+draft/channel-context=#room :alice!u@h PRIVMSG #room :still public",
        &room_member_lookup,
    );
    assert!(!message_of(&tagged_channel).only_visible_to_you);

    // A channel-operator target never reaches the transcript through the translator.
    assert!(translate_joined(":op!u@h PRIVMSG @#room :ops", &room_member_lookup).is_empty());

    let notice = translate_joined(
        "@+draft/channel-context=#room :bot!u@h NOTICE omairc :heads up",
        &room_member_lookup,
    );
    assert_eq!(notice.len(), 1);
    match &notice[0] {
        Event::Notice(value) => {
            assert!(value.only_visible_to_you);
            assert_eq!(value.target, "#room");
        }
        other => panic!("want NoticeEvent, got {other:?}"),
    }
    assert!(translate_line(":alice!u@h NOTICE omairc :dm notice").is_empty());

    let replay = batch(
        "#room",
        None,
        &[
            "@+draft/channel-context=#room :bot!u@h PRIVMSG omairc :replayed",
            ":alice!u@h PRIVMSG #room :public replay",
            "@+draft/channel-context=#room :bot!u@h NOTICE omairc :notice replay",
            ":op!u@h PRIVMSG @#room :ops replay",
        ],
    );
    let lookup: JoinedMemberLookup<'_> = &room_member_lookup;
    let history = history_of(&replay, Some(lookup));
    assert_eq!(history.lines.len(), 2);
    assert!(history.lines[0].only_visible_to_you);
    assert_eq!(history.lines[0].kind, MessageKindTag::Chat);
    assert_eq!(history.lines[0].body, "replayed");
    assert!(!history.lines[1].only_visible_to_you);
    assert_eq!(history.lines[1].body, "public replay");
}

#[test]
fn channel_context_stays_a_direct_message_when_it_is_not_trusted() {
    let expect_direct = |events: &[Event], sender: &str, body: &str| {
        assert_eq!(events.len(), 1, "{body}: event count");
        let message = message_of(events);
        assert_eq!(message.author, sender, "{body}: author");
        assert_eq!(message.body, body, "{body}: body");
        assert_eq!(
            message.conversation.normalized_target,
            sender.to_lowercase(),
            "{body}: conversation"
        );
        assert_eq!(message.target, sender, "{body}: target");
        assert!(!message.only_visible_to_you, "{body}: only you");
    };

    let accept: JoinedMemberLookup<'_> = &accept_any;

    expect_direct(
        &translate_line("@+draft/channel-context=#room :eve!u@h PRIVMSG omairc :unjoined"),
        "eve",
        "unjoined",
    );
    expect_direct(
        &translate_joined(
            "@+draft/channel-context=#room :eve!u@h PRIVMSG omairc :not a member",
            &room_member_lookup,
        ),
        "eve",
        "not a member",
    );
    let overlong = format!(
        "@+draft/channel-context=#{} :eve!u@h PRIVMSG omairc :overlong",
        "x".repeat(60)
    );
    expect_direct(&translate_joined(&overlong, accept), "eve", "overlong");
    expect_direct(
        &translate_joined(
            "@+draft/channel-context=#a,#b :eve!u@h PRIVMSG omairc :comma",
            accept,
        ),
        "eve",
        "comma",
    );
    expect_direct(
        &translate_joined(
            "@+draft/channel-context=#my\\sroom :eve!u@h PRIVMSG omairc :space",
            accept,
        ),
        "eve",
        "space",
    );
    expect_direct(
        &translate_joined(
            "@+draft/channel-context=#room\u{1}x :eve!u@h PRIVMSG omairc :control",
            accept,
        ),
        "eve",
        "control",
    );
    expect_direct(
        &translate_joined(
            "@+draft/channel-context=omairc :eve!u@h PRIVMSG omairc :own nick",
            accept,
        ),
        "eve",
        "own nick",
    );

    let action = translate_line(
        "@+draft/channel-context=#room :eve!u@h PRIVMSG omairc :\u{1}ACTION waves\u{1}",
    );
    assert_eq!(action.len(), 1);
    match &action[0] {
        Event::Action(value) => {
            assert_eq!(value.conversation.normalized_target, "eve");
            assert_eq!(value.body, "waves");
            assert!(!value.only_visible_to_you);
        }
        other => panic!("want ActionEvent, got {other:?}"),
    }
    assert!(
        translate_line("@+draft/channel-context=#room :eve!u@h NOTICE omairc :heads up").is_empty()
    );

    let query = history_of(
        &batch(
            "alice",
            None,
            &["@+draft/channel-context=#room :alice!u@h PRIVMSG omairc :kept in query"],
        ),
        Some(accept),
    );
    assert_eq!(query.conversation.normalized_target, "alice");
    assert_eq!(query.lines.len(), 1);
    assert_eq!(query.lines[0].body, "kept in query");
    assert_eq!(query.lines[0].kind, MessageKindTag::Chat);
    assert!(!query.lines[0].only_visible_to_you);

    let unjoined = history_of(
        &batch(
            "#room",
            None,
            &[
                "@+draft/channel-context=#room :bot!u@h PRIVMSG omairc :dropped",
                ":alice!u@h PRIVMSG #room :public only",
            ],
        ),
        None,
    );
    assert_eq!(unjoined.lines.len(), 1);
    assert_eq!(unjoined.lines[0].body, "public only");
    assert!(!unjoined.lines[0].only_visible_to_you);
}
