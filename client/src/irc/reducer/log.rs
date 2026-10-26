//! The transcript log seam: the reducer persists and reads back transcript lines
//! through this trait, and the file-backed store lands in a later phase.
//!
//! Port of `tui/internal/irc/conversationlog.go`
//! in `tui/internal/storage/conversationlog.go`. The methods take `&self` so the
//! reducer can read the log while it holds a conversation. An implementation
//! that writes keeps its own handle behind interior mutability.

use chrono::{DateTime, Utc};

use crate::irc::casemapping::IrcCaseMapping;

/// One persisted transcript entry (`TranscriptLine`). `kind` is the stored token:
/// `message`, `notice`, `action`, `event` or `whois`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TranscriptLine {
    pub timestamp: DateTime<Utc>,
    pub author: String,
    pub kind: String,
    pub body: String,
    pub msgid: String,
    pub only_visible_to_you: bool,
}

/// Persists and reads back conversation transcripts, one stream per network and
/// case-mapped target.
pub trait ConversationLog: Send {
    /// Stores one line at the end, reporting whether it was written.
    fn append(
        &self,
        network_id: &str,
        target: &str,
        mapping: IrcCaseMapping,
        line: &TranscriptLine,
    ) -> bool;

    /// Stores lines at the front in the order given, oldest first. Reports
    /// whether every line was written.
    fn prepend(
        &self,
        network_id: &str,
        target: &str,
        mapping: IrcCaseMapping,
        lines: &[TranscriptLine],
    ) -> bool;

    /// Returns up to `max_lines` of the most recent lines, oldest first.
    fn read_tail(
        &self,
        network_id: &str,
        target: &str,
        mapping: IrcCaseMapping,
        max_lines: usize,
    ) -> Vec<TranscriptLine>;
}
