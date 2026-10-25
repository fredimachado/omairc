//! Whether the live window is shown, minimized, or hidden.
//!
//! gpui 0.2.2 exposes no `is_minimized()` and `WindowBounds` has no minimized
//! variant, so each platform reads the native handle (or reports Visible when
//! the compositor cannot say). Qt's `Window.Minimized` / `Window.Hidden` both
//! count as away for unread and mention notify (`OmaircWindow.qml`
//! `transcriptSuspended`).
//!
//! Wayland has no minimize state clients can read, so this module always
//! reports [`WindowVisibility::Visible`] there. Hyprland special workspaces
//! are not observable either.

use gpui::Window;

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

/// Native show state. Minimized and Hidden both suspend catch-up.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WindowVisibility {
    Visible,
    Minimized,
    Hidden,
}

impl WindowVisibility {
    /// Qt treats Minimized and Hidden as `transcriptSuspended`.
    pub const fn is_suspended(self) -> bool {
        matches!(self, Self::Minimized | Self::Hidden)
    }
}

/// Focus plus minimize/hide. Effective activity is `window_active && !suspended`
/// (Qt `arrivalWindowActive`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranscriptActivity {
    /// OS focus (`Window.active`). Can stay true while minimized.
    pub window_active: bool,
    /// Minimized or hidden (Qt `transcriptSuspended`).
    pub suspended: bool,
}

impl Default for TranscriptActivity {
    fn default() -> Self {
        Self {
            window_active: true,
            suspended: false,
        }
    }
}

impl TranscriptActivity {
    /// Activity for unread consumption and mention notify.
    pub const fn effective(self) -> bool {
        self.window_active && !self.suspended
    }

    /// Qt `windowFocusGained` / `windowFocusLost`. A focus report while
    /// suspended returns `None` so the controller stays away.
    pub fn set_focus(&mut self, active: bool) -> Option<bool> {
        self.window_active = active;
        if active && self.suspended {
            return None;
        }
        Some(self.effective())
    }

    /// Qt `onVisibilityChanged`. Entering suspension forces away. Leaving it
    /// re-applies focus only when the window is actually active
    /// (`finishTranscriptSuspension`).
    pub fn set_visibility(&mut self, visibility: WindowVisibility) -> Option<bool> {
        if visibility.is_suspended() {
            self.suspended = true;
            return Some(false);
        }
        if !self.suspended {
            return None;
        }
        self.suspended = false;
        Some(self.effective())
    }
}

/// Reads the platform show state. gpui has no minimize flag, so this uses the
/// raw window handle. Query failure, Wayland, and gpui's TestWindow (which
/// panics on `HasWindowHandle`) all report Visible.
pub fn of(window: &Window) -> WindowVisibility {
    // TestWindow's HasWindowHandle is `unimplemented!`, not an Err.
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        platform_visibility(window)
    })) {
        Ok(visibility) => visibility,
        Err(_) => WindowVisibility::Visible,
    }
}

#[cfg(windows)]
fn platform_visibility(window: &Window) -> WindowVisibility {
    windows::visibility(window)
}

#[cfg(target_os = "macos")]
fn platform_visibility(window: &Window) -> WindowVisibility {
    macos::visibility(window)
}

#[cfg(target_os = "linux")]
fn platform_visibility(window: &Window) -> WindowVisibility {
    linux::visibility(window)
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn platform_visibility(_window: &Window) -> WindowVisibility {
    WindowVisibility::Visible
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_activity_is_active_and_not_suspended() {
        let mut activity = TranscriptActivity::default();
        assert!(activity.effective());
        activity.suspended = true;
        assert!(!activity.effective());
        activity.window_active = false;
        assert!(!activity.effective());
        activity.suspended = false;
        assert!(!activity.effective());
    }

    #[test]
    fn focus_while_suspended_does_not_apply() {
        let mut activity = TranscriptActivity {
            window_active: true,
            suspended: true,
        };
        assert_eq!(activity.set_focus(true), None);
        assert!(activity.window_active);
        assert!(activity.suspended);
        assert_eq!(activity.set_focus(false), Some(false));
        assert!(!activity.window_active);
    }

    #[test]
    fn restore_while_inactive_keeps_catch_up_off() {
        let mut activity = TranscriptActivity {
            window_active: false,
            suspended: true,
        };
        assert_eq!(
            activity.set_visibility(WindowVisibility::Visible),
            Some(false)
        );
        assert!(!activity.suspended);
        assert!(!activity.effective());
    }

    #[test]
    fn restore_while_active_applies_focus() {
        let mut activity = TranscriptActivity {
            window_active: true,
            suspended: true,
        };
        assert_eq!(
            activity.set_visibility(WindowVisibility::Visible),
            Some(true)
        );
        assert!(!activity.suspended);
        assert!(activity.effective());
    }

    #[test]
    fn minimize_and_hide_both_suspend() {
        let mut activity = TranscriptActivity::default();
        assert_eq!(
            activity.set_visibility(WindowVisibility::Minimized),
            Some(false)
        );
        assert!(activity.suspended);
        activity.suspended = false;
        activity.window_active = true;
        assert_eq!(
            activity.set_visibility(WindowVisibility::Hidden),
            Some(false)
        );
        assert!(activity.suspended);
    }
}
