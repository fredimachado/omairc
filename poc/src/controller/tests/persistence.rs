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
use crate::controller::tests::fixture::OMARCHY;
use crate::controller::persistence::{SaveOutcome, SecretEdits};
use crate::controller::transports::{TransportFactory, LIVE_IRC_UNAVAILABLE};
use crate::desktop::notifier::NullNotifier;
use crate::irc::autoaway::AutoawayConfig;
use crate::irc::command::ComposerSurface;
use crate::irc::event::IrcConversationCause;
use crate::irc::reducer::ConversationLog;
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

fn launch_with_transcript(
    backing: &Backing,
    transports: Box<dyn TransportFactory>,
    transcript_root: std::path::PathBuf,
) -> Controller {
    let mut controller = Controller::with_backing(
        clock(),
        Box::new(NullNotifier),
        transports,
        backing.clone(),
    );
    controller.credentials = Box::new(MemoryCredentialStore::default());
    controller.reducer.set_conversation_log(Some(Box::new(
        crate::storage::conversationlog::FileConversationLog::new(transcript_root),
    )));
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

fn message_bodies(controller: &Controller) -> Vec<String> {
    controller
        .messages()
        .into_iter()
        .map(|row| row.body.clone())
        .collect()
}

fn conversation_targets(controller: &Controller) -> Vec<String> {
    controller
        .conversations()
        .into_iter()
        .map(|row| row.target)
        .collect()
}

fn has_conversation(controller: &Controller, network_id: &str, target: &str) -> bool {
    let key = controller.reducer.conversation_key(network_id, target);
    controller.reducer.find(&key).is_some()
}

fn direct_listed(controller: &Controller, network_id: &str, target: &str) -> bool {
    let mapping = controller.reducer.server_features(network_id).case_mapping();
    controller
        .open_directs
        .listed(network_id, mapping)
        .iter()
        .any(|listed| mapping.equals(listed, target))
}

fn closed_has(controller: &Controller, network_id: &str, target: &str) -> bool {
    let mapping = controller.reducer.server_features(network_id).case_mapping();
    controller.closed.borrow().contains(network_id, target, mapping)
}

fn frame_count(loopbacks: &Loopbacks, network_id: &str, needle: &str) -> usize {
    loopbacks
        .get(network_id)
        .written_frames()
        .iter()
        .filter(|frame| String::from_utf8_lossy(frame).contains(needle))
        .count()
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
        let mut first = launch(&backing, Box::new(crate::controller::transports::NoTransports));
        save(&mut first, libera(LIBERA));
        assert!(!first.start(LIBERA));
        assert_eq!(first.last_error_for(LIBERA), Some(LIVE_IRC_UNAVAILABLE));
    }
    let second = launch(&backing, Box::new(crate::controller::transports::NoTransports));
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
            password: Some("hunter2".to_owned().into()),
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
fn a_transcript_line_survives_a_relaunch() {
    let dir = TempDir::new("transcript-relaunch");
    let backing = disk(&dir);
    let transcript_root = dir.path().join("logs");
    let loopbacks = Loopbacks::default();
    let unique = "persisted-on-disk-42";
    {
        let mut first = launch_with_transcript(
            &backing,
            Box::new(loopbacks.clone()),
            transcript_root.clone(),
        );
        save(&mut first, libera(LIBERA));
        assert!(first.start(LIBERA));
        feed(&mut first, &loopbacks, LIBERA, ":irc.libera.chat 001 fred :Welcome");
        first.select_conversation(LIBERA, "#omarchy");
        feed(
            &mut first,
            &loopbacks,
            LIBERA,
            &format!(":alice!u@h PRIVMSG #omarchy :{unique}"),
        );
        assert!(
            message_bodies(&first).iter().any(|body| body == unique),
            "the line is in the first run"
        );
        let log = crate::storage::conversationlog::FileConversationLog::new(transcript_root.clone());
        let mapping = first.reducer.server_features(LIBERA).case_mapping();
        let stored = log.read_tail(LIBERA, "#omarchy", mapping, 10);
        assert!(
            stored.iter().any(|line| line.body == unique),
            "the line is on disk: {:?}",
            stored
        );
    }

    let mut second = launch_with_transcript(
        &backing,
        Box::new(loopbacks.clone()),
        transcript_root.clone(),
    );
    let key = second.reducer.conversation_key(LIBERA, "#omarchy");
    assert!(
        second
            .reducer
            .ensure_conversation(&key, "#omarchy", IrcConversationCause::InboundOther)
            .is_some(),
        "opening the channel hydrates from disk"
    );
    second.select_conversation(LIBERA, "#omarchy");
    assert!(
        message_bodies(&second).iter().any(|body| body == unique),
        "the line hydrates on relaunch"
    );
}

#[test]
fn the_demo_writes_no_transcript_files() {
    let mut fixture = crate::controller::tests::fixture::Fixture::seeded();
    assert!(
        fixture.controller.backing.is_ephemeral(),
        "the demo must not install a disk transcript log"
    );
    fixture.controller.select_conversation(OMARCHY, "#omarchy");
    assert!(fixture.controller.send_to_target(OMARCHY, "#omarchy", "demo line"));
}

#[test]
fn a_reopened_direct_is_remembered_across_restart() {
    let dir = TempDir::new("closed-reopen-direct");
    let backing = disk(&dir);
    let loopbacks = Loopbacks::default();
    {
        let mut first = launch(&backing, Box::new(loopbacks.clone()));
        save(&mut first, libera(LIBERA));
        assert!(first.start(LIBERA));
        feed(&mut first, &loopbacks, LIBERA, ":irc.libera.chat 001 fred :Welcome");
        feed(
            &mut first,
            &loopbacks,
            LIBERA,
            ":lena!u@h PRIVMSG fred :hi\r\n\
             :rio!u@h PRIVMSG fred :\x01ACTION waves\x01\r\n\
             :kai!u@h PRIVMSG fred :once",
        );
        feed(&mut first, &loopbacks, LIBERA, ":nia!u@h NOTICE fred :psst");
        for nick in ["lena", "nia", "rio", "kai"] {
            assert!(has_conversation(&first, LIBERA, nick), "{nick} was not opened");
            assert!(
                !direct_listed(&first, LIBERA, nick),
                "first line persisted {nick}"
            );
        }
        for nick in ["lena", "nia", "rio"] {
            first.select_conversation(LIBERA, nick);
            assert!(first.close_direct(), "CloseDirectMessage({nick})");
            assert!(!has_conversation(&first, LIBERA, nick), "{nick} stayed after close");
            assert!(closed_has(&first, LIBERA, nick), "close did not remember {nick}");
        }
        feed(
            &mut first,
            &loopbacks,
            LIBERA,
            ":lena!u@h PRIVMSG fred :again\r\n\
             :rio!u@h PRIVMSG fred :\x01ACTION waves again\x01",
        );
        feed(&mut first, &loopbacks, LIBERA, ":nia!u@h NOTICE fred :again");
        for nick in ["lena", "nia", "rio"] {
            assert!(has_conversation(&first, LIBERA, nick), "live line did not reopen {nick}");
            assert!(!closed_has(&first, LIBERA, nick), "reopen left {nick} closed");
            assert!(
                direct_listed(&first, LIBERA, nick),
                "reopen did not remember {nick}"
            );
        }
        assert!(
            !direct_listed(&first, LIBERA, "kai"),
            "a first line that was never closed was remembered"
        );
    }

    let mut again = launch(&backing, Box::new(loopbacks.clone()));
    assert!(again.start(LIBERA));
    feed(&mut again, &loopbacks, LIBERA, ":irc.libera.chat 001 fred :Welcome");
    for nick in ["lena", "nia", "rio", "kai"] {
        assert!(
            !has_conversation(&again, LIBERA, nick),
            "{nick} existed before MOTD"
        );
    }
    feed(&mut again, &loopbacks, LIBERA, ":irc.libera.chat 376 fred :End of MOTD");
    for nick in ["lena", "nia", "rio"] {
        assert!(
            has_conversation(&again, LIBERA, nick),
            "restart dropped {nick}"
        );
    }
    assert!(
        !has_conversation(&again, LIBERA, "kai"),
        "restart restored a query that was never closed or answered"
    );
}

#[test]
fn an_unsolicited_self_join_keeps_a_closed_channel() {
    const CONVERSATION: ComposerSurface = ComposerSurface::Conversation;
    let dir = TempDir::new("closed-channel-restart");
    let backing = disk(&dir);
    let loopbacks = Loopbacks::default();
    {
        let mut first = launch(&backing, Box::new(loopbacks.clone()));
        save(&mut first, libera(LIBERA));
        assert!(first.start(LIBERA));
        feed(&mut first, &loopbacks, LIBERA, ":irc.libera.chat 001 fred :Welcome");
        feed(
            &mut first,
            &loopbacks,
            LIBERA,
            ":fred!u@h JOIN :#omarchy\r\n\
             :fred!u@h JOIN :#lab\r\n\
             :fred!u@h JOIN :#desk",
        );
        for channel in ["#lab", "#desk"] {
            first.select_conversation(LIBERA, channel);
            assert!(first.submit(CONVERSATION, "/part"), "/part {channel}");
            assert!(first.submit(CONVERSATION, "/close"), "/close {channel}");
            assert!(
                !has_conversation(&first, LIBERA, channel),
                "{channel} stayed after close"
            );
            assert!(closed_has(&first, LIBERA, channel), "close did not remember {channel}");
        }
        assert_eq!(frame_count(&loopbacks, LIBERA, "PART #lab"), 1);

        feed(
            &mut first,
            &loopbacks,
            LIBERA,
            ":fred!u@h JOIN :#lab\r\n\
             :irc.libera.chat 353 fred = #lab :fred\r\n\
             :irc.libera.chat 366 fred #lab :End of NAMES\r\n\
             :lena!u@h PRIVMSG #lab :still",
        );
        assert!(
            !has_conversation(&first, LIBERA, "#lab"),
            "an unsolicited join brought #lab back"
        );
        assert!(
            closed_has(&first, LIBERA, "#lab"),
            "the unsolicited join forgot the close"
        );

        first.select_conversation(LIBERA, "#omarchy");
        assert!(
            first.submit(CONVERSATION, "/join #desk,#gamma"),
            "SendMessage(/join #desk,#gamma)"
        );
        assert!(
            has_conversation(&first, LIBERA, "#gamma"),
            "the last join target was not opened"
        );
        assert!(
            !closed_has(&first, LIBERA, "#desk"),
            "an explicit join left #desk closed"
        );
        assert!(
            closed_has(&first, LIBERA, "#lab"),
            "#lab close was cleared with #desk"
        );
        feed(
            &mut first,
            &loopbacks,
            LIBERA,
            ":fred!u@h JOIN :#desk\r\n\
             :irc.libera.chat 353 fred = #desk :fred\r\n\
             :irc.libera.chat 366 fred #desk :End of NAMES",
        );
        assert!(
            has_conversation(&first, LIBERA, "#desk"),
            "an explicit join did not open #desk"
        );
        assert!(
            !closed_has(&first, LIBERA, "#desk"),
            "an explicit join left #desk closed after the echo"
        );
        assert!(
            closed_has(&first, LIBERA, "#lab"),
            "#lab close was cleared with #desk"
        );
    }

    let mut again = launch(&backing, Box::new(loopbacks.clone()));
    assert!(again.start(LIBERA));
    feed(&mut again, &loopbacks, LIBERA, ":irc.libera.chat 001 fred :Welcome");
    feed(
        &mut again,
        &loopbacks,
        LIBERA,
        ":fred!u@h JOIN :#lab\r\n\
         :irc.libera.chat 353 fred = #lab :fred\r\n\
         :irc.libera.chat 366 fred #lab :End of NAMES\r\n\
         :lena!u@h PRIVMSG #lab :still",
    );
    assert!(
        !has_conversation(&again, LIBERA, "#lab"),
        "restart join brought #lab back"
    );
    assert!(
        closed_has(&again, LIBERA, "#lab"),
        "restart join forgot the close"
    );
    assert_eq!(
        frame_count(&loopbacks, LIBERA, "PART #lab"),
        0,
        "restart join wrote PART"
    );
    again.open_status(LIBERA);
    assert!(again.submit(ComposerSurface::Status, "/join #lab"));
    assert!(
        has_conversation(&again, LIBERA, "#lab"),
        "an explicit join did not open #lab"
    );
    assert!(
        !closed_has(&again, LIBERA, "#lab"),
        "an explicit join left #lab closed"
    );
}

#[test]
fn the_profile_is_the_sidebar_name_before_any_session_starts() {
    let dir = TempDir::new("name");
    let backing = disk(&dir);
    let mut first = launch(&backing, Box::new(crate::controller::transports::NoTransports));
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

#[test]
fn self_join_history_resume_does_not_reenter_session() {
    let dir = TempDir::new("history-resume-reenter");
    let backing = disk(&dir);
    let loopbacks = Loopbacks::default();
    let mut controller = launch(&backing, Box::new(loopbacks.clone()));
    save(&mut controller, libera(LIBERA));

    let mapping = controller.reducer.server_features(LIBERA).case_mapping();
    let when = Utc.with_ymd_and_hms(2024, 3, 9, 16, 0, 0).unwrap()
        + chrono::Duration::milliseconds(620);
    assert!(controller
        .playback_times
        .borrow_mut()
        .note(LIBERA, "#omarchy", when, mapping));

    assert!(controller.start(LIBERA));
    feed(
        &mut controller,
        &loopbacks,
        LIBERA,
        ":irc.libera.chat CAP fred LS :batch chathistory\r\n\
         :irc.libera.chat CAP fred ACK :batch chathistory\r\n\
         :irc.libera.chat 001 fred :Welcome\r\n\
         :fred!u@h JOIN :#omarchy",
    );

    assert!(
        frame_count(&loopbacks, LIBERA, "CHATHISTORY AFTER #omarchy timestamp=2024-03-09T16:00:00.620Z")
            >= 1,
        "frames: {:?}",
        loopbacks
            .get(LIBERA)
            .written_frames()
            .iter()
            .map(|frame| String::from_utf8_lossy(frame))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        frame_count(&loopbacks, LIBERA, "CHATHISTORY LATEST #omarchy"),
        0,
        "resume must not fall back to LATEST"
    );
}
