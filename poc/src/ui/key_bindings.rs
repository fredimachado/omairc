//! Shared key chords that Qt binds in more than one place. Tests here fail if a
//! chord is dropped from the tables in `bind_input_keys` and the sheet binders.

use gpui::{Action, KeyBinding};

/// Clipboard paste, including Shift+Insert (`OmaircWindow.qml` `isPasteChord`).
pub const PASTE_KEYS: &[&str] = &["secondary-v", "shift-insert"];

/// Return and the keypad's Enter (`KP_Enter`), which Qt wires separately from Return.
pub const RETURN_KEYS: &[&str] = &["enter", "numpadenter"];

/// Ctrl+Return / Ctrl+Enter on the Connect sheet.
pub const SECONDARY_RETURN_KEYS: &[&str] = &["secondary-enter", "secondary-numpadenter"];

/// Binds every Return alias to the same action.
pub fn bind_return<A: Action + Clone + 'static>(
    keys: &[&str],
    action: A,
    context: Option<&'static str>,
) -> Vec<KeyBinding> {
    keys
        .iter()
        .map(|key| KeyBinding::new(*key, action.clone(), context))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_chords_include_shift_insert() {
        assert!(
            PASTE_KEYS.contains(&"shift-insert"),
            "Shift+Insert must paste like Ctrl/Cmd+V"
        );
    }

    #[test]
    fn return_chords_include_keypad_enter() {
        assert!(
            RETURN_KEYS.contains(&"numpadenter"),
            "keypad Enter must match Return"
        );
        assert!(
            SECONDARY_RETURN_KEYS.contains(&"secondary-numpadenter"),
            "Ctrl/Cmd+keypad Enter must match Ctrl/Cmd+Return"
        );
    }
}
