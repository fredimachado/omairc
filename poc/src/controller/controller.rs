//! Port of `tui/internal/controller/controller.go` (Go) and `src/irc/irccontroller.cpp` (C++).
//!
//! The controller owns the session manager, the event reducer, the Status console
//! and the session inbox. Its pipeline is one way:
//!
//! 1. A `Transport` delivers bytes to its `Session`, which parses them and queues
//!    `SessionEvent`s.
//! 2. `pump` drains every session's queue. Inbound lines go through the Status
//!    keep table and `statusentry`. Messages go through `translate` and fold into
//!    the `EventReducer`.
//! 3. The reducer's `ViewNotify` surfaces queue up. The shell drains them with
//!    `take_notifications` and repaints.
//!
//! Intents (`select_conversation`, `send_to_target`, `start`, ...) run on the UI
//! thread, write to the session, and pump the events the write produced. The
//! loopback delivers synchronously, so a demo reply is queued before the intent
//! returns. Nothing here depends on gpui.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::demo::seed::world_for;
use crate::demo::server::{DemoError, DemoServer};
use crate::desktop::httpfetch::UreqFetch;
use crate::irc::capability::{IrcCapability, IrcCapabilitySet};
use crate::irc::event::{ConversationKey, Event, NickEvent, WelcomeEvent};
use crate::irc::inbox::{Inbox, InboxItem, InboxKind};
use crate::irc::message::IrcMessage;
use crate::irc::pref::Preferences;
use crate::irc::prefixnick::prefix_nick;
use crate::irc::reducer::EventReducer;
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::statusentry::{self, LogSeverity, StatusEntry};
use crate::irc::translator::translate;
use crate::irc::viewnotify::{self, ViewNotify};
use crate::irc::wiretext::wire_text;
use crate::session::clock::Clock;
use crate::session::events::{SessionEvent, SessionState};
use crate::session::session::Session;
use crate::session::sessionmanager::{SessionManager, SessionManagerError};

use crate::storage::credentialstore::{platform_store, CredentialStore, MemoryCredentialStore};
use crate::storage::lists::{CaseListStore, IniListBacking};
use crate::storage::opendirect::OpenDirectStore;
use crate::storage::preferences::PreferenceStore;
use crate::storage::profilestore::{Profile, ProfileStore};
use crate::storage::playbacktime::PlaybackTimeStore;
use crate::storage::scrollplace::ScrollPlaceFile;
use crate::storage::Backing;

use super::autoawayruntime::AutoawayState;
use super::notify::Arrival;
use super::playback::PlaybackCoordinator;
use super::avatars::AvatarFetches;
use super::channellist::{ChannelListRequests, ChannelListView};
use super::console::{self, StatusConsole};
use super::monitorcoordinator::MonitorState;
use super::replyrouter::ReplyWatches;
use super::scrollplace::ScrollPlaceStore;
use super::snapshots::state_text;
use super::stores::{HighlightStore, IgnoreStore, MonitorStore, MuteStore};
use super::transports::TransportFactory;

/// Why the controller could not be built.
#[derive(Debug)]
pub enum ControllerError {
    /// A seeded session could not be registered.
    Session(SessionManagerError),
    /// The demo could not attach or seed a network.
    Demo(DemoError),
}

impl fmt::Display for ControllerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Session(error) => write!(formatter, "{error}"),
            Self::Demo(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for ControllerError {}

impl From<SessionManagerError> for ControllerError {
    fn from(error: SessionManagerError) -> Self {
        Self::Session(error)
    }
}

impl From<DemoError> for ControllerError {
    fn from(error: DemoError) -> Self {
        Self::Demo(error)
    }
}

/// The controller. The UI thread owns it; it is not `Send`.
pub struct Controller {
    pub(super) clock: Rc<dyn Clock>,
    pub(super) transports: Box<dyn TransportFactory>,
    /// The demo peers, when the controller runs `--demo-server`.
    pub(super) demo: Option<Rc<DemoServer>>,
    pub(super) manager: SessionManager,
    pub(super) reducer: EventReducer,
    pub(super) console: StatusConsole,
    pub(super) inbox: Inbox,
    /// The last error per network (`lastErrors`).
    pub(super) last_errors: BTreeMap<String, String>,
    /// The capabilities each network last reported (`CapabilitiesChanged`).
    pub(super) capabilities: BTreeMap<String, IrcCapabilitySet>,
    /// The nick each network's session holds, updated by NICK on that network.
    pub(super) current_nicks: BTreeMap<String, String>,
    pub(super) selected: Option<ConversationKey>,
    /// The display target of the selection, kept for a conversation that does
    /// not exist yet (`selectedTarget`).
    pub(super) selected_target: String,
    /// The network the Status console points at.
    pub(super) console_network: String,
    /// The Status console's own open flag (`consoleOpen`).
    pub(super) console_open: bool,
    /// The recorded sidebar order. Networks not listed follow in id order.
    pub(super) network_order: Vec<String>,
    pub(super) network_collapsed: BTreeSet<String>,
    notifications: Vec<ViewNotify>,
    /// The auto-away timers, settings and away sets (`autoawayruntime.rs`).
    pub(super) autoaway: AutoawayState,
    /// Where each conversation's transcript was left (`scrollplace.rs`).
    pub(super) scroll_places: ScrollPlaceStore,
    /// The WHOIS, CTCP and labeled-reply watches (`replyrouter.rs`).
    pub(super) reply_watches: ReplyWatches,
    /// The `/list` requests, their rows and idle timers (`channellist.rs`), and the
    /// overlay that shows them.
    pub(super) channel_lists: ChannelListRequests,
    pub(super) channel_view: ChannelListView,
    /// The `/ignore` list. It is shared with the inbound filter each session holds.
    pub(super) ignores: Rc<RefCell<IgnoreStore>>,
    pub(super) mutes: MuteStore,
    pub(super) highlights: HighlightStore,
    pub(super) monitors: MonitorStore,
    /// The session-scoped MONITOR subscription and presence (`monitorcoordinator.rs`).
    pub(super) monitor: MonitorState,
    /// The network and target the composer's typing hint is held on (`typingTarget`).
    pub(super) typing_target: Option<(String, String)>,
    /// The channels a `/part` or a close cancelled while their JOIN was in flight. The
    /// self JOIN that answers the join parts again instead of rejoining (`cancelled`).
    pub(super) cancelled_joins: BTreeSet<ConversationKey>,
    /// The saved preferences `/pref` and the Connect toggles read and write (`prefs`).
    pub(super) prefs: Preferences,
    /// Where the controller's state lives between runs (`Ephemeral` for the demo and tests).
    pub(super) backing: Backing,
    /// The saved Connect profiles, by network id (`IrcProfileStore`'s records).
    pub(super) profiles: BTreeMap<String, Profile>,
    pub(super) profile_store: ProfileStore,
    pub(super) preference_store: PreferenceStore,
    /// The network secrets: the OS keychain for a saved network, memory for an ephemeral one.
    pub(super) credentials: Box<dyn CredentialStore>,
    /// Networks waiting for a keychain read before their session is registered at load.
    pub(super) pending_session_registrations: BTreeSet<String>,
    /// Networks that asked to connect on startup while a keychain read was still in flight.
    pub(super) pending_startup_connects: BTreeSet<String>,
    /// The profile and secrets each network's session was built from, so Apply rebuilds when
    /// either changes (`IrcConnection::reconcile`).
    pub(super) applied_sessions: BTreeMap<String, super::persistence::AppliedSession>,
    /// The direct messages to reopen, and the ones the user closed (`IrcOpenDirectStore`).
    pub(super) open_directs: OpenDirectStore,
    /// The networks whose MOTD-end restore has run (`openDirectsMotdSeen`).
    pub(super) open_directs_motd_seen: BTreeSet<String>,
    /// Networks whose saved avatar was published this registration (`appliedProfileAvatars`).
    pub(super) applied_profile_avatars: BTreeSet<String>,
    /// The avatar cache, its in-flight slots and the seam its jobs fetch through (`avatars.rs`).
    pub(super) avatars: AvatarFetches,
    /// Channels and directs the user closed (`closedConversations` / `targets`).
    pub(super) closed: Rc<RefCell<CaseListStore>>,
    /// Bouncer playback books (Gap 5).
    pub(super) playback: Rc<RefCell<PlaybackCoordinator>>,
    pub(super) playback_times: Rc<RefCell<PlaybackTimeStore>>,
    /// Whether the transcript viewport follows the end after new lines.
    pub(super) transcript_follow_end: bool,
    /// Whether the shell reports the transcript as focused for read-marker sync.
    pub(super) transcript_focused: bool,
    pub(super) read_markers: super::readmarker::ReadMarkerState,
    /// Mention and monitor arrivals waiting for the shell (`notify.rs`).
    pub(super) arrivals: Vec<Arrival>,
    pumping: bool,
}

impl Controller {
    /// A controller with no networks, the no-op notifier and no live transports. It keeps
    /// nothing between runs.
    #[cfg(test)]
    pub fn new(clock: Rc<dyn Clock>) -> Self {
        Self::with_parts(clock, Box::new(super::transports::NoTransports))
    }

    /// A controller with the given transport factory, keeping nothing between runs.
    pub fn with_parts(clock: Rc<dyn Clock>, transports: Box<dyn TransportFactory>) -> Self {
        Self::with_backing(clock, transports, Backing::Ephemeral)
    }

    /// A controller whose stores live in `backing`. Nothing is loaded here: `load_stored`
    /// reads what an earlier run saved.
    pub fn with_backing(
        clock: Rc<dyn Clock>,
        transports: Box<dyn TransportFactory>,
        backing: Backing,
    ) -> Self {
        let channel_lists = ChannelListRequests::new(Rc::clone(&clock));
        let list = |group: &'static str, key: &'static str| {
            CaseListStore::new(Box::new(IniListBacking::new(backing.clone(), group, key)))
        };
        let closed = Rc::new(RefCell::new(list("closedConversations", "targets")));
        let mut reducer = EventReducer::new();
        reducer.set_closed_persistence(Box::new(Rc::clone(&closed)));
        let playback_times = Rc::new(RefCell::new(PlaybackTimeStore::new(&backing)));
        let playback = Rc::new(RefCell::new(PlaybackCoordinator::new(Rc::clone(&playback_times))));
        Self {
            clock,
            transports,
            demo: None,
            manager: SessionManager::new(backing.sts_store()),
            reducer,
            console: StatusConsole::default(),
            inbox: Inbox::new(),
            last_errors: BTreeMap::new(),
            capabilities: BTreeMap::new(),
            current_nicks: BTreeMap::new(),
            selected: None,
            selected_target: String::new(),
            console_network: String::new(),
            console_open: false,
            network_order: Vec::new(),
            network_collapsed: BTreeSet::new(),
            notifications: Vec::new(),
            autoaway: AutoawayState::default(),
            scroll_places: ScrollPlaceStore::new(ScrollPlaceFile::new(backing.clone())),
            reply_watches: ReplyWatches::default(),
            channel_lists,
            channel_view: ChannelListView::default(),
            ignores: Rc::new(RefCell::new(list("ignores", "nicks"))),
            mutes: list("mutes", "targets"),
            highlights: list("highlights", "words"),
            monitors: list("monitors", "nicks"),
            monitor: MonitorState::default(),
            typing_target: None,
            cancelled_joins: BTreeSet::new(),
            prefs: Preferences::default(),
            backing: backing.clone(),
            profiles: BTreeMap::new(),
            profile_store: ProfileStore::new(backing.clone()),
            preference_store: PreferenceStore::new(backing.clone()),
            credentials: if backing.is_ephemeral() {
                Box::new(MemoryCredentialStore::default())
            } else {
                platform_store()
            },
            pending_session_registrations: BTreeSet::new(),
            pending_startup_connects: BTreeSet::new(),
            applied_sessions: BTreeMap::new(),
            open_directs: OpenDirectStore::new(&backing),
            open_directs_motd_seen: BTreeSet::new(),
            applied_profile_avatars: BTreeSet::new(),
            avatars: AvatarFetches::new(Arc::new(UreqFetch)),
            closed,
            playback,
            playback_times,
            transcript_follow_end: true,
            transcript_focused: true,
            read_markers: super::readmarker::ReadMarkerState::new(),
            arrivals: Vec::new(),
            pumping: false,
        }
    }

    /// Runs the timers that are due, then pumps what they queued. The UI loop calls
    /// this on each tick. Returns how many timers fired.
    pub fn tick(&mut self) -> usize {
        self.drain_reachability();
        self.poll_credential_reads();
        let fired = self.clock.fire_due();
        self.pump();
        fired
    }

    /// Starts every reconnecting session without waiting out the backoff. The host
    /// network reachability watcher calls this when connectivity returns.
    pub fn notify_network_reachable(&mut self) {
        for network_id in self.manager.network_ids() {
            if let Some(session) = self.manager.find(&network_id) {
                session.notify_reachable();
            }
        }
        self.pump();
    }

    fn drain_reachability(&mut self) {
        for network_id in self.manager.network_ids() {
            if let Some(session) = self.manager.find(&network_id) {
                session.drain_reachability();
            }
        }
    }

    /// Drains every session's queued events and folds them in, in order, until no
    /// session has anything left. A nested call does nothing, so an intent that an
    /// event handler runs cannot reorder the queue being drained.
    pub fn pump(&mut self) {
        if self.pumping {
            return;
        }
        self.pumping = true;
        loop {
            let mut drained = false;
            while self.run_autoaway_expiry() {
                drained = true;
            }
            while self.run_channel_list_expiry() {
                drained = true;
            }
            for network_id in self.manager.network_ids() {
                let Some(session) = self.manager.find(&network_id) else {
                    continue;
                };
                let events = session.drain_events();
                if events.is_empty() {
                    continue;
                }
                drained = true;
                for event in events {
                    self.handle_session_event(event);
                }
            }
            if !drained {
                break;
            }
        }
        self.pumping = false;
    }

    /// The surfaces the reducer and the intents dirtied since the last call, each
    /// once, in the order they first changed. The shell repaints for each one.
    pub fn take_notifications(&mut self) -> Vec<ViewNotify> {
        std::mem::take(&mut self.notifications)
    }

    /// The demo peers, when the controller runs `--demo-server`.
    // test seam: the controller tests reach the demo peers
    #[cfg(test)]
    pub fn demo(&self) -> Option<&DemoServer> {
        self.demo.as_deref()
    }

    /// The live session for `network_id`, if one is registered.
    pub fn session(&self, network_id: &str) -> Option<Session> {
        self.manager.find(network_id)
    }

    /// Starts `network_id`'s connection (`Start`), the intent Connect's Apply calls. A
    /// live session is left alone. A seeded demo network that is idle gets a fresh
    /// loopback and its peer, as at launch, and its conversations stay. Any other
    /// network goes through the transport factory, and a refusal (the build's reason)
    /// becomes the network's last error and a Status line. An empty id does nothing.
    /// Returns whether the connection began.
    pub fn start(&mut self, network_id: &str) -> bool {
        self.poll_credential_reads();
        self.pump();
        if network_id.is_empty() {
            return false;
        }
        self.set_last_error(network_id, None);
        let started = match (
            self.manager.is_live(network_id),
            world_for(network_id),
            self.demo.is_some(),
        ) {
            (true, Some(_), true) => true,
            (_, Some(network), true) => self.restart_demo(network),
            _ => self.start_profile(network_id),
        };
        self.queue(vec![ViewNotify::Status]);
        self.pump();
        started
    }

    /// Sends QUIT and stops `network_id`'s session (`Disconnect`). Returns false when
    /// the network has no session or the session was already idle.
    pub fn disconnect(&mut self, network_id: &str) -> bool {
        self.disconnect_with_reason(network_id, "")
    }

    /// Sends QUIT with `reason` and stops `network_id`'s session. `/disconnect <reason>` and
    /// `/quit <reason>` run through here, as does `disconnect`.
    pub(super) fn disconnect_with_reason(&mut self, network_id: &str, reason: &str) -> bool {
        self.pump();
        let Some(session) = self.manager.find(network_id) else {
            return false;
        };
        if !session.quit(reason) {
            return false;
        }
        self.queue(vec![ViewNotify::Status]);
        self.pump();
        true
    }

    /// Whether `network_id` can be disconnected: its session is live or reconnecting, so
    /// Disconnect has something to stop (Qt `IrcConnection::canDisconnect`). Connecting
    /// counts as live, as it does in Qt. False for a network with no session.
    pub fn can_disconnect(&self, network_id: &str) -> bool {
        self.manager.is_live(network_id)
    }

    /// The connection word for `network_id`, or for the focused network when the id
    /// is empty (`ConnectionStatusFor`). A network with no session is Offline.
    pub fn connection_status_for(&self, network_id: &str) -> &'static str {
        let id = if network_id.is_empty() {
            self.focused_network_id()
        } else {
            network_id
        };
        match self.manager.find(id) {
            Some(session) => state_text(session.state()),
            None => "Offline",
        }
    }

    /// The last error recorded for `network_id` (`LastErrorFor`), if any.
    pub fn last_error_for(&self, network_id: &str) -> Option<&str> {
        self.last_errors.get(network_id).map(String::as_str)
    }

    /// Records or clears a network's last error (`setLastError`). An empty or `None`
    /// message clears it.
    pub fn set_last_error(&mut self, network_id: &str, message: Option<String>) {
        match message {
            Some(message) if !message.is_empty() => {
                self.last_errors.insert(network_id.to_owned(), message);
            }
            _ => {
                self.last_errors.remove(network_id);
            }
        }
    }

    /// Queues surfaces for the shell. A surface already queued is not repeated.
    pub(super) fn queue(&mut self, surfaces: Vec<ViewNotify>) {
        viewnotify::merge(&mut self.notifications, surfaces);
    }

    /// The nick a network's session holds, or empty before registration.
    pub(super) fn current_nick(&self, network_id: &str) -> String {
        self.current_nicks
            .get(network_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Dispatches one session event. The arms follow the Go `Handler` methods.
    fn handle_session_event(&mut self, event: SessionEvent) {
        match event {
            SessionEvent::StateChanged { network_id, state } => {
                if !network_id.is_empty() && state != SessionState::Registered {
                    self.playback.borrow_mut().on_left_registration(&network_id);
                    self.applied_profile_avatars.remove(&network_id);
                }
                let severity = if state == SessionState::Failed {
                    LogSeverity::Alert
                } else {
                    LogSeverity::Info
                };
                let entry = statusentry::lifecycle(
                    &network_id,
                    severity,
                    console::state_label(state),
                    console::state_text(state),
                    self.clock.now(),
                );
                self.append_status(entry);
            }
            SessionEvent::ReconnectScheduled {
                network_id,
                delay_ms,
                attempt,
            } => {
                let text = format!("Reconnect attempt {attempt} in {delay_ms} ms");
                let entry = statusentry::lifecycle(
                    &network_id,
                    LogSeverity::Info,
                    "reconnect",
                    &text,
                    self.clock.now(),
                );
                self.append_status(entry);
            }
            SessionEvent::ErrorOccurred {
                network_id,
                message,
                ..
            } => {
                // The session's message is already presentable and redacted.
                let entry = statusentry::lifecycle(
                    &network_id,
                    LogSeverity::Alert,
                    "error",
                    &message,
                    self.clock.now(),
                );
                self.set_last_error(&network_id, Some(message));
                self.append_status(entry);
            }
            SessionEvent::Registered { network_id } => self.on_registered(&network_id),
            SessionEvent::Inbound {
                network_id,
                message,
                request_label,
                nick,
                channel_types,
                at,
            } => self.record_inbound(
                &network_id,
                &message,
                &request_label,
                &nick,
                &channel_types,
                at,
            ),
            SessionEvent::Outbound {
                network_id,
                line,
                channel_types,
                at,
            } => {
                if statusentry::keeps_outgoing(&line) {
                    let entry = statusentry::outgoing(&network_id, &line, &channel_types, at);
                    self.status_entry(entry);
                }
            }
            SessionEvent::Lifecycle {
                network_id,
                severity,
                kind,
                text,
                at,
            } => {
                let entry = statusentry::lifecycle(
                    &network_id,
                    console::log_severity(severity),
                    kind,
                    &text,
                    at,
                );
                self.status_entry(entry);
            }
            SessionEvent::MessageReceived {
                network_id,
                message,
            } => self.handle_message(&network_id, &message),
            SessionEvent::HistoryBatchReceived { .. }
            | SessionEvent::ChatHistoryRequestFinished { .. }
            | SessionEvent::ChatHistoryFailed { .. } => self.on_history_event(event),
            SessionEvent::CapabilitiesChanged {
                network_id,
                capabilities,
            } => self.handle_capabilities(&network_id, capabilities),
            SessionEvent::ReadMarkerReceived {
                network_id,
                target,
                marker,
            } => self.read_marker_received(&network_id, &target, marker),
            SessionEvent::RequestLabelFinished {
                network_id,
                label,
            } => self.request_label_finished(&network_id, &label),
            SessionEvent::AutojoinChannelsChanged {
                network_id,
                channels,
                keys,
            } => self.persist_autojoin(&network_id, channels, keys),
        }
    }

    /// The welcome (001). The network is registered, so its nick is known and its
    /// ISUPPORT state starts fresh; the burst's 005 lines fill that state back in.
    fn on_registered(&mut self, network_id: &str) {
        let Some(session) = self.manager.find(network_id) else {
            return;
        };
        self.playback
            .borrow_mut()
            .on_registered(network_id, &session.autojoin_channels());
        self.open_directs_motd_seen.remove(network_id);
        let nick = session.nick();
        self.current_nicks
            .insert(network_id.to_owned(), nick.clone());
        self.reducer
            .set_server_features(network_id, IrcServerFeatures::new());
        self.install_ignore_filter(network_id);
        self.monitor_forget_presence(network_id);
        self.apply(Event::Welcome(WelcomeEvent {
            network_id: network_id.to_owned(),
            current_nick: nick.clone(),
        }));
        let entry = statusentry::lifecycle(
            network_id,
            LogSeverity::Info,
            "registered",
            &format!("Registered as {nick}"),
            self.clock.now(),
        );
        self.append_status(entry);
        self.autoaway_on_registered(network_id);
        self.apply_profile_avatar_on_connect(network_id);
    }

    /// Classifies one inbound line for Status, and raises an inbox row for an
    /// invite. The keep table decides which lines reach Status at all.
    fn record_inbound(
        &mut self,
        network_id: &str,
        message: &IrcMessage,
        request_label: &str,
        nick: &str,
        channel_types: &str,
        at: DateTime<Utc>,
    ) {
        if !statusentry::keeps_incoming(message, nick, channel_types) {
            return;
        }
        for mut entry in statusentry::incoming_all(network_id, message, channel_types, at) {
            // The session's correlation label names the request each line answers.
            if !request_label.is_empty() {
                entry.set_request_label(request_label);
            }
            if entry.label == "INVITE" {
                let channel = message
                    .params
                    .last()
                    .map(|param| wire_text(param))
                    .unwrap_or_default();
                self.append_inbox(InboxItem {
                    kind: InboxKind::Invite,
                    timestamp: at,
                    network_id: network_id.to_owned(),
                    actor: prefix_nick(message),
                    target: channel,
                    preview: entry.text.clone(),
                    msgid: Default::default(),
                });
            }
            self.status_entry(entry);
        }
    }

    /// Stores one Status line and wakes the Status surface.
    pub(super) fn append_status(&mut self, entry: StatusEntry) {
        let network_id = entry.network_id.clone();
        self.console.append(entry);
        if self.console_open && self.console_network == network_id {
            self.console.mark_seen(&network_id);
        }
        self.queue(vec![ViewNotify::Status]);
    }

    /// Stores one inbox row, newest first, under the network's case mapping, and
    /// wakes the inbox surface.
    pub(super) fn append_inbox(&mut self, item: InboxItem) {
        let mapping = self.reducer.server_features(&item.network_id).case_mapping();
        self.inbox.append(item, mapping);
        self.queue(vec![ViewNotify::Inbox]);
    }

    /// Removes one inbox row and hands it back, so the shell can act on it (Enter)
    /// or only drop it (the delete button). `None` for an index past the end.
    pub fn take_inbox_item(&mut self, index: usize) -> Option<InboxItem> {
        self.pump();
        let item = self.inbox.remove_at(index);
        if item.is_some() {
            self.queue(vec![ViewNotify::Inbox]);
        }
        item
    }

    /// Removes one inbox row by its stable identity and hands it back.
    pub fn take_inbox_item_by_identity(
        &mut self,
        identity: &crate::irc::inbox::InboxIdentity,
    ) -> Option<InboxItem> {
        self.pump();
        let item = self.inbox.remove_identity(identity);
        if item.is_some() {
            self.queue(vec![ViewNotify::Inbox]);
        }
        item
    }

    /// Translates one inbound message and folds each event in. A self JOIN that
    /// answers the session's pending invite selects that channel.
    fn handle_message(&mut self, network_id: &str, message: &IrcMessage) {
        self.note_playback_clock(network_id, message);
        if message.command == "FAIL" {
            self.route_own_metadata_fail(network_id, message);
        }
        match message.command.as_str() {
            "005" => {
                self.apply_isupport(network_id, message);
                return;
            }
            "730" => {
                self.handle_monitor_presence(network_id, message, true);
                return;
            }
            "731" => {
                self.handle_monitor_presence(network_id, message, false);
                return;
            }
            "734" => {
                self.handle_monitor_list_full(network_id, message);
                return;
            }
            _ => {}
        }
        if self.handle_channel_list_message(network_id, message) {
            return;
        }
        if message.command == "376" || message.command == "422" {
            let mut features = self.reducer.server_features(network_id).clone();
            if !features.case_mapping_known() {
                features.mark_case_mapping_known();
                self.reducer.set_server_features(network_id, features);
            }
            // MOTD end restores the open queries, after ISUPPORT has settled the case mapping.
            self.note_open_directs_motd(network_id);
        }

        let features = self.reducer.server_features(network_id).clone();
        let current_nick = self.current_nick(network_id);
        let now = self.clock.now();
        let events = {
            let reducer = &self.reducer;
            let joined = |channel: &str, sender: &str| {
                reducer.joined_member_channel(network_id, channel, sender)
            };
            translate(
                network_id,
                &current_nick,
                &features,
                message,
                now,
                Some(&joined),
            )
        };

        let mapping = features.case_mapping();
        for event in events {
            let own_join = match &event {
                Event::Join(join) if mapping.equals(&join.nick, &current_nick) => {
                    Some((join.channel.clone(), self.reducer.conversation_key(network_id, &join.channel)))
                }
                _ => None,
            };
            let self_join = match &event {
                Event::Join(join)
                    if mapping.equals(&join.nick, &current_nick)
                        && features.is_channel(&join.channel) =>
                {
                    Some(join.channel.clone())
                }
                _ => None,
            };
            if let Event::Nick(NickEvent {
                old_nick, new_nick, ..
            }) = &event
            {
                if mapping.equals(old_nick, &current_nick) {
                    self.current_nicks
                        .insert(network_id.to_owned(), new_nick.clone());
                }
            }
            self.apply(event);
            // A JOIN the user cancelled while it was in flight parts again instead of
            // staying in the channel (`TakeCancelledSelfJoin`).
            if let Some((channel, key)) = own_join {
                if self.cancelled_joins.remove(&key) {
                    self.drop_cancelled_join(network_id, &channel, &key);
                    continue;
                }
            }
            if let Some(channel) = self_join {
                self.on_self_join(network_id, &channel);
            }
        }
    }

    /// Parts a channel whose cancelled JOIN just landed. A closed channel drops its row, and a
    /// left one stays as a row that is not joined (`handleMessage`'s cancelled self JOIN).
    fn drop_cancelled_join(&mut self, network_id: &str, channel: &str, key: &ConversationKey) {
        if let Some(session) = self.manager.find(network_id) {
            session.part(channel);
        }
        if self.reducer.closed(key) {
            self.dismiss_channel(network_id, channel);
        } else {
            self.mark_channel_left(network_id, channel);
        }
    }

    /// Applies ISUPPORT tokens (005) to the network's features. The parameters are
    /// the nick, the tokens, then the trailing sentence.
    fn apply_isupport(&mut self, network_id: &str, message: &IrcMessage) {
        if message.params.len() <= 2 {
            return;
        }
        let mut features = self.reducer.server_features(network_id).clone();
        let last = message.params.len() - 1;
        for param in &message.params[1..last] {
            features.apply_token(&wire_text(param));
        }
        self.reducer.set_server_features(network_id, features);
        self.install_ignore_filter(network_id);
        self.monitor_subscribe(network_id);
        self.queue(vec![ViewNotify::Conversations]);
    }

    /// Stores a network's capabilities, and clears the presence and typing facts a
    /// dropped capability can no longer maintain (`handleCapabilities`).
    fn handle_capabilities(&mut self, network_id: &str, capabilities: IrcCapabilitySet) {
        let previous = self
            .capabilities
            .insert(network_id.to_owned(), capabilities)
            .unwrap_or_default();
        let dropped = |capability: IrcCapability| {
            previous.contains(capability) && !capabilities.contains(capability)
        };
        let away_dropped = dropped(IrcCapability::AwayNotify);
        let metadata_dropped =
            dropped(IrcCapability::MemberMetadata) || dropped(IrcCapability::Batch);
        if away_dropped || metadata_dropped {
            self.reducer
                .clear_presence_facts(network_id, away_dropped, metadata_dropped);
            self.queue(viewnotify::reset_all());
            if metadata_dropped {
                self.applied_profile_avatars.remove(network_id);
            }
        }
        if dropped(IrcCapability::MessageTags) {
            self.reducer.clear_typing_facts(network_id);
        }
        self.queue(vec![ViewNotify::Status]);
        if let Some(session) = self.manager.find(network_id) {
            if session.state() == SessionState::Registered {
                self.apply_profile_avatar_on_connect(network_id);
                self.request_znc_playback(&session);
            }
        }
    }

    /// Folds one translated event into the reducer and handles what it hands up: the
    /// first conversation claims the selection, a stale NAMES sync is released, a
    /// mention queues an arrival, and an inbox arrival lands in the inbox.
    pub(super) fn apply(&mut self, event: Event) {
        let now = self.clock.now();
        let metadata_reply = match &event {
            Event::MemberMetadata(metadata) => Some(metadata.clone()),
            _ => None,
        };
        // A welcome re-enters registration, so the away bookkeeping resets first. A
        // 305 or 306 lifts the unaway latch (`IrcController::apply`'s welcome and
        // selfAway branches).
        match &event {
            Event::Welcome(welcome) => {
                self.autoaway_forget_network(&welcome.network_id);
                self.forget_replies(&welcome.network_id);
            }
            Event::SelfAway(away) => {
                self.autoaway.unaway_sent.remove(&away.network_id);
            }
            _ => {}
        }
        let typing_only = matches!(event, Event::Typing(_));
        // What the saved lists need to see, read before the reducer takes the event.
        let renamed = match &event {
            Event::Nick(nick) => Some((nick.network_id.clone(), nick.old_nick.clone(), nick.new_nick.clone())),
            _ => None,
        };
        let self_authored = match &event {
            Event::Message(line) => Some((line.conversation.network_id.clone(), line.author.clone(), line.target.clone())),
            Event::Notice(line) => Some((line.conversation.network_id.clone(), line.author.clone(), line.target.clone())),
            Event::Action(line) => Some((line.conversation.network_id.clone(), line.author.clone(), line.target.clone())),
            _ => None,
        };
        let reopened_direct = self.other_reopened_direct(&event);
        let mut surfaces = self.reducer.apply(event, now);
        if let Some((network_id, old, new)) = renamed {
            self.rekey_nick_lists(&network_id, &old, &new);
        }
        if let Some((network_id, author, target)) = self_authored {
            self.note_self_authored_direct(&network_id, &author, &target);
        }
        if let Some((network_id, target)) = reopened_direct {
            let key = self.reducer.conversation_key(&network_id, &target);
            if self
                .reducer
                .find(&key)
                .is_some_and(|conversation| !conversation.is_channel())
            {
                self.remember_open_direct(&network_id, &target);
            }
        }
        self.note_kept_replay();
        if self.selected.is_none() && !typing_only {
            self.claim_first_conversation();
        }
        if self
            .reducer
            .release_stale_names_sync(self.selected.as_ref(), now)
        {
            viewnotify::merge(
                &mut surfaces,
                vec![
                    ViewNotify::Conversations,
                    ViewNotify::Messages,
                    ViewNotify::MemberReset,
                    ViewNotify::Selection,
                ],
            );
        }
        self.queue(surfaces);
        if let Some(metadata) = metadata_reply {
            self.route_own_metadata_reply(
                &metadata.network_id,
                &metadata.nick,
                &metadata.key,
                &metadata.value,
            );
        }

        if let Some(mention) = self.reducer.take_mention_arrival() {
            self.arrivals.push(Arrival::Mention(mention));
        }
        if let Some(arrival) = self.reducer.take_inbox_arrival() {
            self.append_inbox(InboxItem {
                kind: arrival.kind,
                timestamp: now,
                network_id: arrival.network_id,
                actor: arrival.actor,
                target: arrival.target,
                preview: arrival.body,
                msgid: arrival.msgid,
            });
        }
        self.sync_read_marker_for_selection();
    }

    /// The first conversation the reducer holds claims the selection and takes the
    /// transcript off Status (the first-conversation branch of `Apply`).
    fn claim_first_conversation(&mut self) {
        let Some(key) = self.reducer.conversations().keys().next().cloned() else {
            return;
        };
        let Some(conversation) = self.reducer.find(&key) else {
            return;
        };
        let target = conversation.target.clone();
        let is_channel = conversation.is_channel();
        self.selected = Some(key.clone());
        self.selected_target = target.clone();
        self.reducer.mark_selected(&key);
        self.console_open = false;
        if !is_channel {
            self.request_read_marker_get_once(&key.network_id, &target);
        }
    }

}
