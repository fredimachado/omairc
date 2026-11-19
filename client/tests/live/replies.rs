use omairc::live::{live_daemons, unique_channel, unique_nick, LiveClient, LiveClientOptions};
use omairc::model::transcript::rows_with_reactions;

use super::helpers::{require_live, wait_names_end};

/// A peer's PRIVMSG gives Omairc a msgid. The peer's `+draft/reply` TAGMSG reaction then lands as one chip on
/// that line, and the peer's `+draft/reply` PRIVMSG shows the parent's author on its reply line. Ergo relays
/// client-only tags to every member that negotiated `message-tags`.
#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn reply_and_react() {
    require_live();
    for daemon in live_daemons() {
        if daemon.name != "ergo" || !daemon.message_tags {
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

        let send = |peer: &mut LiveClient, line: &str| {
            assert!(peer
                .controller
                .session(&peer.network_id)
                .expect("session")
                .send_raw(line));
            peer.pump();
        };

        send(&mut peer, &format!("PRIVMSG {channel} :parent line"));
        assert!(
            client.wait_until(|client| client
                .messages()
                .iter()
                .any(|message| message.body == "parent line" && !message.msgid.is_empty())),
            "{} the parent arrives with a msgid",
            daemon.name
        );
        let msgid = client
            .messages()
            .iter()
            .find(|message| message.body == "parent line")
            .map(|message| message.msgid.value.clone())
            .expect("the parent line");

        send(
            &mut peer,
            &format!("@+draft/reply={msgid};+draft/react=\u{1F44D} TAGMSG {channel}"),
        );
        assert!(
            client.wait_until(|client| client.controller.reactions().contains_key(&msgid)),
            "{} the reaction reaches the line",
            daemon.name
        );
        let chips = client.controller.reactions()[&msgid].chips();
        assert_eq!(chips.len(), 1, "{}", daemon.name);
        assert_eq!(chips[0].emoji, "\u{1F44D}");
        assert_eq!(chips[0].count, 1);
        assert!(
            chips[0].nicks[0].eq_ignore_ascii_case(&peer_nick),
            "{} the chip names the peer",
            daemon.name
        );

        send(
            &mut peer,
            &format!("@+draft/reply={msgid} PRIVMSG {channel} :a reply"),
        );
        assert!(
            client.wait_until(|client| client
                .messages()
                .iter()
                .any(|message| message.body == "a reply" && message.reply_to == msgid)),
            "{} the reply keeps its parent",
            daemon.name
        );
        let rows = rows_with_reactions(
            client.messages(),
            None,
            chrono::Local::now().date_naive(),
            client.controller.reactions(),
        );
        let reply = rows
            .iter()
            .find(|row| row.body.plain == "a reply")
            .expect("the reply row");
        assert!(
            reply.reply_preview.starts_with("\u{21AA}")
                && reply.reply_preview.contains(": parent line")
                && reply
                    .reply_preview
                    .to_ascii_lowercase()
                    .contains(&peer_nick.to_ascii_lowercase()),
            "{} reply line = {:?}",
            daemon.name,
            reply.reply_preview
        );
        let parent = rows
            .iter()
            .find(|row| row.body.plain == "parent line")
            .expect("the parent row");
        assert_eq!(parent.reactions.len(), 1, "{}", daemon.name);
    }
}
