use omairc::irc::casemapping::IrcCaseMappingKind;
use omairc::live::{
    live_daemons, unique_nick, CaseMappingExpect, LiveClient, LiveClientOptions,
};
use std::env::temp_dir;

use super::helpers::{channel_has_body, conversation_row, require_live};

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn ascii_direct_restore() {
    require_live();
    for daemon in live_daemons() {
        if daemon.mapping != CaseMappingExpect::Ascii || daemon.nick_length < 8 {
            continue;
        }
        assert_eq!(daemon.mapping.kind(), IrcCaseMappingKind::Ascii);

        let tls = daemon.use_tls();
        let network_id = "ascii-dm".to_owned();
        let self_nick = unique_nick(daemon.nick_length);
        let peer_nick = format!("{}[m]", unique_nick(daemon.nick_length - 3));
        let base = temp_dir().join(format!("omairc-live-ascii-{}", std::process::id()));
        let logs = base.join("logs");
        let settings = base.join("omairc.conf");
        let _ = std::fs::create_dir_all(&logs);

        let mut peer = LiveClient::connect(
            &daemon,
            &peer_nick,
            tls,
            LiveClientOptions::default(),
        );
        assert!(
            peer.wait_registered(),
            "{}",
            peer.last_error().unwrap_or("no error")
        );

        {
            let mut self_client = LiveClient::connect(
                &daemon,
                &self_nick,
                tls,
                LiveClientOptions {
                    network_id: network_id.clone(),
                    transcript_root: Some(logs.clone()),
                    settings_file: Some(settings.clone()),
                    ..LiveClientOptions::default()
                },
            );
            assert!(
                self_client.wait_registered(),
                "{}",
                self_client.last_error().unwrap_or("no error")
            );
            assert!(self_client.wait_motd());
            assert_eq!(
                self_client.features().case_mapping().kind(),
                IrcCaseMappingKind::Ascii
            );
            let self_nick_live = self_client.nick();
            let peer_network = peer.network_id.clone();
            assert!(peer.controller.send_to_target(
                &peer_network,
                &self_nick_live,
                "hi there"
            ));
            peer.pump();
            assert!(self_client.wait_until(|client| {
                conversation_row(&client.controller, &network_id, &peer_nick).is_some()
            }));
            self_client
                .controller
                .select_conversation(&network_id, &peer_nick);
            self_client.pump();
            assert!(self_client.controller.submit(
                omairc::irc::command::ComposerSurface::Conversation,
                "hello back"
            ));
            self_client.pump();
            assert!(self_client.wait_until(|client| channel_has_body(client, "hello back")));
        }

        let mut reloaded = LiveClient::connect(
            &daemon,
            &self_nick,
            tls,
            LiveClientOptions {
                network_id: network_id.clone(),
                transcript_root: Some(logs.clone()),
                settings_file: Some(settings.clone()),
                ..LiveClientOptions::default()
            },
        );
        assert!(
            reloaded.wait_registered(),
            "{}",
            reloaded.last_error().unwrap_or("no error")
        );
        assert!(reloaded.wait_motd());
        assert!(
            conversation_row(&reloaded.controller, &network_id, &peer_nick).is_some(),
            "restored DM"
        );
        let rows = reloaded
            .controller
            .conversations()
            .iter()
            .filter(|row| row.target.eq_ignore_ascii_case(&peer_nick))
            .count();
        assert_eq!(rows, 1);
        let reloaded_nick = reloaded.nick();
        let peer_network = peer.network_id.clone();
        assert!(peer
            .controller
            .send_to_target(&peer_network, &reloaded_nick, "second"));
        peer.pump();
        assert!(reloaded.wait_until(|client| {
            client
                .controller
                .select_conversation(&network_id, &peer_nick);
            channel_has_body(client, "second")
        }));
        reloaded
            .controller
            .select_conversation(&network_id, &peer_nick);
        reloaded.pump();
        assert!(channel_has_body(&reloaded, "hi there"));
        assert!(channel_has_body(&reloaded, "hello back"));
    }
}
