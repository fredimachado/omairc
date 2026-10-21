//! Preferences as the app keeps them on disk: the `/pref` toggles and the joins
//! level, read from and written to a JSON file.
//!
//! The catalog, the value types and the in-memory `Preferences` are protocol
//! vocabulary and live in `irc/pref` and `irc/membershipnoise` (Qt
//! `src/irc/ircpref.cpp` and `src/irc/ircmembershipnoise.h`). They are re-exported
//! here so the Connect sheet and the slash catalog keep their paths. This file
//! adds the file I/O, which moves to the INI store in the storage phase.

use std::io;
use std::path::{Path, PathBuf};

pub use crate::irc::membershipnoise::MembershipNoise;
pub use crate::irc::pref::{PrefName, PrefValue, Preferences};

impl Preferences {
    /// Reads `path`. A missing or unreadable file, or one that is not valid JSON,
    /// gives the defaults. Nothing here panics on a bad file.
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

    /// Sets a preference from the two typed words, as `/pref <name> <value>` does.
    fn set_by_name(prefs: &mut Preferences, name: &str, word: &str) -> bool {
        let Some(name) = PrefName::from_token(name) else {
            return false;
        };
        let Some(value) = PrefValue::parse(name, word) else {
            return false;
        };
        prefs.set(name, value)
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = scratch("round-trip");
        let mut prefs = Preferences::default();
        set_by_name(&mut prefs, "avatars", "off");
        set_by_name(&mut prefs, "joins", "every");
        prefs.save_to(&path).expect("save into a fresh folder");
        assert_eq!(Preferences::load_from(&path), prefs);
    }

    #[test]
    fn the_file_is_plain_json_with_the_joins_token() {
        let path = scratch("shape");
        let mut prefs = Preferences::default();
        set_by_name(&mut prefs, "joins", "hidden");
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
