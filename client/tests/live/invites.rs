//! invite-notify delivery: an op-visible muted line in the channel transcript,
//! never in Status, with no join link and no invented DM (#484).

use std::collections::BTreeSet;

use omairc::irc::capability::IrcCapability;
use omairc::irc::command::ComposerSurface;
use omairc::irc::inbox::InboxKind;
use omairc::irc::world::MessageKind;
use omairc::live::{live_daemons, unique_channel, unique_nick, LiveClient, LiveClientOptions};

use super::helpers::{require_live, wait_names_end};

/// Conversation ids of the observer's direct messages on its primary network.
fn observer_direct_ids(observer: &LiveClient) -> BTreeSet<String> {
    observer
        .controller
        .conversations()
        .into_iter()
        .filter(|row| row.direct && row.key.network_id == observer.network_id)
        .map(|row| row.conversation_id)
        .collect()
}

#[test]
#[ignore = "requires bin/test-live (OMAIRC_LIVE_DAEMONS)"]
fn invite_notify_line() {
    require_live();
    for daemon in live_daemons() {
        let tls = daemon.use_tls();
        let mut observer = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        assert!(observer.wait_registered(), "{}", daemon.name);

        // Skip daemons that did not negotiate invite-notify (Solanum and ngIRCd
        // do not advertise it on these fixtures; capability_flags pins that).
        let caps = observer
            .controller
            .session(&observer.network_id)
            .expect("session")
            .capabilities();
        if !caps.contains(IrcCapability::InviteNotify) {
            continue;
        }

        let channel = unique_channel();
        assert!(observer.join_channel(&channel), "{} observer join", daemon.name);
        assert!(
            wait_names_end(&mut observer, &channel),
            "{} observer names",
            daemon.name
        );
        observer.select_channel(&channel);

        let mut inviter = LiveClient::connect(
            &daemon,
            &unique_nick(daemon.nick_length),
            tls,
            LiveClientOptions::default(),
        );
        assert!(inviter.wait_registered(), "{} inviter", daemon.name);
        assert!(inviter.join_channel(&channel), "{} inviter join", daemon.name);
        assert!(
            wait_names_end(&mut inviter, &channel),
            "{} inviter names",
            daemon.name
        );
        inviter.select_channel(&channel);

        let invitee_nick = unique_nick(daemon.nick_length);
        let mut invitee = LiveClient::connect(
            &daemon,
            &invitee_nick,
            tls,
            LiveClientOptions::default(),
        );
        assert!(invitee.wait_registered(), "{} invitee", daemon.name);

        let inviter_nick = inviter.nick();
        assert!(
            observer.wait_until(|client| client.member(&inviter_nick).is_some()),
            "{} member {}",
            daemon.name,
            inviter_nick
        );

        let directs_before = observer_direct_ids(&observer);

        assert!(
            inviter.controller.submit(
                ComposerSurface::Conversation,
                &format!("/invite {invitee_nick} {channel}")
            ),
            "{} /invite",
            daemon.name
        );
        inviter.pump();

        let line = format!("{inviter_nick} invited {invitee_nick}");
        assert!(
            observer.wait_until(|client| {
                client.messages().iter().any(|message| {
                    message.body == line && message.kind == MessageKind::Event
                })
            }),
            "{} event line {line}",
            daemon.name
        );

        // Positive control: the INVITE reached the invitee.
        assert!(
            invitee.wait_until(|client| {
                client
                    .controller
                    .session(&client.network_id)
                    .and_then(|session| session.pending_invite())
                    .is_some_and(|invite| invite.channel.eq_ignore_ascii_case(&channel))
            }),
            "{} invitee pending_invite for {channel}",
            daemon.name
        );

        assert!(observer.fence("invite"), "{} fence", daemon.name);

        let matching = observer
            .messages()
            .iter()
            .filter(|message| message.body == line)
            .count();
        assert_eq!(
            matching, 1,
            "{} exactly one invite line, got {matching}",
            daemon.name
        );

        for entry in observer.status_lines() {
            assert!(
                !entry.text.contains("invited") && !entry.text.contains(&invitee_nick),
                "{} Status must not mention invite ({})",
                daemon.name,
                entry.text
            );
        }

        assert!(
            observer
                .controller
                .inbox_items()
                .iter()
                .all(|item| item.kind != InboxKind::Invite),
            "{} no Invite inbox row",
            daemon.name
        );

        assert!(
            observer
                .controller
                .session(&observer.network_id)
                .expect("session")
                .pending_invite()
                .is_none(),
            "{} observer pending_invite is None (no /join link)",
            daemon.name
        );

        assert_eq!(
            observer_direct_ids(&observer),
            directs_before,
            "{} invite invents no DM",
            daemon.name
        );
    }
}
