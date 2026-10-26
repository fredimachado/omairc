//! The Status pane: a network's server log, not a conversation. Its rows follow
//! the Status log model, and the glyph rule follows
//! `client/src/ui/status_row.rs`. Toolkit-free: the UI draws the rows.
//! The lines come from `Controller::status_lines`, already classified and redacted by
//! `irc::statusentry`. This module only maps them to rows.

use chrono::{DateTime, Local, Utc};

use super::format::format_irc;
use super::presence::CONNECTED;
use super::transcript::{Row, RowKind};
use crate::controller::controller::Controller;
use crate::irc::statusentry::{LogSeverity, LogSource, StatusEntry};

/// What a Status row carries besides its text: the label, the source that
/// picks the glyph, and the severity that picks the colour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusMark {
    pub label: String,
    pub source: LogSource,
    pub severity: LogSeverity,
}

impl StatusMark {
    /// `>>` for a line the client sent, `--` for a local line, `<<` for the server.
    pub fn glyph(&self) -> &'static str {
        match self.source {
            LogSource::Client => ">>",
            LogSource::Local => "--",
            LogSource::Server => "<<",
        }
    }

    /// The label column: the glyph, a space, then the label.
    pub fn label_text(&self) -> String {
        format!("{} {}", self.glyph(), self.label)
    }
}

/// Which theme colour a Status label takes (`client/src/ui/status_row.rs` `labelColor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelTone {
    /// Alerts: the accent colour.
    Accent,
    /// Trace lines: muted.
    Muted,
    /// Everything else: the page and ink colours mixed at 62% ink.
    Mixed,
}

pub fn label_tone(severity: LogSeverity) -> LabelTone {
    match severity {
        LogSeverity::Alert => LabelTone::Accent,
        LogSeverity::Trace => LabelTone::Muted,
        LogSeverity::Info => LabelTone::Mixed,
    }
}

/// Whether a severity mutes the body text. Only trace lines do; the rest use ink
/// (`client/src/ui/status_row.rs` `bodyColor`).
pub fn body_is_muted(severity: LogSeverity) -> bool {
    severity == LogSeverity::Trace
}

/// The local `HH:MM:SS` a Status line shows (Qt's stamps `hh:mm:ss`).
pub fn log_stamp(time: DateTime<Utc>) -> String {
    time.with_timezone(&Local).format("%H:%M:%S").to_string()
}

/// The Status row for one line. `id` is the row's transcript id.
pub fn row_from_status_entry(id: u64, entry: &StatusEntry) -> Row {
    Row {
        id,
        kind: RowKind::Status,
        author: None,
        time: Some(log_stamp(entry.timestamp)),
        body: format_irc(&entry.text),
        continues: false,
        action: false,
        separator_text: None,
        event_text: None,
        status: Some(StatusMark {
            label: entry.label.clone(),
            source: entry.source,
            severity: entry.severity,
        }),
        msgid: String::new(),
        only_visible_to_you: false,
        mentioned: false,
        origin: crate::irc::world::Origin::Live,
    }
}

/// The Status rows for a network's lines, oldest first. The lines come from
/// `Controller::status_lines`, so an unknown network has none.
pub fn status_rows(entries: &[&StatusEntry]) -> Vec<Row> {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| row_from_status_entry(index as u64 + 1, entry))
        .collect()
}

/// The Status title, `{display name} Status`, such as `irc.example · fred Status`.
/// Qt names the pane from the network's display name (`statusTitleText`). That
/// name already carries the clash suffix, so the conversation duplicate-name
/// rule does not apply here. The window title does not add ` - Omairc`.
pub fn status_title(network_name: &str) -> String {
    if network_name.is_empty() {
        return "Status".to_string();
    }
    format!("{network_name} Status")
}

/// The line under a network's name, in the sidebar's network row and under the Status
/// title (Qt's `liveStatus` and `statusSubtitle`). It is the network's last error while one
/// is set, else its connection word (`Connecting`, `Connected`, `Reconnecting`
/// `Disconnecting` or `Offline`), as the controller reads them.
pub fn connection_line(controller: &Controller, network_id: &str) -> String {
    match controller.last_error_for(network_id) {
        Some(error) if !error.is_empty() => error.to_owned(),
        _ => controller.connection_status_for(network_id).to_owned(),
    }
}

/// Whether a network's sidebar mark is green. Qt paints it only when the line reads
/// `Connected`, so a connected network with an error shows the muted mark.
pub fn network_mark_online(line: &str) -> bool {
    line == CONNECTED
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::statusentry::outcome;
    use chrono::TimeZone;

    fn at(hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 19, hour, minute, second).unwrap()
    }

    #[test]
    fn the_glyph_marks_who_wrote_the_line() {
        let mark = |source| StatusMark {
            label: "PING".to_string(),
            source,
            severity: LogSeverity::Info,
        };
        assert_eq!(mark(LogSource::Client).glyph(), ">>");
        assert_eq!(mark(LogSource::Local).glyph(), "--");
        assert_eq!(mark(LogSource::Server).glyph(), "<<");
        assert_eq!(mark(LogSource::Server).label_text(), "<< PING");
    }

    #[test]
    fn severity_picks_the_label_tone_and_mutes_only_trace_bodies() {
        assert_eq!(label_tone(LogSeverity::Alert), LabelTone::Accent);
        assert_eq!(label_tone(LogSeverity::Trace), LabelTone::Muted);
        assert_eq!(label_tone(LogSeverity::Info), LabelTone::Mixed);
        assert!(body_is_muted(LogSeverity::Trace));
        assert!(!body_is_muted(LogSeverity::Alert));
    }

    #[test]
    fn a_status_line_becomes_a_status_row_with_its_stamp_and_label() {
        let entry = outcome("omarchy", "Command was refused", at(9, 59, 0));
        let rows = status_rows(&[&entry]);
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.kind, RowKind::Status);
        assert_eq!(row.id, 1);
        let mark = row.status.as_ref().expect("a status mark");
        assert_eq!(mark.label, "command");
        assert_eq!(mark.source, LogSource::Local);
        assert_eq!(row.time.as_deref().map(str::len), Some(8), "HH:MM:SS");
    }

    #[test]
    fn the_status_title_names_the_network_and_falls_back_when_it_has_no_name() {
        assert_eq!(status_title("irc.example · fred"), "irc.example · fred Status");
        assert_eq!(status_title(""), "Status");
    }

    #[test]
    fn an_empty_log_has_no_rows() {
        assert!(status_rows(&[]).is_empty());
    }
}
