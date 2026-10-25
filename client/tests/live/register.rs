use omairc::live::{live_daemons, unique_nick, LiveClient, LiveClientOptions};

use super::helpers::require_live;

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn register_plain() {
    require_live();
    for daemon in live_daemons() {
        if daemon.plain_port == 0 {
            continue;
        }
        let mut client = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            false,
            LiveClientOptions::default(),
        );
        assert!(
            client.wait_registered(),
            "{}: {}",
            daemon.name,
            client.last_error().unwrap_or("no error")
        );
    }
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn register_tls() {
    require_live();
    for daemon in live_daemons() {
        if daemon.tls_port == 0 {
            continue;
        }
        let mut client = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            true,
            LiveClientOptions::default(),
        );
        assert!(
            client.wait_registered(),
            "{}: {}",
            daemon.name,
            client.last_error().unwrap_or("no error")
        );
    }
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn nick_in_use() {
    require_live();
    for daemon in live_daemons() {
        let nick = unique_nick(daemon.nick_length.saturating_sub(1).max(1));
        let tls = daemon.use_tls();
        let mut first =
            LiveClient::connect(&daemon, &nick, tls, LiveClientOptions::default());
        assert!(first.wait_registered(), "{}", daemon.name);
        let mut second =
            LiveClient::connect(&daemon, &nick, tls, LiveClientOptions::default());
        assert!(
            second.wait_registered(),
            "{}: {}",
            daemon.name,
            second.last_error().unwrap_or("no error")
        );
        let assigned = second.nick();
        let underscore = format!("{nick}_");
        let digit = format!("{nick}2");
        assert!(
            assigned.eq_ignore_ascii_case(&underscore) || assigned.eq_ignore_ascii_case(&digit),
            "{} assigned {assigned}",
            daemon.name
        );
        assert!(second.has_server_label("433"), "{}", daemon.name);
    }
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn nick_length() {
    require_live();
    for daemon in live_daemons() {
        let too_long = "abcdefghij";
        let mut client =
            LiveClient::connect(&daemon, too_long, daemon.use_tls(), LiveClientOptions::default());
        if daemon.nick_length < too_long.len() {
            assert!(
                client.wait_failed(),
                "{}: {}",
                daemon.name,
                client.last_error().unwrap_or("no error")
            );
        } else {
            assert!(client.wait_registered(), "{}", daemon.name);
        }
    }
}
