use omairc::irc::capability::IrcCapability;
use omairc::live::{
    live_daemons, unique_channel, unique_nick, LiveClient, LiveClientOptions,
};

use super::helpers::{channel_has_body, names_end_count, require_live, wait_names_end};

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn reconnect_autojoin_keeps_channel_state() {
    require_live();
    let Some(daemon) = live_daemons().into_iter().find(|d| d.name == "ergo") else {
        return;
    };
    let tls = daemon.use_tls();
    let channel = unique_channel();
    let initial_seed = format!("reconnect-initial-{channel}");
    let replay_seed = format!("reconnect-replay-{channel}");

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
        .send_to_target(&peer_network, &channel, &initial_seed));
    peer.pump();

    let mut client = LiveClient::connect(
        &daemon,
        &unique_nick(daemon.nick_length),
        tls,
        LiveClientOptions {
            reconnect_enabled: true,
            autojoin_channels: vec![channel.clone()],
            ..LiveClientOptions::default()
        },
    );
    assert!(
        client.wait_registered(),
        "{}",
        client.last_error().unwrap_or("no error")
    );
    assert!(wait_names_end(&mut client, &channel));
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
    client.select_channel(&channel);
    let peer_nick = peer.nick();
    assert!(client.wait_until(|client| client.member(&peer_nick).is_some()));
    assert!(client.wait_until(|client| channel_has_body(client, &initial_seed)));
    assert_eq!(
        client
            .controller
            .conversations()
            .iter()
            .filter(|row| row.target.eq_ignore_ascii_case(&channel))
            .count(),
        1
    );
    assert_eq!(client.controller.selected_target(), channel);

    let names_before = names_end_count(&client.incoming(), &channel);
    assert!(client.force_disconnect());
    assert!(client.wait_reconnecting());
    assert!(
        peer.controller
            .send_to_target(&peer_network, &channel, &replay_seed)
    );
    peer.pump();
    assert!(client.wait_registered_again());
    assert!(client.wait_until(|client| {
        names_end_count(&client.incoming(), &channel) > names_before
    }));
    assert_eq!(
        client
            .controller
            .conversations()
            .iter()
            .filter(|row| row.target.eq_ignore_ascii_case(&channel))
            .count(),
        1
    );
    if !client
        .controller
        .selected_target()
        .eq_ignore_ascii_case(&channel)
    {
        client.select_channel(&channel);
    }
    assert_eq!(client.controller.selected_target(), channel);
    assert!(client.wait_until(|client| client.member(&peer_nick).is_some()));
    assert!(!client.controller.members().is_empty());
    assert!(client.wait_until(|client| {
        channel_has_body(client, &initial_seed) && channel_has_body(client, &replay_seed)
    }));
}
