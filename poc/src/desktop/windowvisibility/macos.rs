//! macOS show state: `NSWindow.isMiniaturized` and `isVisible`.

use std::ptr::NonNull;

use gpui::Window;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSView, NSWindow};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use super::WindowVisibility;

pub(super) fn visibility(window: &Window) -> WindowVisibility {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return WindowVisibility::Visible;
    };
    let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
        return WindowVisibility::Visible;
    };
    let Some(ns_window) = ns_window_from_view(appkit.ns_view) else {
        return WindowVisibility::Hidden;
    };
    if ns_window.isMiniaturized() {
        return WindowVisibility::Minimized;
    }
    if !ns_window.isVisible() {
        return WindowVisibility::Hidden;
    }
    WindowVisibility::Visible
}

fn ns_window_from_view(ns_view: NonNull<core::ffi::c_void>) -> Option<objc2::rc::Retained<NSWindow>> {
    // SAFETY: ns_view is the live gpui NSView for this window.
    unsafe {
        let view = ns_view.as_ptr() as *mut AnyObject;
        let view = view.as_ref()?;
        let view = &*(view as *const AnyObject as *const NSView);
        view.window()
    }
}
