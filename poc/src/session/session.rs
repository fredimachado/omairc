//! Port of `tui/internal/session/session.go` (Go) and `src/irc/ircsession.{h,cpp}`
//! (C++): the connection state machine, the CAP and registration driver, the
//! ping watchdog, reconnect backoff, labeled requests, the batch ledger and the
//! outbound command writers.
//!
//! Concurrency model: the session lives on the UI thread. `Session` is a cheap
//! handle over `Rc<Shared>`, and all mutable state sits in one `RefCell<Core>`.
//! Each entry point runs its body under `locked`, which holds the borrow only
//! while it mutates state. Transport calls (`write`, `connect`, `shutdown`) are
//! queued as `Effect`s and run after the borrow is released, so a loopback peer
//! that answers synchronously can re-enter the session (rule 10 of the master
//! plan). Inbound notifications are queued as `SessionEvent`s and returned by
//! `drain_events`.
//!
//! Submodules hold the state machine's parts: `connection` (disconnect intent,
//! reconnect backoff and retry dedup), `watchdog` (the ping watchdog), `caps` (CAP
//! and SASL: PLAIN, and SCRAM-SHA-256 through `saslscram`), `registration` (NICK/USER,
//! ISUPPORT, 001 and refusals), `batch` (the batch ledger), `history` (CHATHISTORY),
//! `labels` (labeled requests), `readmarker`, `typing`, `metadata`, `autojoin`, `ctcp`,
//! `outbound` (the public writers), `send` (the line writers every write goes
//! through), `dispatch` (inbound bytes and message dispatch), `timers` (the timer
//! slots), `malformed` (protocol-error previews), `wire` (message field helpers),
//! `sink` (the transport's callbacks), `sts` (the STS cache) and `stsflow` (the STS
//! upgrade and reconnect).
//!
//! The socket transport (`transport_net.go`) lives in `net/`. A failed SCRAM
//! exchange is never retried with PLAIN.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::irc::capability::IrcCapabilitySet;
use crate::irc::capabilitynegotiation::IrcCapabilityNegotiation;
use crate::irc::casemapping::IrcCaseMapping;
use crate::irc::framer::IrcFramer;
use crate::irc::message::IrcMessage;
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::wiretext::wire_text;

use super::batch::BatchLedger;
use super::caps::SaslState;
use super::clock::Clock;
use super::config::SessionConfig;
use super::connection::Connection;
use super::events::{ErrorKind, HistoryKind, PendingInvite, SessionEvent, SessionState, Severity};
use super::history::HistoryLedger;
use super::metadata::MetadataCapability;
use super::readmarker::ReadMarkerLedger;
use super::sink::SinkAdapter;
use super::sts::{StsAdvertisement, StsStore};
use super::timers::{ArmedTimer, TimerSlot};
use super::transport::{ConnectionState, Transport, TransportSink};
use super::typing::TypingPublisher;
use super::watchdog::PingWatchdog;
use super::wire;

/// The CHATHISTORY page size before ISUPPORT caps it (`kHistoryLimit`).
pub(crate) const HISTORY_LIMIT: usize = 100;
/// The AFTER pages chained before one LATEST bound closes the gap.
pub(crate) const HISTORY_AFTER_PAGE_CAP: u32 = 10;
/// Playback and LATEST lines kept per batch; the newest lines survive overflow.
pub(crate) const HISTORY_BUFFER_CEILING: usize = 256;
/// Open batches tolerated before unsolicited batches are dropped.
pub(crate) const MAX_OPEN_BATCHES: usize = 16;
/// Ignored batch references remembered before new ones are forgotten.
pub(crate) const MAX_IGNORED_BATCHES: usize = 32;
/// The longest CTCP PING argument that is answered.
pub(crate) const CTCP_PING_PAYLOAD_MAX_BYTES: usize = 32;
/// The minimum spacing of CTCP replies to one nick.
pub(crate) const CTCP_REPLY_INTERVAL: Duration = Duration::from_millis(5000);

/// The inbound filter (`IgnoreFilter`). It runs before any state change or event,
/// with the message, the network id and our own nick (the predicate needs the nick
/// to tell a direct message from a channel line). It must not call the session.
pub type IgnoreFilter = Box<dyn Fn(&IrcMessage, &str, &str) -> bool>;

/// The registration-snapshot lookup a self-JOIN uses as the CHATHISTORY AFTER
/// bound (`setChatHistoryResume`). `Core` passes the live ISUPPORT case mapping;
/// the callback must not call the session.
pub type HistoryResume = Box<dyn Fn(&str, IrcCaseMapping) -> Option<DateTime<Utc>>>;

/// One deferred transport call, run after the state borrow is released.
#[derive(Debug)]
pub(crate) enum Effect {
    /// One framed line, shared with the `Outbound` event that reports it.
    Write(Rc<[u8]>),
    Connect {
        host: String,
        port: u16,
        tls: bool,
    },
    Shutdown,
    /// After a shutdown, move a Closing session to Idle once the transport has
    /// settled (the `post` in Go `stopLocked`).
    SettleStop,
}

/// Which nick the registration tries next (`registrationNickState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegistrationNick {
    Configured,
    Underscore,
    Digit,
}

/// Everything the state machine mutates, behind the `RefCell`.
pub(crate) struct Core {
    pub(crate) config: SessionConfig,
    pub(crate) clock: Rc<dyn Clock>,
    pub(crate) transport: Rc<dyn Transport>,
    /// Weak back-reference, so timer callbacks can reach the session without a
    /// reference cycle.
    pub(crate) weak: Weak<Shared>,

    pub(crate) port: u16,
    pub(crate) tls_enabled: bool,
    pub(crate) nickname: String,
    pub(crate) channel_types: String,
    pub(crate) features: IrcServerFeatures,
    pub(crate) history_limit: usize,

    pub(crate) autojoin_channels: Vec<String>,
    /// Channel keys by channel name, as the profile stores them.
    pub(crate) autojoin_keys: BTreeMap<String, String>,
    /// A key sent with a JOIN, waiting for the server to confirm the join.
    pub(crate) pending_join_keys: BTreeMap<String, String>,

    pub(crate) caps: IrcCapabilityNegotiation,
    /// The read-marker token the server acknowledged (`draft/read-marker` or
    /// `soju.im/read`). The negotiation tracks the capability, not the spelling.
    pub(crate) read_marker_token: Option<&'static str>,
    pub(crate) metadata: MetadataCapability,
    pub(crate) published_caps: IrcCapabilitySet,

    pub(crate) typing: TypingPublisher,
    pub(crate) framer: IrcFramer,
    pub(crate) batches: BatchLedger,
    pub(crate) history: HistoryLedger,
    pub(crate) read_markers: ReadMarkerLedger,

    pub(crate) pending_labels: BTreeMap<String, DateTime<Utc>>,
    pub(crate) next_request_label: u64,
    pub(crate) ctcp_reply_clock: BTreeMap<String, DateTime<Utc>>,

    pub(crate) ping: PingWatchdog,
    pub(crate) connection: Connection,

    pub(crate) registration_sent: bool,
    pub(crate) registration_nick: RegistrationNick,
    pub(crate) sasl: SaslState,

    pub(crate) capability_negotiation_ended: bool,
    pub(crate) capability_list_seen: bool,
    /// The STS advertisement a CAP LS carried, held until its last line (`pendingSTS`).
    pub(crate) pending_sts: Option<StsAdvertisement>,
    /// The duration last advertised over TLS, re-stored on disconnect (`stsDuration`).
    pub(crate) sts_duration: Option<i64>,
    /// The STS policy cache, shared with every session of the manager (`sts`).
    pub(crate) sts: Rc<dyn StsStore>,

    pub(crate) state: SessionState,
    pub(crate) ignore_filter: Option<IgnoreFilter>,
    pub(crate) pending_invite: Option<PendingInvite>,
    pub(crate) history_resume: Option<HistoryResume>,

    pub(crate) timers: [Option<ArmedTimer>; TimerSlot::COUNT],
    pub(crate) timer_sequence: u64,
    pub(crate) events: Vec<SessionEvent>,
    pub(crate) effects: Vec<Effect>,
}

/// The shared allocation behind every `Session` handle.
pub(crate) struct Shared {
    core: RefCell<Core>,
    /// Keeps the transport's weak sink alive for as long as the session lives.
    _sink: Rc<dyn TransportSink>,
}

/// A handle to one network's session (`Session` in Go). Cheap to clone.
#[derive(Clone)]
pub struct Session {
    pub(crate) shared: Rc<Shared>,
}

impl Session {
    /// Builds a session and installs it as the transport's sink. `transport` and
    /// `clock` are shared with the owner, so tests keep a handle to drive them. `sts` is the
    /// policy cache, shared with the other sessions of the manager.
    pub fn new(
        config: SessionConfig,
        transport: Rc<dyn Transport>,
        clock: Rc<dyn Clock>,
        sts: Rc<dyn StsStore>,
    ) -> Self {
        let shared = Rc::new_cyclic(|weak: &Weak<Shared>| {
            let sink: Rc<dyn TransportSink> = Rc::new(SinkAdapter {
                session: weak.clone(),
            });
            let core = Core::new(config, clock, transport.clone(), weak.clone(), sts);
            Shared {
                core: RefCell::new(core),
                _sink: sink,
            }
        });
        let sink: Weak<dyn TransportSink> = Rc::downgrade(&shared._sink);
        transport.set_sink(sink);
        Self { shared }
    }

    pub(crate) fn from_shared(shared: Rc<Shared>) -> Self {
        Self { shared }
    }

    /// Runs `action` under the state borrow, then the queued transport calls in
    /// order, after the borrow is released.
    pub(crate) fn locked<R>(&self, action: impl FnOnce(&mut Core) -> R) -> R {
        let (result, effects) = {
            let mut core = self.shared.core.borrow_mut();
            let result = action(&mut core);
            (result, std::mem::take(&mut core.effects))
        };
        self.run_effects(effects);
        result
    }

    pub(crate) fn case_mapping(&self) -> crate::irc::casemapping::IrcCaseMapping {
        self.shared.core.borrow().features.case_mapping()
    }

    pub(crate) fn run_effects(&self, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Write(bytes) => {
                    let transport = self.shared.core.borrow().transport.clone();
                    transport.write(&bytes);
                }
                Effect::Connect { host, port, tls } => {
                    let transport = self.shared.core.borrow().transport.clone();
                    transport.connect(&host, port, tls);
                }
                Effect::Shutdown => {
                    let transport = self.shared.core.borrow().transport.clone();
                    transport.shutdown();
                }
                Effect::SettleStop => {
                    let transport = self.shared.core.borrow().transport.clone();
                    let settled = matches!(
                        transport.state(),
                        ConnectionState::Idle
                            | ConnectionState::Disconnected
                            | ConnectionState::Failed
                    );
                    if settled {
                        self.locked(|core| core.set_state(SessionState::Idle));
                    }
                }
            }
        }
    }

    /// Returns and clears every event queued since the last call, in order.
    pub fn drain_events(&self) -> Vec<SessionEvent> {
        std::mem::take(&mut self.shared.core.borrow_mut().events)
    }

    /// Installs the inbound drop predicate. `None` keeps every message.
    pub fn set_ignore_filter(&self, filter: Option<IgnoreFilter>) {
        self.shared.core.borrow_mut().ignore_filter = filter;
    }

    /// Installs the self-JOIN resume lookup. `None` keeps LATEST.
    pub fn set_history_resume(&self, resume: Option<HistoryResume>) {
        self.shared.core.borrow_mut().history_resume = resume;
    }

    /// Lowers the AFTER page cap, for tests. Production keeps the default.
    // Read by the socket transport and the later phases; cfg(test) comes off with its first consumer.
    #[cfg(test)]
    pub fn set_history_after_page_cap(&self, cap: u32) {
        if cap == 0 {
            return;
        }
        self.shared.core.borrow_mut().history.after_page_cap = cap;
    }

    /// The network id.
    pub fn network_id(&self) -> String {
        self.shared.core.borrow().config.network_id.clone()
    }

    /// The display name: the trimmed configured name, else the host.
    pub fn name(&self) -> String {
        self.shared.core.borrow().config.resolved_name()
    }

    /// The configured host.
    pub fn host(&self) -> String {
        self.shared.core.borrow().config.host.clone()
    }

    /// The active port: the configured one, or the one an STS upgrade or a cached policy chose.
    pub fn port(&self) -> u16 {
        self.shared.core.borrow().port
    }

    /// Whether the active connection uses TLS.
    pub fn tls_enabled(&self) -> bool {
        self.shared.core.borrow().tls_enabled
    }

    /// The current nick: the configured nick, a fallback, or the server's
    /// assignment after 001 or a self NICK.
    pub fn nick(&self) -> String {
        self.shared.core.borrow().nickname.clone()
    }

    /// The CHANTYPES the server advertised, or empty before ISUPPORT.
    // Read by the socket transport and the later phases; cfg(test) comes off with its first consumer.
    #[cfg(test)]
    pub fn channel_types(&self) -> String {
        self.shared.core.borrow().channel_types.clone()
    }

    /// The state machine's current state.
    pub fn state(&self) -> SessionState {
        self.shared.core.borrow().state
    }

    /// How many reconnect attempts have been scheduled since the last success.
    // Read by the socket transport and the later phases; cfg(test) comes off with its first consumer.
    #[cfg(test)]
    pub fn reconnect_attempt(&self) -> u32 {
        self.shared.core.borrow().connection.reconnect_attempt()
    }

    /// The capabilities the server acknowledged.
    // Read by the socket transport and the later phases; cfg(test) comes off with its first consumer.
    #[cfg(test)]
    pub fn capabilities(&self) -> IrcCapabilitySet {
        self.shared.core.borrow().caps.enabled()
    }

    /// The draft/metadata-2 limits the server last advertised.
    pub fn metadata_capability(&self) -> MetadataCapability {
        self.shared.core.borrow().metadata.clone()
    }

    /// Whether a CHATHISTORY request is outstanding on any target.
    // Read by the socket transport and the later phases; cfg(test) comes off with its first consumer.
    #[cfg(test)]
    pub fn history_pending(&self) -> bool {
        !self.shared.core.borrow().history.pending.is_empty()
    }

    /// A copy of the autojoin channel list.
    pub fn autojoin_channels(&self) -> Vec<String> {
        self.shared.core.borrow().autojoin_channels.clone()
    }

    /// The CHATHISTORY page size for this connection.
    pub fn history_limit(&self) -> usize {
        self.shared.core.borrow().history_limit
    }

    /// Whether a CHATHISTORY request is in flight for `target`.
    pub fn history_inflight_for(&self, target: &str) -> bool {
        self.locked(|core| core.history_inflight(target))
    }

    /// The last unhandled INVITE, if one is still pending.
    pub fn pending_invite(&self) -> Option<PendingInvite> {
        self.shared.core.borrow().pending_invite.clone()
    }

    /// How many batches are open.
    // Read by the socket transport and the later phases; cfg(test) comes off with its first consumer.
    #[cfg(test)]
    pub fn open_batch_count(&self) -> usize {
        self.shared.core.borrow().batches.open.len()
    }

    /// How many labeled requests are outstanding.
    // Read by the socket transport and the later phases; cfg(test) comes off with its first consumer.
    #[cfg(test)]
    pub fn pending_request_label_count(&self) -> usize {
        self.shared.core.borrow().pending_labels.len()
    }

    /// Whether `label` names an outstanding labeled request.
    // Read by the socket transport and the later phases; cfg(test) comes off with its first consumer.
    #[cfg(test)]
    pub fn has_pending_request_label(&self, label: &str) -> bool {
        self.shared.core.borrow().pending_labels.contains_key(label)
    }

    /// Credentials for a file upload: the account, the secret SASL sent (or the
    /// server password until SASL succeeds), the host and the TLS state.
    // Read by the socket transport and the later phases; cfg(test) comes off with its first consumer.
    #[cfg(test)]
    pub fn upload_credential(&self) -> (String, String, String, bool) {
        let core = self.shared.core.borrow();
        let user = core.config.sasl_account_name().to_owned();
        let secret = if core.sasl.succeeded {
            core.config.sasl_secret().expose().to_owned()
        } else {
            core.config.password.expose().to_owned()
        };
        (user, secret, core.config.host.clone(), core.tls_enabled)
    }

    /// Begins connecting. A start is ignored unless the session is Idle or
    /// Failed. An incomplete config fails the session with a registration error
    /// (`IrcSession::start`).
    pub fn start(&self) {
        self.locked(Core::start);
    }

    /// Shuts the session down. A session that is reconnecting or failed goes
    /// straight to Idle.
    pub fn stop(&self) {
        self.locked(Core::stop);
    }

    /// Sends QUIT when registered, then stops. Returns true when the session
    /// was not Idle and has been stopped.
    pub fn quit(&self, reason: &str) -> bool {
        self.locked(|core| {
            if core.state == SessionState::Idle {
                return false;
            }
            if core.state == SessionState::Registered {
                let command = if reason.is_empty() {
                    "QUIT".to_owned()
                } else {
                    format!("QUIT :{reason}")
                };
                if !core.send_command(&command, "") {
                    return false;
                }
            }
            core.stop();
            true
        })
    }

    /// Reports that the host is reachable again. A scheduled reconnect starts
    /// now instead of waiting out the backoff; otherwise this does nothing.
    // Read by the socket transport and the later phases; cfg(test) comes off with its first consumer.
    #[cfg(test)]
    pub fn notify_reachable(&self) {
        self.locked(Core::begin_reconnect_attempt);
    }
}

impl Core {
    fn new(
        config: SessionConfig,
        clock: Rc<dyn Clock>,
        transport: Rc<dyn Transport>,
        weak: Weak<Shared>,
        sts: Rc<dyn StsStore>,
    ) -> Self {
        let sasl_available = !config.sasl_secret().is_empty();
        let port = config.port;
        let tls_enabled = config.tls_enabled;
        let nickname = config.nick.clone();
        let autojoin_channels = config.autojoin_channels.clone();
        let autojoin_keys = config.autojoin_keys.clone();
        Self {
            config,
            clock,
            transport,
            weak,
            port,
            tls_enabled,
            nickname,
            channel_types: String::new(),
            features: IrcServerFeatures::new(),
            history_limit: HISTORY_LIMIT,
            autojoin_channels,
            autojoin_keys,
            pending_join_keys: BTreeMap::new(),
            caps: IrcCapabilityNegotiation::new(sasl_available),
            read_marker_token: None,
            metadata: MetadataCapability::default(),
            published_caps: IrcCapabilitySet::default(),
            typing: TypingPublisher::default(),
            framer: IrcFramer::new(),
            batches: BatchLedger::default(),
            history: HistoryLedger::new(HISTORY_AFTER_PAGE_CAP),
            read_markers: ReadMarkerLedger::default(),
            pending_labels: BTreeMap::new(),
            next_request_label: 0,
            ctcp_reply_clock: BTreeMap::new(),
            ping: PingWatchdog::default(),
            connection: Connection::default(),
            registration_sent: false,
            registration_nick: RegistrationNick::Configured,
            sasl: SaslState::default(),
            capability_negotiation_ended: false,
            capability_list_seen: false,
            pending_sts: None,
            sts_duration: None,
            sts,
            state: SessionState::Idle,
            ignore_filter: None,
            pending_invite: None,
            history_resume: None,
            timers: Default::default(),
            timer_sequence: 0,
            events: Vec::new(),
            effects: Vec::new(),
        }
    }

    // ---- Shared helpers -------------------------------------------------

    pub(crate) fn network_id(&self) -> String {
        self.config.network_id.clone()
    }

    pub(crate) fn emit(&mut self, event: SessionEvent) {
        self.events.push(event);
    }

    pub(crate) fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    /// Moves to `state`, emitting `StateChanged` when it changes. Leaving
    /// Registered drops a pending INVITE.
    pub(crate) fn set_state(&mut self, state: SessionState) {
        if self.state == state {
            return;
        }
        if self.state == SessionState::Registered && state != SessionState::Registered {
            self.pending_invite = None;
        }
        self.state = state;
        let network_id = self.network_id();
        self.emit(SessionEvent::StateChanged { network_id, state });
    }

    pub(crate) fn start(&mut self) {
        if self.state != SessionState::Idle && self.state != SessionState::Failed {
            return;
        }
        let config = &self.config;
        if config.network_id.is_empty()
            || config.host.is_empty()
            || config.port == 0
            || config.nick.is_empty()
            || config.username.is_empty()
            || config.realname.is_empty()
        {
            self.fail(
                ErrorKind::Registration,
                "IRC session configuration is incomplete",
                false,
            );
            return;
        }
        self.connection.begin_start();
        self.port = self.config.port;
        self.tls_enabled = self.config.tls_enabled;
        self.sts_duration = None;
        self.apply_cached_sts();
        self.reset_for_connection();
        self.set_state(SessionState::Connecting);
        self.connect_transport();
    }

    /// Ends the session the way `IrcSession::stop` does: cancels every timer,
    /// clears labels, and shuts the transport down.
    pub(crate) fn stop(&mut self) {
        self.reschedule_sts_expiry();
        self.connection.begin_stop();
        self.cancel_timer(TimerSlot::Reconnect);
        self.cancel_timer(TimerSlot::Capability);
        self.cancel_ping_watchdog();
        self.clear_pending_request_labels(true);

        if self.state == SessionState::Idle {
            return;
        }
        if self.state == SessionState::Reconnecting || self.state == SessionState::Failed {
            self.set_state(SessionState::Idle);
            return;
        }
        self.set_state(SessionState::Closing);
        self.effects.push(Effect::Shutdown);
        self.effects.push(Effect::SettleStop);
    }

    pub(crate) fn connect_transport(&mut self) {
        self.effects.push(Effect::Connect {
            host: self.config.host.clone(),
            port: self.port,
            tls: self.tls_enabled,
        });
    }

    /// Fails the session (`IrcSession::fail`). With `reconnect`, the first
    /// failure of each kind is reported and the session retries; otherwise the
    /// session stops in Failed.
    pub(crate) fn fail(&mut self, kind: ErrorKind, message: &str, reconnect: bool) {
        self.cancel_ping_watchdog();
        self.clear_pending_request_labels(true);

        let network_id = self.network_id();
        if reconnect {
            if self.connection.first_report_of(kind) {
                self.emit(SessionEvent::ErrorOccurred {
                    network_id,
                    kind,
                    message: message.to_owned(),
                });
            }
            if matches!(
                self.transport.state(),
                ConnectionState::Connecting
                    | ConnectionState::Connected
                    | ConnectionState::Encrypted
                    | ConnectionState::Closing
            ) {
                self.connection.reconnect_on_disconnect();
                self.effects.push(Effect::Shutdown);
                return;
            }
            self.schedule_reconnect();
            return;
        }

        self.emit(SessionEvent::ErrorOccurred {
            network_id,
            kind,
            message: message.to_owned(),
        });
        self.connection.end_on_disconnect();
        self.cancel_timer(TimerSlot::Reconnect);
        self.set_state(SessionState::Failed);
        self.effects.push(Effect::Shutdown);
    }

    pub(crate) fn schedule_reconnect(&mut self) {
        if !self.config.reconnect_enabled {
            self.set_state(SessionState::Failed);
            return;
        }
        if self.state == SessionState::Reconnecting {
            return;
        }
        let attempt = self.connection.count_scheduled_reconnect();
        let delay = self.connection.next_delay(&self.config);
        self.set_state(SessionState::Reconnecting);
        self.arm_timer(TimerSlot::Reconnect, delay);
        let network_id = self.network_id();
        self.emit(SessionEvent::ReconnectScheduled {
            network_id,
            delay_ms: delay.as_millis() as u64,
            attempt,
        });
    }

    pub(crate) fn begin_reconnect_attempt(&mut self) {
        if self.state != SessionState::Reconnecting {
            return;
        }
        self.cancel_timer(TimerSlot::Reconnect);
        self.apply_cached_sts();
        self.reset_for_connection();
        self.set_state(SessionState::Connecting);
        self.connect_transport();
    }

    /// Clears everything one connection learned: the framer, batches, history
    /// cursors, negotiated features, SASL and registration progress.
    pub(crate) fn reset_for_connection(&mut self) {
        self.framer = IrcFramer::new();
        self.batches = BatchLedger::default();
        self.history.reset();
        self.history_limit = HISTORY_LIMIT;
        self.features = IrcServerFeatures::new();
        self.nickname = self.config.nick.clone();
        self.registration_sent = false;
        self.registration_nick = RegistrationNick::Configured;
        self.sasl = SaslState::default();
        self.capability_negotiation_ended = false;
        self.capability_list_seen = false;
        self.pending_sts = None;
        self.pending_invite = None;
        self.pending_join_keys.clear();
        self.channel_types.clear();
        self.cancel_timer(TimerSlot::Capability);
        self.cancel_ping_watchdog();
        self.clear_pending_request_labels(true);
        self.caps.reset(!self.config.sasl_secret().is_empty());
        self.read_marker_token = None;
        self.metadata = MetadataCapability::default();
        self.typing.reset();
        self.publish_capabilities();
    }

    /// Queues an inbound message for the translator (`emitMessageReceived`).
    pub(crate) fn emit_message_received(&mut self, message: IrcMessage) {
        let network_id = self.network_id();
        self.emit(SessionEvent::MessageReceived {
            network_id,
            message,
        });
    }

    /// Whether the message's source is this session's own nick.
    pub(crate) fn self_prefixed(&self, message: &IrcMessage) -> bool {
        self.is_self(&wire::claimed_nick(message))
    }

    pub(crate) fn is_self(&self, nick: &str) -> bool {
        !nick.is_empty() && self.nicks_equal(nick, &self.nickname)
    }

    pub(crate) fn nicks_equal(&self, left: &str, right: &str) -> bool {
        self.features.case_mapping().equals(left, right)
    }

    pub(crate) fn fold_channel(&self, channel: &str) -> String {
        wire_text(self.features.case_mapping().normalize(channel).as_bytes())
    }
}

/// Whether a target can take PRIVMSG: non-empty, no leading colon, no whitespace
/// or control character.
pub(crate) fn is_valid_privmsg_target(target: &str) -> bool {
    !target.is_empty()
        && !target.starts_with(':')
        && !target
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
}

/// Shorthand for the history kinds the session asks about.
pub(crate) fn replay_kind_for(batch_type: &str) -> Option<HistoryKind> {
    match batch_type.to_lowercase().as_str() {
        "chathistory" | "draft/chathistory" => Some(HistoryKind::Chat),
        "draft/chathistory-targets" => Some(HistoryKind::Targets),
        "znc.in/playback" => Some(HistoryKind::BouncerPlayback),
        _ => None,
    }
}

/// The notice a session raises itself (a [`SessionEvent::Lifecycle`]).
pub(crate) fn lifecycle_event(
    network_id: &str,
    severity: Severity,
    kind: &'static str,
    text: &str,
    at: DateTime<Utc>,
) -> SessionEvent {
    SessionEvent::Lifecycle {
        network_id: network_id.to_owned(),
        severity,
        kind,
        text: text.to_owned(),
        at,
    }
}
