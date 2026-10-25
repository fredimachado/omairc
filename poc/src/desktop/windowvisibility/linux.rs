//! Linux show state: X11 `_NET_WM_STATE_HIDDEN`. Wayland always reports Visible.

use std::sync::OnceLock;

use gpui::Window;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, Window as XWindow};
use x11rb::rust_connection::RustConnection;

use super::WindowVisibility;

/// Wayland exposes no minimize/hide state to clients. Hyprland special
/// workspaces are not observable either, so arrivals look focused there.
pub(super) fn visibility(window: &Window) -> WindowVisibility {
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return WindowVisibility::Visible;
    };
    match handle.as_raw() {
        RawWindowHandle::Wayland(_) => WindowVisibility::Visible,
        RawWindowHandle::Xlib(xlib) => x11_visibility(xlib.window as XWindow),
        RawWindowHandle::Xcb(xcb) => x11_visibility(xcb.window),
        _ => WindowVisibility::Visible,
    }
}

fn x11_visibility(window: XWindow) -> WindowVisibility {
    let Some(conn) = x11_connection() else {
        return WindowVisibility::Visible;
    };
    match net_wm_state_hidden(conn, window) {
        Ok(true) => WindowVisibility::Minimized,
        Ok(false) | Err(_) => WindowVisibility::Visible,
    }
}

fn x11_connection() -> Option<&'static RustConnection> {
    static CONN: OnceLock<Option<RustConnection>> = OnceLock::new();
    CONN.get_or_init(|| RustConnection::connect(None).ok().map(|(conn, _)| conn))
        .as_ref()
}

fn net_wm_state_hidden(conn: &RustConnection, window: XWindow) -> Result<bool, x11rb::errors::ReplyError> {
    let net_wm_state = conn
        .intern_atom(false, b"_NET_WM_STATE")?
        .reply()?
        .atom;
    let hidden = conn
        .intern_atom(false, b"_NET_WM_STATE_HIDDEN")?
        .reply()?
        .atom;
    let prop = conn
        .get_property(false, window, net_wm_state, AtomEnum::ATOM, 0, 64)?
        .reply()?;
    let Some(values) = prop.value32() else {
        return Ok(false);
    };
    Ok(values.any(|atom| atom == hidden))
}
