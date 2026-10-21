//! Port of the protocol-error preview of `tui/internal/session/session.go` (Go),
//! `previewWire` and `describeMalformed` in `src/irc/ircsession.cpp` (C++), and
//! the registration-sentence reader that feeds server refusals to Status.
//!
//! A malformed frame is reported as a protocol error, never a failed session.
//! Its preview shows printable bytes and '?' for controls. The preview then goes
//! through `secretpolicy::redact_preview_line`, twice: once with the controls
//! removed (so a `PASS\x00hunter2` still masks) and once with them as spaces. A
//! line the redactor masks shows the masked form; one it does not mask shows the
//! preview as is, which is what the Go and C++ ports do.

use crate::irc::message::{IrcError, IrcMessage};
use crate::irc::secretpolicy::redact_preview_line;
use crate::irc::wiretext::wire_text;

/// The text of a protocol error: the kind, the error name, the byte count, and a
/// preview when one can be shown (`describeMalformed`).
pub(crate) fn describe_malformed(
    kind: &str,
    error: &IrcError,
    preview: &[u8],
    byte_count: usize,
    channel_types: &str,
) -> String {
    let text = format!("Malformed IRC {kind}: {} ({byte_count} bytes)", error.name());
    let shown = preview_wire(preview, byte_count, channel_types);
    if shown.is_empty() {
        return text;
    }
    format!("{text}. Preview: {shown}")
}

/// A bounded, redacted preview of a malformed frame (`previewWire`).
fn preview_wire(preview: &[u8], byte_count: usize, channel_types: &str) -> String {
    let mut display = Vec::with_capacity(preview.len());
    let mut spaced = Vec::with_capacity(preview.len());
    let mut stripped = Vec::with_capacity(preview.len());
    for &byte in preview {
        if byte == b'\t' || (0x20..0x7F).contains(&byte) {
            display.push(byte);
            spaced.push(byte);
            stripped.push(byte);
        } else {
            display.push(b'?');
            spaced.push(b' ');
        }
    }

    let mut result = wire_text(&display);
    let stripped_text = wire_text(&stripped);
    if let Some(safe) = redact_preview_line(&stripped_text, channel_types) {
        result = safe;
    } else if let Some(safe) = redact_preview_line(&wire_text(&spaced), channel_types) {
        result = safe;
    }
    if byte_count > preview.len() {
        result.push('\u{2026}');
    }
    result
}

/// The trailing sentence of a registration refusal or ERROR, trimmed. A refusal
/// carries `<client> <nick> :<reason>` (432, 433, 436, 437), `<client> :<reason>`
/// (451, 462, 465), or any trailing parameter (ERROR). `None` when the sentence
/// is missing or blank (`registrationSentence`).
pub(crate) fn registration_sentence(message: &IrcMessage) -> Option<String> {
    let count = message.params.len();
    let minimum = match message.command.as_str() {
        "432" | "433" | "436" | "437" => 3,
        "451" | "462" | "465" => 2,
        _ => 1,
    };
    if count < minimum {
        return None;
    }
    let reason = wire_text(&message.params[count - 1]);
    let reason = reason.trim();
    if reason.is_empty() {
        return None;
    }
    Some(reason.to_owned())
}
