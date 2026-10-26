//! Whether the live window is shown, minimized, or hidden.
//! gpui 0.2.2 exposes no `is_minimized` and `WindowBounds` has no minimized
//! variant, so each platform reads the native handle (or reports Visible when
//! the compositor cannot say). Qt's `Window.Minimized` / `Window.Hidden` both
//! count as away for unread and mention notify (`client/src/ui/shell`
//! `transcriptSuspended`).
//! Minimize often does not fire `observe_window_bounds` while OS focus stays
//! true, so the shell refreshes [`of`] on activation (visibility before focus)
//! and polls while reportedly active or already suspended. Bounds remain a
//! fast path only.
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
/// .
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TranscriptActivity {
    /// OS focus (`Window.active`). Can stay true while minimized.
    pub window_active: bool,
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

/// How often the shell re-reads native minimize/hide while focus stays true
/// (or while already suspended, so restore clears the flag without a bounds
/// event). Bounds observers alone miss that transition on every platform.
pub const VISIBILITY_POLL: std::time::Duration = std::time::Duration::from_millis(250);

/// Focus plus show-state inputs for one watch tick. Unit tests use a fake
/// the shell reads the live window.
pub trait ActivityProbe {
    fn focus_active(&self) -> bool;
    fn visibility(&self) -> WindowVisibility;
}

/// Whether the short visibility poll should run. Minimize while focused and
/// restore while suspended both need a sample that is not bounds-driven.
pub const fn should_poll_visibility(activity: TranscriptActivity) -> bool {
    activity.window_active || activity.suspended
}

/// Activation sample: refresh visibility before focus so FocusIn-while-iconic
/// cannot consume unread. Returns the last effective change, if any.
pub fn apply_activation_sample(
    activity: &mut TranscriptActivity,
    probe: &impl ActivityProbe,
) -> Option<bool> {
    let after_visibility = activity.set_visibility(probe.visibility());
    match activity.set_focus(probe.focus_active()) {
        Some(effective) => Some(effective),
        None => after_visibility,
    }
}

/// Visibility-only sample used by the poll and bounds paths.
pub fn apply_visibility_sample(
    activity: &mut TranscriptActivity,
    probe: &impl ActivityProbe,
) -> Option<bool> {
    activity.set_visibility(probe.visibility())
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

    #[derive(Clone, Copy)]
    struct FakeProbe {
        focus: bool,
        visibility: WindowVisibility,
    }

    impl ActivityProbe for FakeProbe {
        fn focus_active(&self) -> bool {
            self.focus
        }

        fn visibility(&self) -> WindowVisibility {
            self.visibility
        }
    }

    #[test]
    fn activation_refreshes_visibility_before_focus_can_consume() {
        // FocusIn while iconic: OS focus is true and the window is minimized.
        // Visibility must land first or set_focus would briefly consume.
        let mut activity = TranscriptActivity {
            window_active: false,
            suspended: false,
        };
        let probe = FakeProbe {
            focus: true,
            visibility: WindowVisibility::Minimized,
        };
        assert_eq!(apply_activation_sample(&mut activity, &probe), Some(false));
        assert!(activity.window_active);
        assert!(activity.suspended);
        assert!(!activity.effective());
    }

    #[test]
    fn focus_before_visibility_would_consume_while_iconic() {
        // Documents why apply_activation_sample orders visibility first.
        let mut activity = TranscriptActivity {
            window_active: false,
            suspended: false,
        };
        assert_eq!(activity.set_focus(true), Some(true));
        assert!(activity.effective(), "focus-first wrongly turns catch-up on");
        assert_eq!(
            activity.set_visibility(WindowVisibility::Minimized),
            Some(false)
        );
    }

    #[test]
    fn poll_observes_minimize_while_os_focus_stays_true() {
        // The live bug: minimize with focus still true and no bounds event.
        // A bounds-only watch never calls set_visibility; the poll must.
        let mut activity = TranscriptActivity::default();
        let mut probe = FakeProbe {
            focus: true,
            visibility: WindowVisibility::Visible,
        };
        assert!(activity.effective());
        assert!(should_poll_visibility(activity));

        probe.visibility = WindowVisibility::Minimized;
        // No activation change (focus stays true). Only the visibility sample.
        assert_eq!(
            apply_visibility_sample(&mut activity, &probe),
            Some(false),
            "minimize must be observed without a focus or bounds event"
        );
        assert!(activity.window_active);
        assert!(activity.suspended);
        assert!(!activity.effective());
        // A later FocusIn-while-iconic must refresh visibility first and not consume.
        assert_eq!(apply_activation_sample(&mut activity, &probe), Some(false));
        assert_eq!(activity.set_focus(true), None);
        assert!(!activity.effective());
    }

    #[test]
    fn poll_clears_suspension_on_restore_while_inactive() {
        let mut activity = TranscriptActivity {
            window_active: false,
            suspended: true,
        };
        assert!(should_poll_visibility(activity));
        let probe = FakeProbe {
            focus: false,
            visibility: WindowVisibility::Visible,
        };
        assert_eq!(
            apply_visibility_sample(&mut activity, &probe),
            Some(false)
        );
        assert!(!activity.suspended);
        assert!(!activity.effective());
    }

    #[test]
    fn idle_unfocused_visible_skips_visibility_poll() {
        let activity = TranscriptActivity {
            window_active: false,
            suspended: false,
        };
        assert!(!should_poll_visibility(activity));
    }
}
