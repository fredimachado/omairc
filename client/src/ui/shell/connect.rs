//! The Connect sheet's shell side: opening and closing the layer, the keys and
//! clicks it answers, the focus walk, and what Apply and Remove change in the rest
//! of the window. The sheet UI lives in `ui/sheets/connect/`, and the model in
//! Connect is a window-level layer, not a `Sheet`. It sits under Shortcuts and
//! About, which open above it. Apply hands the saved profile to `Controller::start`
//! and Disconnect calls `Controller::disconnect`; the sheet itself never connects.

use gpui::{Context, Window};

use crate::model::chords::Chord;
use crate::controller::persistence::SecretEdits;
use crate::model::connect::{ConnectField, RailEnter, RemoveOutcome, Secret, Stop};
use crate::model::prefs::{PrefName, PrefValue};
use crate::storage::credentialstore::{PURPOSE_NICKSERV, PURPOSE_PASSWORD};
use crate::ui::sheets::connect::form_row_of;
use crate::ui::shell::Shell;

impl Shell {
    /// Ctrl+, and Ctrl+] (Windows): opens Connect on the focused network, or the
    /// network whose Status is showing, or the selected one.
    /// While Connect is open it selects that network again, as Qt does.
    pub fn open_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::Connect, window, cx) {
            return;
        }
        let network_id = self
            .sidebar
            .focus
            .clone()
            .or_else(|| self.status_network())
            .unwrap_or_else(|| self.selected_ref().network_id);
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
    /// first field to take focus. The rail select is refused while a draft is dirty
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
        // The rail shows the network the sheet is on, as Qt's `revealNetworkChoice` does.
        if let Some(row) = self.connect.rows().into_iter().find(|row| row.selected) {
            self.reveal_connect(&Stop::Network(row.network_id));
        }
        cx.notify();
    }

    /// The window's first focus, once it is open. While the first-run Connect sheet is up, its
    /// first field takes the keys now, so the composer behind the sheet does not. Otherwise the
    /// composer takes them. Focusing here, not on the next render, keeps the composer from
    /// winning the race for the window's focus.
    pub fn focus_on_launch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.connect_open {
            let first = self.connect.initial_focus();
            self.focus_connect(first, window, cx);
        } else {
            self.focus_composer(window, cx);
        }
    }

    /// Escape, the dimmer, or a successful Apply. Closes Connect and returns focus to the
    /// composer. A draft that was not applied stays in the model. While no network is saved
    /// yet the sheet is required, so Escape and the dimmer do nothing.
    pub fn close_connect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.connect_open || self.connect.setup_required() {
            return;
        }
        self.connect_open = false;
        self.connect.leave_focus();
        self.connect.rail_left();
        self.focus_composer(window, cx);
        cx.notify();
    }

    /// Ctrl+Enter and Ctrl+Return, or the Apply button. A complete
    /// draft is saved to the profile file and its secrets to the credential store, then the
    /// controller starts the network: a demo network reconnects, and any other is refused with
    /// the reason on its Status. The sheet closes when the session started and nothing needs
    /// the user; otherwise it stays with the sentence that says why. An incomplete draft stays
    /// open with its problem shown.
    pub fn connect_apply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::ConnectApply, window, cx) {
            return;
        }
        let edits = SecretEdits {
            password: self.connect.secret_edit(Secret::Password),
            nickserv: self.connect.secret_edit(Secret::NickServ),
        };
        if let Ok(profile) = self.connect.apply() {
            let case_mapping = self
                .controller
                .features(&profile.network_id)
                .case_mapping();
            let stored = profile.to_stored(self.controller.profile(&profile.network_id), case_mapping);
            let network_id = profile.network_id.clone();
            let saved = self.controller.save_profile(stored, edits);
            let applied_profile = self
                .controller
                .profile(&network_id)
                .cloned()
                .expect("save_profile keeps the profile");
            let started = self
                .controller
                .connect_allowed_after_save(&applied_profile, &saved)
                && self.controller.start(&network_id);
            let close = self
                .connect
                .record_apply(saved.status, saved.password, saved.nickserv, started);
            if close {
                self.connect.clear_secret_text();
                self.close_connect(window, cx);
            }
            self.drain_notifications(cx);
        }
        cx.notify();
    }

    /// Whether the selected network's session can be stopped, so the footer offers
    /// Disconnect. The controller answers it, so the sheet never holds a copy.
    pub fn connect_disconnectable(&self) -> bool {
        self.controller.can_disconnect(self.connect.selected())
    }

    /// The Disconnect button. It stops the selected network's
    /// session through the controller, as `/disconnect` does, and the sheet stays open
    /// with the profile kept. Focus that was on the button moves to Apply, since the
    /// button leaves the walk.
    pub fn connect_disconnect(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.connect_open {
            return;
        }
        let was_focused = self.connect_focused(window) == Some(Stop::Disconnect);
        let network_id = self.connect.selected().to_owned();
        self.controller.disconnect(&network_id);
        if was_focused {
            self.focus_connect(Stop::Apply, window, cx);
        }
        self.drain_notifications(cx);
        cx.notify();
    }

    /// Ctrl+N, or the `+ Add network` row.
    pub fn connect_add(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::ConnectAdd, window, cx) {
            return;
        }
        self.connect.add();
        cx.notify();
    }

    /// Ctrl+Shift+Delete, or the Remove button. The first
    /// press arms it, and the second removes the selected network.
    pub fn connect_remove(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::ConnectRemove, window, cx) {
            return;
        }
        let populated = self.populated_networks();
        if let RemoveOutcome::Removed(network_id) = self.connect.remove(&populated) {
            let was_status = self.status_network().as_deref() == Some(network_id.as_str());
            self.controller.forget_network(&network_id);
            self.after_network_removed(&network_id, was_status, window, cx);
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

    /// Alt+Left and Alt+Right while Connect shows: the neighbouring rail row. Focus
    /// follows the walk when it was on the rail, and the rail scrolls to the row.
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
        let on_rail = matches!(
            self.connect_focused(window),
            Some(Stop::Network(_) | Stop::AddNetwork)
        );
        let Some(target) = self.connect.walk_network(delta) else {
            return;
        };
        if on_rail {
            self.focus_connect(Stop::Network(target), window, cx);
        } else {
            self.reveal_connect(&Stop::Network(target));
            cx.notify();
        }
    }

    /// A Preferences control on the Connect sheet. The value writes through at
    /// once and is saved to the preferences file. No Apply.
    pub fn set_connect_preference(
        &mut self,
        name: PrefName,
        value: PrefValue,
        cx: &mut Context<Self>,
    ) {
        let before = *self.controller.preferences();
        if !self.controller.set_preference(name, value) {
            return;
        }
        self.apply_preference_changes(before, cx);
        self.drain_notifications(cx);
        cx.notify();
    }

    /// Whether the Connect card's keys act: Connect shows, and no sheet sits above it.
    fn connect_keys_live(&self) -> bool {
        self.connect_open && self.modal.is_none()
    }

    /// The stop that holds focus in the Connect card, if one does.
    pub fn connect_focused(&self, window: &Window) -> Option<Stop> {
        self.connect_ui.focused(
            &self
                .connect
                .stops(&self.populated_networks(), self.connect_disconnectable()),
            window,
        )
    }

    /// Moves focus to `stop` and brings it into view. The keys then act on it.
    pub fn focus_connect(&mut self, stop: Stop, window: &mut Window, cx: &mut Context<Self>) {
        self.connect_ui.ensure(cx, std::slice::from_ref(&stop));
        if let Some(handle) = self.connect_ui.handle(&stop) {
            window.focus(handle);
        }
        self.reveal_connect(&stop);
        self.connect.set_focus(Some(stop));
        cx.notify();
    }

    /// Scrolls the rail or the form so `stop` is in view. A rail row is scrolled by
    /// its index among the rail stops, and a form field by its form row.
    fn reveal_connect(&self, stop: &Stop) {
        if let Some(index) = self
            .connect
            .rail_stops()
            .iter()
            .position(|rail| rail == stop)
        {
            self.connect_ui.rail_scroll.scroll_to_item(index);
        }
        if let Some(row) = form_row_of(stop) {
            self.connect_ui.form_scroll.scroll_to_item(row);
        }
    }

    /// A click on a Connect control. A field or help mark takes focus, a tab shows
    /// a row selects, a switch flips, a preference flips at once, and a button acts.
    pub fn connect_click(&mut self, stop: Stop, window: &mut Window, cx: &mut Context<Self>) {
        if !self.connect_open {
            return;
        }
        match stop {
            Stop::Tab(tab) => {
                self.connect.set_tab(tab);
                self.focus_connect(Stop::Tab(tab), window, cx);
            }
            Stop::Network(network_id) => {
                self.connect.select(&network_id);
                self.focus_connect(Stop::Network(network_id), window, cx);
            }
            Stop::AddNetwork => self.connect_add(window, cx),
            Stop::ShortcutsHint => self.toggle_shortcuts(window, cx),
            Stop::Tls => {
                let on = !self.connect.tls();
                self.connect.set_tls(on);
            }
            Stop::ConnectOnStartup => {
                let on = !self.connect.connect_on_startup();
                self.connect.set_connect_on_startup(on);
            }
            Stop::Preference(name) => {
                let value = self.controller.preferences().next_value(name);
                self.set_connect_preference(name, value, cx);
            }
            Stop::Remove => self.connect_remove(window, cx),
            Stop::Discard => self.connect.discard(),
            Stop::ForgetPassword => self.connect_forget(Secret::Password, window, cx),
            Stop::ForgetNickServ => self.connect_forget(Secret::NickServ, window, cx),
            Stop::Disconnect => self.connect_disconnect(window, cx),
            Stop::Apply => self.connect_apply(window, cx),
            stop @ (Stop::Field(_) | Stop::FieldHelp(_) | Stop::PreferenceHelp(_)) => {
                self.focus_connect(stop, window, cx);
            }
        }
        cx.notify();
    }

    /// A forget link: clears the draft slot, then deletes the keychain entry for the
    /// saved network immediately (`forgetPassword` + `removeStoredPassword`). The sheet
    /// slot follows the store settle so a refused delete keeps the forget link. Focus that
    /// was on the link moves when it leaves the walk: the remaining forget link if still
    /// visible, else Remove / Discard / Apply, as Disconnect rehomes to Apply.
    fn connect_forget(&mut self, slot: Secret, window: &mut Window, cx: &mut Context<Self>) {
        let stop = match slot {
            Secret::Password => Stop::ForgetPassword,
            Secret::NickServ => Stop::ForgetNickServ,
        };
        let was_focused = self.connect_focused(window) == Some(stop);
        if let Some(network_id) = self.connect.forget(slot) {
            let purpose = match slot {
                Secret::Password => PURPOSE_PASSWORD,
                Secret::NickServ => PURPOSE_NICKSERV,
            };
            let settled = self.controller.forget_secret(&network_id, purpose);
            self.connect.apply_forget_result(slot, settled);
        }
        if was_focused {
            let populated = self.populated_networks();
            let disconnectable = self.connect_disconnectable();
            if let Some(target) =
                self.connect
                    .focus_after_forget(slot, &populated, disconnectable)
            {
                self.focus_connect(target, window, cx);
            }
        }
        self.sync_connect_inputs(cx);
    }

    /// Enter on the card. A rail row selects and arms, and a second Enter applies
    /// (Qt's rail). A tab, the Add row, the hint and the buttons act. Anything else
    /// a field, a switch, a help mark or no focus, walks to the next stop.
    pub fn connect_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.connect_keys_live() {
            return false;
        }
        match self.connect_focused(window) {
            Some(Stop::Network(network_id)) => match self.connect.rail_enter(&network_id) {
                RailEnter::Apply => self.connect_apply(window, cx),
                RailEnter::Selected => cx.notify(),
            },
            Some(
                stop @ (Stop::Tab(_)
                | Stop::AddNetwork
                | Stop::ShortcutsHint
                | Stop::Remove
                | Stop::Discard
                | Stop::ForgetPassword
                | Stop::ForgetNickServ
                | Stop::Disconnect
                | Stop::Apply),
            ) => self.connect_click(stop, window, cx),
            _ => {
                self.connect_step(false, window, cx);
            }
        }
        true
    }

    /// Space on the card. A switch, a tab, a row and a button act, as Qt's do. A
    /// field takes the space as text, and a help mark does nothing, so both return
    /// false and the key goes on to the field.
    pub fn connect_space(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.connect_keys_live() {
            return false;
        }
        match self.connect_focused(window) {
            None | Some(Stop::Field(_) | Stop::FieldHelp(_) | Stop::PreferenceHelp(_)) => false,
            Some(Stop::Network(network_id)) => {
                // Space selects a row without arming it, as Qt's does.
                self.connect.select(&network_id);
                cx.notify();
                true
            }
            Some(stop) => {
                self.connect_click(stop, window, cx);
                true
            }
        }
    }

    /// Tab and Shift+Tab: the next stop of the focus walk, wrapping at both ends.
    pub fn connect_step(
        &mut self,
        backwards: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connect_keys_live() {
            return false;
        }
        let focused = self.connect_focused(window);
        let populated = self.populated_networks();
        let disconnectable = self.connect_disconnectable();
        if let Some(target) =
            self.connect
                .step_stop(&populated, disconnectable, focused.as_ref(), backwards)
        {
            self.focus_connect(target, window, cx);
        }
        true
    }

    /// Left and Right on a tab step to the neighbouring tab, which then takes focus.
    /// Anywhere else they pass on, so a field keeps its caret keys.
    pub fn connect_arrow(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connect_keys_live() {
            return false;
        }
        match self.connect_focused(window) {
            Some(Stop::Tab(_)) => {
                self.connect.cycle_tab(delta);
                cx.notify();
                true
            }
            _ => false,
        }
    }

    /// Up and Down on the rail move focus along its rows and the Add row, and stop
    /// at the ends. They never change the selection, as Qt's do. Elsewhere they pass on.
    pub fn connect_rail_step(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connect_keys_live() {
            return false;
        }
        let Some(focused) = self.connect_focused(window) else {
            return false;
        };
        match self.connect.rail_step(&focused, delta) {
            Some(target) => {
                self.focus_connect(target, window, cx);
                true
            }
            None => false,
        }
    }

    /// Home and End on the rail: its first or last stop. Elsewhere they pass on.
    pub fn connect_rail_edge(
        &mut self,
        last: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.connect_keys_live() {
            return false;
        }
        let Some(focused) = self.connect_focused(window) else {
            return false;
        };
        if !self.connect.rail_stops().contains(&focused) {
            return false;
        }
        if let Some(target) = self.connect.rail_edge(last) {
            self.focus_connect(target, window, cx);
        }
        true
    }

    /// A field's input changed: the draft takes its text. Nothing is saved until Apply.
    pub fn connect_field_edited(&mut self, field: ConnectField, cx: &mut Context<Self>) {
        let text = self.connect_ui.input(field).read(cx).text().to_string();
        if self.connect.text(field) != text {
            self.connect.set_text(field, &text);
            cx.notify();
        }
    }

    /// Copies the draft into each field whose text differs. Apply, Discard, a switch
    /// and Remove change the draft without typing, and the fields show the draft.
    pub fn sync_connect_inputs(&self, cx: &mut Context<Self>) {
        for (field, input) in self.connect_ui.fields() {
            let wanted = self.connect.text(field).to_string();
            if input.read(cx).text() != wanted.as_str() {
                input.update(cx, |input, cx| {
                    input.set_text(wanted);
                    cx.notify();
                });
            }
        }
    }

    /// Clears what pointed at a network Remove took out: its inbox rows, the header
    /// focus, and the open view. The conversation on screen moves to the first one
    /// left, and Status goes back to the conversation. Focus stays in Connect.
    fn after_network_removed(
        &mut self,
        network_id: &str,
        was_status: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.sidebar.focus.as_deref() == Some(network_id) {
            self.sidebar.focus = None;
        }
        // The controller dropped the selection with the network. The first row left takes
        // it (the model only removes a network when another one has a conversation).
        if self.controller.selected().is_none() {
            let fallback = self.sidebar_walk().first().map(|row| row.conversation.clone());
            if let Some(target) = fallback {
                self.select(target, cx);
            }
        }
        self.drain_notifications(cx);
        if was_status {
            // Closing Status focuses the composer, which sits under Connect.
            self.focus_composer(window, cx);
            self.connect.restore_focus();
        }
    }
}
