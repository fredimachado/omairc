//! The Connect sheet's model: the saved networks, the draft being edited, the
//! rail, the tabs and the keyboard walk.
//!
//! A port of the parts of `IrcConnection` the sheet drives (`select`, `add`,
//! `discard`, `removeSelected`, `apply`, `dirty`, `rosterRows`), and of the
//! strings and checks in `src/qml/ConnectionSheet.qml`, `ConnectionField.qml`,
//! `FieldHelpMark.qml`, `PreferenceToggleRow.qml` and
//! `src/irc/ircnetworkprofile.cpp`.
//!
//! Nothing here connects. Apply returns the saved profile, and the shell hands it to
//! `Controller::start`. The server and NickServ passwords live in the draft for the
//! session only: the PoC has no keychain, so they are never saved, written to
//! the world or sent. Like Qt, the draft drops them on select, add, discard and
//! a successful apply.
//!
//! The window owns one `ConnectState` for its whole life, so an unapplied draft
//! survives closing the sheet, as Qt's does. `Shell::connect_open` says whether
//! the sheet is showing.

use super::prefs::PrefName;

/// The icon colour slots Qt's `IrcNetworkProfile::iconColorCount` offers.
pub const ICON_COLOR_COUNT: u8 = 5;

/// Qt's port for a new network (`IrcNetworkProfile::create`).
const DEFAULT_PORT: &str = "6697";
/// Qt's CHANTYPES when a server sends none.
const DEFAULT_CHANTYPES: &str = "#&";
/// The host every demo network is seeded with (`seedProfile`).
const SEED_HOST: &str = "irc.example";

// Sheet strings, copied from `ConnectionSheet.qml`.
pub const NEW_NETWORK_LABEL: &str = "New network";
pub const NETWORKS_LABEL: &str = "Networks";
pub const ADD_NETWORK_LABEL: &str = "+ Add network";
pub const SHORTCUTS_LABEL: &str = "Shortcuts";
pub const SHORTCUTS_KEYS: &str = "Ctrl + /";
pub const TITLE: &str = "Connect";
pub const PREFERENCES_TITLE: &str = "Preferences";
pub const TLS_LABEL: &str = "TLS";
pub const CONNECT_ON_STARTUP_LABEL: &str = "Connect automatically on startup";
pub const REMOVE_LABEL: &str = "Remove";
pub const DISCARD_LABEL: &str = "Discard";
pub const APPLY_LABEL: &str = "Apply";

/// Why Apply is refused. Qt's `IrcNetworkProfile::Problem`, without the account
/// and bouncer fields the PoC does not edit. The first failing check wins, in
/// this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Problem {
    MissingHost,
    InvalidPort,
    MissingNick,
    UnsendableIdentity,
    UnsendableChannel,
}

impl Problem {
    /// The line under the form, copied from `problemText`.
    pub fn text(self) -> &'static str {
        match self {
            Problem::MissingHost => "Host is required",
            Problem::InvalidPort => "Port is required",
            Problem::MissingNick => "Nick is required",
            Problem::UnsendableIdentity => "Nick, username, or real name cannot be sent",
            Problem::UnsendableChannel => "An autojoin channel cannot be sent",
        }
    }
}

/// The two tabs. Ctrl+Tab steps forward through them and wraps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConnectTab {
    Connection,
    Preferences,
}

impl ConnectTab {
    /// Tab order, which Ctrl+Tab and the focus walk follow.
    pub const ALL: [ConnectTab; 2] = [ConnectTab::Connection, ConnectTab::Preferences];

    pub fn label(self) -> &'static str {
        match self {
            ConnectTab::Connection => "Connection",
            ConnectTab::Preferences => "Preferences",
        }
    }

    /// The glyph drawn before the label. Only Connection has one (`glyph: "#"`).
    pub fn glyph(self) -> Option<&'static str> {
        match self {
            ConnectTab::Connection => Some("#"),
            ConnectTab::Preferences => None,
        }
    }

    /// The tab `delta` places on, wrapping at both ends.
    pub fn step(self, delta: isize) -> ConnectTab {
        let index = Self::ALL.iter().position(|tab| *tab == self).unwrap_or(0) as isize;
        Self::ALL[(index + delta).rem_euclid(Self::ALL.len() as isize) as usize]
    }
}

/// A text field on the Connection tab. Each has a label, and two of them have a
/// help mark and a badge, copied from the Qt sheet.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConnectField {
    Name,
    Host,
    Port,
    Nick,
    Username,
    RealName,
    Autojoin,
    ServerPassword,
    NickServPassword,
}

impl ConnectField {
    pub fn label(self) -> &'static str {
        match self {
            ConnectField::Name => "Name",
            ConnectField::Host => "Host",
            ConnectField::Port => "Port",
            ConnectField::Nick => "Nick",
            ConnectField::Username => "Username",
            ConnectField::RealName => "Real name",
            ConnectField::Autojoin => "Autojoin",
            ConnectField::ServerPassword => "Server password",
            ConnectField::NickServPassword => "NickServ password",
        }
    }

    /// The help mark's text, or `None` when the field has no mark.
    pub fn help(self) -> Option<&'static str> {
        match self {
            ConnectField::ServerPassword => Some(
                "Used as the SASL secret when no NickServ password is set; otherwise sent as PASS while connecting.",
            ),
            ConnectField::NickServPassword => Some(
                "Preferred SASL secret. Sent as NickServ IDENTIFY when SASL did not succeed.",
            ),
            _ => None,
        }
    }

    /// The small chip beside the label.
    pub fn badge(self) -> Option<&'static str> {
        match self {
            ConnectField::ServerPassword => Some("PASS"),
            _ => None,
        }
    }

    /// Whether the text is masked.
    pub fn secret(self) -> bool {
        matches!(
            self,
            ConnectField::ServerPassword | ConnectField::NickServPassword
        )
    }
}

/// One focus stop of the sheet, in Tab order. Enter in a field, switch or help
/// mark walks to the next stop. Enter or Space on a button, a rail row or the
/// add row acts on it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Stop {
    Tab(ConnectTab),
    /// A rail row, by network id. The unsaved draft's row is one too.
    Network(String),
    AddNetwork,
    ShortcutsHint,
    Field(ConnectField),
    FieldHelp(ConnectField),
    Tls,
    ConnectOnStartup,
    Preference(PrefName),
    PreferenceHelp(PrefName),
    Remove,
    Discard,
    Apply,
}

/// One toggle on the Preferences tab. Qt shows these three; `joins` has no row
/// there and stays a `/pref` setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreferenceRow {
    pub name: PrefName,
    pub label: &'static str,
    pub help: &'static str,
}

/// The Preferences tab rows, in Qt's order.
pub fn preference_rows() -> [PreferenceRow; 3] {
    [
        PreferenceRow {
            name: PrefName::Directs,
            label: PrefName::Directs.label(),
            help: "Reopens direct messages you had open last session after each connect. Only tabs you opened or sent in are remembered.",
        },
        PreferenceRow {
            name: PrefName::Avatars,
            label: PrefName::Avatars.label(),
            help: "Fetches IRCv3 avatar images from peer metadata. Nick initials still show when this is off or no image loads. Turn off to keep avatar hosts from seeing your IP on busy channels.",
        },
        PreferenceRow {
            name: PrefName::Unread,
            label: PrefName::Unread.label(),
            help: "Scrolls to the New messages marker when you switch conversations. Already-focused buffers and Status are left at the end.",
        },
    ]
}

/// Shown under the form when a rail click or walk asks for another network while
/// the draft is dirty. Qt refuses the switch silently; the PoC says why.
pub const SWITCH_REFUSED_TEXT: &str = "Apply or discard your edits before switching networks.";

/// The label on the Remove button: `Remove`, or `Remove <name>?` once armed.
pub fn remove_label(armed: bool, display_name: &str) -> String {
    if armed {
        format!("Remove {display_name}?")
    } else {
        REMOVE_LABEL.to_string()
    }
}

/// What the sheet knows of one network when it opens: the controller's nick, icon
/// colour, CHANTYPES and the channels it has joined. The sheet builds its saved
/// profiles from these (until the profile store of Phase 8 keeps them on disk).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NetworkSeed {
    pub network_id: String,
    pub nick: String,
    pub icon_color: Option<u8>,
    pub chantypes: String,
    /// The channels a new profile joins on connect.
    pub channels: Vec<String>,
}

/// One network's profile: what the form edits, and what Apply saves. Text fields
/// hold what was typed. `port` is text too, so a half-typed port is not forced
/// to zero while the user types.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub network_id: String,
    pub name: String,
    pub host: String,
    pub port: String,
    pub tls: bool,
    pub nick: String,
    pub username: String,
    pub realname: String,
    /// Channels separated by spaces or commas. A channel name without its
    /// prefix gets `#`.
    pub autojoin: String,
    pub connect_on_startup: bool,
    /// Index into the nick palette, as `Network::icon_color`.
    pub icon_color: Option<u8>,
    /// The network's CHANTYPES, which decide what an autojoin token is. Not edited.
    pub chantypes: String,
    /// The server password. Session only: never saved, written or sent.
    pub password: String,
    /// The NickServ password. Session only, as `password`.
    pub nickserv_password: String,
}

impl Profile {
    /// A new network with Qt's defaults and nothing typed.
    pub fn blank(network_id: &str) -> Self {
        Profile {
            network_id: network_id.to_string(),
            name: String::new(),
            host: String::new(),
            port: DEFAULT_PORT.to_string(),
            tls: true,
            nick: String::new(),
            username: String::new(),
            realname: String::new(),
            autojoin: String::new(),
            connect_on_startup: false,
            icon_color: None,
            chantypes: DEFAULT_CHANTYPES.to_string(),
            password: String::new(),
            nickserv_password: String::new(),
        }
    }

    /// A demo network's profile, seeded as `seedProfile` does: host `irc.example`,
    /// TLS on, the nick as username and real name, and every channel joined as
    /// autojoin. The name is empty, so the roster label is the host.
    fn from_seed(seed: &NetworkSeed) -> Self {
        Profile {
            network_id: seed.network_id.clone(),
            name: String::new(),
            host: SEED_HOST.to_string(),
            port: DEFAULT_PORT.to_string(),
            tls: true,
            nick: seed.nick.clone(),
            username: seed.nick.clone(),
            realname: seed.nick.clone(),
            autojoin: seed.channels.join(" "),
            connect_on_startup: false,
            icon_color: seed.icon_color,
            chantypes: seed.chantypes.clone(),
            password: String::new(),
            nickserv_password: String::new(),
        }
    }

    /// The name with spaces trimmed, or the host when the name is empty.
    pub fn resolved_name(&self) -> String {
        let name = self.name.trim();
        if name.is_empty() {
            self.host.trim().to_string()
        } else {
            name.to_string()
        }
    }

    /// The profile as it is saved (Qt `normalized`): text trimmed, the port
    /// written as a number when it parses, autojoin reduced to its channels, and
    /// the secrets dropped.
    pub fn normalized(&self) -> Profile {
        let port = match port_number(&self.port) {
            0 => self.port.trim().to_string(),
            number => number.to_string(),
        };
        Profile {
            network_id: self.network_id.clone(),
            name: self.name.trim().to_string(),
            host: self.host.trim().to_string(),
            port,
            tls: self.tls,
            nick: self.nick.trim().to_string(),
            username: self.username.trim().to_string(),
            realname: self.realname.trim().to_string(),
            autojoin: self.channels().join(" "),
            connect_on_startup: self.connect_on_startup,
            icon_color: self.icon_color,
            chantypes: self.chantypes.clone(),
            password: String::new(),
            nickserv_password: String::new(),
        }
    }

    /// The first reason this profile cannot be applied, or `None` when it can.
    pub fn problem(&self) -> Option<Problem> {
        let profile = self.normalized();
        if profile.host.is_empty() {
            return Some(Problem::MissingHost);
        }
        if port_number(&profile.port) == 0 {
            return Some(Problem::InvalidPort);
        }
        if profile.nick.is_empty() {
            return Some(Problem::MissingNick);
        }
        if !single_field(&profile.nick) {
            return Some(Problem::UnsendableIdentity);
        }
        if !profile.username.is_empty() || !profile.realname.is_empty() {
            // A blank username or real name falls back to the nick, as Qt does.
            let user = if profile.username.is_empty() {
                profile.nick.as_str()
            } else {
                profile.username.as_str()
            };
            let real = if profile.realname.is_empty() {
                profile.nick.as_str()
            } else {
                profile.realname.as_str()
            };
            if !single_field(user) || real.is_empty() || contains_forbidden(real) {
                return Some(Problem::UnsendableIdentity);
            }
        }
        if self.channels().iter().any(|channel| !join_field(channel)) {
            return Some(Problem::UnsendableChannel);
        }
        None
    }

    /// The autojoin channels, with prefixes added and keys dropped.
    fn channels(&self) -> Vec<String> {
        parse_autojoin(&self.autojoin, &self.chantypes)
    }
}

/// The autojoin text as channel names (Qt `splitAutojoin`). Tokens split on
/// commas and whitespace. A token with a channel prefix is kept as it is, and any
/// other token gets `#`. A token that follows a channel is a key, and is dropped.
pub fn parse_autojoin(text: &str, chantypes: &str) -> Vec<String> {
    let mut channels = Vec::new();
    let mut previous_was_channel = false;
    for token in text
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|token| !token.is_empty())
    {
        let is_channel = token.chars().next().is_some_and(|c| chantypes.contains(c));
        if !is_channel && previous_was_channel {
            continue;
        }
        channels.push(if is_channel {
            token.to_string()
        } else {
            format!("#{token}")
        });
        previous_was_channel = is_channel;
    }
    channels
}

/// A port as Qt reads it: a whole number from 1 to 65535. Anything else is 0,
/// which validation reports as "Port is required".
pub fn port_number(text: &str) -> u16 {
    text.trim().parse::<u16>().unwrap_or(0)
}

/// Qt's `IrcCommandBuilder` nick and user check: non-empty, no space, CR, LF or NUL.
fn single_field(text: &str) -> bool {
    !text.is_empty() && !contains_forbidden(text) && !text.contains(' ')
}

fn contains_forbidden(text: &str) -> bool {
    text.chars().any(|c| matches!(c, '\r' | '\n' | '\0'))
}

/// Qt's `isJoinField`: non-empty, no leading colon, and no whitespace, comma or NUL.
fn join_field(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with(':')
        && !text
            .chars()
            .any(|c| c.is_whitespace() || c == ',' || c == '\0')
}

/// The label a network shows in the rail and the sidebar (Qt `rosterDisplayName`).
/// The resolved name, with ` · <nick>` added when another saved network already
/// has that name, ignoring case. A profile with no name at all reads `New network`.
fn roster_label(profile: &Profile, saved: &[Profile]) -> String {
    let label = profile.resolved_name();
    if label.is_empty() {
        return NEW_NETWORK_LABEL.to_string();
    }
    let folded = label.to_lowercase();
    let clash = saved.iter().any(|other| {
        other.network_id != profile.network_id && other.resolved_name().to_lowercase() == folded
    });
    let nick = profile.nick.trim();
    if clash && !nick.is_empty() {
        format!("{label} · {nick}")
    } else {
        label
    }
}

/// One rail row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RailRow {
    pub network_id: String,
    pub label: String,
    /// False for the unsaved draft at the end of the rail.
    pub stored: bool,
    pub selected: bool,
    pub icon_color: Option<u8>,
}

/// What Enter did on a rail row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailEnter {
    /// The row was selected and armed. A second Enter on it applies.
    Selected,
    /// The row was already selected and armed: the caller applies.
    Apply,
}

/// What Remove did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoveOutcome {
    /// The first press arms the button. Nothing is removed yet.
    Armed,
    /// The second press removed this network.
    Removed(String),
    /// Nothing to remove: the selection is a draft, or no other network has a
    /// conversation to show afterwards.
    Refused,
}

/// The whole Connect sheet state, held for the window's life.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectState {
    /// The saved networks, in rail order. Each one mirrors a network the controller holds.
    saved: Vec<Profile>,
    /// The editable copy of the selected network. Its secrets are session only.
    draft: Profile,
    /// The selected network's id. It is the draft's id, which may be unsaved.
    selected: String,
    /// The network that was selected when Add ran, which Discard returns to.
    added_from: Option<String>,
    tab: ConnectTab,
    /// Remove was pressed once. Qt's `connectionRemoveArmed`.
    remove_armed: bool,
    /// The rail row Enter armed. Qt's per-row `applyArmed`.
    rail_armed: Option<String>,
    /// The stop the window last reported as focused.
    focus: Option<Stop>,
    /// A stop the window should focus at its next render. Taken by `take_focus_request`.
    focus_request: Option<Stop>,
    /// A switch to another network was refused because the draft is dirty. Cleared
    /// by the next switch, Apply, Discard, Add or Remove.
    switch_refused: bool,
}

impl ConnectState {
    /// The sheet state for the networks the controller holds: one saved profile per
    /// network, with the first network selected and no draft.
    pub fn seed(networks: &[NetworkSeed]) -> Self {
        let saved: Vec<Profile> = networks.iter().map(Profile::from_seed).collect();
        let selected = saved
            .first()
            .map(|profile| profile.network_id.clone())
            .unwrap_or_default();
        let draft = saved.first().cloned().unwrap_or_else(|| Profile::blank(""));
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
        }
    }

    /// The saved profiles in rail order. The sheet reads the rail through `rows`.
    pub fn saved(&self) -> &[Profile] {
        &self.saved
    }

    /// The profile the form shows. The sheet reads the fields through `text`.
    #[cfg(test)]
    pub fn draft(&self) -> &Profile {
        &self.draft
    }

    /// The selected network's id. The sheet reads it through `rows`.
    #[cfg(test)]
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
            ConnectField::ServerPassword => &self.draft.password,
            ConnectField::NickServPassword => &self.draft.nickserv_password,
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
            ConnectField::ServerPassword => self.draft.password = value,
            ConnectField::NickServPassword => self.draft.nickserv_password = value,
        }
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
        self.selected = profile.network_id.clone();
        self.draft = profile.clone();
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
    /// when `can_remove` allows it, and Add only when `can_add` does.
    pub fn stops(&self, populated: &[String]) -> Vec<Stop> {
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
                if self.can_remove(populated) {
                    stops.push(Stop::Remove);
                }
                stops.push(Stop::Discard);
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
        from: Option<&Stop>,
        backwards: bool,
    ) -> Option<Stop> {
        let stops = self.stops(populated);
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
        }
    }

    /// The first `network-N` id no saved network uses.
    fn fresh_network_id(&self) -> String {
        (1..)
            .map(|n| format!("network-{n}"))
            .find(|id| !self.is_saved(id))
            .unwrap_or_default()
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

/// The two demo networks as the sheet's seeds: what `ConnectState::seed` gets at
/// launch on the demo controller, with their channels as autojoin.
#[cfg(test)]
pub fn demo_seeds() -> Vec<NetworkSeed> {
    let seed = |id: &str, nick: &str, icon_color: u8, channels: &[&str]| NetworkSeed {
        network_id: id.to_string(),
        nick: nick.to_string(),
        icon_color: Some(icon_color),
        chantypes: "#".to_string(),
        channels: channels.iter().map(|c| c.to_string()).collect(),
    };
    vec![
        seed("omarchy", "fred", 1, &["#omarchy", "#desktop", "#ricing", "#help"]),
        seed("oftc", "oak", 2, &["#omarchy", "#lab", "#build"]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    

    fn seeded() -> (Vec<String>, ConnectState) {
        let state = ConnectState::seed(&demo_seeds());
        let populated = vec!["omarchy".to_string(), "oftc".to_string()];
        (populated, state)
    }

    fn stored_ids(state: &ConnectState) -> Vec<String> {
        state.saved().iter().map(|p| p.network_id.clone()).collect()
    }

    // --- Seed --------------------------------------------------------------

    #[test]
    fn seed_mirrors_each_demo_network_as_qt_seeds_it() {
        let (_, state) = seeded();
        assert_eq!(stored_ids(&state), ["omarchy", "oftc"]);
        assert_eq!(state.selected(), "omarchy");
        let omarchy = &state.saved()[0];
        assert_eq!(omarchy.name, "");
        assert_eq!(omarchy.host, "irc.example");
        assert_eq!(omarchy.port, "6697");
        assert!(omarchy.tls);
        assert_eq!(omarchy.nick, "fred");
        assert_eq!(omarchy.username, "fred");
        assert_eq!(omarchy.realname, "fred");
        assert_eq!(omarchy.autojoin, "#omarchy #desktop #ricing #help");
        assert!(!omarchy.connect_on_startup);
        assert_eq!(omarchy.icon_color, Some(1));
        assert_eq!(state.saved()[1].icon_color, Some(2));
        assert_eq!(state.saved()[1].autojoin, "#omarchy #lab #build");
    }

    #[test]
    fn a_seeded_draft_is_clean_and_applicable() {
        let (_, state) = seeded();
        assert!(!state.dirty());
        assert_eq!(state.problem(), None);
        assert_eq!(state.draft(), &state.saved()[0]);
    }

    #[test]
    fn rail_labels_match_the_sidebar_names_of_the_demo() {
        let (_, state) = seeded();
        let labels: Vec<String> = state.rows().into_iter().map(|row| row.label).collect();
        // Both demo networks answer for the host, so the clash rule adds each nick: the
        // same names the sidebar draws.
        assert_eq!(labels, ["irc.example · fred", "irc.example · oak"]);
    }

    // --- Strings and tables copied from Qt ---------------------------------

    #[test]
    fn field_labels_help_and_badges_match_the_qt_sheet() {
        assert_eq!(ConnectField::Name.label(), "Name");
        assert_eq!(ConnectField::Host.label(), "Host");
        assert_eq!(ConnectField::Port.label(), "Port");
        assert_eq!(ConnectField::Nick.label(), "Nick");
        assert_eq!(ConnectField::Username.label(), "Username");
        assert_eq!(ConnectField::RealName.label(), "Real name");
        assert_eq!(ConnectField::Autojoin.label(), "Autojoin");
        assert_eq!(ConnectField::ServerPassword.label(), "Server password");
        assert_eq!(ConnectField::NickServPassword.label(), "NickServ password");
        assert_eq!(
            ConnectField::ServerPassword.help(),
            Some("Used as the SASL secret when no NickServ password is set; otherwise sent as PASS while connecting.")
        );
        assert_eq!(
            ConnectField::NickServPassword.help(),
            Some("Preferred SASL secret. Sent as NickServ IDENTIFY when SASL did not succeed.")
        );
        assert_eq!(ConnectField::ServerPassword.badge(), Some("PASS"));
        assert_eq!(ConnectField::NickServPassword.badge(), None);
        assert!(ConnectField::ServerPassword.secret());
        assert!(ConnectField::NickServPassword.secret());
        assert!(!ConnectField::Host.secret());
        assert_eq!(ConnectField::Host.help(), None);
    }

    #[test]
    fn preference_rows_reuse_the_pref_labels_with_qt_help() {
        let rows = preference_rows();
        let names: Vec<PrefName> = rows.iter().map(|row| row.name).collect();
        assert_eq!(
            names,
            [PrefName::Directs, PrefName::Avatars, PrefName::Unread]
        );
        assert_eq!(rows[0].label, "Reopen direct messages on startup");
        assert_eq!(rows[1].label, "Show peer avatars");
        assert_eq!(rows[2].label, "Open conversations at unread");
        assert_eq!(
            rows[0].help,
            "Reopens direct messages you had open last session after each connect. Only tabs you opened or sent in are remembered."
        );
        assert_eq!(
            rows[2].help,
            "Scrolls to the New messages marker when you switch conversations. Already-focused buffers and Status are left at the end."
        );
    }

    #[test]
    fn tabs_label_glyph_and_wrap() {
        assert_eq!(ConnectTab::Connection.label(), "Connection");
        assert_eq!(ConnectTab::Preferences.label(), "Preferences");
        assert_eq!(ConnectTab::Connection.glyph(), Some("#"));
        assert_eq!(ConnectTab::Preferences.glyph(), None);
        assert_eq!(ConnectTab::Connection.step(1), ConnectTab::Preferences);
        assert_eq!(ConnectTab::Preferences.step(1), ConnectTab::Connection);
        assert_eq!(ConnectTab::Connection.step(-1), ConnectTab::Preferences);
    }

    #[test]
    fn remove_label_arms_with_the_name() {
        assert_eq!(remove_label(false, "irc.example · fred"), "Remove");
        assert_eq!(
            remove_label(true, "irc.example · fred"),
            "Remove irc.example · fred?"
        );
    }

    #[test]
    fn problem_text_matches_qt() {
        assert_eq!(Problem::MissingHost.text(), "Host is required");
        assert_eq!(Problem::InvalidPort.text(), "Port is required");
        assert_eq!(Problem::MissingNick.text(), "Nick is required");
        assert_eq!(
            Problem::UnsendableIdentity.text(),
            "Nick, username, or real name cannot be sent"
        );
        assert_eq!(
            Problem::UnsendableChannel.text(),
            "An autojoin channel cannot be sent"
        );
    }

    // --- Validation --------------------------------------------------------

    #[test]
    fn an_empty_nick_disables_apply_with_qt_wording() {
        let (_, mut state) = seeded();
        state.set_text(ConnectField::Nick, "");
        assert_eq!(state.problem(), Some(Problem::MissingNick));
        assert_eq!(state.problem().map(Problem::text), Some("Nick is required"));
        state.set_text(ConnectField::Nick, "   ");
        assert_eq!(state.problem(), Some(Problem::MissingNick));
    }

    #[test]
    fn host_then_port_then_nick_is_the_order_of_the_checks() {
        let mut profile = Profile::blank("n");
        profile.nick = "fred".into();
        assert_eq!(profile.problem(), Some(Problem::MissingHost));
        profile.host = "  ".into();
        assert_eq!(profile.problem(), Some(Problem::MissingHost));
        profile.host = "irc.example".into();
        profile.port = "".into();
        assert_eq!(profile.problem(), Some(Problem::InvalidPort));
        profile.port = "0".into();
        assert_eq!(profile.problem(), Some(Problem::InvalidPort));
        profile.port = "66a".into();
        assert_eq!(profile.problem(), Some(Problem::InvalidPort));
        profile.port = "70000".into();
        assert_eq!(profile.problem(), Some(Problem::InvalidPort));
        profile.port = "6697".into();
        assert_eq!(profile.problem(), None);
        profile.nick = "".into();
        profile.port = "0".into();
        assert_eq!(profile.problem(), Some(Problem::InvalidPort));
    }

    #[test]
    fn port_numbers_parse_like_qt_numbers_for_whole_values() {
        assert_eq!(port_number("6697"), 6697);
        assert_eq!(port_number(" 6697 "), 6697);
        assert_eq!(port_number("0"), 0);
        assert_eq!(port_number(""), 0);
        assert_eq!(port_number("66a"), 0);
        assert_eq!(port_number("6697.5"), 0);
        assert_eq!(port_number("65535"), 65535);
        assert_eq!(port_number("65536"), 0);
    }

    #[test]
    fn a_nick_with_a_space_cannot_be_sent() {
        let (_, mut state) = seeded();
        state.set_text(ConnectField::Nick, "fr ed");
        assert_eq!(state.problem(), Some(Problem::UnsendableIdentity));
        state.set_text(ConnectField::Nick, "fr\0ed");
        assert_eq!(state.problem(), Some(Problem::UnsendableIdentity));
    }

    #[test]
    fn username_and_real_name_are_checked_when_given() {
        let (_, mut state) = seeded();
        state.set_text(ConnectField::Username, "fr ed");
        assert_eq!(state.problem(), Some(Problem::UnsendableIdentity));
        state.set_text(ConnectField::Username, "fred");
        state.set_text(ConnectField::RealName, "Fred\nDev");
        assert_eq!(state.problem(), Some(Problem::UnsendableIdentity));
        state.set_text(ConnectField::RealName, "Fred Dev");
        assert_eq!(state.problem(), None, "a real name may have spaces");
    }

    #[test]
    fn a_blank_username_and_real_name_fall_back_to_the_nick() {
        let mut profile = Profile::blank("n");
        profile.host = "irc.example".into();
        profile.nick = "fred".into();
        profile.username = "".into();
        profile.realname = "".into();
        assert_eq!(profile.problem(), None);
        profile.username = "fr ed".into();
        assert_eq!(
            profile.problem(),
            Some(Problem::UnsendableIdentity),
            "a username that is set is checked, a blank one is not"
        );
    }

    #[test]
    fn an_autojoin_channel_with_a_nul_cannot_be_sent() {
        let mut profile = Profile::blank("n");
        profile.host = "irc.example".into();
        profile.nick = "fred".into();
        profile.autojoin = "#good #bad\0name".into();
        assert_eq!(profile.problem(), Some(Problem::UnsendableChannel));
        profile.autojoin = "#good, omarchy".into();
        assert_eq!(profile.problem(), None);
    }

    // --- Autojoin parsing and normalization ---------------------------------

    #[test]
    fn autojoin_adds_hash_and_drops_keys_after_a_channel() {
        assert_eq!(parse_autojoin("omarchy lab", "#&"), ["#omarchy", "#lab"]);
        assert_eq!(parse_autojoin("a, b", "#&"), ["#a", "#b"]);
        assert_eq!(parse_autojoin("#a secret #b", "#&"), ["#a", "#b"]);
        assert_eq!(parse_autojoin("#a  ,, &b", "#&"), ["#a", "&b"]);
        assert!(parse_autojoin("  ,  ", "#&").is_empty());
    }

    #[test]
    fn autojoin_uses_the_networks_chantypes() {
        assert_eq!(parse_autojoin("!x", "!"), ["!x"]);
        assert_eq!(parse_autojoin("#x", "!"), ["##x"]);
    }

    #[test]
    fn normalized_trims_text_and_drops_secrets() {
        let mut profile = Profile::blank("n");
        profile.name = "  libera ".into();
        profile.host = " irc.libera.chat ".into();
        profile.port = " 6667 ".into();
        profile.nick = " fred ".into();
        profile.autojoin = "omarchy,  lab".into();
        profile.password = "hunter2".into();
        profile.nickserv_password = "secret".into();
        let saved = profile.normalized();
        assert_eq!(saved.name, "libera");
        assert_eq!(saved.host, "irc.libera.chat");
        assert_eq!(saved.port, "6667");
        assert_eq!(saved.nick, "fred");
        assert_eq!(saved.autojoin, "#omarchy #lab");
        assert_eq!(saved.password, "");
        assert_eq!(saved.nickserv_password, "");
    }

    // --- Dirty and text editing --------------------------------------------

    #[test]
    fn editing_makes_the_draft_dirty_and_typing_it_back_clears_it() {
        let (_, mut state) = seeded();
        state.set_text(ConnectField::Name, "libera");
        assert!(state.dirty());
        state.set_text(ConnectField::Name, "");
        assert!(!state.dirty());
    }

    #[test]
    fn surrounding_spaces_are_not_an_edit() {
        let (_, mut state) = seeded();
        state.set_text(ConnectField::Host, "irc.example ");
        assert!(!state.dirty(), "Qt compares the normalized profiles");
    }

    #[test]
    fn secrets_do_not_make_the_draft_dirty() {
        let (_, mut state) = seeded();
        state.set_text(ConnectField::ServerPassword, "hunter2");
        assert!(!state.dirty());
        assert_eq!(state.text(ConnectField::ServerPassword), "hunter2");
    }

    #[test]
    fn every_text_field_reads_back_what_was_typed() {
        let (_, mut state) = seeded();
        let fields = [
            (ConnectField::Name, "n"),
            (ConnectField::Host, "h"),
            (ConnectField::Port, "7000"),
            (ConnectField::Nick, "nick"),
            (ConnectField::Username, "user"),
            (ConnectField::RealName, "real"),
            (ConnectField::Autojoin, "#x"),
            (ConnectField::ServerPassword, "p"),
            (ConnectField::NickServPassword, "q"),
        ];
        for (field, value) in fields {
            state.set_text(field, value);
            assert_eq!(state.text(field), value);
        }
        state.set_tls(false);
        state.set_connect_on_startup(true);
        assert!(!state.tls());
        assert!(state.connect_on_startup());
    }

    // --- Apply -------------------------------------------------------------

    #[test]
    fn apply_writes_the_name_and_nick_into_the_world() {
        let (_, mut state) = seeded();
        state.set_text(ConnectField::Nick, "fred2");
        let profile = state.apply().expect("complete profile applies");
        assert_eq!(profile.network_id, "omarchy");
        assert_eq!(profile.nick, "fred2");
        let saved = state
            .saved()
            .iter()
            .find(|saved| saved.network_id == "omarchy")
            .expect("omarchy is saved");
        assert_eq!(saved.nick, "fred2");
    }

    #[test]
    fn a_rename_that_clashes_adds_the_nick_and_a_clash_removed_drops_it() {
        let (_, mut state) = seeded();
        let labels = |state: &ConnectState| -> Vec<String> {
            state.rows().into_iter().map(|row| row.label).collect()
        };
        assert!(state.select("oftc"));
        state.set_text(ConnectField::Name, "libera");
        state.apply().expect("oftc applies");
        assert_eq!(labels(&state), ["irc.example", "libera"]);

        assert!(state.select("omarchy"));
        state.set_text(ConnectField::Name, "libera");
        state.apply().expect("omarchy applies");
        assert_eq!(labels(&state), ["libera · fred", "libera · oak"]);

        state.set_text(ConnectField::Name, "");
        state.apply().expect("omarchy clears its name");
        assert_eq!(labels(&state), ["irc.example", "libera"]);
    }

    #[test]
    fn an_incomplete_draft_is_refused_and_changes_nothing() {
        let (_, mut state) = seeded();
        let before = state.saved().to_vec();
        state.set_text(ConnectField::Nick, "");
        assert_eq!(state.apply(), Err(Problem::MissingNick));
        assert_eq!(state.saved(), before.as_slice());
    }

    #[test]
    fn apply_clears_the_session_secrets_from_the_draft() {
        let (_populated, mut state) = seeded();
        state.set_text(ConnectField::ServerPassword, "hunter2");
        state.apply().unwrap();
        assert_eq!(state.text(ConnectField::ServerPassword), "");
        assert_eq!(state.saved()[0].password, "");
    }

    #[test]
    fn apply_never_changes_the_connection_state_of_the_world() {
        let (populated, mut state) = seeded();
        assert!(state.apply().is_ok());
        assert_eq!(populated, ["omarchy", "oftc"]);
    }

    // --- Select, walk and the rail -----------------------------------------

    #[test]
    fn select_moves_to_a_saved_network_when_clean() {
        let (_, mut state) = seeded();
        assert!(state.select("oftc"));
        assert_eq!(state.selected(), "oftc");
        assert_eq!(state.draft().nick, "oak");
        assert!(!state.select("oftc"), "the same row is not a move");
        assert!(!state.select(""), "no id is not a move");
        assert!(!state.select("nowhere"));
    }

    #[test]
    fn select_is_refused_while_the_draft_is_dirty_as_in_qt() {
        let (_, mut state) = seeded();
        state.set_text(ConnectField::Name, "edited");
        assert!(!state.select("oftc"));
        assert_eq!(state.selected(), "omarchy");
        assert_eq!(state.draft().name, "edited");
    }

    #[test]
    fn selecting_resets_remove_armed() {
        let (populated, mut state) = seeded();
        assert_eq!(state.remove(&populated), RemoveOutcome::Armed);
        assert!(state.remove_armed());
        state.select("oftc");
        assert!(!state.remove_armed());
    }

    #[test]
    fn alt_arrows_walk_the_rail_and_wrap() {
        let (_, mut state) = seeded();
        assert_eq!(state.walk_network(1).as_deref(), Some("oftc"));
        assert_eq!(state.selected(), "oftc");
        assert_eq!(
            state.walk_network(1).as_deref(),
            Some("omarchy"),
            "wraps forward"
        );
        assert_eq!(
            state.walk_network(-1).as_deref(),
            Some("oftc"),
            "wraps back"
        );
    }

    #[test]
    fn walking_while_dirty_reports_the_row_but_keeps_the_selection() {
        let (_, mut state) = seeded();
        state.set_text(ConnectField::Host, "edited.example");
        assert_eq!(state.walk_network(1).as_deref(), Some("oftc"));
        assert_eq!(state.selected(), "omarchy");
    }

    #[test]
    fn rail_enter_selects_and_arms_then_applies() {
        let (_, mut state) = seeded();
        assert_eq!(state.rail_enter("oftc"), RailEnter::Selected);
        assert_eq!(state.selected(), "oftc");
        assert_eq!(state.rail_enter("oftc"), RailEnter::Apply);
    }

    #[test]
    fn leaving_the_rail_disarms_enter() {
        let (_, mut state) = seeded();
        state.rail_enter("oftc");
        state.rail_left();
        assert_eq!(state.rail_enter("oftc"), RailEnter::Selected);
    }

    #[test]
    fn an_unselected_row_enter_arms_but_does_not_apply() {
        let (_, mut state) = seeded();
        assert_eq!(state.rail_enter("oftc"), RailEnter::Selected);
        assert_eq!(state.rail_enter("omarchy"), RailEnter::Selected);
    }

    // --- Add, discard ------------------------------------------------------

    #[test]
    fn add_makes_an_unsaved_draft_with_qt_defaults() {
        let (_, mut state) = seeded();
        assert!(state.add());
        let draft = state.draft();
        assert_eq!(draft.host, "");
        assert_eq!(draft.port, "6697");
        assert!(draft.tls);
        assert_eq!(draft.icon_color, Some(0), "the first free colour");
        assert!(!state.is_saved(state.selected()));
        assert_eq!(stored_ids(&state), ["omarchy", "oftc"], "not saved yet");
        assert_eq!(
            state.take_focus_request(),
            Some(Stop::Field(ConnectField::Name))
        );
    }

    #[test]
    fn the_draft_row_is_listed_last_as_new_network() {
        let (_, mut state) = seeded();
        state.add();
        let rows = state.rows();
        assert_eq!(rows.len(), 3);
        assert!(!rows[2].stored);
        assert!(rows[2].selected);
        assert_eq!(rows[2].label, "New network");
        assert!(!rows[0].selected);
    }

    #[test]
    fn add_is_refused_while_dirty_or_on_an_unsaved_draft() {
        let (_, mut state) = seeded();
        state.set_text(ConnectField::Name, "edited");
        assert!(!state.add());
        assert_eq!(state.rows().len(), 2);
        state.discard();
        assert!(state.add());
        assert!(!state.add(), "a second add while the draft is unsaved");
    }

    #[test]
    fn discard_of_an_unsaved_draft_returns_to_the_network_it_came_from() {
        let (_, mut state) = seeded();
        state.select("oftc");
        state.add();
        state.discard();
        assert_eq!(state.selected(), "oftc");
        assert_eq!(state.rows().len(), 2);
        assert!(!state.dirty());
    }

    #[test]
    fn discard_of_a_saved_network_reverts_its_edits() {
        let (_, mut state) = seeded();
        state.set_text(ConnectField::Name, "edited");
        state.discard();
        assert_eq!(state.draft().name, "");
        assert!(!state.dirty());
        assert_eq!(state.selected(), "omarchy");
    }

    #[test]
    fn discard_clears_remove_armed_and_secrets() {
        let (populated, mut state) = seeded();
        state.set_text(ConnectField::ServerPassword, "pw");
        state.remove(&populated);
        state.discard();
        assert!(!state.remove_armed());
        assert_eq!(state.text(ConnectField::ServerPassword), "");
    }

    #[test]
    fn applying_a_new_network_adds_it_to_the_world_with_its_colour() {
        let (_, mut state) = seeded();
        assert!(state.add());
        state.set_text(ConnectField::Host, "irc.libera.example");
        state.set_text(ConnectField::Nick, "zed");
        let profile = state.apply().expect("complete");
        assert_eq!(state.saved().len(), 3);
        assert_eq!(profile.icon_color, Some(0), "the first colour no saved network uses");
    }

    #[test]
    fn a_new_network_with_the_demo_host_gets_the_nick_suffix() {
        let (_, mut state) = seeded();
        assert!(state.add());
        state.set_text(ConnectField::Host, "irc.example");
        state.set_text(ConnectField::Nick, "zed");
        state.apply().expect("complete");
        let labels: Vec<String> = state.rows().into_iter().map(|row| row.label).collect();
        assert_eq!(
            labels,
            ["irc.example · fred", "irc.example · oak", "irc.example · zed"]
        );
    }

    #[test]
    fn a_second_new_network_gets_the_next_free_colour_and_id() {
        let (_populated, mut state) = seeded();
        state.add();
        state.set_text(ConnectField::Host, "a.example");
        state.set_text(ConnectField::Nick, "fred");
        state.apply().unwrap();
        state.add();
        assert_eq!(state.selected(), "network-2");
        assert_eq!(state.draft().icon_color, Some(3));
    }

    // --- Remove ------------------------------------------------------------

    #[test]
    fn remove_arms_on_the_first_press_and_removes_on_the_second() {
        let (populated, mut state) = seeded();
        assert_eq!(state.remove(&populated), RemoveOutcome::Armed);
        assert_eq!(stored_ids(&state), ["omarchy", "oftc"]);
        assert_eq!(
            state.remove(&populated),
            RemoveOutcome::Removed("omarchy".to_string())
        );
        assert_eq!(stored_ids(&state), ["oftc"]);
        assert!(!state.is_saved("omarchy"));
        assert_eq!(state.selected(), "oftc");
        assert!(!state.remove_armed());
    }

    #[test]
    fn removing_selects_the_network_now_at_the_same_position() {
        let (populated, mut state) = seeded();
        state.select("oftc");
        state.remove(&populated);
        assert_eq!(
            state.remove(&populated),
            RemoveOutcome::Removed("oftc".into())
        );
        assert_eq!(
            state.selected(),
            "omarchy",
            "the last network takes the place"
        );
        assert_eq!(state.draft().nick, "fred");
    }

    #[test]
    fn remove_is_refused_when_no_other_network_has_a_conversation() {
        let (mut populated, mut state) = seeded();
        populated.retain(|id| id != "oftc");
        assert!(!state.can_remove(&populated));
        assert_eq!(state.remove(&populated), RemoveOutcome::Refused);
        assert!(!state.remove_armed());
        assert_eq!(stored_ids(&state).len(), 2);
    }

    #[test]
    fn remove_is_refused_for_an_unsaved_draft() {
        let (populated, mut state) = seeded();
        state.add();
        assert!(!state.can_remove(&populated));
        assert_eq!(state.remove(&populated), RemoveOutcome::Refused);
    }

    // --- Tabs and focus ----------------------------------------------------

    #[test]
    fn ctrl_tab_steps_the_tab_and_asks_for_its_button() {
        let (_, mut state) = seeded();
        assert_eq!(state.cycle_tab(1), ConnectTab::Preferences);
        assert_eq!(
            state.take_focus_request(),
            Some(Stop::Tab(ConnectTab::Preferences))
        );
        assert_eq!(state.cycle_tab(1), ConnectTab::Connection, "wraps");
        assert_eq!(
            state.take_focus_request(),
            Some(Stop::Tab(ConnectTab::Connection))
        );
        state.set_tab(ConnectTab::Preferences);
        assert_eq!(state.tab(), ConnectTab::Preferences);
        assert_eq!(state.take_focus_request(), None, "clicks do not move focus");
    }

    #[test]
    fn the_connection_walk_follows_the_qt_focus_chain() {
        let (populated, state) = seeded();
        let stops = state.stops(&populated);
        assert_eq!(
            stops,
            vec![
                Stop::Tab(ConnectTab::Connection),
                Stop::Tab(ConnectTab::Preferences),
                Stop::Network("omarchy".into()),
                Stop::Network("oftc".into()),
                Stop::AddNetwork,
                Stop::ShortcutsHint,
                Stop::Field(ConnectField::Name),
                Stop::Field(ConnectField::Host),
                Stop::Field(ConnectField::Port),
                Stop::Tls,
                Stop::Field(ConnectField::Nick),
                Stop::Field(ConnectField::Username),
                Stop::Field(ConnectField::RealName),
                Stop::Field(ConnectField::Autojoin),
                Stop::ConnectOnStartup,
                Stop::FieldHelp(ConnectField::ServerPassword),
                Stop::Field(ConnectField::ServerPassword),
                Stop::FieldHelp(ConnectField::NickServPassword),
                Stop::Field(ConnectField::NickServPassword),
                Stop::Remove,
                Stop::Discard,
                Stop::Apply,
            ]
        );
    }

    #[test]
    fn the_walk_drops_add_while_dirty_and_remove_when_not_allowed() {
        let (mut populated, mut state) = seeded();
        state.set_text(ConnectField::Name, "edited");
        let stops = state.stops(&populated);
        assert!(!stops.contains(&Stop::AddNetwork));
        populated.retain(|id| id != "oftc");
        assert!(!state.stops(&populated).contains(&Stop::Remove));
    }

    #[test]
    fn the_preferences_walk_is_its_three_switches_and_their_marks() {
        let (populated, mut state) = seeded();
        state.set_tab(ConnectTab::Preferences);
        assert_eq!(
            state.stops(&populated),
            vec![
                Stop::Tab(ConnectTab::Connection),
                Stop::Tab(ConnectTab::Preferences),
                Stop::Preference(PrefName::Directs),
                Stop::PreferenceHelp(PrefName::Directs),
                Stop::Preference(PrefName::Avatars),
                Stop::PreferenceHelp(PrefName::Avatars),
                Stop::Preference(PrefName::Unread),
                Stop::PreferenceHelp(PrefName::Unread),
            ]
        );
    }

    #[test]
    fn enter_and_tab_walk_forward_and_shift_tab_walks_back_with_wrap() {
        let (populated, state) = seeded();
        let step = |from: Stop, backwards: bool| state.step_stop(&populated, Some(&from), backwards);
        assert_eq!(
            step(Stop::Field(ConnectField::Name), false),
            Some(Stop::Field(ConnectField::Host))
        );
        assert_eq!(
            step(Stop::Field(ConnectField::Autojoin), false),
            Some(Stop::ConnectOnStartup)
        );
        assert_eq!(
            step(Stop::Apply, false),
            Some(Stop::Tab(ConnectTab::Connection)),
            "forward wraps to the first stop"
        );
        assert_eq!(
            step(Stop::Tab(ConnectTab::Connection), true),
            Some(Stop::Apply),
            "back wraps to the last stop"
        );
        assert_eq!(
            step(Stop::Field(ConnectField::Host), true),
            Some(Stop::Field(ConnectField::Name))
        );
    }

    #[test]
    fn a_stop_off_the_showing_tab_falls_back_to_the_ends() {
        let (populated, state) = seeded();
        let off = Stop::Preference(PrefName::Directs);
        assert_eq!(
            state.step_stop(&populated, Some(&off), false),
            Some(Stop::Tab(ConnectTab::Connection))
        );
        assert_eq!(
            state.step_stop(&populated, Some(&off), true),
            Some(Stop::Apply)
        );
    }

    #[test]
    fn initial_focus_is_nick_when_it_is_empty_else_name() {
        let (_, mut state) = seeded();
        assert_eq!(state.initial_focus(), Stop::Field(ConnectField::Name));
        state.set_text(ConnectField::Nick, "");
        assert_eq!(state.initial_focus(), Stop::Field(ConnectField::Nick));
    }

    #[test]
    fn restore_focus_returns_to_the_last_reported_stop() {
        let (_, mut state) = seeded();
        state.restore_focus();
        assert_eq!(
            state.take_focus_request(),
            Some(Stop::Field(ConnectField::Name))
        );
        state.set_focus(Some(Stop::Field(ConnectField::Autojoin)));
        state.restore_focus();
        assert_eq!(
            state.take_focus_request(),
            Some(Stop::Field(ConnectField::Autojoin))
        );
        assert_eq!(state.take_focus_request(), None, "a request is taken once");
    }

    #[test]
    fn request_focus_is_taken_once() {
        let (_, mut state) = seeded();
        state.request_focus(Stop::Apply);
        assert_eq!(state.take_focus_request(), Some(Stop::Apply));
        assert_eq!(state.take_focus_request(), None);
    }

    // --- Focus steps, rail stepping and the refused switch -----------------

    #[test]
    fn step_stop_without_a_focused_stop_starts_at_the_ends() {
        let (populated, state) = seeded();
        assert_eq!(
            state.step_stop(&populated, None, false),
            Some(Stop::Tab(ConnectTab::Connection))
        );
        assert_eq!(state.step_stop(&populated, None, true), Some(Stop::Apply));
    }

    #[test]
    fn step_stop_walks_from_a_stop_on_the_tab_both_ways() {
        let (populated, state) = seeded();
        let from = Stop::Field(ConnectField::Host);
        assert_eq!(
            state.step_stop(&populated, Some(&from), false),
            Some(Stop::Field(ConnectField::Port))
        );
        assert_eq!(
            state.step_stop(&populated, Some(&from), true),
            Some(Stop::Field(ConnectField::Name))
        );
    }

    #[test]
    fn rail_stops_are_the_rows_then_add_while_it_shows() {
        let (_, mut state) = seeded();
        assert_eq!(
            state.rail_stops(),
            vec![
                Stop::Network("omarchy".into()),
                Stop::Network("oftc".into()),
                Stop::AddNetwork,
            ]
        );
        state.add();
        // A dirty or unsaved draft cannot add again, so the Add row is hidden.
        assert_eq!(
            state.rail_stops().last(),
            Some(&Stop::Network("network-1".into()))
        );
    }

    #[test]
    fn rail_up_and_down_clamp_at_the_ends_without_wrapping() {
        let (_, state) = seeded();
        let first = Stop::Network("omarchy".into());
        let add = Stop::AddNetwork;
        assert_eq!(state.rail_step(&first, -1), Some(first.clone()));
        assert_eq!(
            state.rail_step(&first, 1),
            Some(Stop::Network("oftc".into()))
        );
        assert_eq!(state.rail_step(&add, 1), Some(add.clone()));
        assert_eq!(state.rail_step(&Stop::Apply, 1), None, "not a rail stop");
    }

    #[test]
    fn rail_home_and_end_are_its_first_and_last_stops() {
        let (_, state) = seeded();
        assert_eq!(state.rail_edge(false), Some(Stop::Network("omarchy".into())));
        assert_eq!(state.rail_edge(true), Some(Stop::AddNetwork));
    }

    #[test]
    fn a_refused_switch_raises_the_notice_until_the_draft_is_settled() {
        let (_populated, mut state) = seeded();
        assert!(!state.switch_refused());
        state.set_text(ConnectField::Host, "edited.example");
        assert!(!state.select("oftc"));
        assert!(state.switch_refused(), "a dirty draft refuses the switch");
        assert_eq!(
            SWITCH_REFUSED_TEXT,
            "Apply or discard your edits before switching networks."
        );
        state.discard();
        assert!(!state.switch_refused(), "Discard settles the draft");
        state.set_text(ConnectField::Host, "edited.example");
        assert!(!state.select("oftc"));
        state.apply().unwrap();
        assert!(!state.switch_refused(), "Apply settles the draft");
    }

    #[test]
    fn a_switch_that_moves_clears_the_notice() {
        let (_, mut state) = seeded();
        state.set_text(ConnectField::Host, "edited.example");
        state.select("oftc");
        assert!(state.switch_refused());
        state.set_text(ConnectField::Host, "irc.example");
        assert!(!state.dirty());
        assert!(state.select("oftc"));
        assert!(!state.switch_refused());
    }

    #[test]
    fn a_no_op_select_is_not_a_refusal() {
        let (_, mut state) = seeded();
        state.set_text(ConnectField::Name, "edited");
        assert!(!state.select("omarchy"), "the same row is not a move");
        assert!(!state.switch_refused(), "and it is not a refused switch");
        assert!(!state.select("nowhere"));
        assert!(!state.switch_refused(), "an unknown row is not refused either");
    }
}
