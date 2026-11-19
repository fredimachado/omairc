//! Read markers across two sessions on one Ergo account (#442, #439, #486).

use omairc::irc::capability::IrcCapability;
use omairc::irc::wiretext::wire_text;
use omairc::live::{live_daemons, unique_nick, LiveClient, LiveClientOptions};

use super::helpers::{
    channel_has_body, direct_rows_for, register_ergo_account, require_live,
};

fn markread_gets(lines: &[String], target: &str) -> usize {
    let want = format!("MARKREAD {target}");
    lines.iter().filter(|line| line.as_str() == want).count()
}

fn markread_sets(lines: &[String], target: &str) -> Vec<String> {
    let prefix = format!("MARKREAD {target} timestamp=");
    lines
        .iter()
        .filter(|line| line.starts_with(&prefix))
        .cloned()
        .collect()
}

fn dm_unread(client: &LiveClient, nick: &str) -> Option<u32> {
    direct_rows_for(client, nick).first().map(|row| row.unread)
}

fn markread_lines(lines: &[String], target: &str) -> Vec<String> {
    let prefix = format!("MARKREAD {target}");
    lines
        .iter()
        .filter(|line| line.as_str() == prefix || line.starts_with(&format!("{prefix} ")))
        .cloned()
        .collect()
}

fn has_read_marker(client: &LiveClient) -> bool {
    client
        .controller
        .session(&client.network_id)
        .expect("session")
        .capabilities()
        .contains(IrcCapability::ReadMarker)
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn read_marker_across_sessions() {
    require_live();
    // Ergo only: needs SASL, multiclient, and draft/read-marker. Solanum/ngIRCd
    // have no SASL accounts in the fixture (DaemonInfo.sasl == false).
    let Some(daemon) = live_daemons().into_iter().find(|d| d.name == "ergo") else {
        return;
    };
    let tls = daemon.use_tls();
    let account = unique_nick(daemon.nick_length);
    let password = "live-marker-secret";
    register_ergo_account(&daemon, &account, password);

    let mut reader = LiveClient::connect(
        &daemon,
        &account,
        tls,
        LiveClientOptions {
            password: password.to_owned(),
            reconnect_enabled: true,
            ..LiveClientOptions::default()
        },
    );
    let mut other = LiveClient::connect(
        &daemon,
        &account,
        tls,
        LiveClientOptions {
            password: password.to_owned(),
            ..LiveClientOptions::default()
        },
    );
    assert!(
        reader.wait_registered(),
        "{}: reader register: {}",
        daemon.name,
        reader.last_error().unwrap_or("no error")
    );
    assert!(
        other.wait_registered(),
        "{}: other register: {}",
        daemon.name,
        other.last_error().unwrap_or("no error")
    );
    assert!(
        has_read_marker(&reader)
            || reader.wait_until(|client| has_read_marker(client)),
        "{}: reader missing ReadMarker",
        daemon.name
    );
    assert!(
        has_read_marker(&other) || other.wait_until(|client| has_read_marker(client)),
        "{}: other missing ReadMarker",
        daemon.name
    );
    assert_eq!(
        other.nick(),
        account,
        "{}: other nick must stay on the account",
        daemon.name
    );

    other.controller.set_window_active(false);
    other.pump();
    reader.controller.note_transcript_viewport(true, true);
    reader.pump();

    let mut peer = LiveClient::connect(
        &daemon,
        &unique_nick(daemon.nick_length),
        tls,
        LiveClientOptions::default(),
    );
    assert!(
        peer.wait_registered(),
        "{}: peer register: {}",
        daemon.name,
        peer.last_error().unwrap_or("no error")
    );
    let peer_nick = peer.nick();
    let peer_network = peer.network_id.clone();

    // Leg 1: A reads, B's marker moves.
    let one = format!("marker-one-{account}");
    assert!(
        peer.controller
            .send_to_target(&peer_network, &account, &one),
        "{}: peer send one",
        daemon.name
    );
    peer.pump();
    assert!(
        reader.wait_until(|client| !direct_rows_for(client, &peer_nick).is_empty()),
        "{}: reader DM row for {peer_nick}",
        daemon.name
    );
    reader.select_channel(&peer_nick);
    assert!(
        reader.wait_until(|client| markread_sets(&client.sent_lines(), &peer_nick).len() == 1),
        "{}: reader SET after select; markread={:?}",
        daemon.name,
        markread_lines(&reader.sent_lines(), &peer_nick)
    );
    assert!(
        other.wait_until(|client| dm_unread(client, &peer_nick) == Some(0)),
        "{}: other unread cleared by reader SET",
        daemon.name
    );
    assert!(
        reader.fence("leg1"),
        "{}: reader fence after leg 1",
        daemon.name
    );
    assert_eq!(
        markread_gets(&reader.sent_lines(), &peer_nick),
        1,
        "{}: one GET after leg 1; markread={:?}",
        daemon.name,
        markread_lines(&reader.sent_lines(), &peer_nick)
    );
    assert_eq!(
        markread_sets(&reader.sent_lines(), &peer_nick).len(),
        1,
        "{}: one SET after leg 1; markread={:?}",
        daemon.name,
        markread_lines(&reader.sent_lines(), &peer_nick)
    );

    // Leg 2: B reads, A must not answer with an equal SET.
    reader.controller.set_window_active(false);
    reader.pump();
    let two = format!("marker-two-{account}");
    assert!(
        peer.controller
            .send_to_target(&peer_network, &account, &two),
        "{}: peer send two",
        daemon.name
    );
    peer.pump();
    if !other
        .controller
        .selected_target()
        .eq_ignore_ascii_case(&peer_nick)
    {
        other.select_channel(&peer_nick);
    }
    assert!(
        other.wait_until(|client| channel_has_body(client, &two)),
        "{}: other saw {two}",
        daemon.name
    );
    other.controller.set_window_active(true);
    other.controller.note_transcript_viewport(true, true);
    other.pump();
    assert!(
        reader.wait_until(|client| {
            channel_has_body(client, &two) && dm_unread(client, &peer_nick) == Some(0)
        }),
        "{}: reader unread cleared by other SET",
        daemon.name
    );
    reader.controller.set_window_active(true);
    reader.controller.note_transcript_viewport(true, true);
    reader.pump();
    assert!(
        reader.fence("leg2-a"),
        "{}: reader fence after leg 2",
        daemon.name
    );
    assert!(
        other.fence("leg2-b"),
        "{}: other fence after leg 2",
        daemon.name
    );
    assert_eq!(
        markread_sets(&reader.sent_lines(), &peer_nick).len(),
        1,
        "{}: reader must not SET again at known marker; markread={:?}",
        daemon.name,
        markread_lines(&reader.sent_lines(), &peer_nick)
    );
    assert_eq!(
        markread_sets(&other.sent_lines(), &peer_nick).len(),
        1,
        "{}: other one SET for stamp two; markread={:?}",
        daemon.name,
        markread_lines(&other.sent_lines(), &peer_nick)
    );

    // Leg 3: reconnect writes one GET and no SET.
    let sent_before = reader.sent_lines().len();
    let other_incoming_before = other.incoming().len();
    assert!(
        reader.force_disconnect(),
        "{}: force_disconnect",
        daemon.name
    );
    assert!(
        reader.wait_reconnecting(),
        "{}: wait_reconnecting",
        daemon.name
    );
    assert!(
        reader.wait_registered_again(),
        "{}: wait_registered_again: {}",
        daemon.name,
        reader.last_error().unwrap_or("no error")
    );
    if !reader
        .controller
        .selected_target()
        .eq_ignore_ascii_case(&peer_nick)
    {
        reader.select_channel(&peer_nick);
    }
    // Deviation: plan waits for MARKREAD in incoming(), but deliver_read_marker
    // consumes those lines before the received log. Wait for the outbound GET
    // instead; the fence then orders the reply (Ergo answers in write order).
    assert!(
        reader.wait_until(|client| {
            markread_gets(&client.sent_lines()[sent_before..], &peer_nick) == 1
        }),
        "{}: GET after reconnect; markread={:?}",
        daemon.name,
        markread_lines(&reader.sent_lines()[sent_before..], &peer_nick)
    );
    assert!(
        reader.fence("leg3-a"),
        "{}: reader fence after reconnect",
        daemon.name
    );
    let after = &reader.sent_lines()[sent_before..];
    assert_eq!(
        markread_gets(after, &peer_nick),
        1,
        "{}: one GET after reconnect; markread={:?}",
        daemon.name,
        markread_lines(after, &peer_nick)
    );
    assert!(
        markread_sets(after, &peer_nick).is_empty(),
        "{}: no SET after reconnect; markread={:?}",
        daemon.name,
        markread_lines(after, &peer_nick)
    );
    assert!(
        other.fence("leg3-b"),
        "{}: other fence after reconnect",
        daemon.name
    );
    let other_new = &other.incoming()[other_incoming_before..];
    assert!(
        !other_new.iter().any(|message| {
            message.command.eq_ignore_ascii_case("MARKREAD")
                && message
                    .params
                    .first()
                    .is_some_and(|param| wire_text(param).eq_ignore_ascii_case(&peer_nick))
        }),
        "{}: other must not see a MARKREAD for peer after reconnect",
        daemon.name
    );
    assert_eq!(
        dm_unread(&other, &peer_nick),
        Some(0),
        "{}: other unread stays cleared",
        daemon.name
    );
}
