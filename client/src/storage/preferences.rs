//! The `preferences` group of the settings file: the `/pref` toggles and the joins level
//! the sidebar's network order and collapse set, and the `/autoaway` settings.
//! `IrcController::loadStoredPreferences`, `savePreferenceBool` and friends in
//! `tui/internal/controller/controller.go` and `tui/internal/controller/autoawayruntime.go`, and of the
//! group handling in `tui/internal/controller/controller.go` (Go). The keys are the Qt
//! names, so the three clients share one section.
//!
//! This replaces an earlier JSON `prefs.json`. That file is not migrated, since it
//! never shipped.

use crate::irc::autoaway::AutoawayConfig;
use crate::irc::colorpalette::ColorPaletteId;
use crate::irc::membershipnoise::MembershipNoise;
use crate::irc::pref::Preferences;

use super::settings::Status;
use super::Backing;

/// The QSettings group that holds every preference.
pub const PREFERENCES_GROUP: &str = "preferences";
const REOPEN_DIRECTS_KEY: &str = "reopenDirectMessages";
const LOAD_AVATARS_KEY: &str = "loadPeerAvatars";
const OPEN_AT_UNREAD_KEY: &str = "openConversationsAtUnread";
const SHOW_IRC_COLORS_KEY: &str = "showIrcColors";
const MEMBERSHIP_NOISE_KEY: &str = "membershipNoise";
const COLOR_PALETTE_KEY: &str = "colorPalette";
const NETWORK_ORDER_KEY: &str = "networkOrder";
const COLLAPSED_NETWORKS_KEY: &str = "collapsedNetworks";
const AUTOAWAY_ENABLED_KEY: &str = "autoawayEnabled";
const AUTOAWAY_TIMEOUT_KEY: &str = "autoawayTimeoutSeconds";
const AUTOAWAY_REASON_KEY: &str = "autoawayDefaultReason";

/// The preference values, the sidebar layout and the auto-away settings on disk.
pub struct PreferenceStore {
    backing: Backing,
}

impl PreferenceStore {
    pub fn new(backing: Backing) -> Self {
        Self { backing }
    }

    /// The saved `/pref` values. A missing or unreadable key takes Qt's default.
    pub fn preferences(&self) -> Preferences {
        let settings = self.backing.settings();
        let defaults = Preferences::default();
        Preferences {
            directs: settings.bool_value(PREFERENCES_GROUP, REOPEN_DIRECTS_KEY, defaults.directs),
            avatars: settings.bool_value(PREFERENCES_GROUP, LOAD_AVATARS_KEY, defaults.avatars),
            unread: settings.bool_value(PREFERENCES_GROUP, OPEN_AT_UNREAD_KEY, defaults.unread),
            colors: settings.bool_value(PREFERENCES_GROUP, SHOW_IRC_COLORS_KEY, defaults.colors),
            joins: MembershipNoise::from_token(&settings.value(PREFERENCES_GROUP, MEMBERSHIP_NOISE_KEY))
                .unwrap_or(defaults.joins),
            palette: ColorPaletteId::from_token(&settings.value(PREFERENCES_GROUP, COLOR_PALETTE_KEY))
                .unwrap_or(defaults.palette),
        }
    }

    /// The color palette for nicks and network icons (`preferences().palette`).
    pub fn color_palette(&self) -> ColorPaletteId {
        self.preferences().palette
    }

    /// Writes every `/pref` value. A malformed or unwritable file is left alone.
    pub fn save_preferences(&self, prefs: &Preferences) -> Status {
        self.backing.edit(|settings| {
            settings.set_bool(PREFERENCES_GROUP, REOPEN_DIRECTS_KEY, prefs.directs);
            settings.set_bool(PREFERENCES_GROUP, LOAD_AVATARS_KEY, prefs.avatars);
            settings.set_bool(PREFERENCES_GROUP, OPEN_AT_UNREAD_KEY, prefs.unread);
            settings.set_bool(PREFERENCES_GROUP, SHOW_IRC_COLORS_KEY, prefs.colors);
            settings.set_value(PREFERENCES_GROUP, MEMBERSHIP_NOISE_KEY, prefs.joins.token());
            settings.set_value(PREFERENCES_GROUP, COLOR_PALETTE_KEY, prefs.palette.token());
        })
    }

    /// The saved sidebar order, as network ids.
    pub fn network_order(&self) -> Vec<String> {
        self.list(NETWORK_ORDER_KEY)
    }

    pub fn save_network_order(&self, network_ids: &[String]) -> Status {
        self.save_list(NETWORK_ORDER_KEY, network_ids)
    }

    /// The network ids whose rows are folded.
    pub fn collapsed_networks(&self) -> Vec<String> {
        self.list(COLLAPSED_NETWORKS_KEY)
    }

    pub fn save_collapsed_networks(&self, network_ids: &[String]) -> Status {
        self.save_list(COLLAPSED_NETWORKS_KEY, network_ids)
    }

    /// The saved `/autoaway` settings. The one-shot reason is never saved, as in Qt.
    pub fn autoaway(&self) -> AutoawayConfig {
        let settings = self.backing.settings();
        AutoawayConfig {
            enabled: settings.bool_value(PREFERENCES_GROUP, AUTOAWAY_ENABLED_KEY, false),
            timeout_seconds: settings.int_value(PREFERENCES_GROUP, AUTOAWAY_TIMEOUT_KEY, 0),
            default_reason: settings.value(PREFERENCES_GROUP, AUTOAWAY_REASON_KEY),
            one_shot_reason: String::new(),
        }
    }

    pub fn save_autoaway(&self, config: &AutoawayConfig) -> Status {
        self.backing.edit(|settings| {
            settings.set_bool(PREFERENCES_GROUP, AUTOAWAY_ENABLED_KEY, config.enabled);
            settings.set_int(PREFERENCES_GROUP, AUTOAWAY_TIMEOUT_KEY, config.timeout_seconds);
            settings.set_value(PREFERENCES_GROUP, AUTOAWAY_REASON_KEY, &config.default_reason);
        })
    }

    fn list(&self, key: &str) -> Vec<String> {
        self.backing
            .settings()
            .string_list(PREFERENCES_GROUP, key)
            .unwrap_or_default()
            .into_iter()
            .filter(|id| !id.is_empty())
            .collect()
    }

    fn save_list(&self, key: &str, values: &[String]) -> Status {
        self.backing.edit(|settings| {
            settings.set_string_list(PREFERENCES_GROUP, key, values);
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testdir::TempDir;

    fn store(dir: &TempDir) -> PreferenceStore {
        PreferenceStore::new(Backing::Disk(dir.path().join("omairc.conf")))
    }

    #[test]
    fn a_missing_file_gives_the_defaults() {
        let dir = TempDir::new("prefs-defaults");
        let store = store(&dir);
        assert_eq!(store.preferences(), Preferences::default());
        assert_eq!(store.color_palette(), ColorPaletteId::Classic);
        assert!(store.network_order().is_empty());
        assert!(store.collapsed_networks().is_empty());
        assert_eq!(store.autoaway(), AutoawayConfig::default());
    }

    #[test]
    fn color_palette_reads_known_tokens_and_ignores_unknown() {
        let dir = TempDir::new("prefs-color-palette");
        let path = dir.path().join("omairc.conf");
        std::fs::write(&path, "[preferences]\ncolorPalette=vivid\n").unwrap();
        let store = PreferenceStore::new(Backing::Disk(path.clone()));
        assert_eq!(store.color_palette(), ColorPaletteId::Vivid);

        std::fs::write(&path, "[preferences]\ncolorPalette=loud\n").unwrap();
        let store = PreferenceStore::new(Backing::Disk(path));
        assert_eq!(store.color_palette(), ColorPaletteId::Classic);
    }

    #[test]
    fn preferences_round_trip_under_the_qt_keys() {
        let dir = TempDir::new("prefs-round");
        let store = store(&dir);
        let prefs = Preferences {
            directs: false,
            colors: false,
            joins: MembershipNoise::Every,
            palette: ColorPaletteId::Vivid,
            ..Default::default()
        };
        assert_eq!(store.save_preferences(&prefs), Status::Written);
        assert_eq!(store.preferences(), prefs);
        assert_eq!(store.color_palette(), ColorPaletteId::Vivid);

        let text = std::fs::read_to_string(dir.path().join("omairc.conf")).expect("written");
        assert!(text.contains("[preferences]"), "{text}");
        assert!(text.contains("reopenDirectMessages=false"), "{text}");
        assert!(text.contains("showIrcColors=false"), "{text}");
        assert!(text.contains("membershipNoise=every"), "{text}");
        assert!(text.contains("colorPalette=vivid"), "{text}");
    }

    #[test]
    fn an_unknown_joins_token_keeps_the_default() {
        let dir = TempDir::new("prefs-joins");
        let path = dir.path().join("omairc.conf");
        std::fs::write(&path, "[preferences]\nmembershipNoise=loud\n").unwrap();
        let store = PreferenceStore::new(Backing::Disk(path));
        assert_eq!(store.preferences().joins, MembershipNoise::Folded);
    }

    #[test]
    fn order_collapse_and_autoaway_survive_a_restart() {
        let dir = TempDir::new("prefs-layout");
        let store = store(&dir);
        let order = vec!["oftc".to_owned(), "omarchy".to_owned()];
        store.save_network_order(&order);
        store.save_collapsed_networks(&["omarchy".to_owned()]);
        let config = AutoawayConfig {
            enabled: true,
            timeout_seconds: 300,
            default_reason: "lunch".to_owned(),
            one_shot_reason: "gone".to_owned(),
        };
        store.save_autoaway(&config);

        let again = store_again(&dir);
        assert_eq!(again.network_order(), order);
        assert_eq!(again.collapsed_networks(), ["omarchy"]);
        let saved = again.autoaway();
        assert!(saved.enabled);
        assert_eq!(saved.timeout_seconds, 300);
        assert_eq!(saved.default_reason, "lunch");
        assert!(saved.one_shot_reason.is_empty(), "the one-shot reason is never saved");
    }

    fn store_again(dir: &TempDir) -> PreferenceStore {
        store(dir)
    }
}
