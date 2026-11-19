use omairc::irc::command::ComposerSurface;
use omairc::irc::serverfeatures::IrcServerFeatures;
use omairc::live::{
    live_daemons, unique_channel, unique_nick, CaseMappingExpect, LiveClient, LiveClientOptions,
};

use super::helpers::{direct_rows_for, require_live, wait_names_end};

fn operator_mode(features: &IrcServerFeatures) -> Option<char> {
    IrcServerFeatures::prefix_modes(features)
        .chars()
        .find(|mode| *mode == 'o')
}

fn voice_mode(features: &IrcServerFeatures) -> Option<char> {
    IrcServerFeatures::prefix_modes(features).chars().last()
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn member_prefix_ranks() {
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
        assert!(client.wait_until(|client| {
            !IrcServerFeatures::prefix_modes(client.features()).is_empty()
        }));
        let features = client.features().clone();
        let Some(op_mode) = operator_mode(&features) else {
            continue;
        };
        let Some(voice_mode) = voice_mode(&features) else {
            continue;
        };
        if voice_mode == op_mode {
            continue;
        }

        let channel = unique_channel();
        assert!(client.join_channel(&channel));
        assert!(wait_names_end(&mut client, &channel));
        client.select_channel(&channel);

        let mut peer_op = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        let mut peer_voice = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        assert!(peer_op.wait_registered());
        assert!(peer_voice.wait_registered());
        assert!(peer_op.join_channel(&channel));
        assert!(peer_voice.join_channel(&channel));
        assert!(wait_names_end(&mut peer_op, &channel));
        assert!(wait_names_end(&mut peer_voice, &channel));
        let op_nick = peer_op.nick();
        let voice_nick = peer_voice.nick();
        assert!(client.wait_until(|client| {
            client.member(&op_nick).is_some() && client.member(&voice_nick).is_some()
        }));

        assert!(client.controller.submit(
            ComposerSurface::Conversation,
            &format!("/mode {channel} +{op_mode} {op_nick}")
        ));
        client.pump();
        assert!(
            client.wait_until(|client| {
                client.member(&op_nick).is_some_and(|member| {
                    member.label.starts_with('@')
                        || member.ranks != omairc::irc::serverfeatures::IrcPrefixSet::default()
                })
            }),
            "{} op label",
            daemon.name
        );

        assert!(peer_op
            .controller
            .session(&peer_op.network_id)
            .expect("session")
            .send_raw(&format!("MODE {channel} +{voice_mode} {voice_nick}")));
        peer_op.pump();
        assert!(
            client.wait_until(|client| {
                let members = client.controller.members();
                let nicks: Vec<_> = members.iter().map(|m| m.nick.as_str()).collect();
                // Op before voice before plain (self may also be op as first joiner).
                let op_at = nicks.iter().position(|n| n.eq_ignore_ascii_case(&op_nick));
                let voice_at = nicks.iter().position(|n| n.eq_ignore_ascii_case(&voice_nick));
                match (op_at, voice_at) {
                    (Some(o), Some(v)) => o < v,
                    _ => false,
                }
            }),
            "{} prefix order",
            daemon.name
        );
    }
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn peer_nick_change() {
    require_live();
    for daemon in live_daemons() {
        if daemon.nick_length < 5 {
            continue;
        }
        let tls = daemon.use_tls();
        let base = unique_nick(daemon.nick_length - 1);
        let old_nick = format!("{base}[");
        let new_nick = format!("{base}{{");
        let channel = unique_channel();

        let mut client = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        assert!(client.wait_registered(), "{}", daemon.name);
        assert!(client.join_channel(&channel));
        assert!(wait_names_end(&mut client, &channel));
        client.select_channel(&channel);

        let mut peer = LiveClient::connect(&daemon, &old_nick, tls, LiveClientOptions::default());
        assert!(peer.wait_registered(), "{} peer", daemon.name);
        assert!(peer.join_channel(&channel));
        assert!(wait_names_end(&mut peer, &channel));
        assert!(
            client.wait_until(|client| client.member(&old_nick).is_some()),
            "{} peer visible as {}",
            daemon.name,
            old_nick
        );

        let folds = client
            .features()
            .case_mapping()
            .equals(&old_nick, &new_nick);
        if daemon.mapping == CaseMappingExpect::Rfc1459 {
            assert!(
                folds,
                "{} rfc1459 must fold {old_nick} and {new_nick}",
                daemon.name
            );
        } else {
            assert!(
                !folds,
                "{} ascii must keep {old_nick} and {new_nick} distinct",
                daemon.name
            );
        }

        assert!(
            client.controller.open_direct(&old_nick),
            "{} open_direct",
            daemon.name
        );
        let before = format!("before-{channel}");
        let client_nick = client.nick();
        let peer_network = peer.network_id.clone();
        assert!(peer
            .controller
            .send_to_target(&peer_network, &client_nick, &before));
        peer.pump();
        assert!(
            client.wait_until(|client| {
                client.messages().iter().any(|message| message.body == before)
            }),
            "{} DM before-marker",
            daemon.name
        );
        let before_rows = direct_rows_for(&client, &old_nick);
        assert_eq!(before_rows.len(), 1, "{} one DM before nick", daemon.name);
        let before_id = before_rows[0].conversation_id.clone();

        assert!(peer
            .controller
            .session(&peer.network_id)
            .expect("session")
            .send_raw(&format!("NICK {new_nick}")));
        peer.pump();

        assert!(
            client.wait_until(|client| {
                let rows = direct_rows_for(client, &new_nick);
                rows.len() == 1 && rows[0].target == new_nick
            }),
            "{} DM retargeted to {}",
            daemon.name,
            new_nick
        );
        let new_rows = direct_rows_for(&client, &new_nick);
        assert_eq!(new_rows.len(), 1, "{} one DM for new nick", daemon.name);
        assert_eq!(new_rows[0].target, new_nick, "{}", daemon.name);
        let old_rows = direct_rows_for(&client, &old_nick);
        if folds {
            assert_eq!(
                old_rows.len(),
                1,
                "{} folded DM still one row",
                daemon.name
            );
            assert_eq!(old_rows[0].target, new_nick, "{}", daemon.name);
            assert_eq!(
                new_rows[0].conversation_id, before_id,
                "{} same conversation key",
                daemon.name
            );
        } else {
            assert!(
                old_rows.is_empty(),
                "{} ascii must not keep a DM under {}",
                daemon.name,
                old_nick
            );
            assert_ne!(
                new_rows[0].conversation_id, before_id,
                "{} ascii moves the DM key",
                daemon.name
            );
        }

        client.select_channel(&channel);
        assert!(
            client
                .member(&new_nick)
                .is_some_and(|member| member.nick == new_nick),
            "{} member display {}",
            daemon.name,
            new_nick
        );
        assert!(
            client.member(&old_nick).is_none(),
            "{} old spelling gone from members",
            daemon.name
        );

        client.select_channel(&new_nick);
        assert!(
            client.messages().iter().any(|message| message.body == before),
            "{} DM history moved with rename",
            daemon.name
        );
    }
}
