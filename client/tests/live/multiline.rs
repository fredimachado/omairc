//! `draft/multiline` receive against a real server (#478). Ergo enables the
//! capability and relays a client's batch as a batch, to the other members and as
//! an `echo-message` to the sender. Both sessions must show it as one message.

use omairc::irc::capability::IrcCapability;
use omairc::live::{live_daemons, unique_channel, unique_nick, LiveClient, LiveClientOptions};

use super::helpers::{channel_has_body, require_live, wait_names_end};

const JOINED: &str = "line one\nline two-joined";

fn has_multiline(client: &LiveClient) -> bool {
    client
        .controller
        .session(&client.network_id)
        .expect("session")
        .capabilities()
        .contains(IrcCapability::Multiline)
}

fn rows_with(client: &LiveClient, text: &str) -> usize {
    client
        .messages()
        .iter()
        .filter(|message| message.body.contains(text))
        .count()
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn a_multiline_batch_arrives_as_one_message_and_echoes_as_one() {
    require_live();
    let Some(daemon) = live_daemons().into_iter().find(|d| d.name == "ergo") else {
        return;
    };
    let tls = daemon.use_tls();
    let channel = unique_channel();

    let mut peer = LiveClient::connect(
        &daemon,
        &unique_nick(daemon.nick_length),
        tls,
        LiveClientOptions::default(),
    );
    assert!(peer.wait_registered());
    let mut client = LiveClient::connect(
        &daemon,
        &unique_nick(daemon.nick_length),
        tls,
        LiveClientOptions::default(),
    );
    assert!(client.wait_registered());
    assert!(peer.wait_until(|peer| has_multiline(peer)), "the peer negotiates draft/multiline");
    assert!(client.wait_until(|client| has_multiline(client)), "the receiver negotiates draft/multiline");

    assert!(peer.join_channel(&channel));
    assert!(wait_names_end(&mut peer, &channel));
    assert!(client.join_channel(&channel));
    assert!(wait_names_end(&mut client, &channel));
    peer.select_channel(&channel);
    client.select_channel(&channel);

    // Three lines, the last a concat piece, as one batch.
    let network = peer.network_id.clone();
    for line in [
        format!("BATCH +ml1 draft/multiline {channel}"),
        format!("@batch=ml1 PRIVMSG {channel} :line one"),
        format!("@batch=ml1 PRIVMSG {channel} :line two"),
        format!("@batch=ml1;draft/multiline-concat PRIVMSG {channel} :-joined"),
        "BATCH -ml1".to_owned(),
    ] {
        let session = peer.controller.session(&network).expect("session");
        assert!(session.send_raw(&line), "sent {line:?}");
        peer.pump();
    }

    assert!(
        client.wait_until(|client| channel_has_body(client, JOINED)),
        "the receiver shows one message with the break, got {:?}",
        client.messages().iter().map(|m| m.body.clone()).collect::<Vec<_>>()
    );
    assert_eq!(rows_with(&client, "line one"), 1, "one row, not one per line");
    assert_eq!(rows_with(&client, "line two"), 1);

    assert!(
        peer.wait_until(|peer| channel_has_body(peer, JOINED)),
        "the sender's echo is one message too"
    );
    assert_eq!(rows_with(&peer, "line one"), 1, "the echo is one row");
}
