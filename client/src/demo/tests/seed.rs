//! The seed data and the lines it builds, checked without a session. These are
//! the wire-level halves of `TestSeeds*` in `demo_test.go`. The reducer-level
//! halves (sidebar rows, unread counts, ordered members, peer labels) run in
//! the controller tests, which read this same seed.

use std::collections::BTreeSet;

use crate::demo::seed::{
    channel_named, world_for, CAPS, DEMO_SELF_AVATAR_URL, ISUPPORT, MONITOR_LIMIT, OFTC_WORLD,
    OMARCHY_WORLD, TODAY, YESTERDAY,
};
use crate::demo::wire::{
    account_lines, bench_channel_bytes, bench_channel_lines, identity_lines, live_account_lines,
    names_tokens, presence_lines, registration_lines, rtl_seed_lines, transcript_lines,
    typing_lines, world_lines,
};
use crate::irc::parser::parse;

#[test]
fn the_seed_constants_match_the_go_world() {
    assert_eq!(TODAY, "2026-09-12");
    assert_eq!(YESTERDAY, "2026-09-11");
    assert_eq!(
        CAPS,
        "echo-message message-tags labeled-response away-notify multi-prefix batch \
         draft/metadata-2 server-time account-tag account-notify extended-join"
    );
    assert_eq!(ISUPPORT, "CHANTYPES=# PREFIX=(qaohv)~&@%+ MONITOR=100");
    assert_eq!(MONITOR_LIMIT, 100);
    assert_eq!(DEMO_SELF_AVATAR_URL, "https://omairc.app/omairc128.png");
    assert_eq!(OMARCHY_WORLD.nick, "fred");
    assert_eq!(OFTC_WORLD.nick, "oak");
}

#[test]
fn both_networks_resolve_by_id_and_unknown_ids_do_not() {
    assert_eq!(
        world_for("omarchy").map(|network| network.nick),
        Some("fred")
    );
    assert_eq!(world_for("oftc").map(|network| network.nick), Some("oak"));
    assert!(world_for("freenode").is_none());
}

#[test]
fn the_parity_channels_hold_their_unread_counts() {
    // #desktop is three lines unread. #ricing holds twelve: three seeded lines
    // eight pads, and the line that mentions fred. The mention is the one
    // parity count that comes from the line text.
    let desktop = channel_named(&OMARCHY_WORLD, "#desktop").expect("#desktop");
    assert_eq!(desktop.lines.len(), 3);

    let ricing = channel_named(&OMARCHY_WORLD, "#ricing").expect("#ricing");
    assert_eq!(ricing.lines.len(), 12);
    assert_eq!(
        ricing.lines.last().map(|row| row.body),
        Some("looks good, fred.")
    );
    assert_eq!(ricing.lines.last().map(|row| row.day), Some(YESTERDAY));
}

#[test]
fn omarchy_names_carry_the_rank_ladder_without_late_joiners() {
    let main = channel_named(&OMARCHY_WORLD, "#omarchy").expect("#omarchy");
    assert_eq!(
        names_tokens(main),
        vec!["&anna", "@dax", "@+mira", "~fred", "%kai", "+teo", "lena", "sam", "ivy", "max",]
    );
}

#[test]
fn the_registration_burst_advertises_caps_and_the_icon_only_on_omarchy() {
    let omarchy = registration_lines("fred", "Welcome", OMARCHY_WORLD.icon_url);
    assert_eq!(omarchy[0], format!(":server CAP fred LS :{CAPS}"));
    assert_eq!(omarchy[1], format!(":server CAP fred ACK :{CAPS}"));
    assert!(omarchy[15].ends_with(
        "005 fred CHANTYPES=# PREFIX=(qaohv)~&@%+ MONITOR=100 draft/ICON=qrc:/demo/omarchy-icon.png \
         :are supported by this server"
    ));

    let oftc = registration_lines("oak", "Welcome", OFTC_WORLD.icon_url);
    assert!(oftc[15].ends_with(" MONITOR=100 :are supported by this server"));
    assert!(!oftc[15].contains("draft/ICON"));
}

#[test]
fn the_registration_burst_sends_no_376_like_the_go_and_cpp_seeds() {
    let lines = registration_lines("fred", "Welcome", OMARCHY_WORLD.icon_url);
    assert!(lines.iter().all(|line| !line.contains(" 376 ")));
}

#[test]
fn the_operator_is_the_only_server_operator_on_omarchy() {
    let members: BTreeSet<&str> = ["anna", "dax", "fred", "lena", "mira"]
        .into_iter()
        .collect();
    let omarchy = identity_lines(&OMARCHY_WORLD, &members).join("\n");
    assert!(omarchy.contains(":server 352 fred #omarchy u h server fred H* :0 Fred Machado"));
    assert!(omarchy.contains(":server 352 fred #omarchy u h server anna H :0 Anna Vale"));
    assert!(omarchy.contains(":server 352 fred #omarchy u h server dax H :0 Packet Bot"));
    assert!(omarchy.contains(":server 352 fred #omarchy u h server lena H :0 Lena Pink"));
    assert!(omarchy.contains(":server 352 fred #omarchy u h server mira H :0 Mira Chen"));

    // oak is not an identity the demo seeds, so oftc sends no WHO replies here.
    let oftc_members: BTreeSet<&str> = ["rio", "ness", "oak", "pip"].into_iter().collect();
    assert!(identity_lines(&OFTC_WORLD, &oftc_members).is_empty());
}

#[test]
fn away_status_and_metadata_facts_reach_the_wire() {
    let lines = presence_lines(&OMARCHY_WORLD);
    for nick in ["ivy", "lena", "max", "sam", "teo"] {
        assert!(
            lines.contains(&format!(":{nick}!u@h AWAY :away")),
            "AWAY for {nick}"
        );
    }
    assert!(!lines
        .iter()
        .any(|line| line.contains("!u@h AWAY :away") && line.contains(":anna")));
    assert!(lines.contains(&":server 761 fred anna status * :writing docs".to_owned()));
    assert!(lines.contains(&":server 761 fred dax bot * :PacketBot".to_owned()));
    assert!(lines.contains(&":server 761 fred mira avatar * :qrc:/demo/mira-avatar.png".to_owned()));
    assert!(lines.contains(&":server 761 fred anna display-name * :Anna Docs".to_owned()));
    assert!(lines.contains(&":server 761 fred anna pronouns * :she/her".to_owned()));
    assert!(lines.contains(&":server 761 fred kai avatar * :qrc:/demo/kai-avatar.png".to_owned()));
    assert!(lines.contains(&format!(
        ":server 761 fred fred avatar * :{DEMO_SELF_AVATAR_URL}"
    )));
}

#[test]
fn the_oftc_self_avatar_is_the_same_https_url_and_only_pip_is_away() {
    let lines = presence_lines(&OFTC_WORLD);
    assert!(lines.contains(&format!(
        ":server 761 oak oak avatar * :{DEMO_SELF_AVATAR_URL}"
    )));
    let away: Vec<&String> = lines
        .iter()
        .filter(|line| line.contains("!u@h AWAY"))
        .collect();
    assert_eq!(away, vec![&":pip!u@h AWAY :away".to_owned()]);
}

#[test]
fn accounts_seed_on_omarchy_only() {
    assert_eq!(
        account_lines(&OMARCHY_WORLD),
        vec![
            ":lena!u@h ACCOUNT pinkieval".to_owned(),
            ":fred!u@h ACCOUNT fredm".to_owned(),
            ":ivy!u@h ACCOUNT *".to_owned(),
        ]
    );
    assert_eq!(
        live_account_lines(&OMARCHY_WORLD),
        vec![":teo!u@h ACCOUNT teoval".to_owned()]
    );
    assert!(account_lines(&OFTC_WORLD).is_empty());
    assert!(live_account_lines(&OFTC_WORLD).is_empty());
}

#[test]
fn the_world_opens_each_channel_then_the_presence_and_accounts() {
    let lines = world_lines(&OMARCHY_WORLD);
    assert_eq!(lines[0], ":fred!u@h JOIN :#omarchy");
    assert_eq!(
        lines[1],
        ":server 332 fred #omarchy :A cozy corner for Omarchy users and builders."
    );
    assert_eq!(
        lines[2],
        ":server 353 fred = #omarchy :&anna @dax @+mira ~fred %kai +teo lena sam ivy max"
    );
    assert_eq!(lines[3], ":server 366 fred #omarchy :End of NAMES");
    let desktop_join = lines
        .iter()
        .position(|line| line == ":fred!u@h JOIN :#desktop")
        .expect("#desktop joins");
    let first_presence = lines
        .iter()
        .position(|line| line.contains(" 352 "))
        .expect("WHO identities");
    assert!(
        desktop_join < first_presence,
        "channel states precede presence"
    );
}

#[test]
fn the_transcript_carries_accounts_times_and_the_seeded_link() {
    let omarchy = transcript_lines(&OMARCHY_WORLD);
    assert!(omarchy.contains(&":sol!u@h JOIN #omarchy solarius :sol Example".to_owned()));
    assert!(omarchy.contains(
        &"@account=kaidev;time=2026-09-12T09:59:00.000Z :kai!u@h PRIVMSG #omarchy :That balance is hard to get right: friendly defaults without hiding the actual system."
            .to_owned()
    ));
    assert!(omarchy.contains(
        &"@time=2026-09-12T10:12:00.000Z :anna!u@h PRIVMSG fred :fred: The prototype already feels at home. Nice work."
            .to_owned()
    ));

    let oftc = transcript_lines(&OFTC_WORLD);
    assert!(oftc.iter().any(|line| line.contains(
        ":ness!u@h PRIVMSG #lab :Package build is green: https://ci.example.com/omairc/42"
    )));
}

#[test]
fn the_typing_hint_is_for_anna_only() {
    assert_eq!(
        typing_lines(&OMARCHY_WORLD),
        vec![
            "@+typing=active :anna!u@h TAGMSG #omarchy".to_owned(),
            "@+typing=active :anna!u@h TAGMSG fred".to_owned(),
        ]
    );
    assert!(typing_lines(&OFTC_WORLD).is_empty());
}

#[test]
fn the_rtl_seed_lines_are_one_arabic_and_one_hebrew_privmsg() {
    let lines = rtl_seed_lines();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("PRIVMSG #omarchy :"));
    assert!(lines[0].contains("مرحبا"));
    assert!(lines[1].contains("PRIVMSG #omarchy :"));
    assert!(lines[1].contains("שלום"));
    // Not part of the parity transcript.
    assert!(!transcript_lines(&OMARCHY_WORLD)
        .iter()
        .any(|line| line.contains("مرحبا") || line.contains("שלום")));
}

#[test]
fn the_seeded_directs_carry_their_typing_and_read_flags() {
    let anna = OMARCHY_WORLD
        .directs
        .iter()
        .find(|direct| direct.nick == "anna")
        .expect("anna direct");
    assert!(anna.typing && !anna.mark_read);
    let dax = OMARCHY_WORLD
        .directs
        .iter()
        .find(|direct| direct.nick == "dax")
        .expect("dax direct");
    assert!(dax.mark_read && !dax.typing);
    let rio = OFTC_WORLD
        .directs
        .iter()
        .find(|direct| direct.nick == "rio")
        .expect("rio direct");
    assert!(rio.mark_read && !rio.typing);
}

#[test]
fn every_seeded_line_parses_with_the_portable_parser() {
    let omarchy = OMARCHY_WORLD;
    let mut all = Vec::new();
    all.extend(registration_lines(
        "fred",
        omarchy.welcome,
        omarchy.icon_url,
    ));
    all.extend(world_lines(&omarchy));
    all.extend(transcript_lines(&omarchy));
    all.extend(typing_lines(&omarchy));
    all.extend(live_account_lines(&omarchy));
    for text in &all {
        parse(text.as_bytes()).unwrap_or_else(|error| panic!("{text:?}: {error}"));
    }
}

#[test]
fn the_bench_channel_is_a_join_burst_then_mixed_lines() {
    let first = bench_channel_lines("fred", 50);
    let second = bench_channel_lines("fred", 50);
    assert_eq!(first, second, "the bench generator is deterministic");
    assert_eq!(first.len(), 4 + 50);
    assert_eq!(first[0], ":fred!u@h JOIN :#bench");
    assert_eq!(first[1], ":server 332 fred #bench :Generated bench channel");
    assert!(first[2].starts_with(":server 353 fred = #bench :user0 user1"));
    assert_eq!(first[3], ":server 366 fred #bench :End of NAMES");
    assert!(first[4..]
        .iter()
        .any(|line| line.contains("PRIVMSG #bench :")));

    for text in &first {
        parse(text.as_bytes()).unwrap_or_else(|error| panic!("{text:?}: {error}"));
    }
    let bytes = bench_channel_bytes("fred", 50);
    assert_eq!(
        bytes.iter().filter(|&&byte| byte == b'\n').count(),
        first.len()
    );
    assert!(bytes.ends_with(b"\r\n"));
}

#[test]
fn a_bench_channel_line_count_is_exact() {
    assert_eq!(bench_channel_lines("fred", 0).len(), 4);
    assert_eq!(bench_channel_lines("fred", 1000).len(), 1004);
}

#[test]
fn the_world_transcript_lines_are_all_chat_or_join() {
    for network in [&OMARCHY_WORLD, &OFTC_WORLD] {
        for text in transcript_lines(network) {
            assert!(
                text.contains(" PRIVMSG ") || text.contains(" JOIN "),
                "unexpected transcript line {text:?}"
            );
        }
    }
}
