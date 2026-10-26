//! Inbound dispatch (`handleBytesLocked` and `handleMessageLocked` in Go). `handle_bytes`
//! feeds the framer and reports protocol faults, then `handle_message` runs each parsed
//! message in the Go order the tests pin: the ignore filter, the INVITE and echo
//! bookkeeping, the batch and label routing, and the per-command handlers in the other
//! submodules. The registration refusals and the server-text preview live here too.

use crate::irc::capability::IrcCapability;
use crate::irc::message::IrcMessage;
use crate::irc::parser::parse;
use crate::irc::prefixnick::prefix_nick;
use crate::irc::wiretext::wire_text;

use super::events::{ErrorKind, PendingInvite, SessionEvent, SessionState};
use super::session::Core;
use super::timers::TimerSlot;
use super::wire;

impl Core {
    /// Feeds raw bytes to the framer and dispatches each complete frame
    /// (`handleBytesLocked`). Protocol faults are reported and never fail the
    /// session.
    pub(crate) fn handle_bytes(&mut self, bytes: &[u8]) {
        if self.ping.is_watching() {
            self.arm_ping_watchdog();
        }
        let result = self.framer.feed(bytes);
        for fault in result.faults {
            self.emit_protocol_error("frame", &fault.error, &fault.preview, fault.byte_count);
        }
        for frame in result.frames {
            if self.state == SessionState::StsUpgrading {
                break;
            }
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
            let channel = message.text_param(message.params.len() - 1);
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
            self.finish_request_label(&message.tag_text("label").unwrap_or_default());
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

        if command == "PONG" && self.ping.answers_probe(&message) {
            self.arm_ping_watchdog();
            return;
        }

        if self.deliver_read_marker(&message) {
            return;
        }

        if command == "PRIVMSG"
            && message.params.len() >= 2
            && self.nicks_equal(&message.text_param(0), &self.nickname.clone())
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
            "421" if message.param_index("CAP").is_some() && self.state == SessionState::CapLs => {
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
            let new_nick = message.text_param(0);
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
            self.probe_channel_away(&message.text_param(1));
        }

        self.emit_message_received(message.clone());

        let self_prefixed = self.self_prefixed(&message);
        match command.as_str() {
            "JOIN" if self_prefixed => {
                let channel = message.text_param(0);
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
                let channel = message.text_param(0);
                if !channel.is_empty() {
                    self.bump_history_generation(&channel);
                    self.forget_channel_history(&channel);
                    self.record_autojoin(&channel, false);
                }
            }
            "KICK" if self.is_self(&message.text_param(1)) => {
                let channel = message.text_param(0);
                if !channel.is_empty() {
                    self.bump_history_generation(&channel);
                    self.forget_channel_history(&channel);
                    self.record_autojoin(&channel, false);
                }
            }
            "475" if message.params.len() >= 2 => {
                self.drop_stored_autojoin_key(&message.text_param(1));
            }
            _ => {}
        }

        let carried_label = message.tag_text("label").unwrap_or_default();
        if !carried_label.is_empty() {
            self.finish_request_label(&carried_label);
        }
    }

    /// The label that correlates an inbound line with a request: its own
    /// `@label`, else the label of the batch it sits in.
    pub(crate) fn correlation_label(&self, message: &IrcMessage) -> String {
        let label = message.tag_text("label").unwrap_or_default();
        if !label.is_empty() {
            return label;
        }
        let batch = message.tag_text("batch").unwrap_or_default();
        if batch.is_empty() {
            return String::new();
        }
        self.batches
            .open
            .get(&batch)
            .map(|frame| frame.request_label.clone())
            .unwrap_or_default()
    }

    pub(crate) fn registration_handshake(&self) -> bool {
        matches!(
            self.state,
            SessionState::Connecting
                | SessionState::StsUpgrading
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
}

/// The numerics that refuse registration (`isRegistrationRefusalNumeric`).
fn is_registration_refusal(command: &str) -> bool {
    matches!(
        command,
        "432" | "433" | "436" | "437" | "451" | "462" | "465"
    )
}
