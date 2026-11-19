//! MONITOR around an open DM: Status offline/online lines and a MonitorOnline
//! inbox row, without inventing a second DM. Neither client MONITORs an open DM
//! by itself; this case asserts what the clients do today after `/monitor`.

use omairc::irc::command::ComposerSurface;
use omairc::irc::inbox::InboxKind;
use omairc::irc::message::IrcMessage;
use omairc::live::{live_daemons, unique_channel, unique_nick, LiveClient, LiveClientOptions};

use super::helpers::{direct_rows_for, require_live, wait_names_end};

/// True when a MONITOR numeric's trailing list names `nick` (entry may be
/// `nick!user@host`; compared case-insensitively).
fn saw_monitor_reply(incoming: &[IrcMessage], numeric: &str, nick: &str) -> bool {
    incoming.iter().any(|message| {
        if message.command != numeric || message.params.is_empty() {
            return false;
        }
        let trailing = message.text_param(message.params.len() - 1);
        trailing.split(',').any(|entry| {
            let before_bang = entry.split_once('!').map_or(entry, |(n, _)| n);
            before_bang.trim().eq_ignore_ascii_case(nick)
        })
    })
}

fn status_line_is(client: &LiveClient, text: &str) -> bool {
    client.status_lines().iter().any(|entry| entry.text == text)
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn monitor_direct_presence() {
    require_live();
    for daemon in live_daemons() {
        let tls = daemon.use_tls();
        let mut client = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        assert!(client.wait_registered(), "{}", daemon.name);
        assert!(client.wait_motd(), "{} motd", daemon.name);

        // Skip daemons that do not advertise MONITOR in ISUPPORT.
        if !client.features().monitor_advertised() {
            // Guard against a silent skip on Ergo: the fixture must advertise it.
            assert_ne!(
                daemon.name, "ergo",
                "{} must advertise MONITOR",
                daemon.name
            );
            continue;
        }

        let peer_nick = unique_nick(daemon.nick_length);
        let mut peer = LiveClient::connect(&daemon, &peer_nick, tls, LiveClientOptions::default());
        assert!(peer.wait_registered(), "{} peer", daemon.name);

        let channel = unique_channel();
        assert!(client.join_channel(&channel));
        assert!(wait_names_end(&mut client, &channel));
        client.select_channel(&channel);

        // open_direct needs a selection to name the network.
        assert!(
            client.controller.open_direct(&peer_nick),
            "{} open_direct",
            daemon.name
        );

        assert!(client
            .controller
            .submit(ComposerSurface::Conversation, &format!("/monitor {peer_nick}")));
        assert!(
            client.wait_until(|client| saw_monitor_reply(&client.incoming(), "730", &peer_nick)),
            "{} 730 for {peer_nick} before QUIT",
            daemon.name
        );

        let peer_network = peer.network_id.clone();
        assert!(
            peer.controller.disconnect(&peer_network),
            "{} peer disconnect",
            daemon.name
        );
        drop(peer);

        let offline = format!("{peer_nick} is offline");
        assert!(
            client.wait_until(|client| status_line_is(client, &offline)),
            "{} status {offline}",
            daemon.name
        );

        let mut again =
            LiveClient::connect(&daemon, &peer_nick, tls, LiveClientOptions::default());
        assert!(again.wait_registered(), "{} peer again", daemon.name);
        assert_eq!(again.nick(), peer_nick, "{} peer nick after reconnect", daemon.name);

        let online = format!("{peer_nick} is online");
        assert!(
            client.wait_until(|client| {
                status_line_is(client, &online)
                    && client.controller.inbox_items().iter().any(|item| {
                        item.kind == InboxKind::MonitorOnline && item.actor == peer_nick
                    })
            }),
            "{} status {online} and MonitorOnline inbox",
            daemon.name
        );

        assert_eq!(
            direct_rows_for(&client, &peer_nick).len(),
            1,
            "{} MONITOR invents no extra DM",
            daemon.name
        );
        drop(again);
    }
}
