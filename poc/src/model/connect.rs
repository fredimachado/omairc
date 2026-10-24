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
//! `Controller::start`. The server and NickServ passwords live in the draft until Apply.
//! Apply gives them to the OS keychain (`Controller::save_profile`), never to the settings
//! file, and the sheet words where each one landed. Like Qt, the draft drops them on select,
//! add, discard and a successful apply.
//!
//! The window owns one `ConnectState` for its whole life, so an unapplied draft
//! survives closing the sheet, as Qt's does. `Shell::connect_open` says whether
//! the sheet is showing.

use std::collections::BTreeMap;

use super::prefs::PrefName;
use crate::storage::credentialstore::CredentialState;
use crate::storage::profilestore::Profile as StoredProfile;
use crate::storage::settings::Status;
use crate::util::Secret as SecretText;

mod state;

#[cfg(test)]
mod tests;

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
pub const DISCONNECT_LABEL: &str = "Disconnect";
pub const APPLY_LABEL: &str = "Apply";

/// Why Apply is refused. The rules live with the stored profile, so the draft and the
/// saved network are checked the same way (`storage::profilestore::ProfileProblem`).
pub use crate::storage::profilestore::ProfileProblem as Problem;

impl Problem {
    /// The line under the form, copied from `problemText`.
    pub fn text(self) -> &'static str {
        match self {
            Problem::MissingHost => "Host is required",
            Problem::InvalidPort => "Port is required",
            Problem::MissingNick => "Nick is required",
            Problem::UnsendableIdentity => "Nick, username, or real name cannot be sent",
            Problem::UnsendableAccount => "Account cannot contain a space or a slash",
            Problem::UnsendableBouncerNetwork => "Bouncer network cannot contain a space or a slash",
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
    /// Stops the selected network's live or reconnecting session. Shown only while
    /// that session exists (`stops`), and it leaves the profile as it is.
    Disconnect,
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
/// profiles from these. A launch with saved networks reads them from the profile store instead.
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
    /// The server password, held until Apply. Apply gives it to the keychain, never the file.
    pub password: SecretText,
    /// The NickServ password, held until Apply as `password`.
    pub nickserv_password: SecretText,
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
            password: SecretText::default(),
            nickserv_password: SecretText::default(),
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
            password: SecretText::default(),
            nickserv_password: SecretText::default(),
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
            password: SecretText::default(),
            nickserv_password: SecretText::default(),
        }
    }

    /// The first reason this profile cannot be applied, or `None` when it can.
    pub fn problem(&self) -> Option<Problem> {
        self.to_stored(None).problem()
    }

    /// The profile as the file keeps it. The fields the sheet does not edit (account,
    /// bouncer network, avatar, and the keys of channels that stay) come from `existing`, so
    /// saving a network keeps what the Qt client or the TUI wrote there.
    pub fn to_stored(&self, existing: Option<&StoredProfile>) -> StoredProfile {
        let autojoin_channels = self.channels();
        let autojoin_keys = existing
            .map(|stored| {
                stored
                    .autojoin_keys
                    .iter()
                    .filter(|(channel, _)| autojoin_channels.contains(channel))
                    .map(|(channel, key)| (channel.clone(), key.clone()))
                    .collect()
            })
            .unwrap_or_default();
        StoredProfile {
            network_id: self.network_id.clone(),
            name: self.name.trim().to_owned(),
            host: self.host.trim().to_owned(),
            port: port_number(&self.port),
            tls: self.tls,
            connect_on_startup: self.connect_on_startup,
            secret_saved: existing.is_some_and(|stored| stored.secret_saved),
            nickserv_saved: existing.is_some_and(|stored| stored.nickserv_saved),
            nick: self.nick.trim().to_owned(),
            username: self.username.trim().to_owned(),
            realname: self.realname.trim().to_owned(),
            account: existing
                .map(|stored| stored.account.clone())
                .unwrap_or_default(),
            bouncer_network: existing
                .map(|stored| stored.bouncer_network.clone())
                .unwrap_or_default(),
            autojoin_channels,
            autojoin_keys,
            icon_color: self.icon_color.filter(|color| *color < ICON_COLOR_COUNT),
            avatar_url: existing
                .map(|stored| stored.avatar_url.clone())
                .unwrap_or_default(),
        }
    }

    /// The sheet's view of a saved network. Its secrets are not in the file, so they start
    /// empty, and the channel list is the autojoin list the file holds.
    pub fn from_stored(stored: &StoredProfile) -> Self {
        Profile {
            network_id: stored.network_id.clone(),
            name: stored.name.clone(),
            host: stored.host.clone(),
            port: stored.port.to_string(),
            tls: stored.tls,
            nick: stored.nick.clone(),
            username: stored.username.clone(),
            realname: stored.realname.clone(),
            autojoin: stored.autojoin_channels.join(" "),
            connect_on_startup: stored.connect_on_startup,
            icon_color: stored.icon_color,
            chantypes: DEFAULT_CHANTYPES.to_string(),
            password: SecretText::default(),
            nickserv_password: SecretText::default(),
        }
    }

    /// The profile a first run starts from (`IrcNetworkProfile::suggested`): the Freenode
    /// host, TLS on, and `#omarchy` to autojoin. The name is the host.
    pub fn suggested(network_id: &str) -> Self {
        let host = "chat.freenode.net";
        Profile {
            name: host.to_owned(),
            host: host.to_owned(),
            autojoin: "#omarchy".to_owned(),
            ..Profile::blank(network_id)
        }
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

/// The first `network-N` id none of the saved profiles uses.
fn fresh_network_id_among(saved: &[Profile]) -> String {
    (1..)
        .map(|n| format!("network-{n}"))
        .find(|id| !saved.iter().any(|profile| profile.network_id == *id))
        .unwrap_or_default()
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
    /// Each saved network's secret slots, as the credential store left them.
    secrets: BTreeMap<String, SecretStates>,
    /// The draft's password was typed since the last Apply.
    password_edited: bool,
    /// The draft's NickServ password was typed since the last Apply.
    nickserv_edited: bool,
    /// The sentence the last Apply left for each network (`persistenceMessages`).
    persistence: BTreeMap<String, String>,
}

/// One secret slot of a network, as the sheet words it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SecretStates {
    pub password: CredentialState,
    pub nickserv: CredentialState,
}

/// The two secret slots a network has.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Secret {
    Password,
    NickServ,
}

/// One slot as the status line reads it: its noun, whether text is held for it, whether it was
/// edited since Apply, and where the store left it.
struct SecretSlot {
    noun: &'static str,
    held: bool,
    edited: bool,
    state: CredentialState,
}

impl SecretSlot {
    fn idle(&self) -> bool {
        !self.held && !self.edited
    }

    /// One slot's sentence (`IrcConnection::secretStatus`).
    fn sentence(&self) -> String {
        let stored = matches!(
            self.state,
            CredentialState::Missing | CredentialState::Available
        );
        if self.edited && self.held && stored {
            return format!("{} changed; apply to save securely", self.noun);
        }
        match &self.state {
            CredentialState::Available => format!("{} saved securely", self.noun),
            CredentialState::Missing if self.held => {
                format!("{} is session-only until applied", self.noun)
            }
            CredentialState::Missing => String::new(),
            CredentialState::SessionOnly => {
                format!("secure storage unavailable; {} is session-only", self.noun)
            }
            CredentialState::Error(_) if self.held => {
                format!("secure storage error; {} is session-only", self.noun)
            }
            CredentialState::Error(_) => {
                format!("secure storage error; {} is not saved", self.noun)
            }
        }
    }
}

/// The credential line under the form: the password's sentence, joined with the NickServ one
/// (`IrcConnection::credentialStatus`).
fn combined_secret_status(password: &SecretSlot, nickserv: &SecretSlot) -> String {
    let left = password.sentence();
    let right = if nickserv.idle() {
        String::new()
    } else {
        nickserv.sentence()
    };
    if right.is_empty() {
        return left;
    }
    let shown_left = if password.idle() { String::new() } else { left };
    if shown_left.is_empty() {
        return right;
    }
    if shown_left == right {
        return shown_left;
    }
    let both_session_only = password.state == CredentialState::SessionOnly
        && nickserv.state == CredentialState::SessionOnly;
    if password.held && nickserv.held && both_session_only {
        return "secure storage unavailable; password and NickServ are session-only".to_owned();
    }
    format!("{shown_left}; {right}")
}

/// The sentence an Apply leaves under the form (`recordApplyPersistence`). A saved file with
/// a session that started leaves none, so the sheet closes.
fn apply_sentence(status: Status, started: bool) -> Option<String> {
    match (status, started) {
        (Status::Absent, _) | (Status::Written, true) => None,
        (Status::Written, false) => Some("These settings were saved successfully.".to_owned()),
        (failed, accepted) => {
            let reason = match failed {
                Status::AccessError => "The settings file could not be written.",
                Status::FormatError => "The settings file could not be read.",
                _ => return None,
            };
            Some(if accepted {
                format!("Connected using these settings. {reason}")
            } else {
                reason.to_owned()
            })
        }
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
        seed(
            "omarchy",
            "fred",
            1,
            &["#omarchy", "#desktop", "#ricing", "#help"],
        ),
        seed("oftc", "oak", 2, &["#omarchy", "#lab", "#build"]),
    ]
}
