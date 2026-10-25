//! The line the client sent, as Status shows it: a CAP REQ reads as a request, a
//! PASS is masked, and either line ending is stripped first.

use super::incoming::{first_token, has_prefix_fold, severity_for, split_fields};
use super::{LogSource, StatusEntry};
use crate::irc::secretpolicy::redact_wire_line;
use crate::irc::wiretext::wire_text;
use chrono::{DateTime, Utc};

/// The Status text of an outgoing `CAP REQ` (`statusFormatOutgoingCapReq`).
pub(super) fn format_outgoing_cap_req(display: &str) -> Option<String> {
    let trimmed = display.trim();
    if !has_prefix_fold(trimmed, "CAP") {
        return None;
    }
    let rest = trimmed.get(3..)?.trim();
    if !has_prefix_fold(rest, "REQ") {
        return None;
    }
    let mut rest = rest.get(3..)?.trim();
    if let Some(stripped) = rest.strip_prefix(':') {
        rest = stripped;
    }
    let tokens = split_fields(rest);
    Some(if tokens.is_empty() {
        "Requesting:".to_owned()
    } else {
        format!("Requesting: {}", tokens.join(" | "))
    })
}

/// The wire line without its CRLF or LF.
pub(super) fn strip_line_end(line: &[u8]) -> &[u8] {
    if let Some(stripped) = line.strip_suffix(b"\r\n") {
        stripped
    } else if let Some(stripped) = line.strip_suffix(b"\n") {
        stripped
    } else {
        line
    }
}

/// Classifies one outgoing wire line (`IrcStatusEntry::outgoing`). The text is
/// redacted with `redact_wire_line`, so a PASS, a keyed JOIN or MODE, or a service
/// request never carries its secret into Status.
pub fn outgoing(
    network_id: &str,
    line: &[u8],
    channel_types: &str,
    now: DateTime<Utc>,
) -> StatusEntry {
    let display = wire_text(strip_line_end(line));
    let verb = first_token(&display);
    let text = if let Some(cap_req) = format_outgoing_cap_req(&display) {
        cap_req
    } else if let Some(safe) = redact_wire_line(&display, channel_types) {
        safe
    } else {
        display
    };
    StatusEntry::new(
        network_id,
        now,
        LogSource::Client,
        severity_for(&verb),
        &verb,
        text,
    )
}
