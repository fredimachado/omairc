//! Native macOS application menu (Omairc + Edit).
//!
//! Compiled and wired only on `target_os = "macos"` so system chords and
//! Services work.

use gpui::{App, Menu, MenuItem, OsAction, SystemMenuType};

use crate::ui::input::{Copy, Cut, Paste, SelectAll};
use crate::ui::shell::actions::{OpenAbout, Quit};

/// Register the Omairc and Edit menus. Call after `bind_input_keys` /
/// `bind_shell_keys` so Quit can pick up Cmd+Q from the keymap.
pub fn register_menus(cx: &mut App) {
    cx.set_menus(vec![
        Menu {
            name: "Omairc".into(),
            items: vec![
                MenuItem::action("About", OpenAbout),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit", Quit),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::os_action("Cut", Cut, OsAction::Cut),
                MenuItem::os_action("Copy", Copy, OsAction::Copy),
                MenuItem::os_action("Paste", Paste, OsAction::Paste),
                MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
            ],
        },
    ]);
}
