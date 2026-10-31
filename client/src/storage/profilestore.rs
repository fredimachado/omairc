//! The saved Connect profiles, one `networks/<id>` group each.
//!
//! The record keeps every field the file can hold, including the ones this client's Connect
//! sheet does not edit (account, bouncer network, avatar). A profile the Qt client or the
//! TUI wrote therefore reads and writes back without losing anything.
//! The store holds no state of its own: each call reads the file, so an outside edit is
//! never masked. An ephemeral backing has no profiles.

use std::collections::BTreeMap;

use super::settings::{probe_ini, IniProbe, Settings, Status};
use super::Backing;

/// The QSettings group that owns every persisted network.
pub const NETWORKS_GROUP: &str = "networks";
/// The palette slots a network may use (`IrcNetworkProfile::iconColorCount`).
pub const ICON_COLOR_COUNT: u8 = 5;
/// The default port when a stored profile has none (`IrcNetworkProfile::defaultPort`).
pub const DEFAULT_PORT: u16 = 6697;

/// One saved network, mirroring `IrcNetworkProfile` as the file holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Profile {
    pub network_id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub tls: bool,
    pub connect_on_startup: bool,
    /// The server password is in the credential store.
    pub secret_saved: bool,
    /// The NickServ password is in the credential store.
    pub nickserv_saved: bool,
    pub nick: String,
    pub username: String,
    pub realname: String,
    pub account: String,
    pub bouncer_network: String,
    /// The autojoin channels, in the order they join.
    pub autojoin_channels: Vec<String>,
    /// The channel keys, by channel name as stored.
    pub autojoin_keys: BTreeMap<String, String>,
    /// The palette slot, or `None` when the network has none.
    pub icon_color: Option<u8>,
    pub avatar_url: String,
}

impl Profile {
    /// A profile with Qt's defaults for everything but the id.
    #[cfg(test)]
    pub fn new(network_id: &str) -> Self {
        Self {
            network_id: network_id.to_owned(),
            name: String::new(),
            host: String::new(),
            port: DEFAULT_PORT,
            tls: true,
            connect_on_startup: false,
            secret_saved: false,
            nickserv_saved: false,
            nick: String::new(),
            username: String::new(),
            realname: String::new(),
            account: String::new(),
            bouncer_network: String::new(),
            autojoin_channels: Vec::new(),
            autojoin_keys: BTreeMap::new(),
            icon_color: None,
            avatar_url: String::new(),
        }
    }
}

/// Why a profile cannot be applied (`IrcNetworkProfile::Problem`). The first failing check
/// wins, in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileProblem {
    MissingHost,
    InvalidPort,
    MissingNick,
    UnsendableIdentity,
    UnsendableAccount,
    UnsendableBouncerNetwork,
    UnsendableChannel,
}

impl Profile {
    /// The SASL login name: trimmed account, else nick, with an optional `account/network`
    /// suffix when a bouncer upstream is set (`IrcNetworkProfile::saslAccount`).
    pub fn sasl_account(&self) -> String {
        let account = self.account.trim();
        let nick = self.nick.trim();
        let name = if account.is_empty() { nick } else { account };
        let bouncer = self.bouncer_network.trim();
        if bouncer.is_empty() {
            name.to_owned()
        } else {
            format!("{name}/{bouncer}")
        }
    }

    /// The name the sidebar and the title show: the trimmed name, or the host when the name
    /// is blank (`IrcNetworkProfile::resolvedName`).
    pub fn resolved_name(&self) -> String {
        let name = self.name.trim();
        if name.is_empty() {
            self.host.trim().to_owned()
        } else {
            name.to_owned()
        }
    }

    /// The first reason this profile cannot be used, or `None` when it can. A profile
    /// with a problem is kept as a draft and never starts a session.
    pub fn problem(&self) -> Option<ProfileProblem> {
        if self.host.trim().is_empty() {
            return Some(ProfileProblem::MissingHost);
        }
        if self.port == 0 {
            return Some(ProfileProblem::InvalidPort);
        }
        let nick = self.nick.trim();
        if nick.is_empty() {
            return Some(ProfileProblem::MissingNick);
        }
        if !single_field(nick) {
            return Some(ProfileProblem::UnsendableIdentity);
        }
        let username = self.username.trim();
        let realname = self.realname.trim();
        if !username.is_empty() || !realname.is_empty() {
            // A blank username or real name falls back to the nick, as Qt does.
            let user = if username.is_empty() { nick } else { username };
            let real = if realname.is_empty() { nick } else { realname };
            if !single_field(user) || real.is_empty() || contains_forbidden(real) {
                return Some(ProfileProblem::UnsendableIdentity);
            }
        }
        if !fits_one_login_field(self.account.trim()) {
            return Some(ProfileProblem::UnsendableAccount);
        }
        if !fits_one_login_field(self.bouncer_network.trim()) {
            return Some(ProfileProblem::UnsendableBouncerNetwork);
        }
        if self.autojoin_channels.iter().any(|channel| !join_field(channel)) {
            return Some(ProfileProblem::UnsendableChannel);
        }
        None
    }

    /// Whether the profile can start a session (`IrcNetworkProfile::isComplete`).
    pub fn is_complete(&self) -> bool {
        self.problem().is_none()
    }
}

/// Qt's `IrcCommandBuilder` nick and user check: non-empty, no space, CR, LF or NUL.
fn single_field(text: &str) -> bool {
    !text.is_empty() && !contains_forbidden(text) && !text.contains(' ')
}

fn contains_forbidden(text: &str) -> bool {
    text.chars().any(|c| matches!(c, '\r' | '\n' | '\0'))
}

/// Qt's `fitsOneLoginField`: no whitespace, NUL, or slash (`tui/internal/connection/profile.go`).
fn fits_one_login_field(value: &str) -> bool {
    !value
        .chars()
        .any(|c| c.is_whitespace() || c == '\0' || c == '/')
}

/// Qt's `isJoinField`: non-empty, no leading colon, and no whitespace, comma or NUL.
fn join_field(text: &str) -> bool {
    !text.is_empty()
        && !text.starts_with(':')
        && !text
            .chars()
            .any(|c| c.is_whitespace() || c == ',' || c == '\0')
}

/// Reads and writes the `networks` group of the settings file.
pub struct ProfileStore {
    backing: Backing,
}

impl ProfileStore {
    pub fn new(backing: Backing) -> Self {
        Self { backing }
    }

    /// Every saved network, in id order (`IrcProfileStore::profiles`). A stored name that
    /// is blank falls back to the host.
    pub fn profiles(&self) -> Vec<Profile> {
        let settings = self.backing.settings();
        settings
            .child_groups(NETWORKS_GROUP)
            .into_iter()
            .map(|id| read_profile(&settings, &id))
            .collect()
    }

    /// Writes one network's group, replacing what was there (`IrcProfileStore::save`). A
    /// malformed or unwritable file is refused before anything changes, so a bad line is
    /// kept rather than rebuilt.
    pub fn save(&self, profile: &Profile) -> Status {
        if profile.network_id.is_empty() {
            return Status::Absent;
        }
        let group = format!("{NETWORKS_GROUP}/{}", profile.network_id);
        self.backing.edit(|settings| {
            settings.remove_group(&group);
            write_profile(settings, &group, profile);
        })
    }

    /// Deletes one network's group (`IrcProfileStore::remove`). A missing file or entry is
    /// `Absent`. The bytes are probed first, so a bad file is left as it is.
    pub fn remove(&self, network_id: &str) -> Status {
        let Backing::Disk(path) = &self.backing else {
            return Status::Absent;
        };
        if network_id.is_empty() || !path.exists() {
            return Status::Absent;
        }
        match probe_ini(path) {
            IniProbe::Malformed => return Status::FormatError,
            IniProbe::Unreadable => return Status::AccessError,
            IniProbe::Ok => {}
        }
        let mut settings = Settings::open(path.clone());
        if !settings
            .child_groups(NETWORKS_GROUP)
            .iter()
            .any(|id| id == network_id)
        {
            return Status::Absent;
        }
        settings.remove_group(&format!("{NETWORKS_GROUP}/{network_id}"));
        settings.sync()
    }
}

fn read_profile(settings: &Settings, id: &str) -> Profile {
    let group = format!("{NETWORKS_GROUP}/{id}");
    let host = settings.value(&group, "host");
    let stored_name = settings.value(&group, "name");
    let name = if stored_name.trim().is_empty() {
        host.clone()
    } else {
        stored_name
    };
    let port = settings.uint_value(&group, "port", u64::from(DEFAULT_PORT));
    let autojoin_channels = settings.string_list(&group, "autojoin").unwrap_or_default();
    let key_channels = settings
        .string_list(&group, "autojoinKeyChannels")
        .unwrap_or_default();
    let key_values = settings
        .string_list(&group, "autojoinKeyValues")
        .unwrap_or_default();
 // An absent or out-of-palette colour reads as no colour, as the C++ range test does.
    let icon = settings.int_value(&group, "iconColor", -1);
    let icon_color = (0..i64::from(ICON_COLOR_COUNT))
        .contains(&icon)
        .then(|| icon as u8);
    Profile {
        network_id: id.to_owned(),
        name,
        host,
        port: u16::try_from(port).unwrap_or(DEFAULT_PORT),
        tls: settings.bool_value(&group, "tls", true),
        connect_on_startup: settings.bool_value(&group, "connectOnStartup", false),
        secret_saved: settings.bool_value(&group, "secretSaved", false),
        nickserv_saved: settings.bool_value(&group, "nickServSaved", false),
        nick: settings.value(&group, "nick"),
        username: settings.value(&group, "username"),
        realname: settings.value(&group, "realname"),
        account: settings.value(&group, "account"),
        bouncer_network: settings.value(&group, "bouncerNetwork"),
        autojoin_keys: load_autojoin_keys(&autojoin_channels, &key_channels, &key_values),
        autojoin_channels,
        icon_color,
        avatar_url: settings.value(&group, "avatarUrl"),
    }
}

fn write_profile(settings: &mut Settings, group: &str, profile: &Profile) {
    if !profile.name.trim().is_empty() {
        settings.set_value(group, "name", &profile.name);
    }
    settings.set_value(group, "host", &profile.host);
    settings.set_uint(group, "port", u64::from(profile.port));
    settings.set_bool(group, "tls", profile.tls);
    settings.set_bool(group, "connectOnStartup", profile.connect_on_startup);
    settings.set_bool(group, "secretSaved", profile.secret_saved);
    settings.set_bool(group, "nickServSaved", profile.nickserv_saved);
    settings.set_value(group, "nick", &profile.nick);
    settings.set_value(group, "username", &profile.username);
    settings.set_value(group, "realname", &profile.realname);
    settings.set_value(group, "account", &profile.account);
    settings.set_value(group, "bouncerNetwork", &profile.bouncer_network);
    settings.set_string_list(group, "autojoin", &profile.autojoin_channels);
    if !profile.autojoin_keys.is_empty() {
        // QMap keeps its keys sorted, so the channels and values stay aligned on disk.
        let channels: Vec<String> = profile.autojoin_keys.keys().cloned().collect();
        let values: Vec<String> = profile.autojoin_keys.values().cloned().collect();
        settings.set_string_list(group, "autojoinKeyChannels", &channels);
        settings.set_string_list(group, "autojoinKeyValues", &values);
    }
    if let Some(color) = profile.icon_color.filter(|color| *color < ICON_COLOR_COUNT) {
        settings.set_int(group, "iconColor", i64::from(color));
    }
    if !profile.avatar_url.is_empty() {
        settings.set_value(group, "avatarUrl", &profile.avatar_url);
    }
}

/// Pairs each channel with the key stored at the first matching name, compared without
/// case. A length mismatch means the arrays disagree, so no keys are kept.
fn load_autojoin_keys(channels: &[String], names: &[String], values: &[String]) -> BTreeMap<String, String> {
    let mut keys = BTreeMap::new();
    if names.len() != values.len() {
        return keys;
    }
    for channel in channels {
        let found = names
            .iter()
            .zip(values)
            .find(|(name, value)| {
                !name.is_empty() && !value.is_empty() && name.eq_ignore_ascii_case(channel)
            });
        if let Some((_, value)) = found {
            keys.insert(channel.clone(), value.clone());
        }
    }
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::testdir::TempDir;

    fn disk(dir: &TempDir) -> Backing {
        Backing::Disk(dir.path().join("omairc.conf"))
    }

    fn sample(id: &str) -> Profile {
        Profile {
            name: "Libera".to_owned(),
            host: "irc.libera.chat".to_owned(),
            nick: "fred".to_owned(),
            username: "fredi".to_owned(),
            realname: "Fredi M".to_owned(),
            account: "fred-acct".to_owned(),
            bouncer_network: "bnc".to_owned(),
            autojoin_channels: vec!["#omarchy".to_owned(), "#lab".to_owned()],
            autojoin_keys: BTreeMap::from([("#lab".to_owned(), "hunter2".to_owned())]),
            icon_color: Some(3),
            avatar_url: "https://omairc.app/omairc128.png".to_owned(),
            ..Profile::new(id)
        }
    }

    #[test]
    fn a_profile_needs_a_host_a_port_and_a_sendable_nick_and_channels() {
        let mut profile = Profile::new("a");
        assert_eq!(profile.problem(), Some(ProfileProblem::MissingHost));
        profile.host = "irc.example".to_owned();
        assert_eq!(profile.problem(), Some(ProfileProblem::MissingNick));
        profile.nick = "fred nick".to_owned();
        assert_eq!(profile.problem(), Some(ProfileProblem::UnsendableIdentity));
        profile.nick = "fred".to_owned();
        assert!(profile.is_complete());
        profile.port = 0;
        assert_eq!(profile.problem(), Some(ProfileProblem::InvalidPort));
        profile.port = DEFAULT_PORT;
        profile.autojoin_channels = vec!["lab,omarchy".to_owned()];
        assert_eq!(profile.problem(), Some(ProfileProblem::UnsendableChannel));
    }

    #[test]
    fn stored_bouncer_network_becomes_account_slash_network_for_sasl() {
        let mut profile = Profile::new("znc");
        profile.nick = "omairc".to_owned();
        profile.account = "acct".to_owned();
        profile.bouncer_network = "freenode".to_owned();
        assert_eq!(profile.sasl_account(), "acct/freenode");

        profile.account.clear();
        assert_eq!(profile.sasl_account(), "omairc/freenode");

        profile.bouncer_network.clear();
        assert_eq!(profile.sasl_account(), "omairc");

        profile.account = " acct ".to_owned();
        profile.bouncer_network = " net ".to_owned();
        assert_eq!(profile.sasl_account(), "acct/net");
    }

    #[test]
    fn bouncer_network_with_a_slash_is_unsendable() {
        let mut profile = Profile::new("znc");
        profile.host = "znc.example".to_owned();
        profile.nick = "omairc".to_owned();
        profile.bouncer_network = "bad/network".to_owned();
        assert_eq!(profile.problem(), Some(ProfileProblem::UnsendableBouncerNetwork));
        profile.bouncer_network = "bad network".to_owned();
        assert_eq!(profile.problem(), Some(ProfileProblem::UnsendableBouncerNetwork));
    }

    #[test]
    fn a_name_is_the_host_when_it_is_blank() {
        let mut profile = sample("a");
        profile.name = "  ".to_owned();
        assert_eq!(profile.resolved_name(), "irc.libera.chat");
        profile.name = " Libera ".to_owned();
        assert_eq!(profile.resolved_name(), "Libera");
    }

    #[test]
    fn a_saved_profile_reads_back_field_for_field() {
        let dir = TempDir::new("profiles");
        let store = ProfileStore::new(disk(&dir));
        let profile = sample("network-1");
        assert_eq!(store.save(&profile), Status::Written);
        assert_eq!(store.profiles(), [profile]);
    }

    #[test]
    fn an_identical_resave_after_remove_group_leaves_disk_unchanged() {
        use std::thread;
        use std::time::Duration;

        let dir = TempDir::new("profiles-idempotent");
        let backing = disk(&dir);
        let path = dir.path().join("omairc.conf");
        let profile = sample("network-1");
        let store = ProfileStore::new(backing.clone());
        assert_eq!(store.save(&profile), Status::Written);
        let before = std::fs::read(&path).expect("read");
        let before_mtime = std::fs::metadata(&path).expect("stat").modified().expect("mtime");
        thread::sleep(Duration::from_millis(1100));
        assert_eq!(store.save(&profile), Status::Written);
        assert_eq!(std::fs::read(&path).expect("read after"), before);
        assert_eq!(
            std::fs::metadata(&path).expect("stat after").modified().expect("mtime"),
            before_mtime
        );
    }

    #[test]
    fn the_file_uses_the_qt_group_and_key_names() {
        let dir = TempDir::new("profiles-shape");
        let store = ProfileStore::new(disk(&dir));
        store.save(&sample("network-1"));
        let text = std::fs::read_to_string(dir.path().join("omairc.conf")).expect("written");
        assert!(text.contains("[networks]"), "{text}");
        assert!(text.contains("network-1\\host=irc.libera.chat"), "{text}");
        assert!(text.contains("network-1\\connectOnStartup=false"), "{text}");
        assert!(text.contains("network-1\\iconColor=3"), "{text}");
    }

    #[test]
    fn a_blank_name_falls_back_to_the_host_on_read() {
        let dir = TempDir::new("profiles-blank");
        let store = ProfileStore::new(disk(&dir));
        let mut profile = sample("network-2");
        profile.name = "   ".to_owned();
        store.save(&profile);
        assert_eq!(store.profiles()[0].name, "irc.libera.chat");
    }

    #[test]
    fn keys_without_a_matching_channel_are_dropped_and_mismatched_arrays_drop_all() {
        let channels = vec!["#Lab".to_owned(), "#omarchy".to_owned()];
        let keys = load_autojoin_keys(&channels, &["#lab".to_owned()], &["pw".to_owned()]);
        assert_eq!(keys, BTreeMap::from([("#Lab".to_owned(), "pw".to_owned())]));
        let mismatched = load_autojoin_keys(&channels, &["#lab".to_owned()], &[]);
        assert!(mismatched.is_empty());
    }

    #[test]
    fn an_out_of_palette_colour_reads_as_none() {
        let dir = TempDir::new("profiles-colour");
        let path = dir.path().join("omairc.conf");
        std::fs::write(&path, "[networks]\nn1\\host=h\nn1\\iconColor=9\n").unwrap();
        let store = ProfileStore::new(Backing::Disk(path));
        assert_eq!(store.profiles()[0].icon_color, None);
    }

    #[test]
    fn remove_deletes_one_group_and_reports_absent_for_a_stranger() {
        let dir = TempDir::new("profiles-remove");
        let store = ProfileStore::new(disk(&dir));
        store.save(&sample("a"));
        store.save(&sample("b"));
        assert_eq!(store.remove("a"), Status::Written);
        assert_eq!(store.remove("a"), Status::Absent);
        assert_eq!(store.remove(""), Status::Absent);
        let ids: Vec<String> = store.profiles().into_iter().map(|p| p.network_id).collect();
        assert_eq!(ids, ["b"]);
    }

    #[test]
    fn a_malformed_file_is_left_alone() {
        let dir = TempDir::new("profiles-bad");
        let path = dir.path().join("omairc.conf");
        let bad = b"[networks\nnot an ini at all \" \n";
        std::fs::write(&path, bad).unwrap();
        let store = ProfileStore::new(Backing::Disk(path.clone()));
        assert_eq!(store.save(&sample("x")), Status::FormatError);
        assert_eq!(store.remove("x"), Status::FormatError);
        assert_eq!(std::fs::read(&path).unwrap(), bad, "the bytes are untouched");
    }

    #[test]
    fn an_ephemeral_store_keeps_nothing() {
        let store = ProfileStore::new(Backing::Ephemeral);
        assert_eq!(store.save(&sample("x")), Status::Written);
        assert!(store.profiles().is_empty());
        assert_eq!(store.remove("x"), Status::Absent);
    }
}
