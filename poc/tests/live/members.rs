use omairc::irc::command::ComposerSurface;
use omairc::irc::serverfeatures::IrcServerFeatures;
use omairc::live::{
    live_daemons, unique_channel, unique_nick, LiveClient, LiveClientOptions,
};

use super::helpers::{require_live, wait_names_end};

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
