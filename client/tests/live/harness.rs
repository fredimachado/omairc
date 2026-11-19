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
        let nick_hits: Vec<_> = client
            .sent_lines()
            .into_iter()
            .filter(|line| line == &nick_line)
            .collect();
        assert_eq!(
            nick_hits.len(),
            1,
            "{}: expected one {nick_line}, got {:?}",
            daemon.name,
            client.sent_lines()
        );

        let fence_token = format!("fence-{nick}");
        assert!(
            client.fence(&fence_token),
            "{}: fence {fence_token} timed out",
            daemon.name
        );
        let ping_line = format!("PING :{fence_token}");
        assert!(
            client.sent_lines().iter().any(|line| line == &ping_line),
            "{}: missing {ping_line} in {:?}",
            daemon.name,
            client.sent_lines()
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
        assert_eq!(
            &after[..n],
            &before[..],
            "{}: first {n} sent lines changed after reconnect",
            daemon.name
        );
        let cap_ls = after.iter().filter(|line| line.starts_with("CAP LS")).count();
        assert_eq!(
            cap_ls, 2,
            "{}: expected two CAP LS lines after reconnect, got {cap_ls} in {:?}",
            daemon.name, after
        );
    }
}
