//! The Connect sheet's shell side: opening and closing the layer, the chords it
//! answers, and what Apply and Remove change in the rest of the window. The
//! sheet UI calls these methods, and the model lives in `model/connect.rs`.
//!
//! Connect is a window-level layer, not a `Sheet`. It sits under Shortcuts and
//! About, which open above it. It never connects: Apply writes the name and nick
//! into the mock world and closes the sheet.

use gpui::{Context, Window};

use crate::model::chords::Chord;
use crate::model::connect::RemoveOutcome;
use crate::model::prefs::{PrefName, PrefValue};
use crate::model::sidebar::sidebar_rows;
use crate::ui::shell::Shell;

impl Shell {
    /// Ctrl+, and Ctrl+] (Windows): opens Connect on the focused network, or the
    /// network whose Status is showing, or the selected one (Qt `openConnectSheet`).
    /// While Connect is open it selects that network again, as Qt does.
    pub fn open_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::Connect, window, cx) {
            return;
        }
        let network_id = self
            .sidebar
            .focus
            .clone()
            .or_else(|| self.status.clone())
            .unwrap_or_else(|| self.selected.network_id.clone());
        self.present_connect(&network_id, window, cx);
    }

    /// A click on a network's `edit` label: opens Connect on that network (Qt
    /// `onEditRequested`).
    pub fn open_connect_for(
        &mut self,
        network_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.chord_live(Chord::Connect, window, cx) {
            return;
        }
        self.present_connect(network_id, window, cx);
    }

    /// Selects `network_id` in the rail, then shows the sheet. Opening asks for the
    /// first field to take focus. The rail select is refused while a draft is dirty,
    /// as Qt's is, so the sheet opens on the network that was being edited.
    fn present_connect(&mut self, network_id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let was_open = self.connect_open;
        self.connect.select(network_id);
        if !was_open {
            let first = self.connect.initial_focus();
            self.connect.request_focus(first);
            self.connect_open = true;
            window.focus(&self.connect_focus);
        }
        cx.notify();
    }

    /// Escape, or a successful Apply. Closes Connect and returns focus to the
    /// composer. A draft that was not applied stays in the model.
    pub fn close_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.connect_open {
            return;
        }
        self.connect_open = false;
        self.connect.leave_focus();
        self.focus_composer(window, cx);
        cx.notify();
    }

    /// Ctrl+Enter and Ctrl+Return, or the Apply button (Qt `submitConnection`).
    /// A complete draft is saved and written into the world, and the sheet closes.
    /// An incomplete one stays open with its problem shown.
    pub fn connect_apply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::ConnectApply, window, cx) {
            return;
        }
        if self.connect.apply(&mut self.world).is_ok() {
            self.close_connect(window, cx);
        }
        cx.notify();
    }

    /// Ctrl+N, or the `+ Add network` row (Qt `addSheetNetwork`).
    pub fn connect_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::ConnectAdd, window, cx) {
            return;
        }
        self.connect.add();
        cx.notify();
    }

    /// Ctrl+Shift+Delete, or the Remove button (Qt `removeSheetNetwork`). The first
    /// press arms it, and the second removes the selected network.
    pub fn connect_remove(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::ConnectRemove, window, cx) {
            return;
        }
        if let RemoveOutcome::Removed(network_id) = self.connect.remove(&mut self.world) {
            self.after_network_removed(&network_id, window, cx);
        }
        cx.notify();
    }

    /// Ctrl+Tab and Ctrl+PgDn (`step` 1): the next Connect tab.
    pub fn connect_cycle_tab(&mut self, step: isize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::ConnectTabCycle, window, cx) {
            return;
        }
        self.connect.cycle_tab(step);
        cx.notify();
    }

    /// Alt+Left and Alt+Right while Connect shows: the neighbouring rail row.
    pub fn connect_walk_network(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chord = if delta > 0 {
            Chord::ConnectWalkNetworkRight
        } else {
            Chord::ConnectWalkNetworkLeft
        };
        if !self.chord_live(chord, window, cx) {
            return;
        }
        self.connect.walk_network(delta);
        cx.notify();
    }

    /// A Preferences switch on the Connect sheet. The value writes through at once,
    /// as `/pref` does, and is saved to the preferences file. No Apply. The sheet UI
    /// calls it, so it is dead until that lands.
    #[allow(dead_code)]
    pub fn set_connect_preference(&mut self, name: PrefName, on: bool, cx: &mut Context<Self>) {
        let avatars_before = self.preferences.avatars;
        if !self.preferences.set(name, PrefValue::Toggle(on)) {
            return;
        }
        if let Err(error) = self.preferences.save() {
            eprintln!("omairc-poc: could not save preferences: {error}");
        }
        if self.preferences.avatars != avatars_before {
            let avatars = self.preferences.avatars;
            self.transcript.update(cx, |t, cx| {
                t.set_avatars(avatars);
                cx.notify();
            });
        }
        cx.notify();
    }

    /// Clears what pointed at a network Remove took out: its inbox rows, the header
    /// focus, and the open view. The conversation on screen moves to the first one
    /// left, and Status goes back to the conversation. Focus stays in Connect.
    fn after_network_removed(
        &mut self,
        network_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.inbox.drop_network(network_id);
        if self.sidebar.focus.as_deref() == Some(network_id) {
            self.sidebar.focus = None;
        }
        if self.selected.network_id == network_id {
            // The model only removes a network when another one has a conversation.
            let fallback = sidebar_rows(&self.world, &self.sidebar)
                .first()
                .map(|row| row.conversation.clone());
            if let Some(target) = fallback {
                self.select(target, cx);
            }
        } else if self.status.as_deref() == Some(network_id) {
            self.close_status(window, cx);
            // Closing Status focuses the composer, which sits under Connect.
            self.connect.restore_focus();
        }
    }
}
