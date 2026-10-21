//! The avatar a nick shows, given its reference and the `avatars` preference. Qt
//! keeps a nick's facts per network (the `761` numerics and the bot metadata), and
//! `Controller::nick_facts` answers them from the reducer. This module turns an
//! avatar reference into the bundled image the UI draws. Toolkit-free: the GPUI
//! widgets draw what these functions return.
//!
//! It also holds the identity footer's presence (`SelfPresence`), read from the
//! controller's connection word and our own away state (`ServerListColumn.qml`'s
//! `selfPresence`).

use crate::controller::controller::Controller;

pub use crate::irc::presence::NickFacts;

/// The prefix of a bundled image reference. Qt's demo seed writes it that way.
const BUNDLED_PREFIX: &str = "qrc:/";

/// The connection word of a session that is registered (`connectionStatus`).
pub const CONNECTED: &str = "Connected";

/// The identity footer's presence: the word under the nick, and the colour of its
/// mark. Only a connected network can be away or available. Any other connection
/// word is offline, whatever away state a past session left behind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelfPresence {
    Available,
    Away,
    Offline,
}

impl SelfPresence {
    /// The presence for a connection word and our own away state.
    pub fn of(connection_status: &str, self_away: bool) -> Self {
        if connection_status != CONNECTED {
            return Self::Offline;
        }
        if self_away {
            Self::Away
        } else {
            Self::Available
        }
    }

    /// The word the footer shows under the nick.
    pub fn word(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Away => "away",
            Self::Offline => "offline",
        }
    }

    /// The kind `Style::presence_color` takes for the mark: green, amber or muted.
    pub fn mark(self) -> &'static str {
        match self {
            Self::Available => "online",
            Self::Away => "away",
            Self::Offline => "offline",
        }
    }
}

/// The footer's presence for the focused network, which is the identity Qt's footer reads
/// (`identityNetworkId`). It reads the controller's connection word and its own away state.
pub fn footer_presence(controller: &Controller) -> SelfPresence {
    SelfPresence::of(
        controller.connection_status_for(controller.focused_network_id()),
        controller.self_away(),
    )
}

/// The bundled image path for a `qrc:/` reference (`demo/mira-avatar.png`), or
/// `None` for anything else, such as the https avatar.
pub fn bundled_path(reference: &str) -> Option<&str> {
    reference.strip_prefix(BUNDLED_PREFIX)
}

/// The photo to draw for an avatar reference, given the `avatars` preference.
/// `None` means the initials. An https avatar is never fetched: gpui 0.2.2 ships
/// no HTTP client (its default refuses every request), so it keeps the initials,
/// which is also Qt's fallback when its own fetch fails.
pub fn photo_for(reference: Option<&str>, avatars_on: bool) -> Option<&str> {
    if !avatars_on {
        return None;
    }
    reference.and_then(bundled_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_references_map_to_their_asset_path() {
        assert_eq!(bundled_path("qrc:/demo/mira-avatar.png"), Some("demo/mira-avatar.png"));
        assert_eq!(bundled_path("https://omairc.app/omairc128.png"), None);
    }

    #[test]
    fn the_avatars_pref_gates_the_photo() {
        let reference = Some("qrc:/demo/anna-avatar.png");
        assert_eq!(photo_for(reference, true), Some("demo/anna-avatar.png"));
        assert_eq!(photo_for(reference, false), None);
    }

    #[test]
    fn an_https_or_missing_avatar_falls_back_to_initials() {
        assert_eq!(photo_for(Some("https://omairc.app/omairc128.png"), true), None);
        assert_eq!(photo_for(None, true), None);
    }

    #[test]
    fn the_footer_presence_is_available_away_or_offline() {
        assert_eq!(
            SelfPresence::of("Connected", false),
            SelfPresence::Available
        );
        assert_eq!(SelfPresence::of("Connected", true), SelfPresence::Away);
        for status in ["Offline", "Connecting", "Reconnecting", "Disconnecting"] {
            assert_eq!(
                SelfPresence::of(status, false),
                SelfPresence::Offline,
                "{status}"
            );
            assert_eq!(
                SelfPresence::of(status, true),
                SelfPresence::Offline,
                "{status}: a past away does not show"
            );
        }
    }

    #[test]
    fn each_presence_has_its_word_and_its_mark_kind() {
        let words_and_marks = [
            (SelfPresence::Available, "available", "online"),
            (SelfPresence::Away, "away", "away"),
            (SelfPresence::Offline, "offline", "offline"),
        ];
        for (presence, word, mark) in words_and_marks {
            assert_eq!((presence.word(), presence.mark()), (word, mark));
        }
    }

    #[test]
    fn the_seeded_facts_come_from_the_controller_per_network() {
        let controller = crate::controller::controller::Controller::seeded_for_tests();
        let facts = controller.nick_facts("omarchy");
        let mira = facts.get("mira").expect("mira is on omarchy");
        assert_eq!(photo_for(mira.avatar.as_deref(), true), Some("demo/mira-avatar.png"));
        assert!(facts.get("dax").is_some_and(|dax| dax.bot));
        assert!(facts.get("sol").is_some_and(|sol| sol.avatar.is_none()));
    }
}
