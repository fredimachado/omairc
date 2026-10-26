//! System light or dark mode. Port of `SystemTheme` (`systemtheme`).
//!
//! On Linux, GPUI already subscribes to the XDG Desktop Portal
//! `org.freedesktop.appearance` / `color-scheme` setting (via ashpd in
//! `gpui::platform::linux::xdg_desktop_portal`) and pushes updates into
//! `WindowAppearance`. This crate does **not** open a second portal
//! subscription next to [`crate::desktop::textscale`]: that would race GPUI.
//! The app asks `cx.window_appearance()` at open and follows
//! `observe_window_appearance` so Omarchy `colors.toml` base colours flip
//! without a restart, as Qt's portal `SettingChanged` path does.
//!
//! Hyprland / live portal flip is a host check (needs a session bus and a
//! portal). CI Xvfb points `DBUS_SESSION_BUS_ADDRESS` at a missing socket, so
//! that path is not proven there - only the GPUI wiring and this global.

use gpui::{Global, WindowAppearance};

/// The system light or dark mode, as a global the theme loader reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SystemAppearance {
    pub dark: bool,
}

impl Default for SystemAppearance {
    /// `SystemTheme` starts dark until the system says otherwise.
    fn default() -> Self {
        Self { dark: true }
    }
}

impl SystemAppearance {
    pub fn from_window(appearance: WindowAppearance) -> Self {
        Self {
            dark: is_dark(appearance),
        }
    }
}

impl Global for SystemAppearance {}

/// Dark for the dark appearances, including the vibrant dark variant. Light for the
/// rest.
pub fn is_dark(appearance: WindowAppearance) -> bool {
    matches!(
        appearance,
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_appearances_are_dark() {
        assert!(is_dark(WindowAppearance::Dark));
        assert!(is_dark(WindowAppearance::VibrantDark));
        assert!(!is_dark(WindowAppearance::Light));
        assert!(!is_dark(WindowAppearance::VibrantLight));
    }

    #[test]
    fn starts_dark_like_qt() {
        assert!(SystemAppearance::default().dark);
        assert!(!SystemAppearance::from_window(WindowAppearance::Light).dark);
    }
}
