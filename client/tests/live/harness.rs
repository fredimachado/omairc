use omairc::live::{live_daemons, unique_nick, LiveClient, LiveClientOptions};

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
            "{}: expected one {nick_line}, count {nick_hits}",
            daemon.name
        );

        // Both clients send CAP LS first, then NICK before CAP END.
        let sent = client.sent_lines();
        assert!(
            sent.first().is_some_and(|line| line.starts_with("CAP LS")),
            "{}: first sent line must start with CAP LS",
            daemon.name
        );
        let nick_at = sent
            .iter()
            .position(|line| line == &nick_line)
            .unwrap_or(usize::MAX);
        let cap_end_at = sent
            .iter()
            .position(|line| line == "CAP END")
            .unwrap_or(usize::MAX);
        assert!(
            nick_at < cap_end_at,
            "{}: NICK must precede CAP END (nick@{nick_at} end@{cap_end_at})",
            daemon.name
        );

        let fence_token = format!("fence-{nick}");
        let bare_ping = format!("PING :{fence_token}");
        assert!(
            client
                .controller
                .session(&client.network_id)
                .expect("session")
                .send_raw(&bare_ping),
            "{}: send bare {bare_ping}",
            daemon.name
        );
        client.pump();
        assert!(
            client.fence(&fence_token),
            "{}: fence {fence_token} timed out",
            daemon.name
        );
        let sent = client.sent_lines();
        let bare_hits = sent.iter().filter(|line| *line == &bare_ping).count();
        assert_eq!(
            bare_hits, 1,
            "{}: expected one {bare_ping}, count {bare_hits}",
            daemon.name
        );
        let wire_prefix = format!("PING :{fence_token}-");
        let wire_pings: Vec<_> = sent
            .iter()
            .filter(|line| line.starts_with(&wire_prefix))
            .cloned()
            .collect();
        assert_eq!(
            wire_pings.len(),
            1,
            "{}: expected one {wire_prefix}*, count {}",
            daemon.name,
            wire_pings.len()
        );
        let wire_token = wire_pings[0]
            .strip_prefix("PING :")
            .expect("wire ping prefix");
        assert!(
            client.incoming().iter().any(|message| {
                message.command == "PONG"
                    && !message.params.is_empty()
                    && message.text_param(message.params.len() - 1) == wire_token
            }),
            "{}: missing PONG for wire token {wire_token}",
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
            assert_eq!(
                after[index], before[index],
                "{}: pre-reconnect prefix mismatch at {index}",
                daemon.name
            );
        }
        let second_cap = after
            .iter()
            .enumerate()
            .filter(|(_, line)| line.starts_with("CAP LS"))
            .nth(1)
            .map(|(index, _)| index);
        assert!(
            second_cap.is_some_and(|index| index >= n),
            "{}: second CAP LS must be at index >= {n}, got {second_cap:?}",
            daemon.name
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
        assert!(
            !client
                .sent_lines()
                .iter()
                .any(|line| line == &second_nick_line),
            "{}: primary sent log must not contain {second_nick_line}",
            daemon.name
        );
    }
}
