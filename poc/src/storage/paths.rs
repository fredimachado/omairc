//! Port of `tui/internal/storage/paths*.go` (Go) and `src/omaircpaths.cpp` (C++),
//! under the PoC's own `omairc-poc` folder. The Qt client and the TUI write to
//! `omairc/`, so nothing the PoC writes can land in a real install.
//!
//! The roots follow `$XDG_CONFIG_HOME` and `$XDG_STATE_HOME` when set. Otherwise
//! they are the platform's generic locations: `%LOCALAPPDATA%` on Windows,
//! `~/Library/Preferences` (state under `State/`) on macOS, and `~/.config` and
//! `~/.local/state` elsewhere. `OMAIRC_POC_HOME` moves both roots under one
//! directory, so tests and manual runs can stay out of the user's profile.

use std::ffi::OsString;
use std::path::PathBuf;

/// The folder the PoC owns under each root.
pub const APP_FOLDER: &str = "omairc-poc";
/// The QSettings-compatible INI file, the same name the Qt client uses.
pub const SETTINGS_FILE: &str = "omairc.conf";
/// The application log, the same name the Qt client uses.
pub const LOG_FILE: &str = "omairc.log";
/// Moves the config and state roots under one directory.
pub const HOME_OVERRIDE_VAR: &str = "OMAIRC_POC_HOME";

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
    pub poc_home: Option<PathBuf>,
    /// Where the log goes when no state root is known, as Qt's `TempLocation`.
    pub temp_dir: PathBuf,
}

impl Env {
    /// Reads the live process environment.
    pub fn from_process() -> Self {
        Self::from_lookup(
            |name| std::env::var_os(name),
            dirs::home_dir(),
            std::env::temp_dir(),
        )
    }

    /// Builds the inputs from any variable lookup. Empty values count as unset,
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
            poc_home: path(HOME_OVERRIDE_VAR),
            temp_dir,
        }
    }
}

/// The config folder (`<config root>/omairc-poc`), or `None` when the platform
/// config root is unknown.
pub fn config_dir_for(env: &Env, platform: Platform) -> Option<PathBuf> {
    if let Some(home) = &env.poc_home {
        return Some(home.join("config"));
    }
    config_root(env, platform).map(|root| root.join(APP_FOLDER))
}

/// The state folder (`<state root>/omairc-poc`). Falls back to the temp dir when
/// the platform state root is unknown, as the Qt log does.
pub fn state_dir_for(env: &Env, platform: Platform) -> PathBuf {
    if let Some(home) = &env.poc_home {
        return home.join("state");
    }
    state_root(env, platform)
        .unwrap_or_else(|| env.temp_dir.clone())
        .join(APP_FOLDER)
}

pub fn settings_path_for(env: &Env, platform: Platform) -> Option<PathBuf> {
    config_dir_for(env, platform).map(|dir| dir.join(SETTINGS_FILE))
}

pub fn log_path_for(env: &Env, platform: Platform) -> PathBuf {
    state_dir_for(env, platform).join(LOG_FILE)
}

/// The INI settings file on this machine, or `None` when there is no config root.
pub fn settings_path() -> Option<PathBuf> {
    settings_path_for(&Env::from_process(), Platform::current())
}

/// The application log on this machine.
pub fn log_path() -> PathBuf {
    log_path_for(&Env::from_process(), Platform::current())
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
        Platform::Windows => env.local_app_data.clone(),
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
    fn windows_uses_local_app_data_for_both_roots() {
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
            PathBuf::from("lad").join(APP_FOLDER).join(LOG_FILE)
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
            state_dir_for(&env, Platform::MacOs),
            prefs.join("State").join(APP_FOLDER)
        );
    }

    #[test]
    fn poc_home_moves_both_roots_under_one_directory() {
        let env = Env {
            poc_home: Some(PathBuf::from("scratch")),
            xdg_config_home: Some(PathBuf::from("cfg")),
            local_app_data: Some(PathBuf::from("lad")),
            home: Some(home()),
            temp_dir: PathBuf::from("tmp"),
            ..Env::default()
        };
        for platform in [Platform::Windows, Platform::MacOs, Platform::Linux] {
            assert_eq!(
                settings_path_for(&env, platform),
                Some(PathBuf::from("scratch").join("config").join(SETTINGS_FILE))
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
            ];
            for path in paths {
                assert!(
                    !path.components().any(|part| part.as_os_str() == "omairc"),
                    "{} names the Qt folder",
                    path.display()
                );
                assert!(path.components().any(|part| part.as_os_str() == APP_FOLDER));
            }
        }
    }

    #[test]
    fn lookup_treats_empty_values_as_unset() {
        let env = Env::from_lookup(
            |name| match name {
                "XDG_CONFIG_HOME" => Some(OsString::new()),
                "LOCALAPPDATA" => Some(OsString::from("lad")),
                "OMAIRC_POC_HOME" => Some(OsString::from("scratch")),
                _ => None,
            },
            Some(home()),
            PathBuf::from("tmp"),
        );
        assert_eq!(env.xdg_config_home, None);
        assert_eq!(env.local_app_data, Some(PathBuf::from("lad")));
        assert_eq!(env.poc_home, Some(PathBuf::from("scratch")));
        assert_eq!(env.xdg_state_home, None);
    }
}
