//! Preferences shared by `/pref` and the Connect sheet's Preferences tab. The
//! names, labels and tokens come from `src/irc/ircpref.cpp`, and the joins
//! values from `src/irc/ircmembershipnoise.h`.
//!
//! Toolkit-free. The pure part is the in-memory struct: setting a value and
//! reading it back. Loading and saving are separate functions, and they take a
//! path, so tests can use a temporary file.

// The shell and the Connect sheet wire these in the next phases; drop the allow then.
#![allow(dead_code)]

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Each `/pref` name. `Joins` is the one whose value is a noise level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrefName {
    Directs,
    Avatars,
    Unread,
    Joins,
}

impl PrefName {
    /// Catalog order, which the slash popup and the bare `/pref` list follow.
    pub const ALL: [PrefName; 4] = [
        PrefName::Directs,
        PrefName::Avatars,
        PrefName::Unread,
        PrefName::Joins,
    ];

    /// The word typed after `/pref`.
    pub fn token(self) -> &'static str {
        match self {
            PrefName::Directs => "directs",
            PrefName::Avatars => "avatars",
            PrefName::Unread => "unread",
            PrefName::Joins => "joins",
        }
    }

    /// The label in every reply line, such as `Show peer avatars: on`.
    pub fn label(self) -> &'static str {
        match self {
            PrefName::Directs => "Reopen direct messages on startup",
            PrefName::Avatars => "Show peer avatars",
            PrefName::Unread => "Open conversations at unread",
            PrefName::Joins => "Join, part, quit, and nick lines",
        }
    }

    /// The name for a typed word, ignoring case.
    pub fn from_token(token: &str) -> Option<Self> {
        let folded = token.to_lowercase();
        Self::ALL.into_iter().find(|name| name.token() == folded)
    }
}

/// How join, part, quit and nick lines show (`IrcMembershipNoise`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MembershipNoise {
    /// A run of membership lines collapses into one caption. The default.
    #[default]
    Folded,
    /// Every line shows.
    Every,
    /// The lines are hidden.
    Hidden,
}

impl MembershipNoise {
    /// Catalog order, which the slash popup offers after `/pref joins `.
    pub const ALL: [MembershipNoise; 3] = [
        MembershipNoise::Folded,
        MembershipNoise::Every,
        MembershipNoise::Hidden,
    ];

    pub fn token(self) -> &'static str {
        match self {
            MembershipNoise::Folded => "folded",
            MembershipNoise::Every => "every",
            MembershipNoise::Hidden => "hidden",
        }
    }

    /// The level for a typed word, ignoring case and surrounding spaces.
    pub fn from_token(token: &str) -> Option<Self> {
        let folded = token.trim().to_lowercase();
        Self::ALL.into_iter().find(|level| level.token() == folded)
    }
}

/// A value a `/pref` name can hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefValue {
    Toggle(bool),
    Noise(MembershipNoise),
}

impl PrefValue {
    /// The value `word` sets for `name`, or `None` if the word does not fit.
    /// Toggles take `on` or `off`. `joins` takes folded, every or hidden.
    pub fn parse(name: PrefName, word: &str) -> Option<Self> {
        if name == PrefName::Joins {
            return MembershipNoise::from_token(word).map(PrefValue::Noise);
        }
        match word.to_lowercase().as_str() {
            "on" => Some(PrefValue::Toggle(true)),
            "off" => Some(PrefValue::Toggle(false)),
            _ => None,
        }
    }
}

/// The saved preferences. Every field is independent, and each has a default
/// so a file written by an older build still loads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Reopen direct messages on startup.
    pub directs: bool,
    /// Show peer avatars.
    pub avatars: bool,
    /// Open conversations at their unread mark.
    pub unread: bool,
    /// How join, part, quit and nick lines show.
    pub joins: MembershipNoise,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            directs: true,
            avatars: true,
            unread: true,
            joins: MembershipNoise::Folded,
        }
    }
}

impl Preferences {
    /// The current value of `name`.
    pub fn get(&self, name: PrefName) -> PrefValue {
        match name {
            PrefName::Directs => PrefValue::Toggle(self.directs),
            PrefName::Avatars => PrefValue::Toggle(self.avatars),
            PrefName::Unread => PrefValue::Toggle(self.unread),
            PrefName::Joins => PrefValue::Noise(self.joins),
        }
    }

    /// Whether a toggle name is on. `Joins` is not a toggle and reads false.
    pub fn enabled(&self, name: PrefName) -> bool {
        matches!(self.get(name), PrefValue::Toggle(true))
    }

    /// Stores `value` under `name`. Returns false, and changes nothing, when
    /// the value's kind does not match the name (a toggle for `joins`, or a
    /// noise level for a toggle).
    pub fn set(&mut self, name: PrefName, value: PrefValue) -> bool {
        match (name, value) {
            (PrefName::Directs, PrefValue::Toggle(on)) => self.directs = on,
            (PrefName::Avatars, PrefValue::Toggle(on)) => self.avatars = on,
            (PrefName::Unread, PrefValue::Toggle(on)) => self.unread = on,
            (PrefName::Joins, PrefValue::Noise(level)) => self.joins = level,
            _ => return false,
        }
        true
    }

    /// Sets a preference from the two typed words, as `/pref <name> <value>`
    /// does. Returns false when either word is unknown.
    pub fn set_by_name(&mut self, name: &str, word: &str) -> bool {
        let Some(name) = PrefName::from_token(name) else {
            return false;
        };
        let Some(value) = PrefValue::parse(name, word) else {
            return false;
        };
        self.set(name, value)
    }

    /// Reads `path`. A missing or unreadable file, or one that is not valid
    /// JSON, gives the defaults. Nothing here panics on a bad file.
    pub fn load_from(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Writes the preferences to `path` as pretty JSON, creating its folder.
    pub fn save_to(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(io::Error::other)?;
        std::fs::write(path, format!("{json}\n"))
    }

    /// The preferences at the default path, or the defaults if there is none.
    pub fn load() -> Self {
        default_path().map_or_else(Self::default, |path| Self::load_from(&path))
    }

    /// Writes to the default path. Fails when the system has no config folder.
    pub fn save(&self) -> io::Result<()> {
        match default_path() {
            Some(path) => self.save_to(&path),
            None => Err(io::Error::other("no config directory for prefs.json")),
        }
    }
}

/// `dirs::config_dir()/omairc-poc/prefs.json`, or `None` with no config folder.
pub fn default_path() -> Option<PathBuf> {
    dirs::config_dir().map(|dir| dir.join("omairc-poc").join("prefs.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh path under the system temp dir, unique per test.
    fn scratch(test: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("omairc-poc-prefs-{}-{test}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("prefs.json")
    }

    #[test]
    fn defaults_are_all_on_with_folded_joins() {
        let prefs = Preferences::default();
        assert!(prefs.directs && prefs.avatars && prefs.unread);
        assert_eq!(prefs.joins, MembershipNoise::Folded);
    }

    #[test]
    fn names_resolve_from_tokens_ignoring_case() {
        assert_eq!(PrefName::from_token("Directs"), Some(PrefName::Directs));
        assert_eq!(PrefName::from_token("AVATARS"), Some(PrefName::Avatars));
        assert_eq!(PrefName::from_token("unread"), Some(PrefName::Unread));
        assert_eq!(PrefName::from_token("joins"), Some(PrefName::Joins));
        assert_eq!(PrefName::from_token("notify"), None);
        assert_eq!(PrefName::from_token(""), None);
    }

    #[test]
    fn labels_and_tokens_match_the_qt_catalog() {
        assert_eq!(
            PrefName::Directs.label(),
            "Reopen direct messages on startup"
        );
        assert_eq!(PrefName::Avatars.label(), "Show peer avatars");
        assert_eq!(PrefName::Unread.label(), "Open conversations at unread");
        assert_eq!(PrefName::Joins.label(), "Join, part, quit, and nick lines");
        let tokens: Vec<&str> = PrefName::ALL.iter().map(|name| name.token()).collect();
        assert_eq!(tokens, ["directs", "avatars", "unread", "joins"]);
    }

    #[test]
    fn noise_levels_parse_and_round_trip() {
        for level in MembershipNoise::ALL {
            assert_eq!(MembershipNoise::from_token(level.token()), Some(level));
        }
        assert_eq!(
            MembershipNoise::from_token("  EVERY "),
            Some(MembershipNoise::Every)
        );
        assert_eq!(MembershipNoise::from_token("loud"), None);
    }

    #[test]
    fn values_parse_by_name_kind() {
        assert_eq!(
            PrefValue::parse(PrefName::Unread, "On"),
            Some(PrefValue::Toggle(true))
        );
        assert_eq!(
            PrefValue::parse(PrefName::Avatars, "off"),
            Some(PrefValue::Toggle(false))
        );
        assert_eq!(PrefValue::parse(PrefName::Directs, "folded"), None);
        assert_eq!(
            PrefValue::parse(PrefName::Joins, "hidden"),
            Some(PrefValue::Noise(MembershipNoise::Hidden))
        );
        assert_eq!(PrefValue::parse(PrefName::Joins, "on"), None);
    }

    #[test]
    fn set_writes_only_a_matching_kind() {
        let mut prefs = Preferences::default();
        assert!(prefs.set(PrefName::Avatars, PrefValue::Toggle(false)));
        assert!(!prefs.avatars);
        assert!(prefs.set(PrefName::Joins, PrefValue::Noise(MembershipNoise::Every)));
        assert_eq!(prefs.joins, MembershipNoise::Every);
        assert!(!prefs.set(PrefName::Joins, PrefValue::Toggle(true)));
        assert!(!prefs.set(PrefName::Directs, PrefValue::Noise(MembershipNoise::Hidden)));
        assert_eq!(
            prefs.get(PrefName::Joins),
            PrefValue::Noise(MembershipNoise::Every)
        );
    }

    #[test]
    fn set_by_name_takes_the_two_typed_words() {
        let mut prefs = Preferences::default();
        assert!(prefs.set_by_name("Unread", "off"));
        assert!(!prefs.unread);
        assert!(prefs.set_by_name("joins", "hidden"));
        assert_eq!(prefs.joins, MembershipNoise::Hidden);
        assert!(!prefs.set_by_name("bogus", "on"));
        assert!(!prefs.set_by_name("directs", "sometimes"));
        assert!(!prefs.set_by_name("directs", "hidden"));
        assert!(prefs.directs);
    }

    #[test]
    fn enabled_reads_toggles_and_is_false_for_joins() {
        let mut prefs = Preferences::default();
        assert!(prefs.enabled(PrefName::Directs));
        prefs.set_by_name("directs", "off");
        assert!(!prefs.enabled(PrefName::Directs));
        assert!(!prefs.enabled(PrefName::Joins));
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = scratch("round-trip");
        let mut prefs = Preferences::default();
        prefs.set_by_name("avatars", "off");
        prefs.set_by_name("joins", "every");
        prefs.save_to(&path).expect("save into a fresh folder");
        assert_eq!(Preferences::load_from(&path), prefs);
    }

    #[test]
    fn the_file_is_plain_json_with_the_joins_token() {
        let path = scratch("shape");
        let mut prefs = Preferences::default();
        prefs.set_by_name("joins", "hidden");
        prefs.save_to(&path).expect("save");
        let text = std::fs::read_to_string(&path).expect("read back");
        let json: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(json["joins"], "hidden");
        assert_eq!(json["directs"], true);
    }

    #[test]
    fn a_missing_file_loads_the_defaults() {
        let path = scratch("missing");
        assert_eq!(Preferences::load_from(&path), Preferences::default());
    }

    #[test]
    fn a_broken_file_loads_the_defaults_without_panicking() {
        let path = scratch("broken");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(Preferences::load_from(&path), Preferences::default());
    }

    #[test]
    fn a_partial_file_keeps_the_defaults_for_missing_keys() {
        let path = scratch("partial");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"avatars": false}"#).unwrap();
        let prefs = Preferences::load_from(&path);
        assert!(!prefs.avatars);
        assert!(prefs.directs && prefs.unread);
        assert_eq!(prefs.joins, MembershipNoise::Folded);
    }

    #[test]
    fn the_default_path_sits_under_omairc_poc() {
        if let Some(path) = default_path() {
            assert!(path.ends_with(Path::new("omairc-poc").join("prefs.json")));
        }
    }
}
