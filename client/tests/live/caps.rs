use omairc::irc::capability::IrcCapability;
use omairc::irc::serverfeatures::IrcServerFeatures;
use omairc::live::{live_daemons, unique_channel, unique_nick, LiveClient, LiveClientOptions};

use super::helpers::{require_live, wait_names_end};

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn capability_flags() {
    require_live();
    for daemon in live_daemons() {
        let mut client = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            daemon.use_tls(),
            LiveClientOptions::default(),
        );
        assert!(client.wait_registered(), "{}", daemon.name);
        let channel = unique_channel();
        assert!(client.join_channel(&channel));
        assert!(wait_names_end(&mut client, &channel));
        client.select_channel(&channel);
        assert_eq!(
            client.controller.has_away_presence(),
            daemon.away_notify,
            "{}",
            daemon.name
        );
        assert_eq!(
            client.controller.has_typing(),
            daemon.message_tags,
            "{}",
            daemon.name
        );
        assert_eq!(
            client.controller.has_member_status(),
            daemon.metadata,
            "{}",
            daemon.name
        );
        if !daemon.wanted_caps_advertised {
            let caps = client
                .controller
                .session(&client.network_id)
                .expect("session")
                .capabilities();
            assert!(!caps.contains(IrcCapability::AwayNotify), "{}", daemon.name);
            assert!(!caps.contains(IrcCapability::MessageTags), "{}", daemon.name);
            assert!(
                !caps.contains(IrcCapability::MemberMetadata),
                "{}",
                daemon.name
            );
        }
    }
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn isupport() {
    require_live();
    for daemon in live_daemons() {
        let mut client = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            daemon.use_tls(),
            LiveClientOptions::default(),
        );
        assert!(client.wait_registered(), "{}", daemon.name);
        assert!(
            client.wait_until(|client| IrcServerFeatures::nick_length(client.features()).is_some()),
            "{} nicklen",
            daemon.name
        );
        let features = client.features();
        assert_eq!(
            features.case_mapping().kind(),
            daemon.mapping.kind(),
            "{}",
            daemon.name
        );
        assert!(
            !IrcServerFeatures::prefix_modes(features).is_empty(),
            "{}",
            daemon.name
        );
        assert!(!features.prefix_symbols().is_empty(), "{}", daemon.name);
        assert!(features.is_channel("#live"), "{}", daemon.name);
        assert!(
            IrcServerFeatures::nick_length(features).unwrap() > 0,
            "{}",
            daemon.name
        );
    }
}
