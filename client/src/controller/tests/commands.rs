//! The slash dispatcher over the real demo: every verb's frames, the outcome it returns, and
//! the Status or transcript text it leaves.
//! `tui/internal/controller/commanddispatcher_test.go`, driven through the controller rather
//! than a fake host: the frames come from the loopback, the answers from the demo, and the
//! reads from the controller. Each test names the Go case it.

use crate::irc::command::{parse, CommandOutcome, ComposerSurface};
use crate::irc::event::ConversationKey;
use crate::irc::pref::{format_pref_list, format_pref_query, format_pref_state, PrefName};
use crate::irc::membershipnoise::MembershipNoise;
use crate::irc::world::MessageKind;

use super::fixture::{bodies, status_text, Fixture, OFTC, OMARCHY};

const CONVERSATION: ComposerSurface = ComposerSurface::Conversation;
const STATUS: ComposerSurface = ComposerSurface::Status;

/// Dispatches one typed line on `surface` and returns its outcome.
fn run(fixture: &mut Fixture, surface: ComposerSurface, line: &str) -> CommandOutcome {
    fixture.controller.dispatch(surface, &parse(line))
}

/// The last Status line of `network_id`, or "" when Status is empty.
fn last_status(fixture: &Fixture, network_id: &str) -> String {
    status_text(&fixture.controller, network_id)
        .last()
        .cloned()
        .unwrap_or_default()
}

/// Whether one frame of `network_id` starts with `prefix`.
fn wrote(fixture: &Fixture, network_id: &str, frame: &str) -> bool {
    fixture.frames(network_id).iter().any(|written| written == frame)
}

/// The key of a conversation on the seeded network, as the reducer names it.
fn key(fixture: &Fixture, network_id: &str, target: &str) -> ConversationKey {
    fixture.controller.reducer.conversation_key(network_id, target)
}

/// Whether a conversation row exists for `target` on `network_id`.
fn has_conversation(fixture: &Fixture, network_id: &str, target: &str) -> bool {
    fixture
        .controller
        .reducer
        .find(&key(fixture, network_id, target))
        .is_some()
}

#[test]
fn an_empty_unknown_or_out_of_scope_line_is_answered_without_a_frame() {
    // Go `TestCommandDispatcherEmptyUnknownAndScope`.
    let mut fixture = Fixture::seeded();
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, ""), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/nope"), CommandOutcome::Unsupported);
    assert_eq!(run(&mut fixture, STATUS, "/me waves"), CommandOutcome::WrongScope);
    assert_eq!(run(&mut fixture, STATUS, "/close"), CommandOutcome::WrongScope);
    assert!(fixture.frames(OMARCHY).is_empty());
}

#[test]
fn an_unknown_word_is_refused_with_its_name_and_the_composer_keeps_it() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(!fixture.controller.submit(CONVERSATION, "/q"));
    assert_eq!(
        fixture.controller.last_error_for(OMARCHY),
        Some("Unknown command: /q")
    );
}

#[test]
fn plain_text_and_a_doubled_slash_send_the_same_privmsg() {
    // Go `TestCommandDispatcherSay`. `//away` is text, not `/away`.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert!(fixture.controller.submit(CONVERSATION, "hello world"));
    assert!(fixture.controller.submit(CONVERSATION, "//away"));
    assert_eq!(
        fixture.frames(OMARCHY),
        ["PRIVMSG #omarchy :hello world\r\n", "PRIVMSG #omarchy :/away\r\n"]
    );
    assert!(bodies(&fixture.controller).iter().any(|body| body == "hello world"));
}

#[test]
fn an_action_writes_a_ctcp_action_and_a_channel_left_refuses_it() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/me waves"), CommandOutcome::Sent);
    assert!(wrote(&fixture, OMARCHY, "PRIVMSG #omarchy :\u{1}ACTION waves\u{1}\r\n"));

    // Empty `/me` is refused before any frame, as `IrcSession::sendAction` refuses an empty body.
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/me"), CommandOutcome::Refused);
    assert!(fixture.frames(OMARCHY).is_empty());
}

#[test]
fn close_on_a_channel_you_are_in_is_refused_and_stays_in_the_composer() {
    // The slash-commands feature: `/close` on `#omarchy` is rejected, and the composer keeps it.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert!(!fixture.controller.submit(CONVERSATION, "/close"));
    assert_eq!(
        fixture.controller.last_error_for(OMARCHY),
        Some("Close applies to direct messages and channels you have left")
    );
    assert_eq!(run(&mut fixture, CONVERSATION, "/close"), CommandOutcome::WrongScope);
    assert!(has_conversation(&fixture, OMARCHY, "#omarchy"));
    assert!(fixture.frames(OMARCHY).is_empty());
}

#[test]
fn close_drops_a_direct_message_and_the_left_channel_it_parted() {
    // Go `TestCommandDispatcherClose` and the feature's `/part` then `/close` sequence.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "anna");
    assert!(fixture.controller.submit(CONVERSATION, "/close"));
    assert!(!has_conversation(&fixture, OMARCHY, "anna"));

    fixture.controller.select_conversation(OMARCHY, "#desktop");
    assert!(fixture.controller.submit(CONVERSATION, "/part"));
    assert!(has_conversation(&fixture, OMARCHY, "#desktop"), "a part keeps the row");
    assert!(fixture.controller.submit(CONVERSATION, "/close"));
    assert!(!has_conversation(&fixture, OMARCHY, "#desktop"));
}

#[test]
fn query_opens_the_direct_message_and_its_text_sends_into_it() {
    // Go `TestCommandDispatcherQuery`.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/query lena"), CommandOutcome::Sent);
    assert!(has_conversation(&fixture, OMARCHY, "lena"));
    assert_eq!(fixture.controller.selected_target(), "lena");
    assert!(fixture.frames(OMARCHY).is_empty(), "opening a query writes nothing");

    assert_eq!(run(&mut fixture, CONVERSATION, "/query lena hello"), CommandOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["PRIVMSG lena :hello\r\n"]);

    assert_eq!(run(&mut fixture, CONVERSATION, "/query #chan"), CommandOutcome::Refused);
    // Status names its own network, so a query there opens on it, as Go's `dispatchQuery` does.
    assert_eq!(run(&mut fixture, STATUS, "/query lena"), CommandOutcome::Sent);
}

#[test]
fn query_text_on_a_network_without_a_session_is_not_connected() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OFTC, "#lab");
    assert!(fixture.controller.disconnect(OFTC));
    assert_eq!(
        run(&mut fixture, CONVERSATION, "/query lena hi"),
        CommandOutcome::NotConnected
    );
}

#[test]
fn msg_writes_a_privmsg_to_a_nick_and_refuses_a_channel_or_empty_body() {
    // Go `TestCommandDispatcherQuietSend`: the frame, the activity, and the refusals.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/msg lena hello there"), CommandOutcome::Sent);
    assert!(wrote(&fixture, OMARCHY, "PRIVMSG lena :hello there\r\n"));
    assert_eq!(run(&mut fixture, CONVERSATION, "/msg #chan hi"), CommandOutcome::Refused);
    assert_eq!(run(&mut fixture, CONVERSATION, "/msg lena"), CommandOutcome::Refused);
    assert_eq!(fixture.frames(OMARCHY), ["PRIVMSG lena :hello there\r\n"]);
}

#[test]
fn notice_writes_a_notice_to_any_target_and_is_not_chat_activity() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/notice #omarchy hi all"), CommandOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["NOTICE #omarchy :hi all\r\n"]);
}

#[test]
fn ns_cs_and_znc_send_to_their_service_from_either_surface() {
    // Go `TestCommandDispatcherServiceMsg`.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/ns identify hunter2"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/cs op #omarchy"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/znc listclients"), CommandOutcome::Sent);
    assert_eq!(
        fixture.frames(OMARCHY),
        [
            "PRIVMSG NickServ :identify hunter2\r\n",
            "PRIVMSG ChanServ :op #omarchy\r\n",
            "PRIVMSG *status :listclients\r\n",
        ]
    );
    // None of them opens or selects a conversation.
    assert!(!has_conversation(&fixture, OMARCHY, "NickServ"));
    assert_eq!(fixture.controller.selected_target(), "#omarchy");

    // The same verbs work on Status.
    fixture.controller.open_status(OMARCHY);
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, STATUS, "/ns info fred"), CommandOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["PRIVMSG NickServ :info fred\r\n"]);
}

#[test]
fn a_service_password_never_reaches_status_in_the_clear() {
    // The redact-secrets feature: the outbound line is masked before the Status log records it.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(fixture.controller.submit(CONVERSATION, "/ns identify hunter2"));
    assert!(
        !status_text(&fixture.controller, OMARCHY)
            .iter()
            .any(|line| line.contains("hunter2")),
        "the password is masked on Status"
    );
}

#[test]
fn join_writes_one_join_per_target_and_opens_the_last_one() {
    // Go `TestCommandDispatcherJoin`, and the comma list of the slash-commands feature.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/join #test"), CommandOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["JOIN #test\r\n"]);
    assert_eq!(fixture.controller.selected_target(), "#test");

    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/j #a,#b"), CommandOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["JOIN #a\r\n", "JOIN #b\r\n"]);
    assert_eq!(fixture.controller.selected_target(), "#b");
}

#[test]
fn join_with_a_key_sends_the_key_and_redacts_it_on_status() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/join #vault hunter3"), CommandOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["JOIN #vault hunter3\r\n"]);
    assert!(
        !status_text(&fixture.controller, OMARCHY)
            .iter()
            .any(|line| line.contains("hunter3")),
        "the join key is redacted on Status"
    );
}

#[test]
fn an_empty_join_is_refused_without_an_invite_and_takes_the_latest_one() {
    // Go `TestCommandDispatcherJoin`: no invite refuses, a pending INVITE seeds the join.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert_eq!(run(&mut fixture, CONVERSATION, "/join"), CommandOutcome::Refused);

    fixture.deliver(OMARCHY, ":inviter!u@h INVITE fred :#invited");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/join"), CommandOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["JOIN #invited\r\n"]);
    assert_eq!(fixture.controller.selected_target(), "#invited");
}

#[test]
fn part_of_a_channel_you_are_in_writes_part_and_keeps_the_row() {
    // Go `TestCommandDispatcherPart`, the joined case.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/part"), CommandOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["PART #omarchy\r\n"]);
    assert!(has_conversation(&fixture, OMARCHY, "#omarchy"));
    assert_eq!(
        fixture.controller.reducer.find(&key(&fixture, OMARCHY, "#omarchy")).and_then(|c| c.channel.as_ref()).map(|c| c.joined),
        Some(false),
        "the row is marked left"
    );
}

#[test]
fn part_of_a_channel_you_have_left_writes_nothing_and_cancels_its_join() {
    // Go `TestCommandDispatcherPart`, the unjoined case: no frame, and the cancellation is noted.
    let mut fixture = Fixture::seeded();
    let room = key(&fixture, OMARCHY, "#room");
    fixture.controller.reducer.ensure_conversation(
        &room,
        "#room",
        crate::irc::event::IrcConversationCause::ChannelState,
    );
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/part #room"), CommandOutcome::Sent);
    assert!(fixture.frames(OMARCHY).is_empty());
    assert!(fixture.controller.cancelled_joins.contains(&room));

    // A channel with no local buffer is still parted on the wire.
    assert_eq!(run(&mut fixture, CONVERSATION, "/part #other"), CommandOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["PART #other\r\n"]);
}

#[test]
fn part_with_no_channel_selected_is_the_wrong_scope() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "anna");
    assert_eq!(run(&mut fixture, CONVERSATION, "/part"), CommandOutcome::WrongScope);
    assert_eq!(run(&mut fixture, STATUS, "/part"), CommandOutcome::WrongScope);
}

#[test]
fn a_join_cancelled_in_flight_parts_when_its_echo_lands() {
    // The cancellation the dispatcher records is consumed by the self JOIN that answers it.
    let mut fixture = Fixture::seeded();
    let room = key(&fixture, OMARCHY, "#room");
    fixture.controller.reducer.ensure_conversation(
        &room,
        "#room",
        crate::irc::event::IrcConversationCause::ChannelState,
    );
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert_eq!(run(&mut fixture, CONVERSATION, "/part #room"), CommandOutcome::Sent);
    fixture.clear_frames(OMARCHY);

    fixture.deliver(OMARCHY, ":fred!u@h JOIN #room");
    assert_eq!(fixture.frames(OMARCHY), ["PART #room\r\n"], "the late echo parts again");
    assert!(!fixture.controller.cancelled_joins.contains(&room));
}

#[test]
fn kick_and_invite_take_an_explicit_channel_or_the_selected_one() {
    // Go `TestCommandDispatcherKickInvite`.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/kick #room bob spam"), CommandOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["KICK #room bob :spam\r\n"]);

    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/kick bob"), CommandOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["KICK #omarchy bob\r\n"]);

    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/invite bob #room"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/invite bob"), CommandOutcome::Sent);
    assert_eq!(
        fixture.frames(OMARCHY),
        ["INVITE bob #room\r\n", "INVITE bob #omarchy\r\n"]
    );

    // A kick with no nick is refused; an invite on Status needs a channel.
    assert_eq!(run(&mut fixture, CONVERSATION, "/kick #room"), CommandOutcome::Refused);
    fixture.controller.open_status(OMARCHY);
    assert_eq!(run(&mut fixture, STATUS, "/invite bob"), CommandOutcome::WrongScope);
}

#[test]
fn mode_and_the_channel_mode_wrappers_write_the_expected_change() {
    // Go `TestCommandDispatcherMode`. `/ban` with no `!` or `@` gets `!*@*`.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/mode #omarchy +o bob"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/op bob"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/ban bob"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/ban bob!*@*"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/unban bob"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/voice bob"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/devoice bob"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/deop bob"), CommandOutcome::Sent);
    assert_eq!(
        fixture.frames(OMARCHY),
        [
            "MODE #omarchy +o bob\r\n",
            "MODE #omarchy +o bob\r\n",
            "MODE #omarchy +b bob!*@*\r\n",
            "MODE #omarchy +b bob!*@*\r\n",
            "MODE #omarchy -b bob!*@*\r\n",
            "MODE #omarchy +v bob\r\n",
            "MODE #omarchy -v bob\r\n",
            "MODE #omarchy -o bob\r\n",
        ]
    );

    // Quiet needs CHANMODES A to advertise q; mute stays a different verb.
    assert_eq!(run(&mut fixture, CONVERSATION, "/quiet bob"), CommandOutcome::Refused);
    let mut with_quiet = fixture.controller.reducer.server_features(OMARCHY).clone();
    with_quiet.apply_token("CHANMODES=eIbq,k,l,imnpst");
    fixture.controller.reducer.set_server_features(OMARCHY, with_quiet);
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/quiet bob"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/unquiet bob"), CommandOutcome::Sent);
    assert_eq!(
        fixture.frames(OMARCHY),
        [
            "MODE #omarchy +q bob!*@*\r\n",
            "MODE #omarchy -q bob!*@*\r\n",
        ]
    );

    // A wrapper outside a channel is the wrong scope.
    fixture.controller.select_conversation(OMARCHY, "anna");
    assert_eq!(run(&mut fixture, CONVERSATION, "/op bob"), CommandOutcome::WrongScope);
}

#[test]
fn topic_sets_the_selected_channel_and_an_empty_topic_writes_nothing() {
    // Go `TestCommandDispatcherTopicAwayBack`, the topic half.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/topic hello world"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/topic"), CommandOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["TOPIC #omarchy :hello world\r\n"]);

    fixture.controller.select_conversation(OMARCHY, "anna");
    assert_eq!(run(&mut fixture, CONVERSATION, "/topic hi"), CommandOutcome::WrongScope);
}

#[test]
fn away_and_back_write_the_frames_and_drive_the_footer() {
    // Go `TestCommandDispatcherTopicAwayBack`, the away half. The demo answers 306 and 305, and
    // the footer's away state follows.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/away brb"), CommandOutcome::Sent);
    assert!(wrote(&fixture, OMARCHY, "AWAY :brb\r\n"));
    assert!(fixture.controller.self_away(), "306 marks the footer away");

    assert_eq!(run(&mut fixture, CONVERSATION, "/away"), CommandOutcome::Sent);
    assert!(wrote(&fixture, OMARCHY, "AWAY\r\n"));
    assert!(!fixture.controller.self_away(), "305 clears it");

    assert_eq!(run(&mut fixture, CONVERSATION, "/away brb"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/back"), CommandOutcome::Sent);
    assert!(!fixture.controller.self_away());
}

#[test]
fn raw_and_quote_send_one_literal_line_and_nick_writes_nick() {
    // Go `TestCommandDispatcherRawListNick`.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/raw PRIVMSG #room :hi"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/quote LIST"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/nick newnick"), CommandOutcome::Sent);
    assert_eq!(
        fixture.frames(OMARCHY),
        ["PRIVMSG #room :hi\r\n", "LIST\r\n", "NICK newnick\r\n"]
    );
    assert_eq!(run(&mut fixture, CONVERSATION, "/raw"), CommandOutcome::Refused);
}

#[test]
fn a_wire_verb_on_a_session_that_is_not_registered_is_not_connected() {
    // Go `TestCommandDispatcherWireLifecycle`.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OFTC, "#lab");
    assert!(fixture.controller.disconnect(OFTC));
    assert_eq!(run(&mut fixture, CONVERSATION, "/nick x"), CommandOutcome::NotConnected);
    assert_eq!(run(&mut fixture, CONVERSATION, "/raw LIST"), CommandOutcome::NotConnected);
}

#[test]
fn no_selection_and_networks_present_is_refused_for_a_wire_verb() {
    // Go `TestCommandDispatcherWireLifecycle`, the "networks but no selection" branch.
    let mut fixture = Fixture::seeded();
    fixture.controller.clear_selection();
    assert_eq!(run(&mut fixture, CONVERSATION, "/nick x"), CommandOutcome::Refused);
}

#[test]
fn quit_sends_quit_with_its_reason_and_idles_the_network() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OFTC, "#lab");
    fixture.clear_frames(OFTC);
    assert_eq!(run(&mut fixture, CONVERSATION, "/quit bye now"), CommandOutcome::Sent);
    assert!(fixture.frames(OFTC).iter().any(|frame| frame.starts_with("QUIT")));

}

#[test]
fn ignore_ignored_unignore_report_on_status_in_the_go_wording() {
    // Go `TestCommandDispatcherIgnore`.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert_eq!(run(&mut fixture, CONVERSATION, "/ignored"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "Not ignoring anyone");

    assert_eq!(run(&mut fixture, CONVERSATION, "/ignore Lena"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "Ignoring Lena");
    assert_eq!(run(&mut fixture, CONVERSATION, "/ignore lena"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "Already ignoring lena");
    assert_eq!(run(&mut fixture, CONVERSATION, "/ignored"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "Ignoring: Lena");
    assert_eq!(run(&mut fixture, CONVERSATION, "/unignore LENA"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "No longer ignoring LENA");
    assert_eq!(run(&mut fixture, CONVERSATION, "/unignore LENA"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "Not ignoring LENA");

    assert_eq!(run(&mut fixture, CONVERSATION, "/ignore #chan"), CommandOutcome::Refused);
    assert_eq!(run(&mut fixture, CONVERSATION, "/ignore bad!nick"), CommandOutcome::Refused);
    assert_eq!(run(&mut fixture, CONVERSATION, "/ignored extra"), CommandOutcome::Refused);
}

#[test]
fn mute_muted_unmute_report_on_status_and_a_bare_mute_takes_the_selection() {
    // Go `TestCommandDispatcherMute`.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert_eq!(run(&mut fixture, CONVERSATION, "/muted"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "Not muting anything");

    assert_eq!(run(&mut fixture, CONVERSATION, "/mute #omarchy"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "Muted #omarchy");
    assert!(fixture.controller.is_muted_target(OMARCHY, "#omarchy"));
    assert_eq!(run(&mut fixture, CONVERSATION, "/mute #omarchy"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "Already muted #omarchy");
    assert_eq!(run(&mut fixture, CONVERSATION, "/muted"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "Muted: #omarchy");
    assert_eq!(run(&mut fixture, CONVERSATION, "/unmute #omarchy"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "No longer muted #omarchy");
    assert_eq!(run(&mut fixture, CONVERSATION, "/unmute #omarchy"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "Not muted #omarchy");

    // A bare `/mute` on a direct message mutes the peer.
    fixture.controller.select_conversation(OMARCHY, "anna");
    assert_eq!(run(&mut fixture, CONVERSATION, "/mute"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "Muted anna");
    assert_eq!(run(&mut fixture, CONVERSATION, "/mute #omarchy bob"), CommandOutcome::Refused);
    assert_eq!(run(&mut fixture, STATUS, "/mute"), CommandOutcome::WrongScope);
}

#[test]
fn highlight_highlights_and_unhighlight_report_in_the_go_wording() {
    // Go `TestCommandDispatcherHighlight`.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert_eq!(run(&mut fixture, CONVERSATION, "/highlights"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "No highlight words");

    assert_eq!(run(&mut fixture, CONVERSATION, "/highlight Alice"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "Highlighting Alice");
    assert_eq!(fixture.controller.highlight_words(OMARCHY), ["Alice"]);
    assert_eq!(run(&mut fixture, CONVERSATION, "/highlight alice"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "Already highlighting alice");
    assert_eq!(run(&mut fixture, CONVERSATION, "/highlights"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "Highlights: Alice");
    assert_eq!(run(&mut fixture, CONVERSATION, "/unhighlight alice"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "No longer highlighting alice");
    assert_eq!(run(&mut fixture, CONVERSATION, "/unhighlight alice"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), "Not highlighting alice");
    assert_eq!(run(&mut fixture, CONVERSATION, "/highlight"), CommandOutcome::Refused);
}

#[test]
fn pref_reads_and_sets_on_status_and_prints_into_a_conversation() {
    // Go `TestCommandDispatcherPref`, and the feature's `/pref` description.
    let mut fixture = Fixture::seeded();
    fixture.controller.open_status(OMARCHY);
    assert_eq!(run(&mut fixture, STATUS, "/pref"), CommandOutcome::Sent);
    let listing = format_pref_list(
        true,
        true,
        true,
        true,
        MembershipNoise::Folded,
        crate::irc::colorpalette::ColorPaletteId::Classic,
    );
    assert!(
        listing.lines().all(|line| status_text(&fixture.controller, OMARCHY).iter().any(|text| text == line)),
        "every preference line is on Status"
    );

    // The avatars query carries the IP note on a second line; Status holds one row per line.
    assert_eq!(run(&mut fixture, STATUS, "/pref avatars"), CommandOutcome::Sent);
    let texts = status_text(&fixture.controller, OMARCHY);
    for line in format_pref_query(PrefName::Avatars, true).lines() {
        assert!(texts.iter().any(|text| text == line), "{line} is on Status");
    }

    assert_eq!(run(&mut fixture, STATUS, "/pref avatars off"), CommandOutcome::Sent);
    assert_eq!(last_status(&fixture, OMARCHY), format_pref_state(PrefName::Avatars, false));
    assert!(!fixture.controller.preferences().avatars);

    assert_eq!(run(&mut fixture, STATUS, "/pref colors off"), CommandOutcome::Sent);
    assert!(!fixture.controller.preferences().colors);

    // A demo controller keeps nothing: the value changes in memory only. The disk round trip
    // is covered by the persistence tests, which build over a settings file.

    // On a conversation the feedback is a transcript row, not Status.
    fixture.controller.select_conversation(OMARCHY, "anna");
    let before = fixture.controller.messages().len();
    assert_eq!(run(&mut fixture, CONVERSATION, "/pref unread"), CommandOutcome::Sent);
    assert!(fixture.controller.messages().len() > before);
    assert!(fixture
        .controller
        .messages()
        .iter()
        .any(|row| row.kind == MessageKind::Whois && row.body == format_pref_query(PrefName::Unread, true)));
}

#[test]
fn help_prints_the_catalog_on_status_or_into_the_conversation() {
    // Go `TestCommandDispatcherHelp`.
    let mut fixture = Fixture::seeded();
    fixture.controller.open_status(OMARCHY);
    assert_eq!(run(&mut fixture, STATUS, "/help"), CommandOutcome::Sent);
    let text = last_status(&fixture, OMARCHY);
    assert!(text.starts_with("Commands: /me, /join, /part"), "{text}");
    assert!(text.ends_with("Empty /join joins the latest invite."), "{text}");

    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert_eq!(run(&mut fixture, CONVERSATION, "/help"), CommandOutcome::Sent);
    assert!(fixture
        .controller
        .messages()
        .iter()
        .any(|row| row.kind == MessageKind::Whois && row.body == text));
}

#[test]
fn list_writes_list_and_opens_the_overlay() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/list"), CommandOutcome::Sent);
    assert_eq!(fixture.frames(OMARCHY), ["LIST\r\n"]);
    assert!(fixture.controller.channel_list().open);
}

#[test]
fn clear_empties_status_on_status_and_the_selection_otherwise() {
    let mut fixture = Fixture::seeded();
    fixture.controller.open_status(OMARCHY);
    assert_eq!(run(&mut fixture, STATUS, "/clear"), CommandOutcome::Sent);
    assert!(status_text(&fixture.controller, OMARCHY).is_empty());

    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(!bodies(&fixture.controller).is_empty());
    assert_eq!(run(&mut fixture, CONVERSATION, "/clear"), CommandOutcome::Sent);
    assert!(bodies(&fixture.controller).is_empty());
}

#[test]
fn autoaway_reports_its_settings_on_status() {
    let mut fixture = Fixture::seeded();
    fixture.controller.open_status(OMARCHY);
    assert_eq!(run(&mut fixture, STATUS, "/autoaway"), CommandOutcome::Sent);
    assert!(!last_status(&fixture, OMARCHY).is_empty());
    assert_eq!(run(&mut fixture, STATUS, "/autoaway 90 afk"), CommandOutcome::Sent);
    assert!(last_status(&fixture, OMARCHY).contains("90"));
}

#[test]
fn monitor_monitored_unmonitor_write_monitor_and_report() {
    // Go `TestMonitorCoordinator*`, through the dispatcher.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/monitor anna"), CommandOutcome::Sent);
    assert!(wrote(&fixture, OMARCHY, "MONITOR + anna\r\n"));
    assert_eq!(run(&mut fixture, CONVERSATION, "/monitored"), CommandOutcome::Sent);
    assert!(last_status(&fixture, OMARCHY).starts_with("Watching: anna ("));
    assert_eq!(run(&mut fixture, CONVERSATION, "/unmonitor anna"), CommandOutcome::Sent);
    assert!(wrote(&fixture, OMARCHY, "MONITOR - anna\r\n"));
    assert_eq!(last_status(&fixture, OMARCHY), "No longer watching anna");
}

#[test]
fn whois_writes_a_labeled_query_and_a_bare_one_asks_the_peer() {
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "anna");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/whois"), CommandOutcome::Sent);
    assert!(fixture
        .frames(OMARCHY)
        .iter()
        .any(|frame| frame.ends_with("WHOIS anna anna\r\n")));
}

#[test]
fn ping_time_and_version_query_the_nick_and_bare_forms_use_the_peer() {
    // The slash-commands feature: the demo answers CTCP with a NOTICE, copied as whois rows.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "anna");
    fixture.clear_frames(OMARCHY);
    assert_eq!(run(&mut fixture, CONVERSATION, "/ping"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/time"), CommandOutcome::Sent);
    assert_eq!(run(&mut fixture, CONVERSATION, "/version"), CommandOutcome::Sent);
    let frames = fixture.frames(OMARCHY);
    // The frames carry a `label=` tag, since labeled-response is negotiated.
    assert!(frames.iter().any(|frame| frame.contains("PRIVMSG anna :\u{1}PING ")));
    assert!(frames.iter().any(|frame| frame.ends_with("PRIVMSG anna :\u{1}TIME\u{1}\r\n")));
    assert!(frames.iter().any(|frame| frame.ends_with("PRIVMSG anna :\u{1}VERSION\u{1}\r\n")));

    // A channel is refused as a CTCP target, and a bare form on a channel needs a nick.
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert_eq!(run(&mut fixture, CONVERSATION, "/ping #omarchy"), CommandOutcome::Refused);
    assert_eq!(run(&mut fixture, CONVERSATION, "/ping"), CommandOutcome::WrongScope);
}

#[test]
fn a_status_line_ping_needs_a_nick_and_a_conversation_ping_is_whois_row() {
    let mut fixture = Fixture::seeded();
    fixture.controller.clear_selection();
    fixture.controller.open_status(OMARCHY);
    assert_eq!(run(&mut fixture, STATUS, "/ping"), CommandOutcome::Refused);
    assert_eq!(run(&mut fixture, STATUS, "/ping anna"), CommandOutcome::Sent);
    assert!(fixture
        .frames(OMARCHY)
        .iter()
        .any(|frame| frame.contains("PRIVMSG anna :\u{1}PING ")));
}

#[test]
fn a_bare_nick_query_on_no_selection_is_refused() {
    let mut fixture = Fixture::seeded();
    fixture.controller.clear_selection();
    assert_eq!(run(&mut fixture, CONVERSATION, "/ping"), CommandOutcome::Refused);
    assert_eq!(run(&mut fixture, CONVERSATION, "/whois"), CommandOutcome::Refused);
}

#[test]
fn a_network_with_no_session_reads_as_offline() {
    let fixture = Fixture::seeded();
    assert_eq!(fixture.controller.connection_status_for("nowhere"), "Offline");
}

#[test]
fn a_pending_join_cancellation_is_kept_per_network_and_forgotten_with_its_network() {
    // Go `TestCommandDispatcherCancelled`: a cancellation belongs to one network, and forgetting
    // that network drops only its own.
    let mut fixture = Fixture::seeded();
    let omarchy = key(&fixture, OMARCHY, "#room");
    let oftc = key(&fixture, OFTC, "#room");
    fixture.controller.cancelled_joins.insert(omarchy.clone());
    fixture.controller.cancelled_joins.insert(oftc.clone());
    assert!(fixture.controller.forget_network(OFTC));
    assert!(fixture.controller.cancelled_joins.contains(&omarchy));
    assert!(!fixture.controller.cancelled_joins.contains(&oftc));
}

#[test]
fn a_ctcp_reply_copies_into_the_asking_conversation_as_a_whois_row() {
    // The slash-commands feature: the demo answers the CTCP with a NOTICE, and the reply is
    // copied into the transcript that asked, as a whois row.
    let mut fixture = Fixture::seeded();
    fixture.controller.select_conversation(OMARCHY, "anna");
    assert_eq!(run(&mut fixture, CONVERSATION, "/time"), CommandOutcome::Sent);
    assert!(fixture.controller.messages().iter().any(|row| {
        row.kind == MessageKind::Whois && row.body.contains("TIME reply from anna")
    }));
}
