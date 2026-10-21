//! Port of the CAP and SASL driver in `tui/internal/session/session.go` (Go),
//! `src/irc/ircsession.cpp` (C++) and `tui/internal/irc/capabilitynegotiation.go`.
//!
//! The session sends `CAP LS 302`, requests what `IrcCapabilityNegotiation`
//! wants, acts on `ACK` / `NAK` / `NEW` / `DEL`, runs SASL PLAIN when it was
//! granted, and ends negotiation with `CAP END` once every request has settled.
//! Late replies after a timeout are ignored by the negotiation itself.
//!
//! SCRAM-SHA-256 is Phase 11. When the server would pick it, the session fails
//! the authentication instead of falling back to PLAIN.

use std::time::Duration;

use super::events::{ErrorKind, SessionEvent, SessionState};
use super::session::{Core, TimerSlot};
use super::wire;
use crate::irc::capability::{IrcCapability, IrcCapabilitySet};
use crate::irc::message::IrcMessage;

/// The SCRAM mechanism name. The session names it only to refuse it.
pub(crate) const SASL_SCRAM: &str = "SCRAM-SHA-256";

/// The longest AUTHENTICATE payload per frame (SASL 1.0 section 4.3).
const SASL_AUTHENTICATE_CHUNK: usize = 400;

impl Core {
    /// Starts capability negotiation after the socket opens (`beginCapabilityNegotiationLocked`).
    pub(crate) fn begin_capability_negotiation(&mut self) {
        self.set_state(SessionState::CapLs);
        self.send_line(b"CAP LS 302\r\n");
    }

    /// Requests every capability that can be requested now, then registers
    /// (`requestCapabilitiesLocked`).
    pub(crate) fn request_capabilities(&mut self) {
        let request = self.caps.take_request();
        if request.requests_sasl {
            self.sasl.requested = true;
            self.sasl.pending = true;
            // Committed when AUTHENTICATE is sent. A later CAP DEL or NEW must not
            // retarget an exchange already in progress.
            if !self.sasl.exchange_started {
                self.sasl.mechanism = request.sasl_mechanism;
            }
        }

        if !request.lines.is_empty() {
            if !self.registration_sent {
                self.set_state(SessionState::CapReq);
            }
            for line in &request.lines {
                self.send_line(format!("CAP REQ :{line}\r\n").as_bytes());
            }
            self.arm_timer(
                TimerSlot::Capability,
                Duration::from_millis(self.config.capability_timeout_ms),
            );
        }

        self.send_registration();
        self.end_capability_negotiation();
    }

    /// The CAP timeout fired before every answer came back
    /// (`onCapabilityTimeoutLocked`).
    pub(crate) fn on_capability_timeout(&mut self) {
        let abandoned = self.caps.abandon_outstanding();
        if abandoned.contains(IrcCapability::Sasl) {
            self.sasl.pending = false;
        }
        self.publish_capabilities();
        self.end_capability_negotiation();
    }

    /// Sends `CAP END` once registration was sent and every request has settled
    /// (`endCapabilityNegotiationLocked`).
    pub(crate) fn end_capability_negotiation(&mut self) {
        if self.capability_negotiation_ended || self.state == SessionState::Registered {
            return;
        }
        if !self.registration_sent || !self.caps.settled() || self.sasl.pending {
            return;
        }
        self.capability_negotiation_ended = true;
        self.cancel_timer(TimerSlot::Capability);
        self.send_line(b"CAP END\r\n");
    }

    /// Emits `CapabilitiesChanged` when the acknowledged set moved
    /// (`publishCapabilitiesLocked`).
    pub(crate) fn publish_capabilities(&mut self) {
        let enabled = self.caps.enabled();
        if enabled == self.published_caps {
            return;
        }
        self.published_caps = enabled;
        let network_id = self.network_id();
        self.emit(SessionEvent::CapabilitiesChanged {
            network_id,
            capabilities: enabled,
        });
    }

    /// Handles an inbound `CAP` line (`handleCapLocked`).
    pub(crate) fn handle_cap(&mut self, message: &IrcMessage) {
        if let Some(ls_index) = wire::param_index(message, "LS") {
            self.capability_list_seen = true;
            let tokens = wire::capability_tokens(message, ls_index);
            self.caps.advertise(&tokens);
            if let Some(metadata) = super::metadata::parse_metadata_capability(&tokens) {
                self.metadata = metadata;
            }
            let continuation =
                message.params.len() > ls_index + 1 && wire::param(message, ls_index + 1) == "*";
            if continuation {
                return;
            }
            self.request_capabilities();
            return;
        }

        // Every other subcommand answers a negotiation we started. Acting on one
        // before CAP LS would register the connection with nothing agreed.
        if !self.capability_list_seen {
            return;
        }

        if let Some(new_index) = wire::param_index(message, "NEW") {
            let tokens = wire::capability_tokens(message, new_index);
            self.caps.advertise(&tokens);
            if let Some(metadata) = super::metadata::parse_metadata_capability(&tokens) {
                self.metadata = metadata;
            }
            self.request_capabilities();
            return;
        }

        if let Some(del_index) = wire::param_index(message, "DEL") {
            let tokens = wire::capability_tokens(message, del_index);
            self.caps.withdraw(&tokens);
            if let Some(token) = self.read_marker_token {
                if tokens.iter().any(|name| name == token) {
                    self.read_marker_token = None;
                }
            }
            self.publish_capabilities();
            if !self.caps.enabled().contains(IrcCapability::MemberMetadata) {
                self.metadata = super::metadata::MetadataCapability::default();
            }
            if !self.replay_enabled(super::events::HistoryKind::Chat) {
                self.abandon_history_requests();
            }
            self.request_capabilities();
            return;
        }

        if let Some(ack_index) = wire::param_index(message, "ACK") {
            let tokens = wire::capability_tokens(message, ack_index);
            let granted = self.caps.acknowledge(&tokens);
            self.note_read_marker_ack(&tokens, granted);
            if let Some(metadata) = super::metadata::parse_metadata_capability(&tokens) {
                // A bare ACK keeps the limits learned from CAP LS / NEW.
                if metadata.max_subs.is_some() || metadata.max_value_bytes.is_some() {
                    self.metadata = metadata;
                }
            }
            self.publish_capabilities();
            if granted.contains(IrcCapability::Sasl) && !self.sasl.exchange_started {
                self.sasl.exchange_started = true;
                self.set_state(SessionState::Sasl);
                if self.sasl.mechanism == Some(SASL_SCRAM) {
                    self.fail(
                        ErrorKind::Authentication,
                        "SASL SCRAM-SHA-256 is not built yet",
                        false,
                    );
                    return;
                }
                self.send_line(b"AUTHENTICATE PLAIN\r\n");
                return;
            }
            if self.state == SessionState::Registered {
                self.subscribe_to_member_metadata();
            }
            self.end_capability_negotiation();
            return;
        }

        if let Some(nak_index) = wire::param_index(message, "NAK") {
            let tokens = wire::capability_tokens(message, nak_index);
            let refused = self.caps.reject(&tokens);
            if refused.contains(IrcCapability::Sasl) {
                self.sasl.pending = false;
                self.fail(
                    ErrorKind::Authentication,
                    "Server rejected the SASL capability",
                    false,
                );
                return;
            }
            self.request_capabilities();
        }
    }

    /// Records which read-marker spelling the server acknowledged. The
    /// negotiation tracks the capability, so the spelling is read off the ACK.
    fn note_read_marker_ack(&mut self, tokens: &[String], granted: IrcCapabilitySet) {
        if !granted.contains(IrcCapability::ReadMarker) || self.read_marker_token.is_some() {
            return;
        }
        for token in tokens {
            match token.as_str() {
                "draft/read-marker" => {
                    self.read_marker_token = Some("draft/read-marker");
                    return;
                }
                "soju.im/read" => {
                    self.read_marker_token = Some("soju.im/read");
                    return;
                }
                _ => {}
            }
        }
    }

    /// Handles an `AUTHENTICATE` challenge. Only PLAIN is answered; the `+` after
    /// `AUTHENTICATE PLAIN` asks for the credential (`handleAuthenticateLocked`).
    pub(crate) fn handle_authenticate(&mut self, message: &IrcMessage) {
        if self.state != SessionState::Sasl || message.params.is_empty() {
            return;
        }
        let front = wire::param(message, 0);
        if front == "*" {
            self.fail(
                ErrorKind::Authentication,
                "Server aborted SASL authentication",
                false,
            );
            return;
        }
        if self.sasl.mechanism == Some(SASL_SCRAM) {
            self.fail(
                ErrorKind::Authentication,
                "SASL SCRAM-SHA-256 is not built yet",
                false,
            );
            return;
        }
        if front != "+" {
            return;
        }
        let account = self.config.sasl_account_name().to_owned();
        let secret = self.config.sasl_secret().to_owned();
        let mut plain = Vec::with_capacity(account.len() * 2 + secret.len() + 2);
        plain.extend_from_slice(account.as_bytes());
        plain.push(0);
        plain.extend_from_slice(account.as_bytes());
        plain.push(0);
        plain.extend_from_slice(secret.as_bytes());
        self.send_sasl_response(&plain);
    }

    /// Handles `903`, SASL success (`handleCapLocked` / the 903 branch of
    /// `handleMessageLocked`). A 903 before the exchange started is ignored.
    pub(crate) fn handle_sasl_success(&mut self, _message: &IrcMessage) {
        if self.state != SessionState::Sasl {
            return;
        }
        if self.sasl.pending {
            self.sasl.pending = false;
            self.sasl.succeeded = true;
            self.end_capability_negotiation();
            self.set_state(SessionState::Registering);
        }
    }

    /// Writes a SASL response, base64-encoded and split into 400-byte frames
    /// (`sendSaslResponseLocked`).
    fn send_sasl_response(&mut self, raw: &[u8]) {
        for frame in encode_sasl_frames(raw) {
            self.send_line(&frame);
        }
    }
}

/// The AUTHENTICATE frames for `raw`: base64, 400 bytes a frame. A payload whose
/// encoding is an exact multiple of 400 ends with a bare `+`, so the server knows
/// the message is complete (`encodeSASLFrames`).
pub(crate) fn encode_sasl_frames(raw: &[u8]) -> Vec<Vec<u8>> {
    let encoded = base64_encode(raw);
    if encoded.is_empty() {
        return vec![b"AUTHENTICATE +\r\n".to_vec()];
    }
    let mut frames = Vec::new();
    for chunk in encoded.as_bytes().chunks(SASL_AUTHENTICATE_CHUNK) {
        let mut frame = b"AUTHENTICATE ".to_vec();
        frame.extend_from_slice(chunk);
        frame.extend_from_slice(b"\r\n");
        frames.push(frame);
    }
    if encoded.len() % SASL_AUTHENTICATE_CHUNK == 0 {
        frames.push(b"AUTHENTICATE +\r\n".to_vec());
    }
    frames
}

/// Standard base64 with padding. Written out here so the session stays free of a
/// codec dependency.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = u32::from(chunk[0]);
        let b1 = u32::from(chunk.get(1).copied().unwrap_or(0));
        let b2 = u32::from(chunk.get(2).copied().unwrap_or(0));
        let word = (b0 << 16) | (b1 << 8) | b2;
        out.push(char::from(ALPHABET[(word >> 18) as usize & 63]));
        out.push(char::from(ALPHABET[(word >> 12) as usize & 63]));
        out.push(if chunk.len() > 1 {
            char::from(ALPHABET[(word >> 6) as usize & 63])
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            char::from(ALPHABET[word as usize & 63])
        } else {
            '='
        });
    }
    out
}
