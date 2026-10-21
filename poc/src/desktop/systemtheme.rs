//! System light or dark mode. Port of `SystemTheme` (`src/systemtheme.cpp`). The Qt
//! client reads the color scheme hint and the portal. gpui reports the appearance of
//! each window, so the PoC asks the window and follows `observe_window_appearance`.
//! The value drives the base palette when there is no Omarchy `colors.toml`, as
//! `Backend::loadOmarchyTheme` does.

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
