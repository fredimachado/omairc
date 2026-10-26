use omairc::irc::capability::IrcCapability;
use omairc::irc::command::ComposerSurface;
use omairc::live::{
    live_daemons, unique_channel, unique_nick, LiveClient, LiveClientOptions,
};

use super::helpers::{
    channel_has_body, conversation_row, require_live, status_text_contains, wait_names_end,
};

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn conversation_invention_matrix() {
    require_live();
    let Some(daemon) = live_daemons().into_iter().find(|d| d.name == "ergo") else {
        return;
    };
    let tls = daemon.use_tls();
    let peer_nick = unique_nick(daemon.nick_length);
    let target_nick = unique_nick(daemon.nick_length);
    let dm_body = "hello";
    let peer_body = "hi";

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
            .contains(IrcCapability::EchoMessage)
            || client.wait_until(|client| {
                client
                    .controller
                    .session(&client.network_id)
                    .expect("session")
                    .capabilities()
                    .contains(IrcCapability::EchoMessage)
            })
    );
    client.controller.open_status(&client.network_id);

    let channel = unique_channel();
    assert!(client.join_channel(&channel));
    assert!(wait_names_end(&mut client, &channel));
    client.select_channel(&channel);
    assert_eq!(client.controller.selected_target(), channel);

    let mut msg_target = LiveClient::connect(
        &daemon,
        &target_nick,
        tls,
        LiveClientOptions::default(),
    );
    assert!(msg_target.wait_registered());
    let network = client.network_id.clone();
    assert!(client
        .controller
        .submit(ComposerSurface::Conversation, &format!("/msg {target_nick} {dm_body}")));
    client.pump();
    assert!(conversation_row(&client.controller, &network, &target_nick).is_none());
    assert_eq!(client.controller.selected_target(), channel);

    let self_nick = client.nick();
    assert!(client.wait_until(|client| {
        client.incoming().iter().any(|message| {
            message.command == "PRIVMSG"
                && message
                    .prefix
                    .as_ref()
                    .is_some_and(|prefix| {
                        omairc::irc::wiretext::wire_text(&prefix.nick)
                            .eq_ignore_ascii_case(&self_nick)
                    })
                && message.params.first().is_some_and(|param| {
                    omairc::irc::wiretext::wire_text(param).eq_ignore_ascii_case(&target_nick)
                })
                && message
                    .params
                    .last()
                    .is_some_and(|param| omairc::irc::wiretext::wire_text(param) == dm_body)
        })
    }));
    assert!(conversation_row(&client.controller, &network, &target_nick).is_none());
    assert_eq!(client.controller.selected_target(), channel);
    assert!(!channel_has_body(&client, dm_body));

    assert!(client
        .controller
        .session(&network)
        .expect("session")
        .send_privmsg("NickServ", "HELP"));
    client.pump();
    assert!(client.wait_until(|client| {
        client.incoming().iter().any(|message| {
            message.command == "NOTICE"
                && message.prefix.as_ref().is_some_and(|prefix| {
                    omairc::irc::wiretext::wire_text(&prefix.nick).eq_ignore_ascii_case("NickServ")
                })
        })
    }));
    assert!(conversation_row(&client.controller, &network, "NickServ").is_none());
    assert!(
        status_text_contains(&client, "NickServ")
            || client
                .status_lines()
                .iter()
                .any(|entry| entry.label.eq_ignore_ascii_case("NOTICE"))
    );

    let mut peer = LiveClient::connect(
        &daemon,
        &peer_nick,
        tls,
        LiveClientOptions::default(),
    );
    assert!(peer.wait_registered());
    let peer_network = peer.network_id.clone();
    assert!(peer
        .controller
        .send_to_target(&peer_network, &self_nick, peer_body));
    peer.pump();
    assert!(client.wait_until(|client| {
        conversation_row(&client.controller, &network, &peer_nick).is_some()
    }));
    assert_eq!(client.controller.selected_target(), channel);
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn folded_nick_collision() {
    require_live();
    for daemon in live_daemons() {
        if daemon.nick_length < 5 {
            continue;
        }
        let tls = daemon.use_tls();
        let base = unique_nick(daemon.nick_length - 1);
        let nick_a = format!("{base}[");
        let nick_b = format!("{base}{{");
        let mut a = LiveClient::connect(&daemon, &nick_a, tls, LiveClientOptions::default());
        assert!(a.wait_registered(), "{}", daemon.name);
        let folded = a
            .features()
            .case_mapping()
            .equals(&a.nick(), &nick_b);
        let mut b = LiveClient::connect(&daemon, &nick_b, tls, LiveClientOptions::default());
        if !folded {
            assert!(b.wait_registered(), "{}", daemon.name);
            continue;
        }
        if b.wait_registered() {
            // Daemon advertised folding but accepted both nicks.
            continue;
        }
        assert!(b.wait_failed(), "{}", daemon.name);
    }
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn status_isolation_and_same_nick_directs() {
    require_live();
    let daemons = live_daemons();
    if daemons.len() < 2 {
        // Dual-network proof needs two daemons in the live profile (default compose has them).
        return;
    }
    let left = &daemons[0];
    let right = &daemons[1];
    let nick_len = left.nick_length.min(right.nick_length);
    let client_nick = unique_nick(nick_len);
    let peer_nick = unique_nick(nick_len);
    let left_mark = format!("inbound-left-{peer_nick}");
    let right_mark = format!("inbound-right-{peer_nick}");

    let left_net_id = "live-left".to_owned();
    let right_net_id = "live-right".to_owned();
    let mut client = LiveClient::connect(
        left,
        &client_nick,
        left.use_tls(),
        LiveClientOptions {
            network_id: left_net_id.clone(),
            ..LiveClientOptions::default()
        },
    );
    let right_net = client.connect_additional(
        right,
        &client_nick,
        right.use_tls(),
        LiveClientOptions {
            network_id: right_net_id.clone(),
            ..LiveClientOptions::default()
        },
    );
    assert!(
        client.wait_registered(),
        "{}",
        client.last_error().unwrap_or("no error")
    );
    assert!(
        client.wait_registered_network(&right_net),
        "{}",
        client
            .controller
            .last_error_for(&right_net)
            .unwrap_or("no error")
    );
    let left_self = client.nick();
    let right_self = client
        .controller
        .session(&right_net)
        .expect("right session")
        .nick();

    let mut peer_left = LiveClient::connect(
        left,
        &peer_nick,
        left.use_tls(),
        LiveClientOptions::default(),
    );
    let mut peer_right = LiveClient::connect(
        right,
        &peer_nick,
        right.use_tls(),
        LiveClientOptions::default(),
    );
    assert!(peer_left.wait_registered());
    assert!(peer_right.wait_registered());
    let peer_left_net = peer_left.network_id.clone();
    let peer_right_net = peer_right.network_id.clone();

    assert!(peer_left
        .controller
        .send_to_target(&peer_left_net, &left_self, &left_mark));
    peer_left.pump();
    assert!(peer_right
        .controller
        .send_to_target(&peer_right_net, &right_self, &right_mark));
    peer_right.pump();

    assert!(client.wait_until(|client| {
        conversation_row(&client.controller, &left_net_id, &peer_nick).is_some()
            && conversation_row(&client.controller, &right_net, &peer_nick).is_some()
    }));

    // Status on one network must not surface the other network's inbound DM body.
    client.controller.open_status(&left_net_id);
    client.pump();
    assert!(
        !client
            .controller
            .status_lines(&left_net_id)
            .iter()
            .any(|entry| entry.text.contains(&right_mark)),
        "left Status must not carry the right-network DM body"
    );
    client.controller.open_status(&right_net);
    client.pump();
    assert!(
        !client
            .controller
            .status_lines(&right_net)
            .iter()
            .any(|entry| entry.text.contains(&left_mark)),
        "right Status must not carry the left-network DM body"
    );

    // Same peer nick on both networks: each transcript stays network-scoped.
    client
        .controller
        .select_conversation(&left_net_id, &peer_nick);
    client.pump();
    assert!(client.wait_until(|client| channel_has_body(client, &left_mark)));
    assert!(
        !channel_has_body(&client, &right_mark),
        "left DM must not show the right-network mark"
    );

    client
        .controller
        .select_conversation(&right_net, &peer_nick);
    client.pump();
    assert!(client.wait_until(|client| channel_has_body(client, &right_mark)));
    assert!(
        !channel_has_body(&client, &left_mark),
        "right DM must not show the left-network mark"
    );

    client
        .controller
        .select_conversation(&left_net_id, &peer_nick);
    client.pump();
    assert!(channel_has_body(&client, &left_mark));
    assert!(!channel_has_body(&client, &right_mark));
}
