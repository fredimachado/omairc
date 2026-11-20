//! `/list` against real 321/322/323 (#498). Exact channel-name masks only.
use omairc::irc::command::ComposerSurface;
use omairc::live::{live_daemons, unique_channel, unique_nick, LiveClient, LiveClientOptions};

use super::helpers::{require_live, wait_names_end};

fn is_list_line(line: &str) -> bool {
    line == "LIST" || line.starts_with("LIST ")
}

fn list_lines(sent: &[String]) -> Vec<&str> {
    sent.iter()
        .filter(|line| is_list_line(line))
        .map(String::as_str)
        .collect()
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn channel_list_completes_in_order() {
    require_live();
    for daemon in live_daemons() {
        let mut client = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            daemon.use_tls(),
            LiveClientOptions::default(),
        );
        assert!(
            client.wait_registered(),
            "{}: {}",
            daemon.name,
            client.last_error().unwrap_or("no error")
        );

        let first = unique_channel();
        let second = unique_channel();
        assert!(client.join_channel(&first), "{} join {first}", daemon.name);
        assert!(
            wait_names_end(&mut client, &first),
            "{} names {first}",
            daemon.name
        );
        assert!(client.join_channel(&second), "{} join {second}", daemon.name);
        assert!(
            wait_names_end(&mut client, &second),
            "{} names {second}",
            daemon.name
        );
        client.select_channel(&first);

        let topic = format!("listed {first}");
        assert!(
            client
                .controller
                .submit(ComposerSurface::Conversation, &format!("/topic {topic}")),
            "{} /topic",
            daemon.name
        );
        assert!(
            client.wait_until(|client| client.controller.topic() == topic),
            "{} topic",
            daemon.name
        );

        // Leg 1: one exact-mask /list completes with that channel and its topic.
        assert!(
            client
                .controller
                .submit(ComposerSurface::Conversation, &format!("/list {first}")),
            "{} /list {first}",
            daemon.name
        );
        assert!(
            client.wait_until(|client| {
                let view = client.controller.channel_list();
                view.complete && !view.loading && view.mask == first
            }),
            "{} leg1 complete mask={first}",
            daemon.name
        );
        {
            let view = client.controller.channel_list();
            assert!(
                view.error.is_empty(),
                "{} leg1 error={:?}",
                daemon.name,
                view.error
            );
            assert_eq!(
                view.rows.len(),
                1,
                "{} leg1 rows={:?}",
                daemon.name,
                view.rows
            );
            assert!(
                view.rows[0].name.eq_ignore_ascii_case(&first),
                "{} leg1 name={} want {first}",
                daemon.name,
                view.rows[0].name
            );
            assert_eq!(view.rows[0].users, 1, "{} leg1 users", daemon.name);
            assert!(
                view.rows[0].topic.contains(&topic),
                "{} leg1 topic={:?} want contains {topic}",
                daemon.name,
                view.rows[0].topic
            );
        }
        assert!(
            client.fence("list-leg1"),
            "{} leg1 fence",
            daemon.name
        );

        // Leg 2: a second /list queued behind the first keeps its own end.
        // controller.submit pumps session events, but live inbound only enters
        // through hub.drain_ready in LiveClient::pump, so these two submits
        // cannot start the queued mask before the wait below.
        let sent_at = client.sent_lines().len();
        let want_second = format!("LIST {second}");
        let want_first = format!("LIST {first}");
        assert!(
            client
                .controller
                .submit(ComposerSurface::Conversation, &format!("/list {second}")),
            "{} /list {second}",
            daemon.name
        );
        assert!(
            client
                .controller
                .submit(ComposerSurface::Conversation, &format!("/list {first}")),
            "{} /list {first} queued",
            daemon.name
        );
        {
            let sent = client.sent_lines();
            let mid = list_lines(&sent[sent_at..]);
            assert_eq!(
                mid,
                [want_second.as_str()],
                "{} leg2 mid LIST before pump {:?}",
                daemon.name,
                mid
            );
        }
        assert!(
            client.wait_until(|client| {
                let view = client.controller.channel_list();
                view.complete && !view.loading && view.mask == first
            }),
            "{} leg2 complete mask={first}",
            daemon.name
        );
        {
            let view = client.controller.channel_list();
            assert!(
                view.error.is_empty(),
                "{} leg2 error={:?}",
                daemon.name,
                view.error
            );
            assert_eq!(
                view.rows.len(),
                1,
                "{} leg2 rows={:?}",
                daemon.name,
                view.rows
            );
            assert!(
                view.rows[0].name.eq_ignore_ascii_case(&first),
                "{} leg2 name={} want {first}",
                daemon.name,
                view.rows[0].name
            );
            assert_eq!(view.rows[0].users, 1, "{} leg2 users", daemon.name);
        }
        assert!(
            client.fence("list-leg2"),
            "{} leg2 fence",
            daemon.name
        );

        let sent = client.sent_lines();
        let lists = list_lines(&sent[sent_at..]);
        assert_eq!(
            lists,
            [want_second.as_str(), want_first.as_str()],
            "{} leg2 LIST order {:?}",
            daemon.name,
            lists
        );
    }
}
