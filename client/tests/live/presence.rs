use omairc::irc::capability::IrcCapability;
use omairc::irc::world::Origin;
use omairc::live::{
    live_daemons, unique_channel, unique_nick, LiveClient, LiveClientOptions,
};

use super::helpers::{channel_has_body, require_live, wait_names_end};

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn who_snapshot() {
    require_live();
    for daemon in live_daemons() {
        if !daemon.away_notify {
            continue;
        }
        let mut client = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            daemon.use_tls(),
            LiveClientOptions::default(),
        );
        assert!(client.wait_registered(), "{}", daemon.name);
        let channel = unique_channel();
        assert!(client.join_channel(&channel));
        assert!(
            client.wait_until(|client| {
                omairc::live::message_has_command(&client.incoming(), "352")
            }),
            "{} WHO 352",
            daemon.name
        );
        assert!(
            !omairc::live::message_has_command(&client.incoming(), "354"),
            "{} WHO must be classic 352, not WHOX 354",
            daemon.name
        );
    }
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn peer_away() {
    require_live();
    for daemon in live_daemons() {
        if !daemon.away_notify {
            continue;
        }
        let tls = daemon.use_tls();
        let mut client = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        assert!(client.wait_registered(), "{}", daemon.name);
        let channel = unique_channel();
        assert!(client.join_channel(&channel));
        assert!(wait_names_end(&mut client, &channel));

        let mut peer = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        assert!(peer.wait_registered());
        assert!(peer.join_channel(&channel));
        assert!(wait_names_end(&mut peer, &channel));
        client.select_channel(&channel);
        let peer_nick = peer.nick();
        assert!(client.wait_until(|client| client.member(&peer_nick).is_some()));
        assert!(peer
            .controller
            .session(&peer.network_id)
            .expect("session")
            .set_away("gone"));
        peer.pump();
        assert!(
            client.wait_until(|client| {
                client
                    .member(&peer_nick)
                    .is_some_and(|member| member.is_away())
            }),
            "{} away",
            daemon.name
        );
    }
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn peer_typing() {
    require_live();
    for daemon in live_daemons() {
        if !daemon.message_tags {
            continue;
        }
        let tls = daemon.use_tls();
        let mut client = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        assert!(client.wait_registered(), "{}", daemon.name);
        let channel = unique_channel();
        assert!(client.join_channel(&channel));
        assert!(wait_names_end(&mut client, &channel));
        client.select_channel(&channel);

        let mut peer = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        assert!(peer.wait_registered());
        assert!(peer.join_channel(&channel));
        assert!(wait_names_end(&mut peer, &channel));
        let peer_nick = peer.nick();
        assert!(client.wait_until(|client| client.member(&peer_nick).is_some()));
        assert!(peer
            .controller
            .session(&peer.network_id)
            .expect("session")
            .send_typing(&channel, omairc::irc::typing::TypingPhase::Active));
        peer.pump();
        assert!(
            client.wait_until(|client| {
                client
                    .controller
                    .typing_nicks()
                    .iter()
                    .any(|nick| nick.eq_ignore_ascii_case(&peer_nick))
            }),
            "{} typing",
            daemon.name
        );
    }
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn peer_metadata() {
    require_live();
    let Some(daemon) = live_daemons().into_iter().find(|d| d.name == "ergo") else {
        return;
    };
    let tls = daemon.use_tls();
    let mut client = LiveClient::connect(
        &daemon,
        &unique_nick(daemon.nick_length),
        tls,
        LiveClientOptions::default(),
    );
    assert!(client.wait_registered());
    assert!(
        client.controller.has_member_status()
            || client.wait_until(|client| {
                client
                    .controller
                    .session(&client.network_id)
                    .expect("session")
                    .capabilities()
                    .contains(IrcCapability::MemberMetadata)
            })
    );
    let channel = unique_channel();
    assert!(client.join_channel(&channel));
    assert!(wait_names_end(&mut client, &channel));
    client.select_channel(&channel);

    let mut peer = LiveClient::connect(
        &daemon,
        &unique_nick(daemon.nick_length),
        tls,
        LiveClientOptions::default(),
    );
    assert!(peer.wait_registered());
    assert!(peer.join_channel(&channel));
    assert!(wait_names_end(&mut peer, &channel));
    let peer_nick = peer.nick();
    assert!(client.wait_until(|client| client.member(&peer_nick).is_some()));
    assert!(peer
        .controller
        .session(&peer.network_id)
        .expect("session")
        .set_own_metadata("status", "wave")
        .is_some());
    peer.pump();
    assert!(client.wait_until(|client| {
        client
            .member(&peer_nick)
            .is_some_and(|member| member.status == "wave")
    }));
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn chat_history_on_join() {
    require_live();
    let Some(daemon) = live_daemons().into_iter().find(|d| d.name == "ergo") else {
        return;
    };
    let tls = daemon.use_tls();
    let channel = unique_channel();
    let seed = format!("history-seed-{channel}");

    let mut peer = LiveClient::connect(
        &daemon,
        &unique_nick(daemon.nick_length),
        tls,
        LiveClientOptions::default(),
    );
    assert!(peer.wait_registered());
    assert!(peer.join_channel(&channel));
    assert!(wait_names_end(&mut peer, &channel));
    let peer_network = peer.network_id.clone();
    assert!(peer
        .controller
        .send_to_target(&peer_network, &channel, &seed));
    peer.pump();
    // Prove the seed is in history before the client joins.
    assert!(peer
        .controller
        .session(&peer_network)
        .expect("session")
        .send_raw("PING :history-seed"));
    peer.pump();
    assert!(peer.wait_until(|peer| {
        omairc::live::message_has_command(&peer.incoming(), "PONG")
    }));

    let mut client = LiveClient::connect(
        &daemon,
        &unique_nick(daemon.nick_length),
        tls,
        LiveClientOptions::default(),
    );
    assert!(client.wait_registered());
    assert!(
        client
            .controller
            .session(&client.network_id)
            .expect("session")
            .capabilities()
            .contains(IrcCapability::ChatHistory)
            || client.wait_until(|client| {
                client
                    .controller
                    .session(&client.network_id)
                    .expect("session")
                    .capabilities()
                    .contains(IrcCapability::ChatHistory)
            })
    );
    assert!(client.join_channel(&channel));
    client.select_channel(&channel);
    assert!(client.wait_until(|client| channel_has_body(client, &seed)));
    let row = client
        .messages()
        .iter()
        .find(|message| message.body == seed)
        .expect("seed row");
    assert_eq!(row.origin, Origin::Replay);
    let _ = row.time;
    assert_eq!(client.controller.unread_count_for(&client.network_id), 0);
    for entry in client.status_lines() {
        assert_ne!(entry.label, "CHATHISTORY");
        assert_ne!(entry.label, "PING");
        assert_ne!(entry.label, "PONG");
        assert_ne!(entry.label, "JOIN");
        assert!(!entry.text.contains(&seed));
    }
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn sasl_plain() {
    require_live();
    let Some(daemon) = live_daemons().into_iter().find(|d| d.name == "ergo") else {
        return;
    };
    let nick = unique_nick(daemon.nick_length);
    let password = "live-secret";
    {
        let mut guest = LiveClient::connect(
            &daemon,
            &nick,
            daemon.use_tls(),
            LiveClientOptions::default(),
        );
        assert!(guest.wait_registered());
        let notices = omairc::live::message_command_count(&guest.incoming(), "NOTICE");
        let network = guest.network_id.clone();
        assert!(guest.controller.send_to_target(
            &network,
            "NickServ",
            &format!("REGISTER {password} live@omairc.test")
        ));
        guest.pump();
        assert!(guest.wait_until(|guest| {
            omairc::live::message_command_count(&guest.incoming(), "NOTICE") > notices
        }));
        assert!(super::helpers::conversation_row(&guest.controller, &network, "NickServ").is_none());
        assert!(super::helpers::no_status_text_contains(&guest, password));
    }
    let mut authed = LiveClient::connect(
        &daemon,
        &nick,
        daemon.use_tls(),
        LiveClientOptions {
            password: password.to_owned(),
            ..LiveClientOptions::default()
        },
    );
    assert!(authed.wait_server_label("903"));
    assert!(
        authed.wait_registered(),
        "{}",
        authed.last_error().unwrap_or("no error")
    );
    authed.controller.open_status(&authed.network_id);
    assert!(super::helpers::conversation_row(
        &authed.controller,
        &authed.network_id,
        "NickServ"
    )
    .is_none());
    assert!(!authed.has_server_label("AUTHENTICATE"));
    assert!(super::helpers::no_status_text_contains(&authed, password));
    assert!(super::helpers::no_status_text_contains(&authed, "AUTHENTICATE"));
}
