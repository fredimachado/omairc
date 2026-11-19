use omairc::live::{
    fence_serial, live_daemons, unique_nick, LiveClient, LiveClientOptions,
};

use super::helpers::require_live;

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn outbound_log_and_fence() {
    require_live();
    for daemon in live_daemons() {
        let tls = daemon.use_tls();
        let nick = unique_nick(daemon.nick_length);
        let mut client = LiveClient::connect(
            &daemon,
            &nick,
            tls,
            LiveClientOptions {
                reconnect_enabled: true,
                ..LiveClientOptions::default()
            },
        );
        assert!(
            client.wait_registered(),
            "{}: {}",
            daemon.name,
            client.last_error().unwrap_or("no error")
        );

        let nick_line = format!("NICK {nick}");
        let nick_hits = client
            .sent_lines()
            .iter()
            .filter(|line| *line == &nick_line)
            .count();
        assert_eq!(
            nick_hits, 1,
            "{}: expected one primary NICK, count {nick_hits}",
            daemon.name
        );

        // Both clients send CAP LS first, then NICK before CAP END.
        let sent = client.sent_lines();
        assert!(
            sent.first().is_some_and(|line| line.starts_with("CAP LS")),
            "{}: first sent line must start with CAP LS",
            daemon.name
        );
        let cap_end_at = sent.iter().position(|line| line == "CAP END");
        assert!(
            cap_end_at.is_some(),
            "{}: missing CAP END before NICK order check",
            daemon.name
        );
        let nick_at = sent
            .iter()
            .position(|line| line == &nick_line)
            .expect("primary NICK");
        assert!(
            nick_at < cap_end_at.unwrap(),
            "{}: NICK must precede CAP END (nick@{nick_at} end@{})",
            daemon.name,
            cap_end_at.unwrap()
        );

        let fence_token = format!("fence-{nick}");
        let next_serial = fence_serial();
        let decoys = [
            format!("PING :{fence_token}"),
            format!("PING :{fence_token}-{next_serial}"),
        ];
        for decoy in &decoys {
            assert!(
                client
                    .controller
                    .session(&client.network_id)
                    .expect("session")
                    .send_raw(decoy),
                "{}: send decoy PING",
                daemon.name
            );
        }
        client.pump();
        assert!(
            client.fence(&fence_token),
            "{}: fence timed out",
            daemon.name
        );
        let sent = client.sent_lines();
        let fence_pings: Vec<_> = sent
            .iter()
            .filter(|line| {
                line.starts_with(&format!("PING :{fence_token}-"))
                    && decoys.iter().all(|decoy| decoy != *line)
            })
            .cloned()
            .collect();
        assert_eq!(
            fence_pings.len(),
            1,
            "{}: expected one fence PING distinct from decoys, count {}",
            daemon.name,
            fence_pings.len()
        );
        for decoy in &decoys {
            assert!(
                sent.iter().any(|line| line == decoy),
                "{}: missing decoy PING in sent log",
                daemon.name
            );
            assert!(
                &fence_pings[0] != decoy,
                "{}: fence PING matched a decoy",
                daemon.name
            );
        }
        let wire_token = fence_pings[0]
            .strip_prefix("PING :")
            .expect("wire ping prefix");
        assert!(
            client.incoming().iter().any(|message| {
                message.command == "PONG"
                    && !message.params.is_empty()
                    && message.text_param(message.params.len() - 1) == wire_token
            }),
            "{}: missing PONG for fence wire token",
            daemon.name
        );

        let n = client.sent_lines().len();
        let before = client.sent_lines();
        assert!(
            client.force_disconnect(),
            "{}: force_disconnect while registered",
            daemon.name
        );
        assert!(
            client.wait_reconnecting(),
            "{}: wait_reconnecting",
            daemon.name
        );
        assert!(
            client.wait_registered_again(),
            "{}: wait_registered_again: {}",
            daemon.name,
            client.last_error().unwrap_or("no error")
        );

        let after = client.sent_lines();
        assert!(
            after.len() >= n,
            "{}: sent log shrank after reconnect ({n} -> {})",
            daemon.name,
            after.len()
        );
        for index in 0..n {
            assert!(
                after[index] == before[index],
                "{}: pre-reconnect prefix mismatch at {index}",
                daemon.name
            );
        }
        let cap_indexes: Vec<_> = after
            .iter()
            .enumerate()
            .filter(|(_, line)| line.starts_with("CAP LS"))
            .map(|(index, _)| index)
            .collect();
        assert_eq!(
            cap_indexes.len(),
            2,
            "{}: expected exactly two CAP LS lines, count {}",
            daemon.name,
            cap_indexes.len()
        );
        assert!(
            cap_indexes[1] >= n,
            "{}: second CAP LS must be at index >= {n}, got {}",
            daemon.name,
            cap_indexes[1]
        );

        let second_nick = unique_nick(daemon.nick_length);
        let second_id = client.connect_additional(
            &daemon,
            &second_nick,
            tls,
            LiveClientOptions::default(),
        );
        assert!(
            client.wait_registered_network(&second_id),
            "{}: second network register: {}",
            daemon.name,
            client.last_error().unwrap_or("no error")
        );
        let second_nick_line = format!("NICK {second_nick}");
        let primary_second = client
            .sent_lines()
            .iter()
            .filter(|line| *line == &second_nick_line)
            .count();
        assert_eq!(
            primary_second, 0,
            "{}: primary sent log must not contain second NICK, count {primary_second}",
            daemon.name
        );
        let second_hits = client
            .sent_lines_for(&second_id)
            .iter()
            .filter(|line| *line == &second_nick_line)
            .count();
        assert_eq!(
            second_hits, 1,
            "{}: second network must record one NICK, count {second_hits}",
            daemon.name
        );
    }
}
