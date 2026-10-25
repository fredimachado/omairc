//! The Shortcuts sheet's grouped table, ported from `src/qml/ShortcutsSheet.qml`,
//! and its label rules: the Windows alias suffix (`keys / winKeys`) and the
//! macOS relabel from `OmaircStyle.shortcutKeys` (Ctrl to Cmd, Alt to Option).

/// The host the sheet is labelled for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Windows,
    MacOs,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortcutRow {
    pub keys: &'static str,
    /// Extra Windows Terminal aliases shown after the primary chord.
    pub win_keys: Option<&'static str>,
    pub action: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortcutGroup {
    pub title: &'static str,
    pub rows: &'static [ShortcutRow],
}

const fn row(keys: &'static str, action: &'static str) -> ShortcutRow {
    ShortcutRow {
        keys,
        win_keys: None,
        action,
    }
}

const fn row_win(keys: &'static str, win_keys: &'static str, action: &'static str) -> ShortcutRow {
    ShortcutRow {
        keys,
        win_keys: Some(win_keys),
        action,
    }
}

pub const GROUPS: &[ShortcutGroup] = &[
    ShortcutGroup {
        title: "MOVE",
        rows: &[
            row("Alt+Down / Alt+Up", "walk conversations"),
            row("Alt+Left / Alt+Right", "walk networks"),
            row("Alt+Shift+Left / Right", "collapse / expand network"),
            row("Ctrl+Alt+Shift+Left / Right", "collapse / expand all"),
            row("Alt+Shift+Up / Down", "move network"),
            row("Alt+A", "next unread"),
            row("Alt+Shift+A", "mark all read"),
        ],
    },
    ShortcutGroup {
        title: "JUMP",
        rows: &[
            row("Ctrl+K", "jump to conversation"),
            row("Ctrl+Shift+O", "open link"),
            row("Ctrl+Shift+K", "jump to nick"),
            row("Ctrl+Shift+A", "inbox"),
            row("Ctrl+`", "Status"),
            row("Ctrl+W", "close conversation"),
        ],
    },
    ShortcutGroup {
        title: "WRITE",
        rows: &[
            row("Ctrl+L", "composer"),
            row("Ctrl+Shift+U", "insert a file link"),
            row("Ctrl+C", "copy selection"),
            row("Ctrl+F", "find"),
            row("Enter", "send"),
            row("Page Up / Page Down", "scroll transcript"),
            row(
                "Shift+Page Up / Shift+Page Down",
                "scroll transcript half page",
            ),
            row("Ctrl+Home / Ctrl+End", "top / bottom"),
            row("Tab / Shift+Tab", "complete nick or channel"),
            row("Up / Down", "history"),
            row("Escape", "dismiss"),
        ],
    },
    ShortcutGroup {
        title: "CONNECT",
        rows: &[
            row_win("Ctrl+,", "Ctrl+]", "Connect"),
            row_win("Ctrl+Tab", "Ctrl+PgDn", "Connect tabs"),
            row("Ctrl+N", "add network"),
            row("Ctrl+Shift+Delete", "remove network"),
            row("Ctrl+Enter", "apply selected network"),
        ],
    },
    ShortcutGroup {
        title: "WINDOW",
        rows: &[
            row("Ctrl+Shift+S", "server list"),
            row("Ctrl+Shift+M", "members panel"),
            row("Ctrl+Shift+P", "focus members"),
            row("Page Up / Page Down", "page focused members"),
            row(
                "Shift+Page Up / Shift+Page Down",
                "page focused members half page",
            ),
            row("Home / End", "first / last focused nick"),
            row("Ctrl+/", "this sheet"),
            row("Ctrl+Shift+/", "About"),
            row("Ctrl+Q", "quit"),
        ],
    },
];

/// The key column text for one row. Qt `chordLabel`: on Windows the alias is
/// appended after " / " (only when the row has one), then the platform
/// relabel applies.
pub fn chord_label(row: &ShortcutRow, platform: Platform) -> String {
    let label = match (platform, row.win_keys) {
        (Platform::Windows, Some(alias)) => format!("{} / {}", row.keys, alias),
        _ => row.keys.to_string(),
    };
    shortcut_keys(&label, platform)
}

/// Qt `OmaircStyle.shortcutKeys`: only macOS relabels, Ctrl to Cmd and Alt to
/// Option. Every other platform shows the chord as written.
pub fn shortcut_keys(label: &str, platform: Platform) -> String {
    if platform != Platform::MacOs {
        return label.to_string();
    }
    label.replace("Ctrl", "Cmd").replace("Alt", "Option")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find_row(keys: &str) -> &'static ShortcutRow {
        GROUPS
            .iter()
            .flat_map(|group| group.rows.iter())
            .find(|row| row.keys == keys)
            .unwrap_or_else(|| panic!("no row with keys {keys}"))
    }

    #[test]
    fn groups_match_the_qt_sheet_order_and_sizes() {
        let summary: Vec<(&str, usize)> = GROUPS
            .iter()
            .map(|group| (group.title, group.rows.len()))
            .collect();
        assert_eq!(
            summary,
            [
                ("MOVE", 7),
                ("JUMP", 6),
                ("WRITE", 11),
                ("CONNECT", 5),
                ("WINDOW", 9)
            ]
        );
    }

    #[test]
    fn linux_shows_the_primary_chord_only() {
        let walk = find_row("Alt+Down / Alt+Up");
        assert_eq!(chord_label(walk, Platform::Other), "Alt+Down / Alt+Up");
    }

    #[test]
    fn windows_appends_aliases_only_for_chords_the_binary_binds() {
        let connect = find_row("Ctrl+,");
        assert_eq!(
            chord_label(connect, Platform::Windows),
            "Ctrl+, / Ctrl+]"
        );
        let tabs = find_row("Ctrl+Tab");
        assert_eq!(
            chord_label(tabs, Platform::Windows),
            "Ctrl+Tab / Ctrl+PgDn"
        );
        // MOVE chords have no Windows-only bindings in the PoC binary.
        let walk = find_row("Alt+Down / Alt+Up");
        assert_eq!(chord_label(walk, Platform::Windows), "Alt+Down / Alt+Up");
        let quit = find_row("Ctrl+Q");
        assert_eq!(chord_label(quit, Platform::Windows), "Ctrl+Q");
    }

    #[test]
    fn macos_relabels_ctrl_and_alt() {
        let quit = find_row("Ctrl+Q");
        assert_eq!(chord_label(quit, Platform::MacOs), "Cmd+Q");
        let collapse = find_row("Ctrl+Alt+Shift+Left / Right");
        assert_eq!(
            chord_label(collapse, Platform::MacOs),
            "Cmd+Option+Shift+Left / Right"
        );
        assert_eq!(
            shortcut_keys("Alt+Shift+A", Platform::MacOs),
            "Option+Shift+A"
        );
    }

    #[test]
    fn relabel_is_not_applied_off_macos() {
        assert_eq!(shortcut_keys("Ctrl+Alt+K", Platform::Windows), "Ctrl+Alt+K");
        assert_eq!(shortcut_keys("Ctrl+Alt+K", Platform::Other), "Ctrl+Alt+K");
    }

    #[test]
    fn the_sheet_lists_the_in_scope_chords() {
        for keys in [
            "Ctrl+K",
            "Ctrl+Shift+O",
            "Ctrl+Shift+K",
            "Ctrl+Shift+A",
            "Ctrl+/",
            "Ctrl+Shift+/",
            "Ctrl+Shift+M",
            "Ctrl+Shift+P",
            "Ctrl+Shift+S",
            "Ctrl+W",
            "Ctrl+L",
            "Alt+A",
            "Alt+Shift+A",
            "Ctrl+Q",
        ] {
            find_row(keys);
        }
    }
}
