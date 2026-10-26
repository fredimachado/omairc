//! under this build's app folder ([`crate::identity::APP_FOLDER`]). With the
//! `dev` feature that folder is `omairc-dev`, separate from the Qt/TUI `omairc/`
//! tree, so local `cargo test --features dev` never writes into a real install.
//! The roots follow `$XDG_CONFIG_HOME` and `$XDG_STATE_HOME` when set. Otherwise
//! they are the platform's generic locations:
//! - Windows: `%LOCALAPPDATA%` for config (`GenericConfigLocation`); state is
//!   `%LOCALAPPDATA%\State` (`GenericStateLocation` in Qt 6 — see
//!   `platform paths`).
//! - macOS: `~/Library/Preferences` and `~/Library/Preferences/State`.
//! - Linux: `~/.config` and `~/.local/state`.
//! Qt on Windows stores main settings in the registry
//! (`HKCU\Software\omairc\omairc`), not an INI under LocalAppData. The STS
//! cache stays an INI at `%LOCALAPPDATA%\omairc\sts`. `OMAIRC_DEV_HOME` moves
//! both roots under one directory, so tests and manual runs can stay out of the
//! user's profile.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub use crate::identity::{APP_FOLDER, QT_APP_FOLDER};
#[cfg(test)]
pub use crate::identity::{SETTINGS_FILE, SETTINGS_FILE_MACOS};
use crate::identity::settings_file_name;

/// The STS policy cache, beside the settings file (`IrcStsStore::filePath`, same name as Qt).
pub const STS_FILE: &str = "sts";
/// The application log, the same name the Qt client uses.
pub const LOG_FILE: &str = "omairc.log";
/// Moves the config and state roots under one directory.
pub const HOME_OVERRIDE_VAR: &str = "OMAIRC_DEV_HOME";
/// Overrides the on-disk transcript log root (`OMAIRC_TRANSCRIPT_ROOT`).
pub const TRANSCRIPT_ROOT_VAR: &str = "OMAIRC_TRANSCRIPT_ROOT";

/// The layout family. Only the root directories differ between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOs,
    /// Linux and every other Unix, which use the XDG layout.
    Linux,
}

impl Platform {
    pub fn current() -> Self {
        if cfg!(windows) {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else {
            Platform::Linux
        }
    }
}

/// The inputs the roots are computed from. Built from the live process by
/// [`Env::from_process`]; tests build it directly, so they never read or write
/// the real environment.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Env {
    pub home: Option<PathBuf>,
    pub local_app_data: Option<PathBuf>,
    pub xdg_config_home: Option<PathBuf>,
    pub xdg_state_home: Option<PathBuf>,
    pub dev_home: Option<PathBuf>,
    pub transcript_root: Option<PathBuf>,
    /// Where the log goes when no state root is known, as Qt's `TempLocation`.
    pub temp_dir: PathBuf,
}

impl Env {
    pub fn from_process() -> Self {
        Self::from_lookup(
            |name| std::env::var_os(name),
            dirs::home_dir(),
            std::env::temp_dir(),
        )
    }

    /// Builds the inputs from any variable lookup. Empty values count as unset
    /// as the XDG spec and Qt both treat them.
    pub fn from_lookup(
        lookup: impl Fn(&str) -> Option<OsString>,
        home: Option<PathBuf>,
        temp_dir: PathBuf,
    ) -> Self {
        let path = |name: &str| {
            lookup(name)
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
        };
        Self {
            home,
            local_app_data: path("LOCALAPPDATA"),
            xdg_config_home: path("XDG_CONFIG_HOME"),
            xdg_state_home: path("XDG_STATE_HOME"),
            dev_home: path(HOME_OVERRIDE_VAR),
            transcript_root: path(TRANSCRIPT_ROOT_VAR),
            temp_dir,
        }
    }
}

/// The config folder (`<config root>/<APP_FOLDER>`), or `None` when the platform
/// config root is unknown.
pub fn config_dir_for(env: &Env, platform: Platform) -> Option<PathBuf> {
    if let Some(home) = &env.dev_home {
        return Some(home.join("config"));
    }
    config_dir_for_folder(env, platform, APP_FOLDER)
}

/// The Qt client's config folder (`<config root>/omairc`). `OMAIRC_DEV_HOME` does not
/// apply; cutover reads the install the Qt client already wrote.
pub fn qt_config_dir_for(env: &Env, platform: Platform) -> Option<PathBuf> {
    config_dir_for_folder(env, platform, QT_APP_FOLDER)
}

/// The state folder (`<state root>/<APP_FOLDER>`). Falls back to the temp dir when
/// the platform state root is unknown, as the Qt log does.
pub fn state_dir_for(env: &Env, platform: Platform) -> PathBuf {
    if let Some(home) = &env.dev_home {
        return home.join("state");
    }
    state_root(env, platform)
        .unwrap_or_else(|| env.temp_dir.clone())
        .join(APP_FOLDER)
}

pub fn settings_path_for(env: &Env, platform: Platform) -> Option<PathBuf> {
    config_dir_for(env, platform).map(|dir| dir.join(ini_settings_file(platform)))
}

/// The Qt client's INI settings file, when one exists.
/// - Linux: `<config root>/omairc/omairc.conf`
/// - macOS: `<config root>/omairc/omairc.ini` (Darwin `QSettings::IniFormat` always
///   uses `.ini`; see `settingsFileMissingOnDisk` in `tui/internal/storage/profilestore.go`)
/// - Windows: `None` — main settings are `HKCU\Software\omairc\omairc`
///   (`QSettings::NativeFormat`). STS still lives under the config folder; use
///   [`qt_sts_path_for`].
pub fn qt_settings_path_for(env: &Env, platform: Platform) -> Option<PathBuf> {
    if platform == Platform::Windows {
        return None;
    }
    qt_config_dir_for(env, platform).map(|dir| dir.join(ini_settings_file(platform)))
}

/// The Qt client's STS cache (`<config root>/omairc/sts`), including on Windows
/// where main settings are in the registry and this file sits beside where an
/// INI would live.
pub fn qt_sts_path_for(env: &Env, platform: Platform) -> Option<PathBuf> {
    qt_config_dir_for(env, platform).map(|dir| dir.join(STS_FILE))
}

/// The STS cache file in the folder that holds `settings_file`, so the cache and the
/// settings always live together.
pub fn sts_path_beside(settings_file: &Path) -> PathBuf {
    settings_file.with_file_name(STS_FILE)
}

fn ini_settings_file(platform: Platform) -> &'static str {
    settings_file_name(platform == Platform::MacOs)
}

pub fn log_path_for(env: &Env, platform: Platform) -> PathBuf {
    state_dir_for(env, platform).join(LOG_FILE)
}

/// The conversation transcript folder (`<state>/logs`), or the override when set.
pub fn transcript_root_for(env: &Env, platform: Platform) -> PathBuf {
    if let Some(root) = &env.transcript_root {
        return root.clone();
    }
    state_dir_for(env, platform).join("logs")
}

/// The state folder of an installed Qt client (`<state root>/omairc`).
pub fn qt_state_dir_for(env: &Env, platform: Platform) -> PathBuf {
    state_root(env, platform)
        .unwrap_or_else(|| env.temp_dir.clone())
        .join(QT_APP_FOLDER)
}

/// The Qt client's transcript folder (`<state root>/omairc/logs`), or the override when set.
pub fn qt_transcript_root_for(env: &Env, platform: Platform) -> PathBuf {
    if let Some(root) = &env.transcript_root {
        return root.clone();
    }
    qt_state_dir_for(env, platform).join("logs")
}

pub fn transcript_root() -> PathBuf {
    transcript_root_for(&Env::from_process(), Platform::current())
}

/// The INI settings file on this machine, or `None` when there is no config root.
pub fn settings_path() -> Option<PathBuf> {
    settings_path_for(&Env::from_process(), Platform::current())
}

pub fn log_path() -> PathBuf {
    log_path_for(&Env::from_process(), Platform::current())
}

fn config_dir_for_folder(env: &Env, platform: Platform, folder: &str) -> Option<PathBuf> {
    config_root(env, platform).map(|root| root.join(folder))
}

fn config_root(env: &Env, platform: Platform) -> Option<PathBuf> {
    if let Some(xdg) = &env.xdg_config_home {
        return Some(xdg.clone());
    }
    match platform {
        Platform::Windows => env.local_app_data.clone(),
        Platform::MacOs => env
            .home
            .as_deref()
            .map(|home| home.join("Library").join("Preferences")),
        Platform::Linux => env.home.as_deref().map(|home| home.join(".config")),
    }
}

fn state_root(env: &Env, platform: Platform) -> Option<PathBuf> {
    if let Some(xdg) = &env.xdg_state_home {
        return Some(xdg.clone());
    }
    match platform {
        // Qt 6 `GenericStateLocation` on Windows is `%LOCALAPPDATA%\State`
        // (`platform paths`), not bare LocalAppData. Transcripts and
        // `omairc.log` from the Qt client live under that tree.
        Platform::Windows => env
            .local_app_data
            .as_ref()
            .map(|root| root.join("State")),
        Platform::MacOs => env
            .home
            .as_deref()
            .map(|home| home.join("Library").join("Preferences").join("State")),
        Platform::Linux => env
            .home
            .as_deref()
            .map(|home| home.join(".local").join("state")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        PathBuf::from("home").join("fred")
    }

    fn env_with_home() -> Env {
        Env {
            home: Some(home()),
            temp_dir: PathBuf::from("tmp"),
            ..Env::default()
        }
    }

    #[test]
    fn linux_defaults_follow_xdg_locations() {
        let env = env_with_home();
        assert_eq!(
            settings_path_for(&env, Platform::Linux),
            Some(home().join(".config").join(APP_FOLDER).join(SETTINGS_FILE))
        );
        assert_eq!(
            log_path_for(&env, Platform::Linux),
            home()
                .join(".local")
                .join("state")
                .join(APP_FOLDER)
                .join(LOG_FILE)
        );
    }

    #[test]
    fn xdg_overrides_win_on_every_platform() {
        let env = Env {
            xdg_config_home: Some(PathBuf::from("cfg")),
            xdg_state_home: Some(PathBuf::from("st")),
            local_app_data: Some(PathBuf::from("lad")),
            home: Some(home()),
            temp_dir: PathBuf::from("tmp"),
            ..Env::default()
        };
        for platform in [Platform::Windows, Platform::MacOs, Platform::Linux] {
            assert_eq!(
                config_dir_for(&env, platform),
                Some(PathBuf::from("cfg").join(APP_FOLDER))
            );
            assert_eq!(
                state_dir_for(&env, platform),
                PathBuf::from("st").join(APP_FOLDER)
            );
        }
    }

    #[test]
    fn windows_uses_local_app_data_config_and_state_subdir() {
        let env = Env {
            local_app_data: Some(PathBuf::from("lad")),
            home: Some(home()),
            temp_dir: PathBuf::from("tmp"),
            ..Env::default()
        };
        assert_eq!(
            settings_path_for(&env, Platform::Windows),
            Some(PathBuf::from("lad").join(APP_FOLDER).join(SETTINGS_FILE))
        );
        assert_eq!(
            log_path_for(&env, Platform::Windows),
            PathBuf::from("lad")
                .join("State")
                .join(APP_FOLDER)
                .join(LOG_FILE)
        );
    }

    #[test]
    fn macos_uses_library_preferences_and_state() {
        let env = env_with_home();
        let prefs = home().join("Library").join("Preferences");
        assert_eq!(
            config_dir_for(&env, Platform::MacOs),
            Some(prefs.join(APP_FOLDER))
        );
        assert_eq!(
            settings_path_for(&env, Platform::MacOs),
            Some(prefs.join(APP_FOLDER).join(SETTINGS_FILE_MACOS))
        );
        assert_eq!(
            state_dir_for(&env, Platform::MacOs),
            prefs.join("State").join(APP_FOLDER)
        );
    }

    #[test]
    fn the_sts_cache_sits_beside_the_settings_file() {
        let env = env_with_home();
        let settings = settings_path_for(&env, Platform::Linux).expect("a config root");
        assert_eq!(
            sts_path_beside(&settings),
            home().join(".config").join(APP_FOLDER).join(STS_FILE)
        );
    }

    #[test]
    fn dev_home_moves_both_roots_under_one_directory() {
        let env = Env {
            dev_home: Some(PathBuf::from("scratch")),
            xdg_config_home: Some(PathBuf::from("cfg")),
            local_app_data: Some(PathBuf::from("lad")),
            home: Some(home()),
            temp_dir: PathBuf::from("tmp"),
            ..Env::default()
        };
        for platform in [Platform::Windows, Platform::MacOs, Platform::Linux] {
            assert_eq!(
                settings_path_for(&env, platform),
                Some(
                    PathBuf::from("scratch")
                        .join("config")
                        .join(ini_settings_file(platform))
                )
            );
            assert_eq!(
                log_path_for(&env, platform),
                PathBuf::from("scratch").join("state").join(LOG_FILE)
            );
        }
    }

    #[test]
    fn missing_roots_give_no_config_and_a_temp_log() {
        let env = Env {
            temp_dir: PathBuf::from("tmp"),
            ..Env::default()
        };
        assert_eq!(settings_path_for(&env, Platform::Linux), None);
        assert_eq!(settings_path_for(&env, Platform::Windows), None);
        assert_eq!(
            log_path_for(&env, Platform::Windows),
            PathBuf::from("tmp").join(APP_FOLDER).join(LOG_FILE)
        );
    }

    /// With `dev`, this build must not share the installed client's folder.
    #[cfg(feature = "dev")]
    #[test]
    fn the_qt_folder_is_never_used() {
        let env = Env {
            xdg_config_home: Some(PathBuf::from("cfg")),
            xdg_state_home: Some(PathBuf::from("st")),
            local_app_data: Some(PathBuf::from("lad")),
            home: Some(home()),
            temp_dir: PathBuf::from("tmp"),
            ..Env::default()
        };
        for platform in [Platform::Windows, Platform::MacOs, Platform::Linux] {
            let paths = [
                settings_path_for(&env, platform).unwrap(),
                log_path_for(&env, platform),
                transcript_root_for(&env, platform),
            ];
            for path in paths {
                assert!(
                    !path.components().any(|part| part.as_os_str() == QT_APP_FOLDER),
                    "{} names the Qt folder",
                    path.display()
                );
                assert!(
                    path.components().any(|part| part.as_os_str() == APP_FOLDER),
                    "{} should stay under {}",
                    path.display(),
                    APP_FOLDER
                );
            }
        }
    }

    #[test]
    fn transcript_root_honours_override() {
        let env = Env {
            transcript_root: Some(PathBuf::from("transcripts")),
            home: Some(home()),
            temp_dir: PathBuf::from("tmp"),
            ..Env::default()
        };
        assert_eq!(
            transcript_root_for(&env, Platform::Linux),
            PathBuf::from("transcripts")
        );
    }

    #[test]
    fn transcript_root_defaults_under_state() {
        let env = env_with_home();
        assert_eq!(
            transcript_root_for(&env, Platform::Linux),
            home()
                .join(".local")
                .join("state")
                .join(APP_FOLDER)
                .join("logs")
        );
    }

    #[test]
    fn qt_linux_paths_use_the_installed_omairc_folder() {
        let env = env_with_home();
        assert_eq!(
            qt_settings_path_for(&env, Platform::Linux),
            Some(home().join(".config").join(QT_APP_FOLDER).join(SETTINGS_FILE))
        );
        assert_eq!(
            qt_transcript_root_for(&env, Platform::Linux),
            home()
                .join(".local")
                .join("state")
                .join(QT_APP_FOLDER)
                .join("logs")
        );
        assert_eq!(
            qt_sts_path_for(&env, Platform::Linux),
            Some(home().join(".config").join(QT_APP_FOLDER).join(STS_FILE))
        );
    }

    #[test]
    fn qt_macos_paths_use_library_preferences_ini_layout() {
        let env = env_with_home();
        let prefs = home().join("Library").join("Preferences");
        assert_eq!(
            qt_config_dir_for(&env, Platform::MacOs),
            Some(prefs.join(QT_APP_FOLDER))
        );
        assert_eq!(
            qt_settings_path_for(&env, Platform::MacOs),
            Some(prefs.join(QT_APP_FOLDER).join(SETTINGS_FILE_MACOS))
        );
        assert_eq!(
            qt_sts_path_for(&env, Platform::MacOs),
            Some(prefs.join(QT_APP_FOLDER).join(STS_FILE))
        );
        assert_eq!(
            qt_state_dir_for(&env, Platform::MacOs),
            prefs.join("State").join(QT_APP_FOLDER)
        );
    }

    #[test]
    fn qt_windows_settings_are_registry_not_ini() {
        let env = Env {
            local_app_data: Some(PathBuf::from("lad")),
            home: Some(home()),
            temp_dir: PathBuf::from("tmp"),
            ..Env::default()
        };
        assert_eq!(qt_settings_path_for(&env, Platform::Windows), None);
        assert_eq!(
            qt_sts_path_for(&env, Platform::Windows),
            Some(PathBuf::from("lad").join(QT_APP_FOLDER).join(STS_FILE))
        );
        assert_eq!(
            qt_state_dir_for(&env, Platform::Windows),
            PathBuf::from("lad").join("State").join(QT_APP_FOLDER)
        );
        assert_eq!(
            qt_transcript_root_for(&env, Platform::Windows),
            PathBuf::from("lad")
                .join("State")
                .join(QT_APP_FOLDER)
                .join("logs")
        );
    }

    #[test]
    fn qt_paths_ignore_dev_home_override() {
        let env = Env {
            dev_home: Some(PathBuf::from("scratch")),
            home: Some(home()),
            temp_dir: PathBuf::from("tmp"),
            ..Env::default()
        };
        assert_eq!(
            qt_settings_path_for(&env, Platform::Linux),
            Some(home().join(".config").join(QT_APP_FOLDER).join(SETTINGS_FILE))
        );
    }

    /// With `product`, macOS settings match the Qt `.ini` path for in-place takeover.
    #[cfg(not(feature = "dev"))]
    #[test]
    fn product_macos_settings_match_qt_ini() {
        let env = env_with_home();
        assert_eq!(
            settings_path_for(&env, Platform::MacOs),
            qt_settings_path_for(&env, Platform::MacOs)
        );
        assert_eq!(
            settings_path_for(&env, Platform::MacOs)
                .unwrap()
                .file_name()
                .and_then(|name| name.to_str()),
            Some(SETTINGS_FILE_MACOS)
        );
    }

    #[test]
    fn lookup_treats_empty_values_as_unset() {
        let env = Env::from_lookup(
            |name| match name {
                "XDG_CONFIG_HOME" => Some(OsString::new()),
                "LOCALAPPDATA" => Some(OsString::from("lad")),
                "OMAIRC_DEV_HOME" => Some(OsString::from("scratch")),
                _ => None,
            },
            Some(home()),
            PathBuf::from("tmp"),
        );
        assert_eq!(env.xdg_config_home, None);
        assert_eq!(env.local_app_data, Some(PathBuf::from("lad")));
        assert_eq!(env.dev_home, Some(PathBuf::from("scratch")));
        assert_eq!(env.xdg_state_home, None);
    }
}
