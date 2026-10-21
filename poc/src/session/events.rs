//! The session's outbound contract: what a session tells its owner, and the
//! small value types those events carry.
//!
//! Port of the `Handler` interface in `tui/internal/session/session.go` and the
//! `IrcSession` signals in `src/irc/ircsession.h`. Go delivers each callback
//! through a Handler; here the session queues `SessionEvent`s, and its owner
//! drains them with `Session::drain_events` once an entry point returns. That
//! keeps the session free of callbacks into the controller, so `session/` never
//! depends on `controller/` or `World` (the Phase 3 translator consumes these).
//!
//! Status and transcript text is not built here. Phase 3 classifies the raw
//! `Inbound` and `Outbound` lines into Status entries (`irc/statusentry`) and
//! the translator turns `MessageReceived` into transcript rows, exactly as the
//! Go `StatusEntry` and translator do.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::irc::capability::IrcCapabilitySet;
use crate::irc::message::IrcMessage;

/// The connection state machine (`IrcSession::State`, src/irc/ircsession.h). The
/// STS upgrade state of Go is absent: STS is Phase 11.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SessionState {
    #[default]
    Idle,
    Connecting,
    CapLs,
    CapReq,
    Sasl,
    Registering,
    Registered,
    Closing,
    Reconnecting,
    Failed,
}

impl SessionState {
    /// The C++ enumerator name, for logs and tests.
    pub fn name(self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Connecting => "Connecting",
            Self::CapLs => "CapLs",
            Self::CapReq => "CapReq",
            Self::Sasl => "Sasl",
            Self::Registering => "Registering",
            Self::Registered => "Registered",
            Self::Closing => "Closing",
            Self::Reconnecting => "Reconnecting",
            Self::Failed => "Failed",
        }
    }
}

/// Why a session failed (`IrcSession::ErrorKind`). The order is significant:
/// `reported_retry_errors` keeps one dedup bit per kind, in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorKind {
    Protocol,
    Authentication,
    Tls,
    Network,
    Registration,
}

impl ErrorKind {
    /// The bit this kind occupies in the retry-error dedup mask.
    pub(crate) fn bit(self) -> u32 {
        1 << (self as u32)
    }
}

/// The last unhandled INVITE (`IrcPendingInvite`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingInvite {
    pub nick: String,
    pub channel: String,
}

/// Which reply produced a replay batch (`IrcHistoryKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryKind {
    /// A `chathistory` / `draft/chathistory` reply.
    Chat,
    /// A `znc.in/playback` batch from a bouncer.
    BouncerPlayback,
    /// A `draft/chathistory-targets` answer: names, not lines.
    Targets,
}

/// One row of `CHATHISTORY TARGETS` (`IrcHistoryTarget`). `latest` is the
/// server's time of that target's newest message, when the reply gave one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryTarget {
    pub name: String,
    pub latest: Option<DateTime<Utc>>,
}

/// One completed replay batch (`IrcHistoryBatch`). The lines are still raw
/// protocol, so the translator applies the same rules it applies to live
/// traffic.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HistoryBatch {
    pub target: String,
    pub lines: Vec<IrcMessage>,
    pub kind: Option<HistoryKind>,
    /// True for a solicited `CHATHISTORY BEFORE` answer.
    pub older_page: bool,
    /// True when `draft/chathistory-end` was on the opening `BATCH`.
    pub history_ended: bool,
    /// True when the batch answers `CHATHISTORY AFTER`.
    pub after_request: bool,
    /// The rows of a `Targets` batch. Empty for every other kind.
    pub targets: Vec<HistoryTarget>,
}

/// Severity of a lifecycle note (`irc.LogSeverity`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Info,
    Alert,
}

/// One event from a session. The owner drains them in order.
#[derive(Clone, Debug, PartialEq)]
pub enum SessionEvent {
    /// The state machine moved.
    StateChanged {
        network_id: String,
        state: SessionState,
    },
    /// A failure, or a protocol error. `message` is presentable and redacted.
    ErrorOccurred {
        network_id: String,
        kind: ErrorKind,
        message: String,
    },
    /// Welcome (001) arrived and the session is registered.
    Registered { network_id: String },
    /// A reconnect was scheduled `delay_ms` from now; `attempt` counts from 1.
    ReconnectScheduled {
        network_id: String,
        delay_ms: u64,
        attempt: u32,
    },
    /// An inbound line, before any state change it causes. The Status recorder
    /// classifies it. `request_label` is the labeled-response correlation, or
    /// empty; `nick` and `channel_types` are the session's values at receipt.
    Inbound {
        network_id: String,
        message: IrcMessage,
        request_label: String,
        nick: String,
        channel_types: String,
        at: DateTime<Utc>,
    },
    /// An outbound line as written, CRLF included. The Status recorder masks
    /// secrets (`secretpolicy`) before anything is shown.
    Outbound {
        network_id: String,
        line: Vec<u8>,
        channel_types: String,
        at: DateTime<Utc>,
    },
    /// A note the session raises itself, for example that NickServ IDENTIFY goes
    /// out in clear text. `kind` names the note (`identify`).
    Lifecycle {
        network_id: String,
        severity: Severity,
        kind: &'static str,
        text: String,
        at: DateTime<Utc>,
    },
    /// An inbound message that survived the ignore filter, for the translator.
    MessageReceived {
        network_id: String,
        message: IrcMessage,
    },
    /// A completed history or playback batch.
    HistoryBatchReceived {
        network_id: String,
        batch: HistoryBatch,
    },
    /// A CHATHISTORY request ended without a batch. `failed` is true on FAIL;
    /// `before` is true when the request was `CHATHISTORY BEFORE`.
    ChatHistoryRequestFinished {
        network_id: String,
        target: String,
        failed: bool,
        before: bool,
    },
    /// `FAIL CHATHISTORY`. `subcommand` is `TARGETS` or `AFTER`; `target` is
    /// empty for TARGETS.
    ChatHistoryFailed {
        network_id: String,
        subcommand: &'static str,
        target: String,
    },
    /// The acknowledged capabilities changed.
    CapabilitiesChanged {
        network_id: String,
        capabilities: IrcCapabilitySet,
    },
    /// A labeled request ended, by response, failure or timeout.
    RequestLabelFinished { network_id: String, label: String },
    /// The autojoin list or its keys changed. The owner persists the new value.
    AutojoinChannelsChanged {
        network_id: String,
        channels: Vec<String>,
        keys: BTreeMap<String, String>,
    },
    /// A MARKREAD or READ line. `marker` is `None` for `*` or a bare target.
    ReadMarkerReceived {
        network_id: String,
        target: String,
        marker: Option<DateTime<Utc>>,
    },
}
