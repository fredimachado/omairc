//! Port of the CAP and SASL driver in `tui/internal/session/session.go` (Go),
//! `src/irc/ircsession.cpp` (C++) and `tui/internal/irc/capabilitynegotiation.go`.
//!
//! The session sends `CAP LS 302`, requests what `IrcCapabilityNegotiation`
//! wants, acts on `ACK` / `NAK` / `NEW` / `DEL`, runs SASL when it was granted
//! (PLAIN, or SCRAM-SHA-256 through `saslscram`), and ends negotiation with
//! `CAP END` once every request has settled. Late replies after a timeout are
//! ignored by the negotiation itself.
//!
//! A SCRAM-SHA-256 exchange that fails is never retried with PLAIN.

use std::time::Duration;

use super::events::{ErrorKind, SessionEvent, SessionState};
use super::saslscram::{encode_sasl_frames, ScramExchange, ScramReply};
use super::session::Core;
use super::sts::parse_sts_advertisement;
use super::timers::TimerSlot;
use super::wire;
use crate::irc::capability::{IrcCapability, IrcCapabilitySet};
use crate::irc::message::IrcMessage;

/// The SCRAM mechanism name. The negotiation chooses it when the server lists it, and
/// the session drives it through `saslscram`.
pub(crate) const SASL_SCRAM: &str = "SCRAM-SHA-256";

/// The SASL bookkeeping of one connection (the `saslX` fields of Go). `scram` holds the
/// exchange while a SCRAM-SHA-256 dialogue is open.
#[derive(Debug, Default)]
pub(crate) struct SaslState {
    pub(crate) requested: bool,
    pub(crate) pending: bool,
    pub(crate) succeeded: bool,
    pub(crate) mechanism: Option<&'static str>,
    pub(crate) exchange_started: bool,
    pub(crate) scram: Option<ScramExchange>,
}

impl SaslState {
    /// Whether SCRAM-SHA-256 is the mechanism and the server has not verified its signature
    /// yet. A 903 in that state is not success (`saslScramExchange` with `saslScramStep`
    /// short of verified in Go).
    fn scram_unverified(&self) -> bool {
        self.mechanism == Some(SASL_SCRAM)
            && !self.scram.as_ref().is_some_and(ScramExchange::is_verified)
    }
}

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
        if let Some(ls_index) = message.param_index("LS") {
            self.capability_list_seen = true;
            let tokens = wire::capability_tokens(message, ls_index);
            self.caps.advertise(&tokens);
            if let Some(advertisement) = parse_sts_advertisement(&tokens) {
                self.pending_sts = Some(advertisement);
            }
            if let Some(metadata) = super::metadata::parse_metadata_capability(&tokens) {
                self.metadata = metadata;
            }
            let continuation =
                message.params.len() > ls_index + 1 && message.text_param(ls_index + 1) == "*";
            if continuation {
                return;
            }
            // The STS advertisement, if any, is acted on once the list is complete. An upgrade
            // ends this connection, so nothing else is requested here.
            self.handle_sts_advertisement(self.pending_sts.clone());
            if self.state == SessionState::StsUpgrading {
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

        if let Some(new_index) = message.param_index("NEW") {
            let tokens = wire::capability_tokens(message, new_index);
            self.caps.advertise(&tokens);
            if let Some(advertisement) = parse_sts_advertisement(&tokens) {
                self.handle_sts_advertisement(Some(advertisement));
            }
            if let Some(metadata) = super::metadata::parse_metadata_capability(&tokens) {
                self.metadata = metadata;
            }
            if self.state == SessionState::StsUpgrading {
                return;
            }
            self.request_capabilities();
            return;
        }

        if let Some(del_index) = message.param_index("DEL") {
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

        if let Some(ack_index) = message.param_index("ACK") {
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
                    self.sasl.scram = Some(ScramExchange::default());
                    self.send_line(b"AUTHENTICATE SCRAM-SHA-256\r\n");
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

        if let Some(nak_index) = message.param_index("NAK") {
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

    /// Handles an `AUTHENTICATE` challenge. PLAIN and SCRAM-SHA-256 are answered. For PLAIN,
    /// the `+` after `AUTHENTICATE PLAIN` asks for the credential (`handleAuthenticateLocked`).
    pub(crate) fn handle_authenticate(&mut self, message: &IrcMessage) {
        if self.state != SessionState::Sasl || message.params.is_empty() {
            return;
        }
        let front = message.text_param(0);
        if front == "*" {
            self.fail(
                ErrorKind::Authentication,
                "Server aborted SASL authentication",
                false,
            );
            return;
        }
        if self.sasl.mechanism == Some(SASL_SCRAM) {
            self.handle_scram_authenticate(front.as_bytes());
            return;
        }
        if front != "+" {
            return;
        }
        let account = self.config.sasl_account_name().to_owned();
        let secret = self.config.sasl_secret().expose();
        let mut plain = Vec::with_capacity(account.len() * 2 + secret.len() + 2);
        plain.extend_from_slice(account.as_bytes());
        plain.push(0);
        plain.extend_from_slice(account.as_bytes());
        plain.push(0);
        plain.extend_from_slice(secret.as_bytes());
        self.send_sasl_response(&plain);
    }

    /// Drives the SCRAM dialogue one AUTHENTICATE payload at a time
    /// (`handleScramAuthenticateLocked`). A payload with no open exchange is ignored.
    fn handle_scram_authenticate(&mut self, payload: &[u8]) {
        let Some(exchange) = self.sasl.scram.as_mut() else {
            return;
        };
        let reply = exchange.take_payload(
            payload,
            self.config.sasl_account_name(),
            self.config.sasl_secret(),
        );
        match reply {
            ScramReply::Send(raw) => self.send_sasl_response(&raw),
            ScramReply::Continue => {}
            ScramReply::Fail => self.fail(
                ErrorKind::Authentication,
                "SASL authentication failed",
                false,
            ),
        }
    }

    /// Handles `903`, SASL success (`handleCapLocked` / the 903 branch of
    /// `handleMessageLocked`). A 903 before the exchange started is ignored, and a 903
    /// before a SCRAM exchange verified its server signature fails authentication.
    pub(crate) fn handle_sasl_success(&mut self, _message: &IrcMessage) {
        if self.state != SessionState::Sasl {
            return;
        }
        if self.sasl.pending {
            if self.sasl.scram_unverified() {
                self.fail(
                    ErrorKind::Authentication,
                    "SASL authentication failed",
                    false,
                );
                return;
            }
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
