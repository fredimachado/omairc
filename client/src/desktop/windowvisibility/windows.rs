//! Windows show state: `IsIconic` for minimize, `IsWindowVisible` for hide.

use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindowVisible};

use super::WindowVisibility;

pub(super) fn visibility(window: &Window) -> WindowVisibility {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return WindowVisibility::Visible;
    };
    let RawWindowHandle::Win32(win32) = handle.as_raw() else {
        return WindowVisibility::Visible;
    };
    let hwnd = HWND(win32.hwnd.get() as *mut _);
    // SAFETY: hwnd is the live gpui window; both calls are read-only.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            return WindowVisibility::Minimized;
        }
        if !IsWindowVisible(hwnd).as_bool() {
            return WindowVisibility::Hidden;
        }
    }
    WindowVisibility::Visible
}
