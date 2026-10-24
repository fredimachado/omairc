//! Omarchy `colors.toml` loading and live reload.
//!
//! The palette comes from Omarchy's `colors.toml`. Without that file, or for any
//! key the file leaves out, the system light or dark mode decides the base colours,
//! as `Backend::loadOmarchyTheme` does in the Qt client. A `mode` line, or a
//! background to measure, overrides the system mode.
//!
//! Shells subscribe to theme changes by calling [`ThemeWatcher::start`] once at
//! startup, then polling or blocking on [`ThemeWatcher::receiver`]:
//!
//! ```ignore
//! let mut watcher = ThemeWatcher::start(None)?;
//! let mut theme = Theme::load(None, system_dark)?;
//! loop {
//!     while watcher.receiver().try_recv().is_ok() {
//!         theme = Theme::load(None, system_dark)?;
//!     }
//!     // draw with `theme`
//! }
//! ```

use super::color::Color;
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub background: Color,
    pub foreground: Color,
    pub accent: Color,
    pub selection: Color,
    pub dark_mode: bool,
}

impl Theme {
    /// Loads the palette at `path`, or the Omarchy default path. `system_dark` is the
    /// system light or dark mode and sets the base colours when the file is absent or
    /// leaves them out.
    pub fn load(path: Option<&Path>, system_dark: bool) -> std::io::Result<Self> {
        let defaults = Self::defaults(system_dark);
        let Some(resolved) = resolve_path(path) else {
            return Ok(defaults);
        };
        let content = match std::fs::read_to_string(&resolved) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(defaults);
            }
            Err(e) => return Err(e),
        };
        Ok(parse_colors_toml(&content, defaults))
    }

    /// The base palette for a system light or dark mode.
    pub fn defaults(dark: bool) -> Self {
        if dark {
            Self::defaults_dark()
        } else {
            Self::defaults_light()
        }
    }

    pub fn defaults_dark() -> Self {
        Self {
            background: Color::from_hex("#101010").unwrap(),
            foreground: Color::from_hex("#eeeeee").unwrap(),
            accent: Color::from_hex("#5584aa").unwrap(),
            selection: Color::from_hex("#186a9a").unwrap(),
            dark_mode: true,
        }
    }

    pub fn defaults_light() -> Self {
        Self {
            background: Color::from_hex("#ffffff").unwrap(),
            foreground: Color::from_hex("#222324").unwrap(),
            accent: Color::from_hex("#2077b2").unwrap(),
            selection: Color::from_hex("#2077b2").unwrap(),
            dark_mode: false,
        }
    }
}

pub fn default_omarchy_colors_path() -> PathBuf {
    let home = dirs::home_dir().expect("home directory");
    home.join(".local/state/omarchy/current/theme/colors.toml")
}

fn resolve_path(path: Option<&Path>) -> Option<PathBuf> {
    path.map(Path::to_path_buf)
        .or_else(|| Some(default_omarchy_colors_path()))
}

pub fn parse_colors_toml(content: &str, mut base: Theme) -> Theme {
    let mut theme_mode = String::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let mut value = value.trim().to_string();
        if value.len() >= 2 {
            let bytes = value.as_bytes();
            if (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
                || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
            {
                value = value[1..value.len() - 1].to_string();
            }
        }
        match key {
            "mode" => theme_mode = value,
            "background" => {
                if let Some(c) = Color::from_hex(&value) {
                    base.background = c;
                }
            }
            "foreground" => {
                if let Some(c) = Color::from_hex(&value) {
                    base.foreground = c;
                }
            }
            "accent" => {
                if let Some(c) = Color::from_hex(&value) {
                    base.accent = c;
                }
            }
            "selection" => {
                if let Some(c) = Color::from_hex(&value) {
                    base.selection = c;
                }
            }
            _ => {}
        }
    }

    let (dark_mode, _) = resolve_mode(&theme_mode, base.background);
    base.dark_mode = dark_mode;
    base
}

fn resolve_mode(theme_mode: &str, background: Color) -> (bool, bool) {
    if theme_mode == "dark" {
        return (true, true);
    }
    if theme_mode == "light" {
        return (false, true);
    }
    if background.luminance() < 0.5 {
        (true, true)
    } else {
        (false, true)
    }
}

/// Paths to register with the filesystem watcher, matching Qt `Backend::watchOmarchyTheme`.
pub fn theme_watch_paths(override_path: Option<&Path>) -> Vec<PathBuf> {
    let home = dirs::home_dir().expect("home directory");
    theme_watch_paths_in_home(&home, override_path)
}

fn push_watch_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if path.is_dir() || path.is_file() {
        let resolved = path.canonicalize().unwrap_or(path);
        paths.push(resolved);
    }
}

pub fn theme_watch_paths_in_home(home: &Path, override_path: Option<&Path>) -> Vec<PathBuf> {
    if let Some(path) = override_path {
        let mut paths = Vec::new();
        push_watch_path(&mut paths, path.to_path_buf());
        if let Some(parent) = path.parent() {
            push_watch_path(&mut paths, parent.to_path_buf());
        }
        return paths;
    }

    let current = home.join(".local/state/omarchy/current");
    let theme_dir = current.join("theme");
    let colors = theme_dir.join("colors.toml");
    let mut paths = Vec::new();
    push_watch_path(&mut paths, current);
    push_watch_path(&mut paths, theme_dir);
    push_watch_path(&mut paths, colors);
    paths
}

fn register_watch_paths(watcher: &mut RecommendedWatcher, paths: &[PathBuf]) {
    for path in paths {
        if path.is_dir() || path.is_file() {
            let _ = watcher.watch(path, RecursiveMode::NonRecursive);
        }
    }
}

pub struct ThemeWatcher {
    watcher: RecommendedWatcher,
    receiver: Receiver<()>,
    watched: Vec<PathBuf>,
}

impl ThemeWatcher {
    pub fn start(override_path: Option<&Path>) -> notify::Result<Self> {
        let (tx, rx) = mpsc::channel();
        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                if let Ok(event) = res {
                    if matches!(
                        event.kind,
                        EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                    ) {
                        let _ = tx.send(());
                    }
                }
            },
            notify::Config::default().with_poll_interval(Duration::from_millis(500)),
        )?;

        let watched = theme_watch_paths(override_path);
        register_watch_paths(&mut watcher, &watched);

        Ok(Self {
            watcher,
            receiver: rx,
            watched,
        })
    }

    /// Re-register watches after the Omarchy `current` symlink or theme file moves.
    pub fn rearm(&mut self, override_path: Option<&Path>) -> notify::Result<()> {
        for path in &self.watched {
            let _ = self.watcher.unwatch(path);
        }
        self.watched = theme_watch_paths(override_path);
        register_watch_paths(&mut self.watcher, &self.watched);
        Ok(())
    }

    pub fn receiver(&self) -> &Receiver<()> {
        &self.receiver
    }

    #[cfg(test)]
    pub fn watched_paths(&self) -> &[PathBuf] {
        &self.watched
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_temp_toml_and_mode_dark() {
        let base = Theme::defaults_dark();
        let parsed = parse_colors_toml(
            "background = \"#101010\"\nforeground = \"#eeeeee\"\nmode = dark\n",
            base.clone(),
        );
        assert!(parsed.dark_mode);
        assert_eq!(parsed.background.to_hex(), "#101010");
    }

    #[test]
    fn mode_light() {
        let parsed = parse_colors_toml("mode = light\n", Theme::defaults_dark());
        assert!(!parsed.dark_mode);
        assert!(!Theme::defaults_light().dark_mode);
    }

    #[test]
    fn luminance_fallback_dark_background() {
        let parsed = parse_colors_toml("background = \"#101010\"\n", Theme::defaults_dark());
        assert!(parsed.dark_mode);
    }

    #[test]
    fn luminance_fallback_light_background() {
        let parsed = parse_colors_toml("background = \"#ffffff\"\n", Theme::defaults_dark());
        assert!(!parsed.dark_mode);
    }

    #[test]
    fn quoted_values_and_comments() {
        let text = "# comment\naccent = '#5584aa'\n";
        let parsed = parse_colors_toml(text, Theme::defaults_dark());
        assert_eq!(parsed.accent.to_hex(), "#5584aa");
    }

    #[test]
    fn missing_file_uses_system_mode_defaults() {
        let missing = Path::new("/nonexistent/omairc-theme-test.toml");
        let dark = Theme::load(Some(missing), true).unwrap();
        assert_eq!(dark.background.to_hex(), "#101010");
        assert!(dark.dark_mode);

        let light = Theme::load(Some(missing), false).unwrap();
        assert_eq!(light.background.to_hex(), "#ffffff");
        assert!(!light.dark_mode);
    }

    #[test]
    fn colors_file_mode_overrides_system_mode() {
        let dark = parse_colors_toml("mode = dark\n", Theme::defaults(false));
        assert!(dark.dark_mode);
        let light = parse_colors_toml("mode = light\n", Theme::defaults(true));
        assert!(!light.dark_mode);
        let by_background = parse_colors_toml("background = \"#101010\"\n", Theme::defaults(false));
        assert!(by_background.dark_mode);
    }

    #[test]
    fn partial_colors_file_takes_base_colours_from_system() {
        let light = parse_colors_toml("accent = \"#2077b2\"\n", Theme::defaults(false));
        assert!(!light.dark_mode);
        assert_eq!(light.background.to_hex(), "#ffffff");
        assert_eq!(light.accent.to_hex(), "#2077b2");

        let dark = parse_colors_toml("accent = \"#2077b2\"\n", Theme::defaults(true));
        assert!(dark.dark_mode);
        assert_eq!(dark.background.to_hex(), "#101010");
    }

    #[test]
    fn load_from_temp_file() {
        let dir = tempfile_dir();
        let path = dir.join("colors.toml");
        std::fs::write(
            &path,
            "background = \"#222222\"\nforeground = \"#dddddd\"\nmode = dark\n",
        )
        .unwrap();
        let theme = Theme::load(Some(&path), false).unwrap();
        assert_eq!(theme.background.to_hex(), "#222222");
        assert!(theme.dark_mode);
    }

    fn tempfile_dir() -> PathBuf {
        let path = std::env::temp_dir().join(format!("omairc-theme-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn consecutive_theme_switch_updates_watch_paths() {
        let base = tempfile_dir();
        let home = base.join("home");
        let omarchy = home.join(".local/state/omarchy");
        let bundle_a = omarchy.join("bundles/a");
        let bundle_b = omarchy.join("bundles/b");
        let theme_a = bundle_a.join("theme");
        let theme_b = bundle_b.join("theme");
        std::fs::create_dir_all(&theme_a).unwrap();
        std::fs::create_dir_all(&theme_b).unwrap();
        std::fs::write(theme_a.join("colors.toml"), "mode = dark\n").unwrap();
        std::fs::write(theme_b.join("colors.toml"), "mode = light\n").unwrap();

        let current = omarchy.join("current");
        std::os::unix::fs::symlink(&bundle_a, &current).unwrap();

        let colors_a = theme_a.join("colors.toml").canonicalize().unwrap();
        let colors_b = theme_b.join("colors.toml").canonicalize().unwrap();

        let paths_a = theme_watch_paths_in_home(&home, None);
        assert!(paths_a.contains(&colors_a));
        assert!(!paths_a.contains(&colors_b));

        std::fs::remove_file(&current).unwrap();
        std::os::unix::fs::symlink(&bundle_b, &current).unwrap();

        let paths_b = theme_watch_paths_in_home(&home, None);
        assert!(paths_b.contains(&colors_b));
        assert!(!paths_b.contains(&colors_a));

        let prev_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", home.as_os_str());

        std::fs::remove_file(&current).unwrap();
        std::os::unix::fs::symlink(&bundle_a, &current).unwrap();

        let mut watcher = ThemeWatcher::start(None).expect("theme watcher");
        assert!(watcher.watched_paths().contains(&colors_a));

        std::fs::remove_file(&current).unwrap();
        std::os::unix::fs::symlink(&bundle_b, &current).unwrap();
        watcher.rearm(None).expect("rearm after first switch");
        assert!(
            watcher.watched_paths().contains(&colors_b),
            "second theme file must be watched after rearm: {:?}",
            watcher.watched_paths()
        );
        assert!(!watcher.watched_paths().contains(&colors_a));

        std::fs::remove_file(&current).unwrap();
        std::os::unix::fs::symlink(&bundle_a, &current).unwrap();
        watcher.rearm(None).expect("rearm after second switch");
        assert!(watcher.watched_paths().contains(&colors_a));
        assert!(!watcher.watched_paths().contains(&colors_b));

        if let Some(prev) = prev_home {
            std::env::set_var("HOME", prev);
        }
    }
}
