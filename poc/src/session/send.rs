//! The line writers (`sendLineLocked`, `sendCommandLocked` and `sendTrailingBodyLocked`
//! in Go). Every line the session writes goes through `send_line`, which queues the
//! outbound event and the frame together. The public writers in `outbound` and the
//! protocol replies in the other submodules build on these three.

use std::rc::Rc;

use super::events::{SessionEvent, SessionState};
use super::session::{Core, Effect};

impl Core {
    /// Queues one already-built line (`sendLineLocked`). It emits the outbound
    /// event, then writes the frame.
    pub(crate) fn send_line(&mut self, line: &[u8]) {
        if line.is_empty() {
            return;
        }
        let line: Rc<[u8]> = Rc::from(line);
        let event = SessionEvent::Outbound {
            network_id: self.network_id(),
            line: Rc::clone(&line),
            channel_types: self.channel_types.clone(),
            at: self.now(),
        };
        self.emit(event);
        self.effects.push(Effect::Write(line));
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
}

/// A label may carry only tag-safe characters, at most 64 of them.
pub(crate) fn is_tag_safe_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= 64
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}
