use omairc::irc::command::ComposerSurface;
use omairc::irc::inbox::InboxKind;
use omairc::live::{live_daemons, unique_channel, unique_nick, LiveClient, LiveClientOptions};

use super::helpers::{conversation_row, require_live, wait_names_end};

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn peer_channel_changes() {
    require_live();
    for daemon in live_daemons() {
        let tls = daemon.use_tls();
        let channel = unique_channel();
        let topic = format!("changed {channel}");
        let reason = format!("out-{channel}");

        let mut op = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        assert!(op.wait_registered(), "{}", daemon.name);
        assert!(op.join_channel(&channel), "{} op join", daemon.name);
        assert!(wait_names_end(&mut op, &channel), "{} op names", daemon.name);
        op.select_channel(&channel);

        let mut client = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        assert!(client.wait_registered(), "{} client", daemon.name);
        assert!(client.join_channel(&channel), "{} client join", daemon.name);
        assert!(
            wait_names_end(&mut client, &channel),
            "{} client names",
            daemon.name
        );
        client.select_channel(&channel);

        let mut leaver = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        assert!(leaver.wait_registered(), "{} leaver", daemon.name);
        assert!(leaver.join_channel(&channel), "{} leaver join", daemon.name);
        assert!(
            wait_names_end(&mut leaver, &channel),
            "{} leaver names",
            daemon.name
        );

        let op_nick = op.nick();
        let client_nick = client.nick();
        let leaver_nick = leaver.nick();
        assert!(
            client.wait_until(|client| {
                client.member(&op_nick).is_some() && client.member(&leaver_nick).is_some()
            }),
            "{} members visible",
            daemon.name
        );
        assert_eq!(
            client.controller.people_count(),
            3,
            "{} people_count",
            daemon.name
        );
        assert_eq!(
            client.controller.members().len(),
            3,
            "{} members.len",
            daemon.name
        );

        assert!(
            op.controller
                .submit(ComposerSurface::Conversation, &format!("/topic {topic}")),
            "{} /topic",
            daemon.name
        );
        op.pump();
        assert!(
            client.wait_until(|client| client.controller.topic() == topic),
            "{} topic",
            daemon.name
        );

        let leaver_network = leaver.network_id.clone();
        assert!(
            leaver.controller.disconnect(&leaver_network),
            "{} leaver disconnect",
            daemon.name
        );
        leaver.pump();
        assert!(
            client.wait_until(|client| client.member(&leaver_nick).is_none()),
            "{} leaver gone",
            daemon.name
        );
        assert_eq!(
            client.controller.people_count(),
            2,
            "{} people after quit",
            daemon.name
        );
        assert_eq!(
            client.controller.members().len(),
            2,
            "{} members after quit",
            daemon.name
        );
        assert!(
            client.controller.channel_joined(),
            "{} client joined before kick",
            daemon.name
        );

        assert!(
            op.controller.submit(
                ComposerSurface::Conversation,
                &format!("/kick {client_nick} {reason}")
            ),
            "{} /kick",
            daemon.name
        );
        op.pump();
        assert!(
            client.wait_until(|client| !client.controller.channel_joined()),
            "{} kicked",
            daemon.name
        );
        assert!(
            conversation_row(&client.controller, &client.network_id, &channel).is_some(),
            "{} kicked channel keeps its row",
            daemon.name
        );
        assert_eq!(
            client.controller.people_count(),
            0,
            "{} people after kick",
            daemon.name
        );
        assert!(
            client.controller.members().is_empty(),
            "{} members after kick",
            daemon.name
        );
        assert!(
            client
                .controller
                .inbox_items()
                .iter()
                .any(|item| {
                    item.kind == InboxKind::Kick
                        && item.target.eq_ignore_ascii_case(&channel)
                        && item.actor.eq_ignore_ascii_case(&op_nick)
                        && item.preview == reason
                }),
            "{} kick inbox",
            daemon.name
        );
        let kicked_line = format!("{client_nick} was kicked");
        assert!(
            client
                .messages()
                .iter()
                .any(|message| message.body == kicked_line),
            "{} kick event line",
            daemon.name
        );
    }
}
