//! The avatar a nick shows, given its reference and the `avatars` preference. Qt
//! keeps a nick's facts per network (the `761` numerics and the bot metadata), and
//! `Controller::nick_facts` answers them from the reducer. This module turns an
//! avatar reference into the bundled image the UI draws. Toolkit-free: the GPUI
//! widgets draw what these functions return.
//!
//! It also holds the identity footer's presence (`SelfPresence`), read from the
//! controller's connection word and our own away state (`client/src/ui/sidebar/`
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

/// The avatar reference a nick's photo is drawn from, given the `avatars` preference.
/// `None` means the initials. Both a bundled `qrc:/` reference and an https one come
/// back, and the shell resolves each to an image (`ui::shell::avatars`). An https
/// reference is fetched only while the preference is on, and until its image is ready
/// the nick keeps its initials, as it does again when the fetch fails (Qt's fallback).
pub fn photo_for(reference: Option<&str>, avatars_on: bool) -> Option<&str> {
    if !avatars_on {
        return None;
    }
    reference.filter(|reference| !reference.is_empty())
}

/// Whether a member row draws another member's presence dot (`client/src/ui/members/`:
/// `awayPresenceVisible || isSelf`). Our own away state comes from `305`/`306`, not
/// away-notify.
pub fn member_presence_dot_visible(away_notify: bool, is_self: bool) -> bool {
    away_notify || is_self
}

/// Whether away dimming applies on a member row (`memberDelegate.away` in Qt).
pub fn member_away_chrome_visible(away: bool, away_notify: bool, is_self: bool) -> bool {
    away && member_presence_dot_visible(away_notify, is_self)
}

/// Whether a sidebar direct-message row draws the avatar presence dot
/// (`client/src/ui/sidebar/conversation_row.rs`: `direct && awayPresenceVisible`).
pub fn sidebar_dm_presence_dot_visible(away_notify: bool) -> bool {
    away_notify
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
        assert_eq!(photo_for(reference, true), reference);
        assert_eq!(photo_for(reference, false), None);
    }

    #[test]
    fn an_https_reference_is_a_photo_only_while_avatars_are_on() {
        let https = Some("https://omairc.app/omairc128.png");
        assert_eq!(photo_for(https, true), https, "fetched and drawn once ready");
        assert_eq!(photo_for(https, false), None, "the pref off never fetches");
    }

    #[test]
    fn a_missing_or_empty_avatar_falls_back_to_initials() {
        assert_eq!(photo_for(None, true), None);
        assert_eq!(photo_for(Some(""), true), None);
    }

    #[test]
    fn presence_dots_follow_away_notify_like_qt() {
        use super::{
            member_away_chrome_visible, member_presence_dot_visible,
            sidebar_dm_presence_dot_visible,
        };
        assert!(!member_presence_dot_visible(false, false));
        assert!(member_presence_dot_visible(false, true));
        assert!(member_presence_dot_visible(true, false));
        assert!(!sidebar_dm_presence_dot_visible(false));
        assert!(sidebar_dm_presence_dot_visible(true));
        assert!(!member_away_chrome_visible(true, false, false));
        assert!(member_away_chrome_visible(true, false, true));
        assert!(member_away_chrome_visible(true, true, false));
        assert!(!member_away_chrome_visible(false, true, false));
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
        assert_eq!(photo_for(mira.avatar.as_deref(), true), Some("qrc:/demo/mira-avatar.png"));
        assert!(facts.get("dax").is_some_and(|dax| dax.bot));
        assert!(facts.get("sol").is_some_and(|sol| sol.avatar.is_none()));
    }
}
