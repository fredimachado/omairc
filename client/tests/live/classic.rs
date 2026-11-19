use omairc::irc::command::ComposerSurface;
use omairc::irc::wiretext::wire_text;
use omairc::irc::world::MessageKind;
use omairc::live::{
    live_daemons, unique_channel, unique_nick, LiveClient, LiveClientOptions,
};

use super::helpers::{
    conversation_row, names_end_count, require_live, saw_channel_mode, saw_part, wait_names_end,
};

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn classic_traffic() {
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
        let mut peer = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        assert!(peer.wait_registered(), "{} peer", daemon.name);

        let channel = unique_channel();
        assert!(client.join_channel(&channel));
        assert!(wait_names_end(&mut client, &channel));
        assert!(peer.join_channel(&channel));
        assert!(wait_names_end(&mut peer, &channel));
        peer.select_channel(&channel);

        let topic = "live topic";
        let privmsg = "hello live";
        let notice = "notice live";
        let action = "waves";
        let network = client.network_id.clone();
        assert!(client
            .controller
            .session(&network)
            .expect("session")
            .set_topic(&channel, topic));
        client.pump();
        assert!(client
            .controller
            .send_to_target(&network, &channel, privmsg));
        client.pump();
        assert!(client
            .controller
            .send_notice(&network, &channel, notice));
        client.pump();
        assert!(client
            .controller
            .send_action(&network, &channel, action));
        client.pump();
        assert!(client.controller.submit(
            ComposerSurface::Conversation,
            &format!("/part {channel}")
        ));
        client.pump();

        assert!(
            peer.wait_until(|peer| {
                peer.select_channel(&channel);
                let bodies: Vec<_> = peer.messages().iter().map(|m| m.body.as_str()).collect();
                bodies.contains(&privmsg)
                    && bodies.iter().any(|b| b.contains(action) || *b == action)
                    && (bodies.contains(&notice)
                        || peer
                            .status_lines()
                            .iter()
                            .any(|e| e.text.contains(notice)))
                    && peer.controller.topic() == topic
            }),
            "{} classic peer did not see traffic",
            daemon.name
        );
    }
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn join_multiple_channels() {
    require_live();
    for daemon in live_daemons() {
        let mut client = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            daemon.use_tls(),
            LiveClientOptions::default(),
        );
        assert!(client.wait_registered(), "{}", daemon.name);
        client.controller.open_status(&client.network_id);
        let first = unique_channel();
        let second = unique_channel();
        let second_name = second.trim_start_matches('#');
        assert!(client.controller.submit(
            ComposerSurface::Status,
            &format!("/join {first}, {second_name}")
        ));
        assert!(
            client.wait_until(|client| {
                let ends = names_end_count(&client.incoming(), &first)
                    + names_end_count(&client.incoming(), &second);
                ends >= 2
            }),
            "{} join ends",
            daemon.name
        );
        let conversations = client.controller.conversations();
        assert!(
            conversations
                .iter()
                .any(|row| row.target.eq_ignore_ascii_case(&first)),
            "{}",
            daemon.name
        );
        assert!(
            conversations
                .iter()
                .any(|row| row.target.eq_ignore_ascii_case(&second)),
            "{}",
            daemon.name
        );

        let keyed = unique_channel();
        assert!(client.join_channel(&keyed));
        assert!(wait_names_end(&mut client, &keyed));
        client.select_channel(&keyed);
        assert!(client.controller.submit(
            ComposerSurface::Conversation,
            &format!("/mode {keyed} +k s3cret")
        ));
        assert!(
            client.wait_until(|client| saw_channel_mode(&client.incoming(), &keyed, 'k')),
            "{} mode +k",
            daemon.name
        );
        assert!(client
            .controller
            .submit(ComposerSurface::Conversation, &format!("/part {keyed}")));
        assert!(
            client.wait_until(|client| saw_part(&client.incoming(), &keyed)),
            "{} part",
            daemon.name
        );
        assert!(client.controller.submit(
            ComposerSurface::Conversation,
            &format!("/join {keyed} s3cret")
        ));
        assert!(
            client.wait_until(|client| names_end_count(&client.incoming(), &keyed) >= 2),
            "{} rejoin",
            daemon.name
        );
    }
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn failed_join_reason() {
    require_live();
    for daemon in live_daemons() {
        let tls = daemon.use_tls();
        let channel = unique_channel();

        let mut peer = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        assert!(peer.wait_registered(), "{} peer", daemon.name);
        assert!(peer.join_channel(&channel));
        assert!(wait_names_end(&mut peer, &channel));
        assert!(peer
            .controller
            .session(&peer.network_id)
            .expect("session")
            .send_raw(&format!("MODE {channel} +i")));
        peer.pump();
        assert!(
            peer.wait_until(|peer| saw_channel_mode(&peer.incoming(), &channel, 'i')),
            "{} mode +i",
            daemon.name
        );

        let mut client = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        assert!(client.wait_registered(), "{}", daemon.name);
        client.controller.open_status(&client.network_id);
        assert!(client
            .controller
            .submit(ComposerSurface::Status, &format!("/join {channel}")));

        let mut reason = String::new();
        assert!(
            client.wait_until(|client| {
                let mapping = client.features().case_mapping();
                client.incoming().iter().any(|message| {
                    if message.command != "473" || message.params.len() < 3 {
                        return false;
                    }
                    if !mapping.equals(&wire_text(&message.params[1]), &channel) {
                        return false;
                    }
                    let body = wire_text(&message.params[2]);
                    if body.is_empty() {
                        return false;
                    }
                    reason = body;
                    true
                })
            }),
            "{} 473",
            daemon.name
        );
        assert!(!reason.is_empty(), "{} non-empty 473 reason", daemon.name);
        assert!(
            conversation_row(&client.controller, &client.network_id, &channel).is_some(),
            "{} composer /join opened the row",
            daemon.name
        );
        client.select_channel(&channel);
        assert!(
            client.wait_until(|client| {
                client.messages().iter().any(|message| {
                    message.kind == MessageKind::Error && message.body == reason
                })
            }),
            "{} error line in channel buffer ({reason})",
            daemon.name
        );
    }
}
