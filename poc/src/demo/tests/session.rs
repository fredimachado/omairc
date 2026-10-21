//! The demo attached to real `Session`s over `LoopbackTransport`, read back
//! through `drain_events`. These are the frame- and event-level halves of the Go
//! `demo_test.go` cases: the registration, the seeded facts on the wire, the
//! seeded selections, and each answered command round-tripped through the
//! session's own writers.

use std::rc::Rc;

use chrono::{TimeZone, Utc};

use crate::demo::answers::demo_server_features;
use crate::demo::seed::OMARCHY_WORLD;
use crate::demo::server::{DemoError, DemoServer, SeedStep};
use crate::demo::wire::{bench_channel_bytes, bench_channel_lines};
use crate::irc::capability::IrcCapability;
use crate::irc::jointarget::IrcJoinTarget;
use crate::irc::message::IrcMessage;
use crate::irc::wiretext::wire_text;
use crate::session::clock::FakeClock;
use crate::session::config::SessionConfig;
use crate::session::events::{PendingInvite, SessionEvent, SessionState};
use crate::session::loopback::LoopbackTransport;
use crate::session::session::Session;

use super::fixture::{
    errors_of, expect_err, inbound_of, nick_of, params_of, tag_of, with_command, Harness, Lane,
    Recorder,
};

fn commands(messages: &[IrcMessage]) -> Vec<String> {
    messages
        .iter()
        .map(|message| message.command.clone())
        .collect()
}

fn seeded_clock() -> Rc<FakeClock> {
    Rc::new(FakeClock::new(
        Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap(),
    ))
}

#[test]
fn both_seeded_sessions_register_and_negotiate_the_demo_caps() {
    let harness = Harness::seeded(false);
    for lane in [&harness.omarchy, &harness.oftc] {
        assert_eq!(lane.session.state(), SessionState::Registered);
        let caps = lane.session.capabilities();
        assert!(caps.contains(IrcCapability::EchoMessage));
        assert!(caps.contains(IrcCapability::MemberMetadata));
        assert!(caps.contains(IrcCapability::Batch));
        assert_eq!(lane.session.channel_types(), "#");

        let events = lane.events();
        let welcomes = events
            .iter()
            .filter(|event| matches!(event, SessionEvent::Registered { .. }))
            .count();
        assert_eq!(welcomes, 1, "one welcome per network");
        assert!(errors_of(&events).is_empty(), "the seed raises no error");
    }
    assert_eq!(harness.omarchy.session.nick(), "fred");
    assert_eq!(harness.oftc.session.nick(), "oak");
}

#[test]
fn the_registration_arrives_as_the_qt_burst_in_order() {
    let harness = Harness::seeded(false);
    let inbound = harness.omarchy.inbound();
    assert_eq!(
        &commands(&inbound)[..16],
        [
            "CAP", "CAP", "NOTICE", "001", "002", "003", "004", "250", "251", "252", "253", "254",
            "255", "265", "266", "005",
        ]
    );
    assert!(
        !commands(&inbound).iter().any(|command| command == "376"),
        "the seed sends no 376, as Qt does"
    );
}

#[test]
fn the_seeded_names_and_away_reach_the_session() {
    let harness = Harness::seeded(false);
    let inbound = harness.omarchy.inbound();

    let names = with_command(&inbound, "353");
    let main = names
        .iter()
        .find(|message| params_of(message)[2] == "#omarchy")
        .expect("#omarchy NAMES");
    assert_eq!(
        params_of(main)[3],
        "&anna @dax @+mira ~fred %kai +teo lena sam ivy max"
    );

    let mut away: Vec<String> = with_command(&inbound, "AWAY")
        .iter()
        .map(|message| nick_of(message))
        .collect();
    away.sort();
    assert_eq!(away, ["ivy", "lena", "max", "sam", "teo"]);

    let statuses = with_command(&inbound, "761");
    let anna_status = statuses
        .iter()
        .find(|message| params_of(message)[1] == "anna" && params_of(message)[2] == "status")
        .expect("anna status");
    assert_eq!(params_of(anna_status)[4], "writing docs");
}

#[test]
fn service_accounts_and_the_join_account_reach_the_session() {
    let harness = Harness::seeded(false);
    let inbound = harness.omarchy.inbound();

    let mut accounts: Vec<(String, String)> = with_command(&inbound, "ACCOUNT")
        .iter()
        .map(|message| (nick_of(message), params_of(message)[0].clone()))
        .collect();
    accounts.sort();
    assert_eq!(
        accounts,
        [
            ("fred".to_owned(), "fredm".to_owned()),
            ("ivy".to_owned(), "*".to_owned()),
            ("lena".to_owned(), "pinkieval".to_owned()),
            ("teo".to_owned(), "teoval".to_owned()),
        ]
    );

    let joins = with_command(&inbound, "JOIN");
    let sol_join = joins
        .iter()
        .find(|message| nick_of(message) == "sol")
        .expect("sol joins");
    assert_eq!(params_of(sol_join)[1], "solarius");

    let speaks = with_command(&inbound, "PRIVMSG");
    let kai = speaks
        .iter()
        .find(|message| nick_of(message) == "kai")
        .expect("kai speaks");
    assert_eq!(tag_of(kai, "account").as_deref(), Some("kaidev"));
}

#[test]
fn the_query_header_facts_reach_the_session() {
    let harness = Harness::seeded(false);
    let inbound = harness.omarchy.inbound();
    let who = with_command(&inbound, "352");

    let facts = |nick: &str| -> (String, String) {
        let message = who
            .iter()
            .find(|message| params_of(message)[5] == nick)
            .unwrap_or_else(|| panic!("WHO for {nick}"));
        let params = params_of(message);
        (params[6].clone(), params[7].clone())
    };
    assert_eq!(facts("anna"), ("H".to_owned(), "0 Anna Vale".to_owned()));
    assert_eq!(facts("dax"), ("H".to_owned(), "0 Packet Bot".to_owned()));
    assert_eq!(
        facts("fred"),
        ("H*".to_owned(), "0 Fred Machado".to_owned())
    );
    assert_eq!(facts("lena"), ("H".to_owned(), "0 Lena Pink".to_owned()));
    assert_eq!(facts("mira"), ("H".to_owned(), "0 Mira Chen".to_owned()));
    assert!(
        !who.iter().any(|message| params_of(message)[5] == "ivy"),
        "ivy has no WHO identity"
    );
}

#[test]
fn the_seeded_invite_is_the_only_pending_invite_and_it_is_on_oftc() {
    let harness = Harness::seeded(false);
    assert_eq!(
        harness.oftc.session.pending_invite(),
        Some(PendingInvite {
            nick: "rio".to_owned(),
            channel: "#debian".to_owned(),
        })
    );
    assert_eq!(harness.omarchy.session.pending_invite(), None);
}

#[test]
fn the_seeded_link_and_the_typing_hint_reach_the_session() {
    let harness = Harness::seeded(false);

    let oftc_inbound = harness.oftc.inbound();
    let links = with_command(&oftc_inbound, "PRIVMSG");
    let link = links
        .iter()
        .find(|message| params_of(message)[1].contains("https://ci.example.com/omairc/42"))
        .expect("the OFTC #lab link");
    assert_eq!(params_of(link)[0], "#lab");

    let omarchy_inbound = harness.omarchy.inbound();
    let hints = with_command(&omarchy_inbound, "TAGMSG");
    let typing = hints
        .iter()
        .find(|message| nick_of(message) == "anna")
        .expect("anna's typing hint");
    assert_eq!(tag_of(typing, "+typing").as_deref(), Some("active"));
    assert_eq!(params_of(typing)[0], "#omarchy");
}

#[test]
fn the_seed_plays_the_selections_in_qt_order() {
    let harness = Harness::seeded(false);
    assert_eq!(
        harness.controls.orders,
        vec![vec!["omarchy".to_owned(), "oftc".to_owned()]]
    );
    let selects: Vec<(&str, &str)> = harness
        .controls
        .selects
        .iter()
        .map(|(network, target)| (network.as_str(), target.as_str()))
        .collect();
    assert_eq!(
        selects,
        [
            ("omarchy", "#omarchy"),
            ("omarchy", "#help"),
            ("omarchy", "dax"),
            ("oftc", "#omarchy"),
            ("oftc", "#lab"),
            ("oftc", "rio"),
            ("omarchy", "#omarchy"),
        ]
    );
}

#[test]
fn the_seed_steps_are_the_qt_order_as_data() {
    let steps = DemoServer::seed_steps();
    let kinds: Vec<&str> = steps
        .iter()
        .map(|step| match step {
            SeedStep::NetworkOrder(_) => "order",
            SeedStep::Inject { .. } => "inject",
            SeedStep::Select { .. } => "select",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "order", "inject", "inject", "inject", "inject", "select", "inject", "inject",
            "select", "select", "select", "select", "select", "select", "inject", "inject",
            "inject",
        ]
    );
    match steps.last() {
        Some(SeedStep::Inject { network_id, bytes }) => {
            assert_eq!(network_id, "oftc");
            assert_eq!(bytes, b":rio!u@h INVITE oak :#debian\r\n");
        }
        other => panic!("the INVITE comes last, got {other:?}"),
    }
}

#[test]
fn seeding_needs_both_networks_attached() {
    let clock = seeded_clock();
    let demo = DemoServer::new(clock.clone());
    let omarchy = Lane::new(&OMARCHY_WORLD, &clock);
    demo.attach(&omarchy.session, &omarchy.transport)
        .expect("attach omarchy");

    let mut controls = Recorder::default();
    let error = expect_err(demo.seed(&mut controls, false));
    assert_eq!(error, DemoError::NotAttached("oftc".to_owned()));
    assert!(
        controls.selects.is_empty(),
        "nothing is seeded on a refusal"
    );
}

#[test]
fn attach_refuses_a_network_the_demo_does_not_seed() {
    let clock = seeded_clock();
    let transport = Rc::new(LoopbackTransport::new());
    let session = Session::new(
        SessionConfig::with_defaults("freenode", "", "irc.example", "fred"),
        transport.clone(),
        clock.clone(),
    );
    let demo = DemoServer::new(clock);
    let error = expect_err(demo.attach(&session, &transport));
    assert_eq!(error, DemoError::UnknownNetwork("freenode".to_owned()));
}

#[test]
fn without_auto_echo_the_client_frames_go_unanswered() {
    let harness = Harness::seeded(false);
    harness.omarchy.send("PING :omairc-watchdog\r\n");
    assert!(
        !commands(&harness.omarchy.fresh_inbound())
            .iter()
            .any(|command| command == "PONG"),
        "no hook means no PONG"
    );
}

#[test]
fn a_ping_is_answered_with_auto_echo_on() {
    let harness = Harness::seeded(true);
    harness.omarchy.send("PING :omairc-watchdog\r\n");
    let fresh = harness.omarchy.fresh_inbound();
    let pongs = with_command(&fresh, "PONG");
    let pong = pongs.first().expect("PONG");
    assert_eq!(params_of(pong), ["irc.example", "omairc-watchdog"]);
}

#[test]
fn a_part_is_echoed_under_the_senders_prefix_with_its_reason() {
    let harness = Harness::seeded(true);
    harness.omarchy.send("PART #omarchy :heading out\r\n");
    let fresh = harness.omarchy.fresh_inbound();
    let parts = with_command(&fresh, "PART");
    let echo = parts.first().expect("the PART echo");
    assert_eq!(nick_of(echo), "fred");
    assert_eq!(params_of(echo), ["#omarchy", "heading out"]);
}

#[test]
fn a_nick_change_is_echoed_and_later_echoes_use_the_new_nick() {
    let harness = Harness::seeded(true);
    harness.omarchy.send("NICK fred2\r\n");
    let fresh = harness.omarchy.fresh_inbound();
    let nicks = with_command(&fresh, "NICK");
    let echo = nicks.first().expect("the NICK echo");
    assert_eq!(nick_of(echo), "fred");
    assert_eq!(params_of(echo), ["fred2"]);

    harness.omarchy.send("PRIVMSG #omarchy :hello\r\n");
    let fresh = harness.omarchy.fresh_inbound();
    let said = with_command(&fresh, "PRIVMSG");
    assert_eq!(nick_of(said.first().expect("the PRIVMSG echo")), "fred2");
}

#[test]
fn a_topic_is_echoed_and_the_next_join_restores_it() {
    let harness = Harness::seeded(true);
    let features = demo_server_features();
    harness.omarchy.send("TOPIC #omarchy :Fresh topic\r\n");
    let fresh = harness.omarchy.fresh_inbound();
    let topics = with_command(&fresh, "TOPIC");
    let echo = topics.first().expect("the TOPIC echo");
    assert_eq!(nick_of(echo), "fred");
    assert_eq!(params_of(echo), ["#omarchy", "Fresh topic"]);

    assert!(harness.omarchy.session.part("#omarchy"));
    harness.omarchy.fresh_inbound();
    let target = IrcJoinTarget::make("#omarchy", None, &features).expect("join target");
    assert!(harness.omarchy.session.join(&target));
    let rejoin = harness.omarchy.fresh_inbound();
    let restored = with_command(&rejoin, "332");
    assert_eq!(params_of(restored[0])[2], "Fresh topic");
}

#[test]
fn list_round_trips_through_the_session() {
    let harness = Harness::seeded(true);
    assert!(harness.omarchy.session.list("#l*"));
    assert_eq!(
        harness.omarchy.frames().last().map(String::as_str),
        Some("LIST #l*\r\n")
    );

    let fresh = harness.omarchy.fresh_inbound();
    let rows = with_command(&fresh, "322");
    assert_eq!(rows.len(), 1);
    assert_eq!(params_of(rows[0])[1], "#linux");
    assert!(!with_command(&fresh, "323").is_empty());
}

#[test]
fn monitor_round_trips_through_the_session() {
    let harness = Harness::seeded(true);
    assert!(harness
        .omarchy
        .session
        .send_monitor('+', &["anna".to_owned(), "ghost".to_owned()]));
    assert_eq!(
        harness.omarchy.frames().last().map(String::as_str),
        Some("MONITOR + anna,ghost\r\n")
    );

    let fresh = harness.omarchy.fresh_inbound();
    let online = with_command(&fresh, "730");
    assert_eq!(params_of(online[0]), ["fred", "anna!u@h"]);
    let offline = with_command(&fresh, "731");
    assert_eq!(params_of(offline[0]), ["fred", "ghost"]);
}

#[test]
fn away_round_trips_through_the_session() {
    let harness = Harness::seeded(true);
    assert!(harness.omarchy.session.mark_away("brb"));
    assert!(harness.omarchy.session.clear_away());
    let fresh = harness.omarchy.fresh_inbound();
    let marked = with_command(&fresh, "306");
    assert_eq!(
        params_of(marked[0])[1],
        "You have been marked as being away"
    );
    let cleared = with_command(&fresh, "305");
    assert_eq!(
        params_of(cleared[0])[1],
        "You are no longer marked as being away"
    );
}

#[test]
fn metadata_set_round_trips_through_the_session() {
    let harness = Harness::seeded(true);
    assert_eq!(
        harness
            .omarchy
            .session
            .set_own_metadata("display-name", "Fred"),
        Some("Fred".to_owned())
    );
    let fresh = harness.omarchy.fresh_inbound();
    let sets = with_command(&fresh, "761");
    assert_eq!(
        params_of(sets[0]),
        ["fred", "fred", "display-name", "*", "Fred"]
    );

    assert_eq!(
        harness.omarchy.session.set_own_metadata("display-name", ""),
        Some(String::new())
    );
    let cleared = harness.omarchy.fresh_inbound();
    assert!(!with_command(&cleared, "766").is_empty());
}

#[test]
fn ctcp_to_self_is_answered_from_the_queried_nick() {
    let harness = Harness::seeded(true);
    harness.omarchy.send("PRIVMSG anna :\u{1}PING 42\u{1}\r\n");
    let fresh = harness.omarchy.fresh_inbound();
    let notices = with_command(&fresh, "NOTICE");
    let notice = notices.first().expect("the CTCP reply");
    assert_eq!(nick_of(notice), "anna");
    assert_eq!(wire_text(&notice.params[1]), "\u{1}PING 42\u{1}");
}

#[test]
fn a_channel_ctcp_gets_no_reply() {
    let harness = Harness::seeded(true);
    harness
        .omarchy
        .send("PRIVMSG #omarchy :\u{1}PING 1\u{1}\r\n");
    assert!(with_command(&harness.omarchy.fresh_inbound(), "NOTICE").is_empty());
}

#[test]
fn a_join_echoes_the_self_join_with_its_seeded_topic() {
    let harness = Harness::seeded(true);
    let features = demo_server_features();
    assert!(harness.omarchy.session.part("#omarchy"));
    let parted = harness.omarchy.fresh_inbound();
    assert_eq!(with_command(&parted, "PART").len(), 1, "a PART is echoed");
    assert!(
        with_command(&parted, "JOIN").is_empty(),
        "the PART echo is not a JOIN"
    );

    let target = IrcJoinTarget::make("#omarchy", None, &features).expect("join target");
    assert!(harness.omarchy.session.join(&target));
    let fresh = harness.omarchy.fresh_inbound();
    let joins = with_command(&fresh, "JOIN");
    let echo = joins
        .iter()
        .find(|message| nick_of(message) == "fred")
        .expect("self JOIN echo");
    assert_eq!(params_of(echo)[0], "#omarchy");
    let topics = with_command(&fresh, "332");
    assert_eq!(
        params_of(topics[0])[2],
        "A cozy corner for Omarchy users and builders."
    );
}

#[test]
fn a_two_channel_rejoin_echoes_each_seeded_topic() {
    let harness = Harness::seeded(true);
    let features = demo_server_features();
    for channel in ["#desktop", "#omarchy"] {
        assert!(harness.omarchy.session.part(channel));
    }
    harness.omarchy.fresh_inbound();

    for channel in ["#desktop", "#omarchy"] {
        let target = IrcJoinTarget::make(channel, None, &features).expect("join target");
        assert!(harness.omarchy.session.join(&target));
    }
    let fresh = harness.omarchy.fresh_inbound();
    let topics: Vec<(String, String)> = with_command(&fresh, "332")
        .iter()
        .map(|message| (params_of(message)[1].clone(), params_of(message)[2].clone()))
        .collect();
    assert_eq!(
        topics,
        [
            (
                "#desktop".to_owned(),
                "Desktops should feel personal, fast, and calm.".to_owned()
            ),
            (
                "#omarchy".to_owned(),
                "A cozy corner for Omarchy users and builders.".to_owned()
            ),
        ]
    );
}

#[test]
fn a_privmsg_is_echoed_back_and_can_be_replayed() {
    let harness = Harness::seeded(true);
    assert!(harness.omarchy.session.send_privmsg("#omarchy", "hello"));
    assert_eq!(
        harness.omarchy.frames().last().map(String::as_str),
        Some("PRIVMSG #omarchy :hello\r\n")
    );
    assert!(harness.demo.echo_last_privmsg("omarchy", "fred"));

    let events = harness.omarchy.events();
    let inbound = inbound_of(&events);
    let hellos = with_command(&inbound, "PRIVMSG")
        .into_iter()
        .filter(|message| params_of(message).get(1).map(String::as_str) == Some("hello"))
        .count();
    assert_eq!(hellos, 2, "the hook echo and the replay");
}

#[test]
fn oftc_has_no_client_line_to_replay_and_an_empty_nick_is_refused() {
    let harness = Harness::seeded(true);
    assert!(!harness.demo.echo_last_privmsg("oftc", "oak"));
    assert!(!harness.demo.echo_last_privmsg("omarchy", ""));
}

#[test]
fn a_redundant_account_tag_arrives_as_a_plain_message() {
    let harness = Harness::seeded(false);
    assert!(harness.demo.inject(
        "omarchy",
        b"@account=kaidev :kai!u@h PRIVMSG #omarchy :still here\r\n"
    ));
    let events = harness.omarchy.pump();
    assert!(errors_of(&events).is_empty());
    assert!(events.iter().any(|event| matches!(
        event,
        SessionEvent::MessageReceived { message, .. }
            if params_of(message).get(1).map(String::as_str) == Some("still here")
    )));
}

#[test]
fn the_bench_channel_seeds_a_session_with_its_lines() {
    let harness = Harness::seeded(false);
    assert!(harness
        .demo
        .inject("omarchy", &bench_channel_bytes("fred", 60)));
    let events = harness.omarchy.pump();
    assert!(errors_of(&events).is_empty(), "bench lines parse cleanly");

    let inbound = inbound_of(&events);
    let delivered = with_command(&inbound, "PRIVMSG")
        .into_iter()
        .filter(|message| params_of(message).first().map(String::as_str) == Some("#bench"))
        .count();
    let expected = bench_channel_lines("fred", 60)
        .iter()
        .filter(|text| text.contains(" PRIVMSG #bench :"))
        .count();
    assert!(expected > 0);
    assert_eq!(delivered, expected);
}

#[test]
fn the_two_seeded_networks_keep_their_own_nicks() {
    let harness = Harness::seeded(false);
    let omarchy_inbound = harness.omarchy.inbound();
    let oftc_inbound = harness.oftc.inbound();
    assert!(with_command(&omarchy_inbound, "JOIN")
        .iter()
        .all(|message| nick_of(message) != "oak"));
    assert!(with_command(&oftc_inbound, "JOIN")
        .iter()
        .all(|message| nick_of(message) != "fred"));
    assert_eq!(harness.lane("oftc").session.nick(), "oak");
}
