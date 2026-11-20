//! CHATHISTORY BEFORE paging against Ergo (#485 / #501 slice 2).

use omairc::irc::capability::IrcCapability;
use omairc::live::{live_daemons, unique_channel, unique_nick, LiveClient, LiveClientOptions};

use super::helpers::{require_live, wait_names_end};

fn seed_body(channel: &str, index: usize) -> String {
    format!("{channel}-page-{index:03}")
}

fn seed_indices(client: &LiveClient, channel: &str) -> Vec<usize> {
    let prefix = format!("{channel}-page-");
    client
        .messages()
        .iter()
        .filter_map(|message| {
            let body = message.body.as_str();
            let rest = body.strip_prefix(&prefix)?;
            rest.parse().ok()
        })
        .collect()
}

fn sent_history(lines: &[String], verb: &str, channel: &str) -> usize {
    let prefix = format!("CHATHISTORY {verb} {channel} ");
    lines.iter().filter(|line| line.starts_with(&prefix)).count()
}

fn expected_range(from: usize, to: usize) -> Vec<usize> {
    (from..=to).collect()
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn chat_history_before_pages() {
    require_live();
    // CHATHISTORY is Ergo-only on this fixture (Solanum/ngIRCd do not offer it).
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
    assert!(
        peer.wait_registered(),
        "{}: peer register: {}",
        daemon.name,
        peer.last_error().unwrap_or("no error")
    );
    assert!(
        peer.join_channel(&channel),
        "{}: peer join {channel}",
        daemon.name
    );
    assert!(
        wait_names_end(&mut peer, &channel),
        "{}: peer names end",
        daemon.name
    );

    let mut client = LiveClient::connect(
        &daemon,
        &unique_nick(daemon.nick_length),
        tls,
        LiveClientOptions::default(),
    );
    assert!(
        client.wait_registered(),
        "{}: client register: {}",
        daemon.name,
        client.last_error().unwrap_or("no error")
    );
    assert!(
        client.wait_motd(),
        "{}: wait MOTD for CHATHISTORY= ISUPPORT",
        daemon.name
    );
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
            }),
        "{}: negotiated ChatHistory",
        daemon.name
    );

    let limit = client
        .controller
        .session(&client.network_id)
        .expect("session")
        .history_limit();
    let total = limit + 30;
    let peer_network = peer.network_id.clone();
    for index in 1..=total {
        let body = seed_body(&channel, index);
        assert!(
            peer.controller
                .send_to_target(&peer_network, &channel, &body),
            "{}: seed {index}/{total}",
            daemon.name
        );
    }
    peer.pump();
    assert!(
        peer.fence("seeded"),
        "{}: fence after seeding {total} lines",
        daemon.name
    );

    assert!(
        client.join_channel(&channel),
        "{}: client join {channel}",
        daemon.name
    );
    client.select_channel(&channel);

    // The LATEST page is the newest `limit` items, in order and ending at
    // `total`. A server may keep our own JOIN in the channel history and count
    // it in that page (Ergo does, replayed as a HistServ PRIVMSG without
    // draft/event-playback), leaving `limit - 1` seed lines.
    let latest_pages = [
        expected_range(total - limit + 1, total),
        expected_range(total - (limit - 1) + 1, total),
    ];
    assert!(
        client.wait_until(|client| latest_pages.contains(&seed_indices(client, &channel))),
        "{}: LATEST page want one of {latest_pages:?}, got {:?}; chathistory/znc sent {:?}",
        daemon.name,
        seed_indices(&client, &channel),
        client
            .sent_lines()
            .iter()
            .filter(|line| {
                line.starts_with("CHATHISTORY ") || line.starts_with("ZNC *playback ")
            })
            .collect::<Vec<_>>()
    );
    assert_eq!(
        sent_history(&client.sent_lines(), "LATEST", &channel),
        1,
        "{}: one LATEST for {channel}; history lines {:?}",
        daemon.name,
        client
            .sent_lines()
            .iter()
            .filter(|line| line.starts_with("CHATHISTORY "))
            .collect::<Vec<_>>()
    );

    // One phase per target: both calls in one turn (no pump between).
    assert!(
        client.controller.request_older_transcript_history(),
        "{}: first BEFORE must send",
        daemon.name
    );
    assert!(
        !client.controller.request_older_transcript_history(),
        "{}: second same-turn BEFORE must refuse (one in flight)",
        daemon.name
    );

    let all_want = expected_range(1, total);
    assert!(
        client.wait_until(|client| {
            !client.controller.older_transcript_history_inflight()
                && seed_indices(client, &channel) == all_want
        }),
        "{}: after first BEFORE want {all_want:?}, got {:?}, inflight={}",
        daemon.name,
        seed_indices(&client, &channel),
        client.controller.older_transcript_history_inflight()
    );
    assert_eq!(
        sent_history(&client.sent_lines(), "BEFORE", &channel),
        1,
        "{}: one BEFORE in flight per target; history lines {:?}",
        daemon.name,
        client
            .sent_lines()
            .iter()
            .filter(|line| line.starts_with("CHATHISTORY "))
            .collect::<Vec<_>>()
    );

    // Scrolling past the start stops once history is exhausted.
    let mut before_count = 1usize;
    let mut ended_on_false = false;
    for _ in 0..3 {
        if client.controller.request_older_transcript_history() {
            assert!(
                client.wait_until(|client| !client.controller.older_transcript_history_inflight()),
                "{}: wait BEFORE settle",
                daemon.name
            );
            assert!(client.fence("page"), "{}: fence after BEFORE page", daemon.name);
            before_count += 1;
            assert_eq!(
                sent_history(&client.sent_lines(), "BEFORE", &channel),
                before_count,
                "{}: BEFORE count after page; history lines {:?}",
                daemon.name,
                client
                    .sent_lines()
                    .iter()
                    .filter(|line| line.starts_with("CHATHISTORY "))
                    .collect::<Vec<_>>()
            );
        } else {
            assert_eq!(
                sent_history(&client.sent_lines(), "BEFORE", &channel),
                before_count,
                "{}: false call must write no BEFORE; history lines {:?}",
                daemon.name,
                client
                    .sent_lines()
                    .iter()
                    .filter(|line| line.starts_with("CHATHISTORY "))
                    .collect::<Vec<_>>()
            );
            ended_on_false = true;
            break;
        }
    }
    assert!(
        ended_on_false,
        "{}: exhaustion loop must end on a false request_older call",
        daemon.name
    );
    assert_eq!(
        seed_indices(&client, &channel),
        all_want,
        "{}: transcript must stay complete with no duplicates",
        daemon.name
    );
}
