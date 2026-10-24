//! The sheet's state: the saved networks, the draft and its dirty check, the rail
//! selection, Apply, Add, Discard and Remove, the tabs, and the keyboard walk with its
//! focus. `ConnectState` and the types it holds stay in the parent module.

use super::*;
use crate::storage::profilestore::Profile as StoredProfile;

impl ConnectState {
    /// The sheet state for the networks the controller holds: one saved profile per
    /// network, with the first network selected and no draft.
    pub fn seed(networks: &[NetworkSeed]) -> Self {
        let saved: Vec<Profile> = networks.iter().map(Profile::from_seed).collect();
        // With no network yet, the draft takes a fresh id, so that Apply has a network to
        // start. An empty id would have nothing to name on Status or in the sidebar.
        let (selected, draft) = match saved.first() {
            Some(profile) => (profile.network_id.clone(), profile.clone()),
            None => {
                let id = fresh_network_id_among(&saved);
                (id.clone(), Profile::blank(&id))
            }
        };
        Self::assembled(saved, selected, draft)
    }

    /// The sheet for the networks the controller saved (`IrcConnection`'s start). The first
    /// complete network is selected, or the first one. With none saved, the first run starts.
    /// A network whose file says a secret is saved starts that slot as [`CredentialState::Available`],
    /// so the forget links and the credential line match a relaunch after Apply.
    pub fn from_stored(stored: &[StoredProfile]) -> Self {
        let saved: Vec<Profile> = stored.iter().map(Profile::from_stored).collect();
        if saved.is_empty() {
            return Self::first_run();
        }
        let chosen = saved
            .iter()
            .find(|profile| profile.problem().is_none())
            .unwrap_or(&saved[0])
            .clone();
        let mut state = Self::assembled(saved, chosen.network_id.clone(), chosen);
        for profile in stored {
            let password = if profile.secret_saved {
                CredentialState::Available
            } else {
                CredentialState::Missing
            };
            let nickserv = if profile.nickserv_saved {
                CredentialState::Available
            } else {
                CredentialState::Missing
            };
            if password != CredentialState::Missing || nickserv != CredentialState::Missing {
                state.secrets.insert(
                    profile.network_id.clone(),
                    SecretStates { password, nickserv },
                );
            }
        }
        state
    }

    /// The sheet for a first run with nothing saved: the suggested Freenode network, as a new
    /// draft with a fresh id, so that Apply has a network to start (`IrcConnection::suggested`).
    pub fn first_run() -> Self {
        let id = fresh_network_id_among(&[]);
        let mut draft = Profile::suggested(&id);
        draft.icon_color = Some(0);
        Self::assembled(Vec::new(), id, draft)
    }

    fn assembled(saved: Vec<Profile>, selected: String, draft: Profile) -> Self {
        Self {
            saved,
            draft,
            selected,
            added_from: None,
            tab: ConnectTab::Connection,
            remove_armed: false,
            rail_armed: None,
            focus: None,
            focus_request: None,
            switch_refused: false,
            secrets: BTreeMap::new(),
            password_edited: false,
            nickserv_edited: false,
            persistence: BTreeMap::new(),
        }
    }

    /// Whether no saved network is complete, so the sheet stays up until one is
    /// (`setupRequired`).
    pub fn setup_required(&self) -> bool {
        !self.saved.iter().any(|profile| profile.problem().is_none())
    }

    /// The text the draft's secret slot was given since the last Apply, or `None` when the slot
    /// was not typed in. Apply hands this to the credential store.
    pub fn secret_edit(&self, slot: Secret) -> Option<SecretText> {
        match slot {
            Secret::Password if self.password_edited => Some(self.draft.password.clone()),
            Secret::NickServ if self.nickserv_edited => Some(self.draft.nickserv_password.clone()),
            _ => None,
        }
    }

    /// Where the selected network's secret slots stand.
    pub fn secret_states(&self) -> SecretStates {
        self.secrets
            .get(&self.selected)
            .cloned()
            .unwrap_or_default()
    }

    /// The credential sentence under the form for the selected network (`credentialStatus`).
    pub fn credential_status(&self) -> String {
        let states = self.secret_states();
        let password = SecretSlot {
            noun: "password",
            held: !self.draft.password.is_empty(),
            edited: self.password_edited,
            state: states.password,
        };
        let nickserv = SecretSlot {
            noun: "NickServ",
            held: !self.draft.nickserv_password.is_empty(),
            edited: self.nickserv_edited,
            state: states.nickserv,
        };
        combined_secret_status(&password, &nickserv)
    }

    /// The name of an unsaved draft network, while the sheet holds it. `None` for a saved
    /// network or an unknown id, which the controller names.
    pub fn draft_name(&self, network_id: &str) -> Option<String> {
        if self.is_saved(network_id) || self.draft.network_id != network_id {
            return None;
        }
        let name = self.draft.resolved_name();
        (!name.is_empty()).then_some(name)
    }

    /// The persistence sentence for the selected network (`persistenceStatus`), or empty.
    pub fn persistence_status(&self) -> String {
        self.persistence
            .get(&self.selected)
            .cloned()
            .unwrap_or_default()
    }

    /// Records what an Apply did after the controller saved and started the network. The
    /// secret states and the persistence sentence are kept for the selected network. Returns
    /// whether the sheet may close: the session started, no network is still required, and
    /// nothing needs the user's attention (`submitConnection`).
    pub fn record_apply(
        &mut self,
        status: Status,
        password: CredentialState,
        nickserv: CredentialState,
        started: bool,
    ) -> bool {
        let network_id = self.selected.clone();
        self.secrets
            .insert(network_id.clone(), SecretStates { password, nickserv });
        match apply_sentence(status, started) {
            Some(sentence) => {
                self.persistence.insert(network_id, sentence);
            }
            None => {
                self.persistence.remove(&network_id);
            }
        }
        started && !self.setup_required() && self.persistence_status().is_empty()
    }

    /// Forgets the secret text in the draft, as the sheet does once it closes after a
    /// successful Apply (`clearConnectionPassword`).
    pub fn clear_secret_text(&mut self) {
        self.draft.password = SecretText::default();
        self.draft.nickserv_password = SecretText::default();
        self.password_edited = false;
        self.nickserv_edited = false;
    }

    /// The saved profiles in rail order. The sheet reads the rail through `rows`.
    pub fn saved(&self) -> &[Profile] {
        &self.saved
    }

    /// Copies a background profile edit into the saved network and the draft when it is
    /// selected (`IrcConnection::persistAutojoin` / `persistAvatarUrl` on the draft).
    pub fn sync_background_profile(&mut self, stored: &StoredProfile) {
        let autojoin = stored.autojoin_channels.join(" ");
        if let Some(index) = self
            .saved
            .iter()
            .position(|profile| profile.network_id == stored.network_id)
        {
            self.saved[index].autojoin = autojoin.clone();
        }
        if self.selected == stored.network_id && self.draft.autojoin != autojoin {
            self.draft.autojoin = autojoin;
        }
    }

    /// The profile the form shows. The sheet reads the fields through `text`.
    #[cfg(test)]
    pub fn draft(&self) -> &Profile {
        &self.draft
    }

    /// The selected network's id. Disconnect and Apply act on it.
    pub fn selected(&self) -> &str {
        &self.selected
    }

    pub fn tab(&self) -> ConnectTab {
        self.tab
    }

    pub fn remove_armed(&self) -> bool {
        self.remove_armed
    }

    /// Whether the last switch to another network was refused for a dirty draft.
    pub fn switch_refused(&self) -> bool {
        self.switch_refused
    }

    /// Whether `network_id` is one of the saved networks.
    pub fn is_saved(&self, network_id: &str) -> bool {
        self.saved
            .iter()
            .any(|profile| profile.network_id == network_id)
    }

    /// The text a field shows.
    pub fn text(&self, field: ConnectField) -> &str {
        match field {
            ConnectField::Name => &self.draft.name,
            ConnectField::Host => &self.draft.host,
            ConnectField::Port => &self.draft.port,
            ConnectField::Nick => &self.draft.nick,
            ConnectField::Username => &self.draft.username,
            ConnectField::RealName => &self.draft.realname,
            ConnectField::Autojoin => &self.draft.autojoin,
            ConnectField::ServerPassword => self.draft.password.expose(),
            ConnectField::NickServPassword => self.draft.nickserv_password.expose(),
        }
    }

    /// Stores what the user typed in a field. Nothing is saved until Apply.
    pub fn set_text(&mut self, field: ConnectField, value: &str) {
        let value = value.to_string();
        match field {
            ConnectField::Name => self.draft.name = value,
            ConnectField::Host => self.draft.host = value,
            ConnectField::Port => self.draft.port = value,
            ConnectField::Nick => self.draft.nick = value,
            ConnectField::Username => self.draft.username = value,
            ConnectField::RealName => self.draft.realname = value,
            ConnectField::Autojoin => self.draft.autojoin = value,
            ConnectField::ServerPassword => {
                // Emptying a held secret is refused: forget is the only clearing path
                // (`IrcConnection::setPassword`).
                if value.is_empty() && !self.draft.password.is_empty() {
                    return;
                }
                self.draft.password = SecretText::from(value);
                self.password_edited = true;
            }
            ConnectField::NickServPassword => {
                if value.is_empty() && !self.draft.nickserv_password.is_empty() {
                    return;
                }
                self.draft.nickserv_password = SecretText::from(value);
                self.nickserv_edited = true;
            }
        }
    }

    /// Whether the forget link for `slot` shows (`canForgetPassword` / `canForgetNickServ`).
    /// True for a saved network when that slot is in the keychain or the draft still holds text.
    pub fn can_forget(&self, slot: Secret) -> bool {
        if !self.is_saved(&self.selected) {
            return false;
        }
        let states = self.secret_states();
        let (state, held) = match slot {
            Secret::Password => (states.password, !self.draft.password.is_empty()),
            Secret::NickServ => (states.nickserv, !self.draft.nickserv_password.is_empty()),
        };
        state == CredentialState::Available || held
    }

    /// Clears the draft text for `slot`, marks it edited, and sets the slot to Missing
    /// (`forgetPassword` / `forgetNickServ`). Returns the selected saved network's id so the
    /// shell can delete the keychain entry immediately. No-op (returns `None`) when the slot
    /// is already empty and Missing.
    pub fn forget(&mut self, slot: Secret) -> Option<String> {
        let states = self.secret_states();
        let (empty, missing) = match slot {
            Secret::Password => (
                self.draft.password.is_empty(),
                states.password == CredentialState::Missing,
            ),
            Secret::NickServ => (
                self.draft.nickserv_password.is_empty(),
                states.nickserv == CredentialState::Missing,
            ),
        };
        if empty && missing {
            return None;
        }
        match slot {
            Secret::Password => {
                self.draft.password = SecretText::default();
                self.password_edited = true;
                self.secrets
                    .entry(self.selected.clone())
                    .or_default()
                    .password = CredentialState::Missing;
            }
            Secret::NickServ => {
                self.draft.nickserv_password = SecretText::default();
                self.nickserv_edited = true;
                self.secrets
                    .entry(self.selected.clone())
                    .or_default()
                    .nickserv = CredentialState::Missing;
            }
        }
        self.is_saved(&self.selected)
            .then(|| self.selected.clone())
    }

    pub fn tls(&self) -> bool {
        self.draft.tls
    }

    pub fn set_tls(&mut self, on: bool) {
        self.draft.tls = on;
    }

    pub fn connect_on_startup(&self) -> bool {
        self.draft.connect_on_startup
    }

    pub fn set_connect_on_startup(&mut self, on: bool) {
        self.draft.connect_on_startup = on;
    }

    /// Whether the draft differs from the saved network it came from (Qt `dirty`).
    /// A draft that is not saved yet is always dirty. Secrets do not count.
    pub fn dirty(&self) -> bool {
        match self
            .saved
            .iter()
            .find(|profile| profile.network_id == self.selected)
        {
            None => true,
            Some(saved) => saved.normalized() != self.draft.normalized(),
        }
    }

    /// Why Apply is refused, or `None` when the draft can be applied.
    pub fn problem(&self) -> Option<Problem> {
        self.draft.problem()
    }

    /// The rail, in order: the saved networks, then an unsaved draft.
    pub fn rows(&self) -> Vec<RailRow> {
        let mut rows: Vec<RailRow> = self
            .saved
            .iter()
            .map(|profile| {
                let shown = if profile.network_id == self.selected {
                    &self.draft
                } else {
                    profile
                };
                RailRow {
                    network_id: profile.network_id.clone(),
                    label: roster_label(shown, &self.saved),
                    stored: true,
                    selected: profile.network_id == self.selected,
                    icon_color: shown.icon_color,
                }
            })
            .collect();
        if !self.selected.is_empty() && !self.is_saved(&self.selected) {
            rows.push(RailRow {
                network_id: self.selected.clone(),
                label: roster_label(&self.draft, &self.saved),
                stored: false,
                selected: true,
                icon_color: self.draft.icon_color,
            });
        }
        rows
    }

    /// Whether Add (Ctrl+N) can run: the selection is saved and has no edits.
    pub fn can_add(&self) -> bool {
        self.is_saved(&self.selected) && !self.dirty()
    }

    /// Whether Remove can run on the selection. It must be saved, and another
    /// saved network must have a conversation, so the shell always has one to show.
    /// `populated` names the networks that hold a conversation.
    pub fn can_remove(&self, populated: &[String]) -> bool {
        self.is_saved(&self.selected)
            && self.saved.iter().any(|profile| {
                profile.network_id != self.selected && populated.contains(&profile.network_id)
            })
    }

    /// Selects a saved network (Qt `select` through `selectSheetNetwork`). It is
    /// refused while the draft is dirty, so an edit is applied or discarded
    /// first, as in Qt. Remove's armed state resets either way. Returns whether
    /// the selection moved.
    pub fn select(&mut self, network_id: &str) -> bool {
        self.remove_armed = false;
        if network_id.is_empty() || network_id == self.selected || !self.is_saved(network_id) {
            return false;
        }
        if self.dirty() {
            self.switch_refused = true;
            return false;
        }
        self.switch_refused = false;
        self.rail_armed = None;
        self.load_draft(network_id);
        true
    }

    /// Up or Alt+Left/Right in the rail: the neighbouring row, wrapping. Returns
    /// that row's id so the caller can focus it. The selection moves only when
    /// `select` allows it.
    pub fn walk_network(&mut self, step: isize) -> Option<String> {
        let ids: Vec<String> = self.rows().into_iter().map(|row| row.network_id).collect();
        if ids.is_empty() {
            return None;
        }
        let len = ids.len() as isize;
        let next = match ids.iter().position(|id| *id == self.selected) {
            None if step > 0 => 0,
            None => len as usize - 1,
            Some(index) => (index as isize + step).rem_euclid(len) as usize,
        };
        let target = ids[next].clone();
        self.select(&target);
        Some(target)
    }

    /// Enter on a rail row (Qt `rail row Return`): a selected row that is already
    /// armed asks for Apply. Otherwise the row is selected and armed.
    pub fn rail_enter(&mut self, network_id: &str) -> RailEnter {
        if self.selected == network_id && self.rail_armed.as_deref() == Some(network_id) {
            return RailEnter::Apply;
        }
        self.select(network_id);
        self.rail_armed = Some(network_id.to_string());
        RailEnter::Selected
    }

    /// Focus left the rail row, so its Enter arming resets.
    pub fn rail_left(&mut self) {
        self.rail_armed = None;
    }

    /// Add (Ctrl+N, Qt `add`): a new unsaved network with Qt's defaults, selected
    /// with the Connection tab showing. The Name field is asked for focus. Refused
    /// while the draft is dirty or the selection is not saved.
    pub fn add(&mut self) -> bool {
        if !self.can_add() {
            return false;
        }
        let id = self.fresh_network_id();
        let mut profile = Profile::blank(&id);
        profile.icon_color = self.first_free_icon_color();
        self.added_from = Some(self.selected.clone());
        self.draft = profile;
        self.password_edited = false;
        self.nickserv_edited = false;
        self.selected = id;
        self.tab = ConnectTab::Connection;
        self.remove_armed = false;
        self.rail_armed = None;
        self.focus_request = Some(Stop::Field(ConnectField::Name));
        self.switch_refused = false;
        true
    }

    /// Discard (Qt `discard`). An unsaved draft is dropped and the sheet returns to
    /// the network it was added from, or to the first saved one. A saved network's
    /// edits are reverted to the saved values.
    pub fn discard(&mut self) {
        self.remove_armed = false;
        self.rail_armed = None;
        self.switch_refused = false;
        if !self.is_saved(&self.selected) {
            let returned = self
                .added_from
                .take()
                .filter(|id| self.is_saved(id))
                .or_else(|| self.saved.first().map(|profile| profile.network_id.clone()));
            if let Some(id) = returned {
                self.load_draft(&id);
            }
            return;
        }
        let selected = self.selected.clone();
        self.load_draft(&selected);
    }

    /// Apply (Ctrl+Enter, Qt `apply` through `submitConnection`). Saves the
    /// normalized draft, makes it the saved network, and returns it so the shell can
    /// hand it to `Controller::start`. Refused with the problem when the draft is
    /// incomplete. Nothing connects here.
    pub fn apply(&mut self) -> Result<Profile, Problem> {
        if let Some(problem) = self.draft.problem() {
            return Err(problem);
        }
        let profile = self.draft.normalized();
        match self
            .saved
            .iter()
            .position(|saved| saved.network_id == profile.network_id)
        {
            Some(index) => self.saved[index] = profile.clone(),
            None => self.saved.push(profile.clone()),
        }
        // The secret text stays in the draft while the sheet is open, as Qt keeps it until the
        // sheet closes. The saved network never holds it.
        let password = self.draft.password.clone();
        let nickserv_password = self.draft.nickserv_password.clone();
        self.selected = profile.network_id.clone();
        self.draft = profile.clone();
        self.draft.password = password;
        self.draft.nickserv_password = nickserv_password;
        self.password_edited = false;
        self.nickserv_edited = false;
        self.added_from = None;
        self.remove_armed = false;
        self.rail_armed = None;
        self.switch_refused = false;
        Ok(profile)
    }

    /// Remove (Ctrl+Shift+Delete, Qt `removeSheetNetwork`). The first press arms
    /// the button. The second removes the selected saved network and selects the
    /// network now at its position, or the last one. The shell then forgets the
    /// network on the controller. `populated` names the networks with a conversation.
    pub fn remove(&mut self, populated: &[String]) -> RemoveOutcome {
        if !self.can_remove(populated) {
            self.remove_armed = false;
            return RemoveOutcome::Refused;
        }
        if !self.remove_armed {
            self.remove_armed = true;
            return RemoveOutcome::Armed;
        }
        let removed = self.selected.clone();
        let index = self
            .saved
            .iter()
            .position(|profile| profile.network_id == removed)
            .unwrap_or(0);
        self.saved.remove(index);
        let next = index.min(self.saved.len().saturating_sub(1));
        self.load_draft(&self.saved[next].network_id.clone());
        self.remove_armed = false;
        self.rail_armed = None;
        self.added_from = None;
        self.switch_refused = false;
        RemoveOutcome::Removed(removed)
    }

    /// Ctrl+Tab and Ctrl+PgDn: steps the tab and asks for focus on its button.
    pub fn cycle_tab(&mut self, step: isize) -> ConnectTab {
        self.tab = self.tab.step(step);
        self.focus_request = Some(Stop::Tab(self.tab));
        self.tab
    }

    /// Clicking a tab shows it, without moving focus.
    pub fn set_tab(&mut self, tab: ConnectTab) {
        self.tab = tab;
    }

    /// Every focus stop on the showing tab, in Tab order. Remove is listed only
    /// when `can_remove` allows it, Add only when `can_add` does, and Disconnect only
    /// when `disconnectable` says the selected network's session can be stopped.
    pub fn stops(&self, populated: &[String], disconnectable: bool) -> Vec<Stop> {
        let mut stops = vec![
            Stop::Tab(ConnectTab::Connection),
            Stop::Tab(ConnectTab::Preferences),
        ];
        match self.tab {
            ConnectTab::Connection => {
                stops.extend(
                    self.rows()
                        .into_iter()
                        .map(|row| Stop::Network(row.network_id)),
                );
                if self.can_add() {
                    stops.push(Stop::AddNetwork);
                }
                stops.push(Stop::ShortcutsHint);
                stops.extend(
                    [ConnectField::Name, ConnectField::Host, ConnectField::Port].map(Stop::Field),
                );
                stops.push(Stop::Tls);
                stops.extend(
                    [
                        ConnectField::Nick,
                        ConnectField::Username,
                        ConnectField::RealName,
                        ConnectField::Autojoin,
                    ]
                    .map(Stop::Field),
                );
                stops.push(Stop::ConnectOnStartup);
                for field in [ConnectField::ServerPassword, ConnectField::NickServPassword] {
                    if field.help().is_some() {
                        stops.push(Stop::FieldHelp(field));
                    }
                    stops.push(Stop::Field(field));
                }
                // After both password fields, as Qt's Row under the credential line.
                if self.can_forget(Secret::Password) {
                    stops.push(Stop::ForgetPassword);
                }
                if self.can_forget(Secret::NickServ) {
                    stops.push(Stop::ForgetNickServ);
                }
                if self.can_remove(populated) {
                    stops.push(Stop::Remove);
                }
                stops.push(Stop::Discard);
                if disconnectable {
                    stops.push(Stop::Disconnect);
                }
                stops.push(Stop::Apply);
            }
            ConnectTab::Preferences => {
                for row in preference_rows() {
                    stops.push(Stop::Preference(row.name));
                    stops.push(Stop::PreferenceHelp(row.name));
                }
            }
        }
        stops
    }

    /// The stop after `from` in Tab order, or before it when `backwards`. Tab and
    /// Enter walk forward, and Shift+Tab backward. The order wraps at both ends. A
    /// `from` that is not on the showing tab falls back to the first stop, or the
    /// last when going back. With no `from` (nothing in the sheet has focus), Tab
    /// goes to the first stop and Shift+Tab to the last.
    pub fn step_stop(
        &self,
        populated: &[String],
        disconnectable: bool,
        from: Option<&Stop>,
        backwards: bool,
    ) -> Option<Stop> {
        let stops = self.stops(populated, disconnectable);
        if stops.is_empty() {
            return None;
        }
        let len = stops.len();
        let index = from.and_then(|from| stops.iter().position(|stop| stop == from));
        Some(match index {
            Some(index) if backwards => stops[(index + len - 1) % len].clone(),
            Some(index) => stops[(index + 1) % len].clone(),
            None if backwards => stops[len - 1].clone(),
            None => stops[0].clone(),
        })
    }

    /// The Up and Down stops of the network rail: each saved row, then the Add row
    /// when it shows. Up and Down move focus along this list and stop at its ends
    /// (Qt `focusNetworkChoiceStop`). They never change the selection.
    pub fn rail_stops(&self) -> Vec<Stop> {
        let mut stops: Vec<Stop> = self
            .rows()
            .into_iter()
            .map(|row| Stop::Network(row.network_id))
            .collect();
        if self.can_add() {
            stops.push(Stop::AddNetwork);
        }
        stops
    }

    /// The rail stop `delta` places from `from`, clamped to the rail's ends, so
    /// focus never wraps here. `None` when `from` is not a rail stop.
    pub fn rail_step(&self, from: &Stop, delta: isize) -> Option<Stop> {
        let stops = self.rail_stops();
        let index = stops.iter().position(|stop| stop == from)? as isize;
        let last = stops.len() as isize - 1;
        Some(stops[(index + delta).clamp(0, last) as usize].clone())
    }

    /// The first rail stop (Home), or the last (End). `None` when the rail is empty.
    pub fn rail_edge(&self, last: bool) -> Option<Stop> {
        let stops = self.rail_stops();
        if last {
            stops.last().cloned()
        } else {
            stops.first().cloned()
        }
    }

    /// The field focus starts on when the sheet opens: Nick when it is empty,
    /// otherwise Name (Qt `focusConnectionSheetStart`).
    pub fn initial_focus(&self) -> Stop {
        if self.draft.nick.is_empty() {
            Stop::Field(ConnectField::Nick)
        } else {
            Stop::Field(ConnectField::Name)
        }
    }

    /// Forgets the focused stop and any pending request. Closing the sheet calls it.
    pub fn leave_focus(&mut self) {
        self.focus = None;
        self.focus_request = None;
    }

    /// Called by the sheet when a stop gains focus, so a later restore can return there.
    pub fn set_focus(&mut self, stop: Option<Stop>) {
        self.focus = stop;
    }

    /// Asks the sheet to focus `stop` at its next render.
    pub fn request_focus(&mut self, stop: Stop) {
        self.focus_request = Some(stop);
    }

    /// Asks for the last focused stop again. Used when a sheet on top of Connect
    /// closes, so focus does not fall to the composer behind the veil.
    pub fn restore_focus(&mut self) {
        self.focus_request = Some(
            self.focus
                .clone()
                .unwrap_or(Stop::Field(ConnectField::Name)),
        );
    }

    /// The pending focus request, once. The render takes it and focuses that stop.
    pub fn take_focus_request(&mut self) -> Option<Stop> {
        self.focus_request.take()
    }

    /// Loads a saved network into the draft and selects it. Secrets are dropped.
    fn load_draft(&mut self, network_id: &str) {
        if let Some(profile) = self
            .saved
            .iter()
            .find(|profile| profile.network_id == network_id)
        {
            self.draft = profile.clone();
            self.selected = network_id.to_string();
            self.password_edited = false;
            self.nickserv_edited = false;
        }
    }

    /// The first `network-N` id no saved network uses.
    fn fresh_network_id(&self) -> String {
        fresh_network_id_among(&self.saved)
    }

    /// The lowest icon colour no saved network uses, or colour 0 when all are.
    fn first_free_icon_color(&self) -> Option<u8> {
        let used: Vec<u8> = self
            .saved
            .iter()
            .filter_map(|profile| profile.icon_color)
            .collect();
        Some(
            (0..ICON_COLOR_COUNT)
                .find(|color| !used.contains(color))
                .unwrap_or(0),
        )
    }
}
