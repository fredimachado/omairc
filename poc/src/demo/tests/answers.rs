//! The auto-answers as pure functions over one client frame: the Go
//! `TestAnswers*` cases of `demo_test.go`, plus the branches the ladder adds
//! (CTCP targets, METADATA ownership, MONITOR limits and removal, self JOIN
//! topics, and the echo).

use chrono::{Local, TimeZone, Utc};

use crate::demo::answers::{
    answer_away, answer_ctcp, answer_list, answer_metadata, answer_monitor, answer_ping,
    client_echo, demo_online_nicks, list_mask_matches, self_join,
};
use crate::demo::seed::{OFTC_WORLD, OMARCHY_WORLD};

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap()
}

fn text(reply: Option<Vec<u8>>) -> String {
    String::from_utf8(reply.expect("the frame should be answered")).expect("utf-8 reply")
}

#[test]
fn answers_client_ping() {
    assert_eq!(
        answer_ping(b"PING :omairc-watchdog\r\n"),
        Some(b":server PONG irc.example :omairc-watchdog\r\n".to_vec())
    );
}

#[test]
fn a_bare_ping_is_not_answered() {
    assert_eq!(answer_ping(b"PING\r\n"), None);
}

#[test]
fn monitor_add_splits_online_and_offline() {
    let online = demo_online_nicks(&OMARCHY_WORLD);
    let mut watched = Vec::new();
    let reply = answer_monitor("fred", b"MONITOR + anna,ghost\r\n", &online, &mut watched);
    assert_eq!(
        reply,
        Some(b":server 730 fred :anna!u@h\r\n:server 731 fred :ghost\r\n".to_vec())
    );
    assert_eq!(watched, vec!["anna".to_owned(), "ghost".to_owned()]);
}

#[test]
fn monitor_add_past_the_ceiling_overflows_with_734() {
    let online = demo_online_nicks(&OMARCHY_WORLD);
    let mut watched: Vec<String> = (0..100).map(|index| format!("n{index}")).collect();
    let reply = answer_monitor("fred", b"MONITOR + zed\r\n", &online, &mut watched);
    assert_eq!(
        reply,
        Some(b":server 734 fred 100 zed :Monitor list is full.\r\n".to_vec())
    );
    assert_eq!(watched.len(), 100, "a refused nick is not watched");
}

#[test]
fn monitor_list_clear_and_remove() {
    let online = demo_online_nicks(&OMARCHY_WORLD);
    let mut watched = vec!["anna".to_owned(), "ghost".to_owned()];

    let listed = answer_monitor("fred", b"MONITOR L\r\n", &online, &mut watched);
    assert_eq!(
        listed,
        Some(b":server 732 fred :anna,ghost\r\n:server 733 fred :End of MONITOR list\r\n".to_vec())
    );

    assert_eq!(
        answer_monitor("fred", b"MONITOR -ghost\r\n", &online, &mut watched),
        Some(Vec::new()),
        "a removal is handled with no reply"
    );
    assert_eq!(watched, vec!["anna".to_owned()]);

    assert_eq!(
        answer_monitor("fred", b"MONITOR C\r\n", &online, &mut watched),
        Some(Vec::new())
    );
    assert!(watched.is_empty());
}

#[test]
fn monitor_status_reports_the_watched_nicks() {
    let online = demo_online_nicks(&OMARCHY_WORLD);
    let mut watched = vec!["anna".to_owned(), "ghost".to_owned()];
    assert_eq!(
        answer_monitor("fred", b"MONITOR S\r\n", &online, &mut watched),
        Some(b":server 730 fred :anna!u@h\r\n:server 731 fred :ghost\r\n".to_vec())
    );
}

#[test]
fn monitor_ignores_an_unknown_modifier() {
    let online = demo_online_nicks(&OMARCHY_WORLD);
    let mut watched = Vec::new();
    assert_eq!(
        answer_monitor("fred", b"MONITOR X anna\r\n", &online, &mut watched),
        None
    );
}

#[test]
fn answers_list_with_the_seeded_directory() {
    let reply = text(answer_list(&OMARCHY_WORLD, b"LIST\r\n"));
    let lines: Vec<&str> = reply
        .split("\r\n")
        .filter(|line| !line.is_empty())
        .collect();
    assert_eq!(
        lines.first(),
        Some(&":server 321 fred Channel :Users  Name")
    );
    assert_eq!(lines.last(), Some(&":server 323 fred :End of /LIST"));

    let rows: Vec<&&str> = lines
        .iter()
        .filter(|line| line.starts_with(":server 322 "))
        .collect();
    assert_eq!(rows.len(), 6, "four seeded channels and two directory rows");
    assert!(rows
        .contains(&&":server 322 fred #omarchy 12 :A cozy corner for Omarchy users and builders."));
    assert!(rows.contains(
        &&":server 322 fred #linux 42 :\u{2}Kernel\u{2} discussion and \u{3}04distro\u{3} help."
    ));
}

#[test]
fn list_masks_are_case_insensitive_globs_or_exact() {
    assert!(list_mask_matches("#linux", ""));
    assert!(list_mask_matches("#linux", "#LINUX"));
    assert!(list_mask_matches("#linux", "#l*"));
    assert!(list_mask_matches("#linux", "#l?nux"));
    assert!(list_mask_matches("#linux", "#random,#linux"));
    assert!(!list_mask_matches("#linux", "#lin"));
    assert!(!list_mask_matches("#linux", "#random"));

    let filtered = text(answer_list(&OMARCHY_WORLD, b"LIST #l*\r\n"));
    let rows: Vec<&str> = filtered
        .split("\r\n")
        .filter(|line| line.starts_with(":server 322 "))
        .collect();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].contains(" #linux "));
}

#[test]
fn a_non_list_frame_is_not_answered() {
    assert_eq!(answer_list(&OMARCHY_WORLD, b"MODE #omarchy\r\n"), None);
}

#[test]
fn metadata_set_on_self_or_star_is_echoed_as_761() {
    assert_eq!(
        answer_metadata("fred", b"METADATA * SET display-name :Fred\r\n"),
        Some(b":server 761 fred fred display-name * :Fred\r\n".to_vec())
    );
    assert_eq!(
        answer_metadata("fred", b"METADATA fred SET DISPLAY-NAME :Fred\r\n"),
        Some(b":server 761 fred fred display-name * :Fred\r\n".to_vec()),
        "the key is canonicalized"
    );
}

#[test]
fn metadata_clear_is_766_and_unknown_or_foreign_is_not_answered() {
    assert_eq!(
        answer_metadata("fred", b"METADATA * SET display-name\r\n"),
        Some(b":server 766 fred fred display-name :unset\r\n".to_vec())
    );
    assert_eq!(
        answer_metadata("fred", b"METADATA * SET x-unknown :v\r\n"),
        None
    );
    assert_eq!(
        answer_metadata("fred", b"METADATA anna SET display-name :A\r\n"),
        None
    );
}

#[test]
fn away_is_306_with_a_reason_and_305_without() {
    assert_eq!(
        answer_away("fred", b"AWAY :brb\r\n"),
        Some(b":server 306 fred :You have been marked as being away\r\n".to_vec())
    );
    assert_eq!(
        answer_away("fred", b"AWAY\r\n"),
        Some(b":server 305 fred :You are no longer marked as being away\r\n".to_vec())
    );
    assert_eq!(answer_away("fred", b"AWAYS\r\n"), None);
}

#[test]
fn ctcp_ping_is_a_notice_from_the_queried_nick() {
    assert_eq!(
        answer_ctcp("fred", b"PRIVMSG anna :\x01PING 42\x01\r\n", now()),
        Some(b":anna!u@h NOTICE fred :\x01PING 42\x01\r\n".to_vec())
    );
}

#[test]
fn ctcp_version_is_the_desktop_reply() {
    assert_eq!(
        answer_ctcp("fred", b"PRIVMSG anna :\x01VERSION\x01\r\n", now()),
        Some(b":anna!u@h NOTICE fred :\x01VERSION https://omairc.app\x01\r\n".to_vec())
    );
}

#[test]
fn ctcp_time_is_rfc1123_in_local_time() {
    let reply = text(answer_ctcp(
        "fred",
        b"PRIVMSG anna :\x01TIME\x01\r\n",
        now(),
    ));
    let argument = reply
        .strip_prefix(":anna!u@h NOTICE fred :\x01TIME ")
        .and_then(|rest| rest.strip_suffix("\x01\r\n"))
        .expect("TIME reply shape");
    let expected = now()
        .with_timezone(&Local)
        .format("%a, %d %b %Y %H:%M:%S %z")
        .to_string();
    assert_eq!(argument, expected);
}

#[test]
fn ctcp_label_is_echoed_on_the_reply() {
    assert_eq!(
        answer_ctcp("fred", b"@label=req1 PRIVMSG anna :\x01PING\x01\r\n", now()),
        Some(b"@label=req1 :anna!u@h NOTICE fred :\x01PING\x01\r\n".to_vec())
    );
}

#[test]
fn ctcp_to_a_channel_or_an_action_is_not_answered() {
    assert_eq!(
        answer_ctcp("fred", b"PRIVMSG #omarchy :\x01PING 1\x01\r\n", now()),
        None
    );
    assert_eq!(
        answer_ctcp("fred", b"PRIVMSG anna :\x01ACTION waves\x01\r\n", now()),
        None
    );
    assert_eq!(
        answer_ctcp("fred", b"PRIVMSG anna :\x01FINGER\x01\r\n", now()),
        None
    );
}

#[test]
fn a_join_echoes_each_channel_with_its_seeded_topic() {
    let reply = text(Some(self_join(
        &OMARCHY_WORLD,
        "fred",
        b"JOIN #omarchy,#desktop\r\n",
    )));
    assert_eq!(
        reply,
        ":fred!u@h JOIN :#omarchy\r\n\
         :server 332 fred #omarchy :A cozy corner for Omarchy users and builders.\r\n\
         :fred!u@h JOIN :#desktop\r\n\
         :server 332 fred #desktop :Desktops should feel personal, fast, and calm.\r\n"
    );
}

#[test]
fn a_join_of_an_unseeded_channel_has_no_topic_line() {
    let reply = text(Some(self_join(&OMARCHY_WORLD, "fred", b"JOIN #new\r\n")));
    assert_eq!(reply, ":fred!u@h JOIN :#new\r\n");
}

#[test]
fn a_join_topic_matches_the_channel_case_insensitively() {
    let reply = text(Some(self_join(
        &OMARCHY_WORLD,
        "fred",
        b"JOIN #OMARCHY\r\n",
    )));
    assert!(reply.contains(":server 332 fred #OMARCHY :A cozy corner"));
}

#[test]
fn a_join_on_oftc_restores_the_oftc_topic() {
    let reply = text(Some(self_join(&OFTC_WORLD, "oak", b"JOIN #lab\r\n")));
    assert_eq!(
        reply,
        ":oak!u@h JOIN :#lab\r\n:server 332 oak #lab :Build lab for packaging and CI.\r\n"
    );
}

#[test]
fn the_client_echo_keeps_the_frame_bytes() {
    assert_eq!(
        client_echo("fred", b"PRIVMSG #omarchy :hi\r\n"),
        b":fred!u@h PRIVMSG #omarchy :hi\r\n".to_vec()
    );
}

#[test]
fn the_online_set_is_self_members_and_directs_sorted_and_unique() {
    let online = demo_online_nicks(&OMARCHY_WORLD);
    let mut sorted = online.clone();
    sorted.sort();
    assert_eq!(online, sorted);
    assert_eq!(
        online.iter().filter(|nick| nick.as_str() == "fred").count(),
        1
    );
    for nick in ["anna", "dax", "fred", "ivy", "max", "teo", "sam", "lena"] {
        assert!(online.iter().any(|candidate| candidate == nick), "{nick}");
    }
}
