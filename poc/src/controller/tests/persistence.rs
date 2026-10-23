//! The Phase 8 persistence tests. Each one builds a controller over a settings file in a
//! scratch folder, drops it, and builds another over the same file, as a relaunch does. The
//! transports are loopbacks the test can read and feed, and the factory keeps them by network,
//! so a test reaches the connection a session got. The Go cases these port are the profile,
//! open-direct, preference and network-order tests under `tui/internal/`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use chrono::{TimeZone, Utc};

use crate::controller::controller::Controller;
use crate::controller::persistence::{SaveOutcome, SecretEdits};
use crate::controller::transports::{TransportFactory, LIVE_IRC_UNAVAILABLE};
use crate::desktop::notifier::NullNotifier;
use crate::irc::autoaway::AutoawayConfig;
use crate::irc::membershipnoise::MembershipNoise;
use crate::irc::pref::{PrefName, PrefValue};
use crate::session::clock::{Clock, FakeClock};
use crate::session::loopback::LoopbackTransport;
use crate::session::transport::Transport;
use crate::storage::credentialstore::{CredentialState, MemoryCredentialStore};
use crate::storage::profilestore::Profile;
use crate::storage::scrollplace::ScrollPlace;
use crate::storage::settings::Status;
use crate::storage::Backing;
use crate::util::testdir::TempDir;

const LIBERA: &str = "libera";
const OFTC: &str = "oftc";

/// Hands out loopbacks by network id and keeps them, so a test can feed or read a session's
/// connection. Cloning shares the same table.
#[derive(Clone, Default)]
struct Loopbacks(Rc<RefCell<BTreeMap<String, Rc<LoopbackTransport>>>>);

impl Loopbacks {
    fn get(&self, network_id: &str) -> Rc<LoopbackTransport> {
        self.0
            .borrow()
            .get(network_id)
            .cloned()
            .expect("the network was given a transport")
    }
}

impl TransportFactory for Loopbacks {
    fn open(&self, network_id: &str) -> Result<Rc<dyn Transport>, String> {
        let transport = Rc::new(LoopbackTransport::new());
        self.0
            .borrow_mut()
            .insert(network_id.to_owned(), transport.clone());
        Ok(transport)
    }
}

fn clock() -> Rc<dyn Clock> {
    Rc::new(FakeClock::new(
        Utc.with_ymd_and_hms(2026, 10, 21, 9, 0, 0).unwrap(),
    ))
}

/// A settings file in `dir`, the backing a launch reads.
fn disk(dir: &TempDir) -> Backing {
    Backing::Disk(dir.path().join("omairc.conf"))
}

/// A controller over `backing`, loaded the way a launch loads it.
fn launch(backing: &Backing, transports: Box<dyn TransportFactory>) -> Controller {
    let mut controller = Controller::with_backing(
        clock(),
        Box::new(NullNotifier),
        transports,
        backing.clone(),
    );
    // A disk launch would use the OS keychain. A test keeps secrets in memory, so it never
    // writes to the user's Credential Manager, Keychain or Secret Service.
    controller.credentials = Box::new(MemoryCredentialStore::default());
    controller.load_stored();
    controller
}

/// A complete profile on `libera.chat`, with one autojoin channel.
fn libera(id: &str) -> Profile {
    Profile {
        name: "Libera".to_owned(),
        host: "irc.libera.chat".to_owned(),
        nick: "fred".to_owned(),
        autojoin_channels: vec!["#omarchy".to_owned()],
        ..Profile::new(id)
    }
}

fn save(controller: &mut Controller, profile: Profile) -> SaveOutcome {
    controller.save_profile(profile, SecretEdits::default())
}

fn conversation_targets(controller: &Controller) -> Vec<String> {
    controller
        .conversations()
        .into_iter()
        .map(|row| row.target)
        .collect()
}

/// Feeds one server line to a network's connection and pumps what it queued.
fn feed(controller: &mut Controller, loopbacks: &Loopbacks, network_id: &str, line: &str) {
    let transport = loopbacks.get(network_id);
    transport.complete_connect();
    let mut bytes = line.as_bytes().to_vec();
    bytes.extend_from_slice(b"\r\n");
    transport.inject_bytes(&bytes);
    controller.tick();
}

#[test]
fn a_saved_profile_comes_back_on_relaunch_with_its_session() {
    let dir = TempDir::new("relaunch-profile");
    let backing = disk(&dir);
    let loopbacks = Loopbacks::default();
    {
        let mut first = launch(&backing, Box::new(loopbacks.clone()));
        assert!(first.stored_profiles().is_empty(), "a fresh file holds no networks");
        assert_eq!(save(&mut first, libera(LIBERA)).status, Status::Written);
        assert!(first.start(LIBERA), "the loopback accepts the connection");
    }

    let second = launch(&backing, Box::new(loopbacks.clone()));
    let stored = second.stored_profiles();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].host, "irc.libera.chat");
    assert_eq!(stored[0].nick, "fred");
    assert_eq!(stored[0].autojoin_channels, ["#omarchy"]);
    assert!(second.session(LIBERA).is_some(), "a complete profile registers its session");
    assert_eq!(second.network_name(LIBERA), "Libera", "the profile names the network");
}

#[test]
fn a_refused_start_keeps_the_profile_and_says_why() {
    let dir = TempDir::new("relaunch-refused");
    let backing = disk(&dir);
    {
        let mut first = launch(&backing, Box::new(crate::controller::transports::LiveTransports));
        save(&mut first, libera(LIBERA));
        assert!(!first.start(LIBERA));
        assert_eq!(first.last_error_for(LIBERA), Some(LIVE_IRC_UNAVAILABLE));
    }
    let second = launch(&backing, Box::new(crate::controller::transports::LiveTransports));
    assert_eq!(second.stored_profiles().len(), 1, "the refusal does not remove the profile");
    assert!(second.session(LIBERA).is_none(), "no transport, so no session");
}

#[test]
fn a_secret_is_held_for_the_run_and_never_written_to_the_file() {
    let dir = TempDir::new("secrets");
    let backing = disk(&dir);
    let mut first = launch(&backing, Box::new(Loopbacks::default()));
    let outcome = first.save_profile(
        libera(LIBERA),
        SecretEdits {
            password: Some("hunter2".to_owned()),
            nickserv: None,
        },
    );
    assert_eq!(outcome.password, CredentialState::SessionOnly);
    assert_eq!(outcome.nickserv, CredentialState::Missing);
    let text = std::fs::read_to_string(dir.path().join("omairc.conf")).expect("written");
    assert!(!text.contains("hunter2"), "the password is not in the file: {text}");
    assert!(text.contains("secretSaved=false"), "no durable store exists: {text}");
    // A relaunch has no secret, so a start from the file would not carry one.
    let second = launch(&backing, Box::new(Loopbacks::default()));
    assert_eq!(second.stored_profiles()[0].secret_saved, false);
}

#[test]
fn forgetting_a_network_removes_its_profile_from_the_file() {
    let dir = TempDir::new("forget");
    let backing = disk(&dir);
    {
        let mut first = launch(&backing, Box::new(Loopbacks::default()));
        save(&mut first, libera(LIBERA));
        save(&mut first, libera(OFTC));
        assert!(first.forget_network(LIBERA));
    }
    let second = launch(&backing, Box::new(Loopbacks::default()));
    let ids: Vec<String> = second
        .stored_profiles()
        .into_iter()
        .map(|profile| profile.network_id)
        .collect();
    assert_eq!(ids, [OFTC]);
}

#[test]
fn only_the_networks_that_ask_connect_on_launch() {
    let dir = TempDir::new("startup");
    let backing = disk(&dir);
    let loopbacks = Loopbacks::default();
    {
        let mut first = launch(&backing, Box::new(loopbacks.clone()));
        save(&mut first, libera(LIBERA));
        save(
            &mut first,
            Profile {
                connect_on_startup: true,
                ..libera(OFTC)
            },
        );
    }
    let mut second = launch(&backing, Box::new(loopbacks.clone()));
    assert_eq!(second.activate_startup(), 1, "only oftc asked");
    assert!(second.can_disconnect(OFTC));
    assert!(!second.can_disconnect(LIBERA));
}

#[test]
fn an_autojoin_edit_is_saved_into_the_profile() {
    let dir = TempDir::new("autojoin");
    let backing = disk(&dir);
    {
        let mut first = launch(&backing, Box::new(Loopbacks::default()));
        save(&mut first, libera(LIBERA));
        first.persist_autojoin(
            LIBERA,
            vec!["#omarchy".to_owned(), "#lab".to_owned()],
            BTreeMap::from([("#lab".to_owned(), "hunter2".to_owned())]),
        );
    }
    let second = launch(&backing, Box::new(Loopbacks::default()));
    let stored = &second.stored_profiles()[0];
    assert_eq!(stored.autojoin_channels, ["#omarchy", "#lab"]);
    assert_eq!(stored.autojoin_keys.get("#lab").map(String::as_str), Some("hunter2"));
}

#[test]
fn open_queries_come_back_after_the_motd_and_closed_ones_stay_closed() {
    let dir = TempDir::new("directs");
    let backing = disk(&dir);
    let loopbacks = Loopbacks::default();
    {
        let mut first = launch(&backing, Box::new(loopbacks.clone()));
        save(&mut first, libera(LIBERA));
        assert!(first.start(LIBERA));
        feed(&mut first, &loopbacks, LIBERA, ":irc.libera.chat 001 fred :Welcome");
        first.select_conversation(LIBERA, "#omarchy");
        assert!(first.open_direct("anna"));
        assert!(first.open_direct("dax"));
        first.select_conversation(LIBERA, "dax");
        assert!(first.close_direct(), "a direct message closes");
    }

    let mut second = launch(&backing, Box::new(loopbacks.clone()));
    assert!(second.start(LIBERA));
    assert!(
        !conversation_targets(&second).iter().any(|target| target == "anna"),
        "nothing is restored before the MOTD ends"
    );
    feed(&mut second, &loopbacks, LIBERA, ":irc.libera.chat 376 fred :End of MOTD");
    let targets = conversation_targets(&second);
    assert!(targets.iter().any(|target| target == "anna"), "anna is restored: {targets:?}");
    assert!(!targets.iter().any(|target| target == "dax"), "dax was closed: {targets:?}");
}

#[test]
fn a_line_sent_into_a_direct_message_through_the_cli_path_is_remembered() {
    let dir = TempDir::new("directs-cli-send");
    let backing = disk(&dir);
    let loopbacks = Loopbacks::default();
    {
        let mut first = launch(&backing, Box::new(loopbacks.clone()));
        save(&mut first, libera(LIBERA));
        assert!(first.start(LIBERA));
        feed(&mut first, &loopbacks, LIBERA, ":irc.libera.chat 001 fred :Welcome");
        // The product CLI's `send` calls `send_to_target`. The query is never opened in the window.
        assert!(first.send_to_target(LIBERA, "anna", "hello from the cli"));
    }

    let mut second = launch(&backing, Box::new(loopbacks.clone()));
    assert!(second.start(LIBERA));
    feed(&mut second, &loopbacks, LIBERA, ":irc.libera.chat 376 fred :End of MOTD");
    let targets = conversation_targets(&second);
    assert!(targets.iter().any(|target| target == "anna"), "anna is restored: {targets:?}");
}

#[test]
fn turning_the_reopen_preference_off_stops_the_restore() {
    let dir = TempDir::new("directs-pref");
    let backing = disk(&dir);
    let loopbacks = Loopbacks::default();
    {
        let mut first = launch(&backing, Box::new(loopbacks.clone()));
        save(&mut first, libera(LIBERA));
        assert!(first.start(LIBERA));
        feed(&mut first, &loopbacks, LIBERA, ":irc.libera.chat 001 fred :Welcome");
        first.select_conversation(LIBERA, "#omarchy");
        first.open_direct("anna");
        assert!(first.set_preference(PrefName::Directs, PrefValue::Toggle(false)));
    }

    let mut second = launch(&backing, Box::new(loopbacks.clone()));
    assert!(!second.preferences().directs, "the off switch is kept");
    assert!(second.start(LIBERA));
    feed(&mut second, &loopbacks, LIBERA, ":irc.libera.chat 001 fred :Welcome");
    feed(&mut second, &loopbacks, LIBERA, ":irc.libera.chat 376 fred :End of MOTD");
    assert!(
        !conversation_targets(&second).iter().any(|target| target == "anna"),
        "the query stays closed while the preference is off"
    );

    // Turning it back on restores the queries of the registered network at once.
    assert!(second.set_preference(PrefName::Directs, PrefValue::Toggle(true)));
    assert!(
        conversation_targets(&second).iter().any(|target| target == "anna"),
        "turning it on restores now"
    );
}

#[test]
fn preferences_and_the_sidebar_layout_survive_a_relaunch() {
    let dir = TempDir::new("prefs");
    let backing = disk(&dir);
    {
        let mut first = launch(&backing, Box::new(Loopbacks::default()));
        save(&mut first, libera(LIBERA));
        save(&mut first, libera(OFTC));
        first.set_preference(PrefName::Avatars, PrefValue::Toggle(false));
        first.set_preference(PrefName::Unread, PrefValue::Toggle(false));
        first.set_preference(PrefName::Joins, PrefValue::parse(PrefName::Joins, "every").unwrap());
        first.set_network_order(&[OFTC.to_owned(), LIBERA.to_owned()]);
        first.set_network_collapsed(LIBERA, true);
    }
    let second = launch(&backing, Box::new(Loopbacks::default()));
    let prefs = second.preferences();
    assert!(!prefs.avatars && !prefs.unread);
    assert_eq!(prefs.joins, MembershipNoise::Every);
    assert_eq!(second.network_order(), [OFTC, LIBERA]);
    assert!(second.is_network_collapsed(LIBERA));
    assert!(!second.is_network_collapsed(OFTC));
}

#[test]
fn autoaway_settings_survive_a_relaunch() {
    let dir = TempDir::new("autoaway");
    let backing = disk(&dir);
    {
        let mut first = launch(&backing, Box::new(Loopbacks::default()));
        let request = crate::irc::autoaway::parse_autoaway_argument("reason lunch");
        first.autoaway_command(request);
    }
    let second = launch(&backing, Box::new(Loopbacks::default()));
    assert_eq!(
        second.autoaway.config,
        AutoawayConfig {
            default_reason: "lunch".to_owned(),
            ..AutoawayConfig::default()
        }
    );
}

#[test]
fn ignores_mutes_and_scroll_places_survive_a_relaunch() {
    let dir = TempDir::new("lists");
    let backing = disk(&dir);
    {
        let mut first = launch(&backing, Box::new(Loopbacks::default()));
        save(&mut first, libera(LIBERA));
        first.ignore_nick(LIBERA, "spam");
        first.apply_mute(LIBERA, "#omarchy", true);
        first.remember_place(
            LIBERA,
            "anna",
            ScrollPlace {
                follow_end: true,
                ..ScrollPlace::default()
            },
        );
    }
    let second = launch(&backing, Box::new(Loopbacks::default()));
    assert_eq!(second.ignored_nicks(LIBERA), ["spam"]);
    assert!(second.place_for(LIBERA, "anna").is_some_and(|place| place.follow_end));
    assert_eq!(second.place_for(LIBERA, "dax"), None);
}

#[test]
fn an_ephemeral_controller_keeps_nothing_on_disk() {
    let dir = TempDir::new("ephemeral");
    let mut controller = Controller::with_backing(
        clock(),
        Box::new(NullNotifier),
        Box::new(Loopbacks::default()),
        Backing::Ephemeral,
    );
    controller.load_stored();
    save(&mut controller, libera(LIBERA));
    controller.set_preference(PrefName::Avatars, PrefValue::Toggle(false));
    assert!(std::fs::read_dir(dir.path()).unwrap().next().is_none());
    assert!(!controller.preferences().avatars, "the process keeps its own values");
}

#[test]
fn the_profile_is_the_sidebar_name_before_any_session_starts() {
    let dir = TempDir::new("name");
    let backing = disk(&dir);
    let mut first = launch(&backing, Box::new(crate::controller::transports::LiveTransports));
    save(&mut first, libera(LIBERA));
    assert_eq!(first.network_name(LIBERA), "Libera");
    save(
        &mut first,
        Profile {
            name: String::new(),
            ..libera(OFTC)
        },
    );
    assert_eq!(first.network_name(OFTC), "irc.libera.chat", "a blank name is the host");
}
