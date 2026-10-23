//! Port of `tui/internal/irc/statusentry.go` (Go), `src/irc/ircstatusentry.{h,cpp}`
//! (C++, `IrcStatusEntry`).
//!
//! The entry types and the constructors every caller uses: `lifecycle`, `outcome` and
//! `ctcp_reply`. The classifiers sit beside this file: `incoming` (the server's lines),
//! `outgoing` (the line the client sent), `whois`, `notice` (notices and invites), and
//! `keep` (the keep table that decides which lines reach Status at all).
//!
//! Classification turns one wire line into the Status entries it produces: the
//! server's registration burst, the CAP and lusers numerics, the WHOIS and notice
//! shapes, CTCP replies, and the outgoing line the client sent. Which lines reach
//! Status at all is the keep table (`keeps_incoming`, `keeps_outgoing`).
//!
//! Secrets are masked here, at classification time. A stored entry text is the
//! redacted form, never the raw line, and every redaction call fails closed: a
//! line the policy cannot read is dropped or masked, not kept.
//!
//! Pure: `now` is passed in, and no entry reads the IRCv3 `time` tag. Qt stamps
//! each entry with the wall clock when it classifies the line; the Rust core
//! receives that clock reading from its caller.

mod incoming;
mod keep;
mod notice;
mod outgoing;
mod whois;

pub use incoming::incoming_all;
pub use keep::{keeps_incoming, keeps_outgoing};
pub use outgoing::outgoing;

use chrono::{DateTime, Utc};

/// The label a command-outcome line carries (`IrcStatusEntry::outcome`).
pub const OUTCOME_LABEL: &str = "command";
/// The label a CTCP reply or request line carries.
pub const CTCP_LABEL: &str = "CTCP";
/// The label a notice carries.
pub const NOTICE_LABEL: &str = "NOTICE";

/// Who produced a Status line (`LogSource`, `IrcLogSource`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogSource {
    /// A line the server sent.
    Server,
    /// A line this client sent.
    Client,
    /// A locally generated lifecycle or outcome line.
    Local,
}

/// How a Status line is coloured (`LogSeverity`, `IrcLogSeverity`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LogSeverity {
    /// PING, PONG and AUTHENTICATE chatter.
    Trace,
    /// The default.
    Info,
    /// Errors and standard-reply failures.
    Alert,
}

/// Where a WHOIS sub-line sits in its reply sequence (`WhoisProgress`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhoisProgress {
    /// A mid-sequence detail line.
    Detail,
    /// The 318 end-of-WHOIS line.
    Terminal,
    /// A 401 or 402 no-such-nick or no-such-server line.
    Failed,
}

/// The WHOIS detail a server Status entry carries (`WhoisLine`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WhoisLine {
    pub nick: String,
    pub text: String,
    pub progress: WhoisProgress,
}

impl WhoisLine {
    /// Whether the line ends the WHOIS exchange (`Terminal`).
    pub fn is_terminal(&self) -> bool {
        self.progress != WhoisProgress::Detail
    }
}

/// The CTCP reply metadata a server Status entry carries (`CtcpReplyLine`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtcpReplyLine {
    pub nick: String,
    /// The uppercased CTCP command.
    pub command: String,
}

/// One classified Status line (`StatusEntry`, `IrcStatusEntry`). The text is the
/// redacted form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusEntry {
    pub network_id: String,
    pub timestamp: DateTime<Utc>,
    pub source: LogSource,
    pub severity: LogSeverity,
    pub label: String,
    /// The correlation label of the request this line answers, or empty.
    pub request_label: String,
    pub text: String,
    pub whois_line: Option<WhoisLine>,
    pub ctcp_reply: Option<CtcpReplyLine>,
}

impl StatusEntry {
    fn new(
        network_id: &str,
        timestamp: DateTime<Utc>,
        source: LogSource,
        severity: LogSeverity,
        label: &str,
        text: String,
    ) -> Self {
        Self {
            network_id: network_id.to_owned(),
            timestamp,
            source,
            severity,
            label: label.to_owned(),
            request_label: String::new(),
            text,
            whois_line: None,
            ctcp_reply: None,
        }
    }

    /// Records the correlation label of the request this line answers
    /// (`SetRequestLabel`).
    pub fn set_request_label(&mut self, request_label: &str) {
        self.request_label = request_label.to_owned();
    }
}

/// A locally generated lifecycle line (`Lifecycle`).
pub fn lifecycle(
    network_id: &str,
    severity: LogSeverity,
    label: &str,
    text: &str,
    now: DateTime<Utc>,
) -> StatusEntry {
    StatusEntry::new(
        network_id,
        now,
        LogSource::Local,
        severity,
        label,
        text.to_owned(),
    )
}

/// A locally generated command-outcome line (`Outcome`).
pub fn outcome(network_id: &str, text: &str, now: DateTime<Utc>) -> StatusEntry {
    StatusEntry::new(
        network_id,
        now,
        LogSource::Local,
        LogSeverity::Info,
        OUTCOME_LABEL,
        text.to_owned(),
    )
}

/// A CTCP reply line written on Status, labelled `CTCP`, such as the `/version`
/// answer `VERSION reply from anna: https://omairc.app`. It reads as a server
/// line, as the reply branch of a NOTICE does.
#[cfg(test)]
pub fn ctcp_reply(network_id: &str, text: &str, now: DateTime<Utc>) -> StatusEntry {
    StatusEntry::new(
        network_id,
        now,
        LogSource::Server,
        LogSeverity::Info,
        CTCP_LABEL,
        text.to_owned(),
    )
}

#[cfg(test)]
mod tests;
