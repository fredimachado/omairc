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
//! Submodules hold the state machine's parts: `caps` (CAP and SASL PLAIN),
//! `registration` (NICK/USER, ISUPPORT, 001 and refusals), `batch` (the batch
//! ledger), `history` (CHATHISTORY), `labels` (labeled requests), `readmarker`,
//! `typing`, `metadata`, `autojoin`, `ctcp`, `outbound` (the public writers),
//! `malformed` (protocol-error previews) and `wire` (message field helpers).
//!
//! Out of scope until Phase 11: STS (`sts.go`), SASL SCRAM (`saslscram.go`) and
//! the socket transport (`transport_net.go`). The state machine refuses SCRAM
//! rather than fall back to PLAIN.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};
use std::time::Duration;

use chrono::{DateTime, Utc};

use crate::irc::capability::{IrcCapability, IrcCapabilitySet};
use crate::irc::capabilitynegotiation::IrcCapabilityNegotiation;
use crate::irc::framer::IrcFramer;
use crate::irc::message::IrcMessage;
use crate::irc::parser::parse;
use crate::irc::prefixnick::prefix_nick;
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::wiretext::wire_text;

use super::batch::BatchLedger;
use super::clock::Clock;
use super::config::SessionConfig;
use super::events::{ErrorKind, HistoryKind, PendingInvite, SessionEvent, SessionState, Severity};
use super::history::HistoryLedger;
use super::metadata::MetadataCapability;
use super::readmarker::ReadMarkerLedger;
use super::transport::{ConnectionState, Transport, TransportSink};
use super::typing::TypingPublisher;
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
/// The PING token the watchdog sends, and expects back in PONG.
pub(crate) const PING_WATCHDOG_TOKEN: &str = "omairc-watchdog";
/// The longest CTCP PING argument that is answered.
pub(crate) const CTCP_PING_PAYLOAD_MAX_BYTES: usize = 32;
/// The minimum spacing of CTCP replies to one nick.
pub(crate) const CTCP_REPLY_INTERVAL: Duration = Duration::from_millis(5000);

/// The inbound filter (`IgnoreFilter`). It runs before any state change or event,
/// with the message, the network id and our own nick (the predicate needs the nick
/// to tell a direct message from a channel line). It must not call the session.
pub type IgnoreFilter = Box<dyn Fn(&IrcMessage, &str, &str) -> bool>;

/// The registration-snapshot lookup a self-JOIN uses as the CHATHISTORY AFTER
/// bound (`setChatHistoryResume`). It must not call the session.
pub type HistoryResume = Box<dyn Fn(&str) -> Option<DateTime<Utc>>>;

/// One deferred transport call, run after the state borrow is released.
#[derive(Debug)]
pub(crate) enum Effect {
    Write(Vec<u8>),
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

/// Where the ping watchdog stands (`pingWatchdogState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PingWatchdog {
    Off,
    Watching,
    Probing,
}

/// Which nick the registration tries next (`registrationNickState`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RegistrationNick {
    Configured,
    Underscore,
    Digit,
}

/// The SASL bookkeeping of one connection (the `saslX` fields of Go).
#[derive(Clone, Debug, Default)]
pub(crate) struct SaslState {
    pub(crate) requested: bool,
    pub(crate) pending: bool,
    pub(crate) succeeded: bool,
    pub(crate) mechanism: Option<&'static str>,
    pub(crate) exchange_started: bool,
}

/// The timers the session owns. Each slot holds at most one armed timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TimerSlot {
    Reconnect,
    Capability,
    Ping,
    Label,
}

impl TimerSlot {
    const COUNT: usize = 4;

    fn index(self) -> usize {
        self as usize
    }
}

struct ArmedTimer {
    id: u64,
    handle: Box<dyn super::clock::ClockTimer>,
}

/// Everything the state machine mutates, behind the `RefCell`.
// Phase 4: the slash catalog and the socket transport drive these
#[allow(dead_code)]
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
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub(crate) next_request_label: u64,
    pub(crate) ctcp_reply_clock: BTreeMap<String, DateTime<Utc>>,

    pub(crate) ping: PingWatchdog,
    pub(crate) reconnect_attempt: u32,
    pub(crate) reported_retry_errors: u32,

    pub(crate) registration_sent: bool,
    pub(crate) registration_nick: RegistrationNick,
    pub(crate) sasl: SaslState,

    pub(crate) capability_negotiation_ended: bool,
    pub(crate) capability_list_seen: bool,

    pub(crate) expected_disconnect: bool,
    pub(crate) reconnect_after_disconnect: bool,

    pub(crate) state: SessionState,
    pub(crate) ignore_filter: Option<IgnoreFilter>,
    pub(crate) pending_invite: Option<PendingInvite>,
    pub(crate) history_resume: Option<HistoryResume>,

    timers: [Option<ArmedTimer>; TimerSlot::COUNT],
    timer_sequence: u64,
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

/// Receives the transport's callbacks and forwards them to the session. Holds a
/// weak reference, so a dropped session stops receiving.
struct SinkAdapter {
    session: Weak<Shared>,
}

impl SinkAdapter {
    fn with_session(&self, action: impl FnOnce(&Session)) {
        if let Some(shared) = self.session.upgrade() {
            action(&Session { shared });
        }
    }
}

impl TransportSink for SinkAdapter {
    fn connected(&self) {
        self.with_session(|session| session.locked(Core::on_connected));
    }

    fn encrypted(&self) {
        self.with_session(|session| session.locked(Core::on_encrypted));
    }

    fn disconnected(&self) {
        self.with_session(|session| session.locked(Core::on_disconnected));
    }

    fn bytes_received(&self, bytes: &[u8]) {
        let bytes = bytes.to_vec();
        self.with_session(|session| session.locked(|core| core.handle_bytes(&bytes)));
    }

    fn error(&self, message: &str) {
        let message = message.to_owned();
        self.with_session(|session| session.locked(|core| core.error_occurred(&message)));
    }
}

impl Session {
    /// Builds a session and installs it as the transport's sink. `transport` and
    /// `clock` are shared with the owner, so tests keep a handle to drive them.
    pub fn new(config: SessionConfig, transport: Rc<dyn Transport>, clock: Rc<dyn Clock>) -> Self {
        let shared = Rc::new_cyclic(|weak: &Weak<Shared>| {
            let sink: Rc<dyn TransportSink> = Rc::new(SinkAdapter {
                session: weak.clone(),
            });
            let core = Core::new(config, clock, transport.clone(), weak.clone());
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
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn set_history_resume(&self, resume: Option<HistoryResume>) {
        self.shared.core.borrow_mut().history_resume = resume;
    }

    /// Lowers the AFTER page cap, for tests. Production keeps the default.
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
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
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn host(&self) -> String {
        self.shared.core.borrow().config.host.clone()
    }

    /// The active port, as configured until STS (Phase 11) changes it.
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn port(&self) -> u16 {
        self.shared.core.borrow().port
    }

    /// Whether the active connection uses TLS.
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn tls_enabled(&self) -> bool {
        self.shared.core.borrow().tls_enabled
    }

    /// The current nick: the configured nick, a fallback, or the server's
    /// assignment after 001 or a self NICK.
    pub fn nick(&self) -> String {
        self.shared.core.borrow().nickname.clone()
    }

    /// The CHANTYPES the server advertised, or empty before ISUPPORT.
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn channel_types(&self) -> String {
        self.shared.core.borrow().channel_types.clone()
    }

    /// The state machine's current state.
    pub fn state(&self) -> SessionState {
        self.shared.core.borrow().state
    }

    /// How many reconnect attempts have been scheduled since the last success.
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn reconnect_attempt(&self) -> u32 {
        self.shared.core.borrow().reconnect_attempt
    }

    /// The capabilities the server acknowledged.
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn capabilities(&self) -> IrcCapabilitySet {
        self.shared.core.borrow().caps.enabled()
    }

    /// The draft/metadata-2 limits the server last advertised.
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn metadata_capability(&self) -> MetadataCapability {
        self.shared.core.borrow().metadata.clone()
    }

    /// Whether a CHATHISTORY request is outstanding on any target.
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn history_pending(&self) -> bool {
        !self.shared.core.borrow().history.pending.is_empty()
    }

    /// Whether a CHATHISTORY request is outstanding on `target`, for the current
    /// membership generation.
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn history_inflight_for(&self, target: &str) -> bool {
        self.shared.core.borrow().history_inflight(target)
    }

    /// The CHATHISTORY page size: 100, capped by ISUPPORT `CHATHISTORY=`.
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn history_limit(&self) -> usize {
        self.shared.core.borrow().history_limit
    }

    /// A copy of the autojoin channel list.
    pub fn autojoin_channels(&self) -> Vec<String> {
        self.shared.core.borrow().autojoin_channels.clone()
    }

    /// The last unhandled INVITE, if one is still pending.
    pub fn pending_invite(&self) -> Option<PendingInvite> {
        self.shared.core.borrow().pending_invite.clone()
    }

    /// How many batches are open.
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn open_batch_count(&self) -> usize {
        self.shared.core.borrow().batches.open.len()
    }

    /// How many labeled requests are outstanding.
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn pending_request_label_count(&self) -> usize {
        self.shared.core.borrow().pending_labels.len()
    }

    /// Whether `label` names an outstanding labeled request.
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn has_pending_request_label(&self, label: &str) -> bool {
        self.shared.core.borrow().pending_labels.contains_key(label)
    }

    /// Credentials for a file upload: the account, the secret SASL sent (or the
    /// server password until SASL succeeds), the host and the TLS state.
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn upload_credential(&self) -> (String, String, String, bool) {
        let core = self.shared.core.borrow();
        let user = core.config.sasl_account_name().to_owned();
        let secret = if core.sasl.succeeded {
            core.config.sasl_secret().to_owned()
        } else {
            core.config.password.clone()
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
    // Phase 4: the slash catalog and the socket transport drive these
    #[allow(dead_code)]
    pub fn notify_reachable(&self) {
        self.locked(Core::begin_reconnect_attempt);
    }
}

// Phase 4: the slash catalog and the socket transport drive these
#[allow(dead_code)]
impl Core {
    fn new(
        config: SessionConfig,
        clock: Rc<dyn Clock>,
        transport: Rc<dyn Transport>,
        weak: Weak<Shared>,
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
            ping: PingWatchdog::Off,
            reconnect_attempt: 0,
            reported_retry_errors: 0,
            registration_sent: false,
            registration_nick: RegistrationNick::Configured,
            sasl: SaslState::default(),
            capability_negotiation_ended: false,
            capability_list_seen: false,
            expected_disconnect: false,
            reconnect_after_disconnect: false,
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

    pub(crate) fn on_connected(&mut self) {
        if !self.tls_enabled && self.state == SessionState::Connecting {
            self.begin_capability_negotiation();
        }
    }

    pub(crate) fn on_encrypted(&mut self) {
        if self.tls_enabled && self.state == SessionState::Connecting {
            self.begin_capability_negotiation();
        }
    }

    /// The connection ended (`IrcTransport::disconnected`).
    pub(crate) fn on_disconnected(&mut self) {
        if self.reconnect_after_disconnect {
            self.reconnect_after_disconnect = false;
            self.schedule_reconnect();
            return;
        }
        if self.expected_disconnect {
            if self.state == SessionState::Closing {
                self.set_state(SessionState::Idle);
            }
            return;
        }
        self.fail(ErrorKind::Network, "Connection closed by the server", true);
    }

    /// A transport failure (`IrcTransport::errorOccurred`). A TLS or certificate
    /// message is classified as a TLS failure.
    // Phase 11: the socket transport reports its errors
    #[allow(dead_code)]
    pub(crate) fn error_occurred(&mut self, message: &str) {
        if self.expected_disconnect {
            return;
        }
        let lowered = message.to_lowercase();
        let tls_failure = lowered.contains("tls") || lowered.contains("certificate");
        let kind = if tls_failure {
            ErrorKind::Tls
        } else {
            ErrorKind::Network
        };
        self.fail(kind, message, true);
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
        self.expected_disconnect = false;
        self.reconnect_after_disconnect = false;
        self.reconnect_attempt = 0;
        self.reported_retry_errors = 0;
        self.port = self.config.port;
        self.tls_enabled = self.config.tls_enabled;
        self.reset_for_connection();
        self.set_state(SessionState::Connecting);
        self.connect_transport();
    }

    /// Ends the session the way `IrcSession::stop` does: cancels every timer,
    /// clears labels, and shuts the transport down.
    pub(crate) fn stop(&mut self) {
        self.expected_disconnect = true;
        self.reconnect_after_disconnect = false;
        self.cancel_timer(TimerSlot::Reconnect);
        self.cancel_timer(TimerSlot::Capability);
        self.cancel_ping_watchdog();
        self.clear_pending_request_labels(true);
        self.reconnect_attempt = 0;
        self.reported_retry_errors = 0;

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
            let bit = kind.bit();
            if self.reported_retry_errors & bit == 0 {
                self.reported_retry_errors |= bit;
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
                self.reconnect_after_disconnect = true;
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
        self.expected_disconnect = true;
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
        self.reconnect_attempt += 1;
        let delay = self.reconnect_delay_ms();
        self.set_state(SessionState::Reconnecting);
        self.arm_timer(TimerSlot::Reconnect, Duration::from_millis(delay));
        let network_id = self.network_id();
        let attempt = self.reconnect_attempt;
        self.emit(SessionEvent::ReconnectScheduled {
            network_id,
            delay_ms: delay,
            attempt,
        });
    }

    pub(crate) fn begin_reconnect_attempt(&mut self) {
        if self.state != SessionState::Reconnecting {
            return;
        }
        self.cancel_timer(TimerSlot::Reconnect);
        self.reset_for_connection();
        self.set_state(SessionState::Connecting);
        self.connect_transport();
    }

    /// The backoff for the current attempt: the base, doubled per attempt, capped
    /// at the maximum (`reconnectDelayLocked`).
    pub(crate) fn reconnect_delay_ms(&self) -> u64 {
        let base = self.config.reconnect_base_delay_ms;
        let maximum = self.config.reconnect_maximum_delay_ms.max(base);
        let mut delay = base;
        let mut step = 1;
        while step < self.reconnect_attempt && delay < maximum {
            delay = (delay * 2).min(maximum);
            step += 1;
        }
        delay.min(i32::MAX as u64)
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

    // ---- Writers --------------------------------------------------------

    /// Queues one already-built line (`sendLineLocked`). It emits the outbound
    /// event, then writes the frame.
    pub(crate) fn send_line(&mut self, line: &[u8]) {
        if line.is_empty() {
            return;
        }
        let event = SessionEvent::Outbound {
            network_id: self.network_id(),
            line: line.to_vec(),
            channel_types: self.channel_types.clone(),
            at: self.now(),
        };
        self.emit(event);
        self.effects.push(Effect::Write(line.to_vec()));
    }

    /// Builds a command as a line, optionally labeled, and sends it. A command
    /// is written only while registered (`sendCommandLocked`).
    pub(crate) fn send_command(&mut self, command: &str, request_label: &str) -> bool {
        if self.state != SessionState::Registered {
            return false;
        }
        let wire = if request_label.is_empty() {
            command.to_owned()
        } else {
            if !is_tag_safe_label(request_label) {
                return false;
            }
            format!("@label={request_label} {command}")
        };
        let Ok(line) = crate::irc::commandbuilder::line(&wire, &self.features) else {
            return false;
        };
        self.send_line(line.as_bytes());
        true
    }

    /// Sends a body that may need several frames, each a `prefix`, a chunk, and a
    /// `suffix` (`sendTrailingBodyLocked`). A labeled request must fit one frame.
    pub(crate) fn send_trailing_body(
        &mut self,
        prefix: &str,
        body: &str,
        suffix: &str,
        request_label: &str,
    ) -> bool {
        let chunks: Vec<String> =
            crate::irc::commandbuilder::split_trailing_param(prefix, body, suffix, &self.features)
                .into_iter()
                .map(str::to_owned)
                .collect();
        if chunks.is_empty() {
            return false;
        }
        if !request_label.is_empty() && chunks.len() != 1 {
            return false;
        }
        for chunk in &chunks {
            let command = format!("{prefix}{chunk}{suffix}");
            if !self.send_command(&command, request_label) {
                return false;
            }
        }
        true
    }

    /// Queues an inbound message for the translator (`emitMessageReceived`).
    pub(crate) fn emit_message_received(&mut self, message: IrcMessage) {
        let network_id = self.network_id();
        self.emit(SessionEvent::MessageReceived {
            network_id,
            message,
        });
    }

    /// Records the one-shot handle for `slot`, cancelling any armed predecessor.
    /// The callback is ignored when a newer arm replaced it.
    pub(crate) fn arm_timer(&mut self, slot: TimerSlot, delay: Duration) {
        self.cancel_timer(slot);
        self.timer_sequence += 1;
        let id = self.timer_sequence;
        let weak = self.weak.clone();
        let handle = self.clock.after(
            delay,
            Box::new(move || {
                if let Some(shared) = weak.upgrade() {
                    Session::from_shared(shared).locked(|core| core.timer_fired(slot, id));
                }
            }),
        );
        self.timers[slot.index()] = Some(ArmedTimer { id, handle });
    }

    pub(crate) fn cancel_timer(&mut self, slot: TimerSlot) {
        if let Some(armed) = self.timers[slot.index()].take() {
            armed.handle.stop();
        }
    }

    pub(crate) fn timer_fired(&mut self, slot: TimerSlot, id: u64) {
        let current = matches!(&self.timers[slot.index()], Some(armed) if armed.id == id);
        if !current {
            return;
        }
        self.timers[slot.index()] = None;
        match slot {
            TimerSlot::Reconnect => self.begin_reconnect_attempt(),
            TimerSlot::Capability => self.on_capability_timeout(),
            TimerSlot::Ping => self.on_ping_watchdog_fired(),
            TimerSlot::Label => self.on_label_timer_fired(),
        }
    }

    // ---- Inbound bytes and dispatch -------------------------------------

    /// Feeds raw bytes to the framer and dispatches each complete frame
    /// (`handleBytesLocked`). Protocol faults are reported and never fail the
    /// session.
    pub(crate) fn handle_bytes(&mut self, bytes: &[u8]) {
        if self.ping == PingWatchdog::Watching {
            self.arm_ping_watchdog();
        }
        let result = self.framer.feed(bytes);
        for fault in result.faults {
            self.emit_protocol_error("frame", &fault.error, &fault.preview, fault.byte_count);
        }
        for frame in result.frames {
            match parse(&frame) {
                Ok(message) => self.handle_message(message),
                Err(error) => {
                    self.emit_protocol_error("message", &error, &frame, frame.len());
                }
            }
        }
    }

    pub(crate) fn emit_protocol_error(
        &mut self,
        kind: &str,
        error: &crate::irc::message::IrcError,
        preview: &[u8],
        byte_count: usize,
    ) {
        let message = super::malformed::describe_malformed(
            kind,
            error,
            preview,
            byte_count,
            &self.channel_types,
        );
        self.emit(SessionEvent::ErrorOccurred {
            network_id: self.network_id(),
            kind: ErrorKind::Protocol,
            message,
        });
    }

    /// Dispatches one inbound message (`handleMessageLocked`). The order is the
    /// Go order, which the tests pin.
    pub(crate) fn handle_message(&mut self, message: IrcMessage) {
        if let Some(filter) = &self.ignore_filter {
            if filter(&message, &self.config.network_id, &self.nickname) {
                return;
            }
        }

        let command = message.command.clone();
        if command == "INVITE" && message.params.len() >= 2 {
            let nick = prefix_nick(&message);
            let channel = wire::param(&message, message.params.len() - 1);
            if !nick.is_empty() && !channel.is_empty() {
                self.pending_invite = Some(PendingInvite { nick, channel });
            }
        }

        let request_label = self.correlation_label(&message);
        let inbound = SessionEvent::Inbound {
            network_id: self.network_id(),
            message: message.clone(),
            request_label,
            nick: self.nickname.clone(),
            channel_types: self.channel_types.clone(),
            at: self.now(),
        };
        self.emit(inbound);

        self.apply_isupport(&message);
        if command == "376" || command == "422" {
            self.features.mark_case_mapping_known();
        }

        if command == "BATCH" {
            self.handle_batch(&message);
            return;
        }
        if self.capture_in_batch(&message) {
            return;
        }

        if command == "ACK" {
            self.finish_request_label(&wire::tag_value(&message, "label"));
            return;
        }

        if command == "PING" {
            let Some(token) = message.params.last() else {
                let network_id = self.network_id();
                self.emit(SessionEvent::ErrorOccurred {
                    network_id,
                    kind: ErrorKind::Protocol,
                    message: "PING message has no token".to_owned(),
                });
                return;
            };
            let mut line = b"PONG :".to_vec();
            line.extend_from_slice(token);
            line.extend_from_slice(b"\r\n");
            self.send_line(&line);
            return;
        }

        if command == "PONG" && self.pong_matches_watchdog(&message) {
            self.arm_ping_watchdog();
            return;
        }

        if self.deliver_read_marker(&message) {
            return;
        }

        if command == "PRIVMSG"
            && message.params.len() >= 2
            && self.nicks_equal(&wire::param(&message, 0), &self.nickname.clone())
        {
            if self.answer_ctcp(&message) {
                return;
            }
        }

        match command.as_str() {
            "CAP" => {
                self.handle_cap(&message);
                return;
            }
            "AUTHENTICATE" => {
                self.handle_authenticate(&message);
                return;
            }
            "001" => {
                self.handle_welcome(&message);
                return;
            }
            "903" => {
                self.handle_sasl_success(&message);
                return;
            }
            "774" => {
                self.handle_metadata_sync_later(&message);
                return;
            }
            "904" | "905" => {
                self.fail(
                    ErrorKind::Authentication,
                    "SASL authentication failed",
                    false,
                );
                return;
            }
            "421"
                if wire::param_index(&message, "CAP").is_some()
                    && self.state == SessionState::CapLs =>
            {
                self.capability_negotiation_ended = true;
                self.cancel_timer(TimerSlot::Capability);
                self.send_registration();
                return;
            }
            "464" => {
                self.fail(
                    ErrorKind::Authentication,
                    "Server password or authentication was rejected",
                    false,
                );
                return;
            }
            _ => {}
        }

        if is_registration_refusal(&command) && self.state != SessionState::Registered {
            if command == "433" && self.try_registration_nick_fallback() {
                return;
            }
            let fallback = format!(
                "IRC registration was refused ({})",
                wire_text(command.as_bytes())
            );
            let text = self.server_sentence_or(&message, &fallback);
            self.fail(ErrorKind::Registration, &text, false);
            return;
        }

        if command == "ERROR" {
            self.emit_message_received(message.clone());
            let text = self.server_sentence_or(&message, "IRC server reported an error");
            if self.registration_handshake() {
                self.fail(ErrorKind::Registration, &text, false);
                return;
            }
            self.fail(ErrorKind::Network, &text, true);
            return;
        }

        if command == "NICK" {
            let old_nick = wire::claimed_nick(&message);
            let new_nick = wire::param(&message, 0);
            if !old_nick.is_empty()
                && !new_nick.is_empty()
                && self.nicks_equal(&old_nick, &self.nickname.clone())
            {
                self.nickname = new_nick;
            }
        }

        if command == "FAIL" {
            self.handle_chat_history_fail(&message);
            self.handle_read_marker_fail(&message);
        }
        if command == "366" && message.params.len() >= 2 {
            self.probe_channel_away(&wire::param(&message, 1));
        }

        self.emit_message_received(message.clone());

        let self_prefixed = self.self_prefixed(&message);
        match command.as_str() {
            "JOIN" if self_prefixed => {
                let channel = wire::param(&message, 0);
                if !channel.is_empty() {
                    self.bump_history_generation(&channel);
                    self.request_channel_history(&channel);
                    self.record_autojoin(&channel, true);
                    if let Some(invite) = self.pending_invite.clone() {
                        if self.fold_channel(&channel) == self.fold_channel(&invite.channel) {
                            self.pending_invite = None;
                        }
                    }
                }
            }
            "PART" if self_prefixed => {
                let channel = wire::param(&message, 0);
                if !channel.is_empty() {
                    self.bump_history_generation(&channel);
                    self.forget_channel_history(&channel);
                    self.record_autojoin(&channel, false);
                }
            }
            "KICK" if self.is_self(&wire::param(&message, 1)) => {
                let channel = wire::param(&message, 0);
                if !channel.is_empty() {
                    self.bump_history_generation(&channel);
                    self.forget_channel_history(&channel);
                    self.record_autojoin(&channel, false);
                }
            }
            "475" if message.params.len() >= 2 => {
                self.drop_stored_autojoin_key(&wire::param(&message, 1));
            }
            _ => {}
        }

        let carried_label = wire::tag_value(&message, "label");
        if !carried_label.is_empty() {
            self.finish_request_label(&carried_label);
        }
    }

    /// The label that correlates an inbound line with a request: its own
    /// `@label`, else the label of the batch it sits in.
    pub(crate) fn correlation_label(&self, message: &IrcMessage) -> String {
        let label = wire::tag_value(message, "label");
        if !label.is_empty() {
            return label;
        }
        let batch = wire::tag_value(message, "batch");
        if batch.is_empty() {
            return String::new();
        }
        self.batches
            .open
            .get(&batch)
            .map(|frame| frame.request_label.clone())
            .unwrap_or_default()
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

    pub(crate) fn registration_handshake(&self) -> bool {
        matches!(
            self.state,
            SessionState::Connecting
                | SessionState::CapLs
                | SessionState::CapReq
                | SessionState::Sasl
                | SessionState::Registering
        )
    }

    pub(crate) fn probe_channel_away(&mut self, channel: &str) {
        if channel.is_empty() || !self.caps.enabled().contains(IrcCapability::AwayNotify) {
            return;
        }
        self.send_command(&format!("WHO {channel}"), "");
    }

    /// The server's trailing sentence on a refusal or ERROR, when it is safe to
    /// show. Either redactor vetoes it, and the fallback stays.
    pub(crate) fn server_sentence_or(&self, message: &IrcMessage, fallback: &str) -> String {
        let Some(reason) = super::malformed::registration_sentence(message) else {
            return fallback.to_owned();
        };
        if !crate::irc::secretpolicy::allows_transcript(&reason, &self.channel_types) {
            return fallback.to_owned();
        }
        if crate::irc::secretpolicy::redact_preview_line(&reason, &self.channel_types).is_some() {
            return fallback.to_owned();
        }
        reason
    }

    // ---- Ping watchdog --------------------------------------------------

    pub(crate) fn arm_ping_watchdog(&mut self) {
        if self.state != SessionState::Registered {
            return;
        }
        self.ping = PingWatchdog::Watching;
        self.arm_timer(
            TimerSlot::Ping,
            Duration::from_millis(self.config.ping_timeout_ms),
        );
    }

    pub(crate) fn cancel_ping_watchdog(&mut self) {
        self.ping = PingWatchdog::Off;
        self.cancel_timer(TimerSlot::Ping);
    }

    pub(crate) fn on_ping_watchdog_fired(&mut self) {
        if self.state != SessionState::Registered {
            return;
        }
        match self.ping {
            PingWatchdog::Watching => {
                self.ping = PingWatchdog::Probing;
                self.send_line(format!("PING :{PING_WATCHDOG_TOKEN}\r\n").as_bytes());
                self.arm_timer(
                    TimerSlot::Ping,
                    Duration::from_millis(self.config.ping_timeout_ms),
                );
            }
            PingWatchdog::Probing => {
                self.fail(ErrorKind::Network, "Ping timeout", true);
            }
            PingWatchdog::Off => {}
        }
    }

    pub(crate) fn pong_matches_watchdog(&self, message: &IrcMessage) -> bool {
        if self.ping != PingWatchdog::Probing {
            return false;
        }
        message
            .params
            .last()
            .is_some_and(|token| token.as_slice() == PING_WATCHDOG_TOKEN.as_bytes())
    }
}

/// The numerics that refuse registration (`isRegistrationRefusalNumeric`).
fn is_registration_refusal(command: &str) -> bool {
    matches!(
        command,
        "432" | "433" | "436" | "437" | "451" | "462" | "465"
    )
}

/// A label may carry only tag-safe characters, at most 64 of them.
pub(crate) fn is_tag_safe_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 64
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
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
