//! Read markers across two sessions on one Ergo account (#442, #439, #486).

use omairc::irc::capability::IrcCapability;
use omairc::irc::wiretext::wire_text;
use omairc::live::{live_daemons, unique_channel, unique_nick, LiveClient, LiveClientOptions};

use super::helpers::{
    channel_has_body, direct_rows_for, register_ergo_account, require_live, wait_names_end,
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

/// Whether a PRIVMSG carrying `body` has arrived on the wire. Used with a DM
/// row's unread to prove a non-selected DM holds a line without selecting it
/// (selecting would clear unread via mark_selected).
fn saw_privmsg_body(client: &LiveClient, body: &str) -> bool {
    client.incoming().iter().any(|message| {
        message.command.eq_ignore_ascii_case("PRIVMSG")
            && !message.params.is_empty()
            && wire_text(message.params.last().expect("params")) == body
    })
}

fn dm_unread_with_body(client: &LiveClient, nick: &str, body: &str) -> bool {
    saw_privmsg_body(client, body)
        && dm_unread(client, nick).is_some_and(|unread| unread >= 1)
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

    // Own channels first so neither session auto-selects the DM as its first
    // conversation (claim_first_conversation → mark_selected clears unread).
    let reader_channel = unique_channel();
    let other_channel = unique_channel();
    assert!(
        reader.join_channel(&reader_channel),
        "{}: reader join {reader_channel}",
        daemon.name
    );
    assert!(
        wait_names_end(&mut reader, &reader_channel),
        "{}: reader names end",
        daemon.name
    );
    reader.select_channel(&reader_channel);
    assert!(
        other.join_channel(&other_channel),
        "{}: other join {other_channel}",
        daemon.name
    );
    assert!(
        wait_names_end(&mut other, &other_channel),
        "{}: other names end",
        daemon.name
    );
    other.select_channel(&other_channel);

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
        other.wait_until(|client| dm_unread_with_body(client, &peer_nick, &one)),
        "{}: other DM unread before reader select",
        daemon.name
    );
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
    // Keep A's DM selected with the window inactive so a selected-DM server
    // marker clear is exercised (live selected chat counts unread while unfocused).
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
    assert!(
        reader.wait_until(|client| {
            client
                .controller
                .selected_target()
                .eq_ignore_ascii_case(&peer_nick)
                && dm_unread_with_body(client, &peer_nick, &two)
        }),
        "{}: reader selected DM unread before other publishes",
        daemon.name
    );
    other.select_channel(&peer_nick);
    assert!(
        other.wait_until(|client| channel_has_body(client, &two)),
        "{}: other saw {two}",
        daemon.name
    );
    other.controller.set_window_active(true);
    other.controller.note_transcript_viewport(true, true);
    other.pump();
    // Wait on other first so its GET reply is pumped and the SET goes out;
    // reader.wait_until only pumps reader.
    assert!(
        other.wait_until(|client| markread_sets(&client.sent_lines(), &peer_nick).len() == 1),
        "{}: other SET after activate; markread={:?}",
        daemon.name,
        markread_lines(&other.sent_lines(), &peer_nick)
    );
    assert!(
        reader.wait_until(|client| dm_unread(client, &peer_nick) == Some(0)),
        "{}: reader unread cleared by other SET",
        daemon.name
    );
    // Off the DM and back while still inactive, then activate, for the I2 fence
    // (no second GET expected).
    reader.select_channel(&reader_channel);
    reader.select_channel(&peer_nick);
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
        markread_gets(&reader.sent_lines(), &peer_nick),
        1,
        "{}: still one GET before reconnect; markread={:?}",
        daemon.name,
        markread_lines(&reader.sent_lines(), &peer_nick)
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
    // I4: no SET after reconnect. That is also the proof the reconnect did not
    // move B's marker forward (a forward SET would have been written here if A
    // had anything newer). MARKREAD is consumed before incoming(), so a
    // "no MARKREAD in B.incoming" check can never fail.
    assert!(
        markread_sets(after, &peer_nick).is_empty(),
        "{}: no SET after reconnect; markread={:?}",
        daemon.name,
        markread_lines(after, &peer_nick)
    );
}
