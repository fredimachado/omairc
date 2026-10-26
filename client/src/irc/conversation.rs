//! and the conversation-cause rules of `tui/internal/irc/reducer.go`.
//! The stable conversation id, the cause table behind `ensure_conversation`, the
//! service-target test, and the neighbor pick after a row is dropped. The sidebar
//! order (`SidebarOrder`) reads reducer state, so it lands with the reducer.
//! The cause table is the contract. `.cursor/skills/conversation-cause` states it
//! for the Qt side, and `IrcConversationCause::inserts` is the only place this
//! port decides whether a cause may create a row.

use super::event::{ConversationKey, IrcConversationCause};
use super::serverfeatures::IrcServerFeatures;
use super::servicenick::is_service_identity;

/// The stable id of a conversation: the network id, a newline, then the
/// normalized target (`ircConversationId`).
pub fn conversation_id(key: &ConversationKey) -> String {
    format!("{}\n{}", key.network_id, key.normalized_target)
}

/// Parses a `conversation_id` back into its key. `None` unless the id holds
/// exactly one newline with both sides non-empty (`ircParseConversationId`).
#[cfg(test)]
pub fn parse_conversation_id(id: &str) -> Option<ConversationKey> {
    let (network_id, normalized_target) = id.split_once('\n')?;
    if network_id.is_empty() || normalized_target.is_empty() || normalized_target.contains('\n') {
        return None;
    }
    Some(ConversationKey::new(network_id, normalized_target))
}

/// Whether a target names a network service rather than a person. It passes an
/// empty host and the advertised CHANTYPES, as `ircTargetLooksLikeService` does.
pub fn target_looks_like_service(target: &str, features: &IrcServerFeatures) -> bool {
    is_service_identity(target, "", features.channel_types())
}

impl IrcConversationCause {
    /// Whether this cause may create a conversation (`ircConversationCauseInserts`).
    /// | cause | inserts |
    /// |---|---|
    /// | `QuietSend`, `InboundSelf` | never |
    /// | `UserOpen` | DMs only |
    /// | `ChannelState` | channels only |
    /// | `InboundOther` | channels and non-service DMs |
    /// | `Restore` | non-service DMs, and only after ISUPPORT (the caller's gate) |
    pub fn inserts(self, target_is_channel: bool, target_looks_like_service: bool) -> bool {
        match self {
            Self::UserOpen => !target_is_channel,
            Self::ChannelState => target_is_channel,
            Self::InboundOther => target_is_channel || !target_looks_like_service,
            Self::Restore => !target_is_channel && !target_looks_like_service,
            Self::QuietSend | Self::InboundSelf => false,
        }
    }
}

/// The conversation to select after `dropping` leaves `ordered` (`ircNeighborAfterDrop`).
/// It prefers the next row of the same network, then the previous one, then the
/// next row of any network, then the previous one. A key that is not listed falls
/// back to the last row. `None` when there is no candidate.
pub fn neighbor_after_drop(
    ordered: &[ConversationKey],
    dropping: &ConversationKey,
) -> Option<ConversationKey> {
    let count = ordered.len();
    match ordered.iter().position(|key| key == dropping) {
        Some(index) => {
            if index + 1 < count && ordered[index + 1].network_id == dropping.network_id {
                return Some(ordered[index + 1].clone());
            }
            if index > 0 && ordered[index - 1].network_id == dropping.network_id {
                return Some(ordered[index - 1].clone());
            }
            if index + 1 < count {
                return Some(ordered[index + 1].clone());
            }
            if index > 0 {
                return Some(ordered[index - 1].clone());
            }
            None
        }
        None => ordered.last().cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn features() -> IrcServerFeatures {
        IrcServerFeatures::new()
    }

    /// The three targets the cause table distinguishes.
    fn targets() -> [(&'static str, bool, bool); 3] {
        // (target, is a channel, looks like a service)
        [
            ("#omarchy", true, false),
            ("alice", false, false),
            ("NickServ", false, true),
        ]
    }

    #[test]
    fn service_targets_follow_the_services_rule() {
        let features = features();
        for (target, channel, service) in targets() {
            assert_eq!(features.is_channel(target), channel, "is_channel({target})");
            assert_eq!(
                target_looks_like_service(target, &features),
                service,
                "service({target})"
            );
        }
    }

    #[test]
    fn cause_table_matches_the_go_insert_predicate() {
        // Each row is one cause, and the expected insert for (channel, DM, service DM).
        let table = [
            (IrcConversationCause::QuietSend, [false, false, false]),
            (IrcConversationCause::InboundSelf, [false, false, false]),
            (IrcConversationCause::UserOpen, [false, true, true]),
            (IrcConversationCause::ChannelState, [true, false, false]),
            (IrcConversationCause::InboundOther, [true, true, false]),
            (IrcConversationCause::Restore, [false, true, false]),
        ];
        for (cause, expected) in table {
            for (index, (target, channel, service)) in targets().into_iter().enumerate() {
                assert_eq!(
                    cause.inserts(channel, service),
                    expected[index],
                    "{cause:?} on {target}"
                );
            }
        }
    }

    #[test]
    fn conversation_id_round_trips_and_rejects_malformed_ids() {
        let key = ConversationKey::new("oftc", "#omarchy");
        let id = conversation_id(&key);
        assert_eq!(id, "oftc\n#omarchy");
        assert_eq!(parse_conversation_id(&id), Some(key));

        assert_eq!(parse_conversation_id("\n#room"), None);
        assert_eq!(parse_conversation_id("oftc\n"), None);
        assert_eq!(parse_conversation_id("oftc"), None);
        assert_eq!(parse_conversation_id("oftc\n#a\n#b"), None);
    }

    #[test]
    fn neighbor_prefers_the_same_network_then_any_row() {
        let ordered = vec![
            ConversationKey::new("a", "#one"),
            ConversationKey::new("a", "#two"),
            ConversationKey::new("a", "alice"),
            ConversationKey::new("b", "#x"),
        ];

        // Next row, same network.
        assert_eq!(
            neighbor_after_drop(&ordered, &ConversationKey::new("a", "#two")),
            Some(ConversationKey::new("a", "alice"))
        );
        // No next row in the network, so the previous row of the network.
        assert_eq!(
            neighbor_after_drop(&ordered, &ConversationKey::new("a", "alice")),
            Some(ConversationKey::new("a", "#two"))
        );
        // The last row has no same-network neighbor, so the previous row.
        assert_eq!(
            neighbor_after_drop(&ordered, &ConversationKey::new("b", "#x")),
            Some(ConversationKey::new("a", "alice"))
        );
        // A lone row has no candidate.
        assert_eq!(
            neighbor_after_drop(&ordered[..1], &ConversationKey::new("a", "#one")),
            None
        );
        // An unlisted key falls back to the last row, and an empty list to None.
        assert_eq!(
            neighbor_after_drop(&ordered, &ConversationKey::new("c", "#z")),
            Some(ConversationKey::new("b", "#x"))
        );
        assert_eq!(
            neighbor_after_drop(&[], &ConversationKey::new("a", "#one")),
            None
        );
    }

    #[test]
    fn next_row_of_another_network_is_used_when_nothing_else_is_left() {
        let ordered = vec![
            ConversationKey::new("a", "#one"),
            ConversationKey::new("b", "#x"),
        ];
        assert_eq!(
            neighbor_after_drop(&ordered, &ConversationKey::new("a", "#one")),
            Some(ConversationKey::new("b", "#x"))
        );
    }
}
