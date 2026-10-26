//! Port of the registration half of `tui/internal/session/session.go` (Go),
//! `tui/internal/session/session.go`: the PASS / NICK / USER burst, the nick
//! collision fallback, ISUPPORT (005) and welcome (001).
//!
//! Registration waits for CAP END (or a `421` for CAP), then sends PASS, NICK and
//! USER. A `433` before welcome retries with `_` and then `2`; past that the
//! session fails visibly. After welcome, the session auto-joins, identifies with
//! NickServ when a password is set, and arms the ping watchdog.

use super::events::{ErrorKind, SessionEvent, SessionState, Severity};
use super::session::{lifecycle_event, Core, RegistrationNick, HISTORY_LIMIT};
use crate::irc::commandbuilder;
use crate::irc::jointarget::IrcJoinTarget;
use crate::irc::message::IrcMessage;
use crate::irc::serverfeatures::IrcServerFeatures;

impl Core {
    /// Sends PASS (when set and not reserved for SASL), NICK and USER, once
    /// (`sendRegistrationLocked`).
    pub(crate) fn send_registration(&mut self) {
        if self.registration_sent {
            return;
        }
        let send_pass = !self.config.password.is_empty()
            && (!self.sasl.requested || !self.config.nickserv_password.is_empty());
        let pass = if send_pass {
            commandbuilder::pass(self.config.password.expose(), &self.features).ok()
        } else {
            None
        };
        let nick = commandbuilder::nick(&self.config.nick, &self.features);
        let user =
            commandbuilder::user(&self.config.username, &self.config.realname, &self.features);
        if (send_pass && pass.is_none()) || nick.is_err() || user.is_err() {
            self.fail(
                ErrorKind::Registration,
                "IRC registration fields contain invalid characters",
                false,
            );
            return;
        }
        if let Some(pass) = pass {
            self.send_line(pass.as_bytes());
        }
        if let Ok(nick) = nick {
            self.send_line(nick.as_bytes());
        }
        if let Ok(user) = user {
            self.send_line(user.as_bytes());
        }
        self.registration_sent = true;
        self.set_state(SessionState::Registering);
    }

    /// Retries a colliding nick with `_`, then `2` (`tryRegistrationNickFallbackLocked`).
    /// Returns false when no fallback is left.
    pub(crate) fn try_registration_nick_fallback(&mut self) -> bool {
        if !self.registration_sent {
            return false;
        }
        let (fallback, next) = match self.registration_nick {
            RegistrationNick::Configured => (
                format!("{}_", self.config.nick),
                RegistrationNick::Underscore,
            ),
            RegistrationNick::Underscore => {
                (format!("{}2", self.config.nick), RegistrationNick::Digit)
            }
            RegistrationNick::Digit => return false,
        };
        let Ok(line) = commandbuilder::nick(&fallback, &self.features) else {
            return false;
        };
        self.registration_nick = next;
        self.nickname = fallback;
        self.send_line(line.as_bytes());
        true
    }

    /// Applies an ISUPPORT (005) line: its tokens go to the features, and
    /// CHANTYPES and CHATHISTORY are read out (`applyISupportLocked`).
    pub(crate) fn apply_isupport(&mut self, message: &IrcMessage) {
        if message.command != "005" || message.params.len() <= 2 {
            return;
        }
        const CHANTYPES: &str = "CHANTYPES=";
        const CHATHISTORY: &str = "CHATHISTORY=";
        let last = message.params.len() - 1;
        for index in 1..last {
            let token = message.text_param(index);
            self.features.apply_token(&token);
            if let Some(value) = token.strip_prefix(CHANTYPES) {
                self.channel_types = value.to_owned();
            } else if let Some(value) = token.strip_prefix(CHATHISTORY) {
                if let Ok(maximum) = value.parse::<i64>() {
                    // The token is the server's per-command maximum. Zero means it
                    // imposes none.
                    self.history_limit = if maximum > 0 {
                        HISTORY_LIMIT.min(usize::try_from(maximum).unwrap_or(HISTORY_LIMIT))
                    } else {
                        HISTORY_LIMIT
                    };
                }
            }
        }
    }

    /// Handles 001, welcome (`handleWelcomeLocked`). It registers the session,
    /// identifies with NickServ when configured, and joins the autojoin list.
    pub(crate) fn handle_welcome(&mut self, message: &IrcMessage) {
        if self.state == SessionState::Registered || self.state == SessionState::Failed {
            return;
        }
        // 001 is not SASL success. Fail while SCRAM is still outstanding. A CAP
        // timeout that already sent CAP END without AUTHENTICATE has ended
        // negotiation, so that welcome registers.
        if self.sasl.mechanism == Some(super::caps::SASL_SCRAM)
            && !self.sasl.succeeded
            && (self.sasl.pending || self.state == SessionState::Sasl || self.sasl.exchange_started)
        {
            self.fail(
                ErrorKind::Authentication,
                "SASL authentication failed",
                false,
            );
            return;
        }

        let assigned = message.text_param(0);
        if !assigned.is_empty() {
            self.nickname = assigned;
        }

        self.connection.reset_retries();
        self.cancel_timer(super::timers::TimerSlot::Capability);
        self.caps.abandon_outstanding();
        self.set_state(SessionState::Registered);
        let network_id = self.network_id();
        self.emit(SessionEvent::Registered {
            network_id: network_id.clone(),
        });
        self.arm_ping_watchdog();
        self.subscribe_to_member_metadata();

        if !self.config.nickserv_password.is_empty()
            && !self.sasl.succeeded
            && self.sasl.mechanism != Some(super::caps::SASL_SCRAM)
        {
            if !self.tls_enabled {
                let at = self.now();
                self.emit(lifecycle_event(
                    &network_id,
                    Severity::Alert,
                    "identify",
                    "NickServ identify will be sent in clear text",
                    at,
                ));
            }
            let identify = format!("IDENTIFY {}", self.config.nickserv_password.expose());
            self.send_privmsg_locked("NickServ", &identify);
        }

        for channel in self.autojoin_channels.clone() {
            let key = self.autojoin_key_for(&channel);
            let target = IrcJoinTarget::make(&channel, key.as_deref(), &IrcServerFeatures::new());
            let joined = match target {
                Some(target) => self.join_locked(&target),
                None => false,
            };
            if !joined {
                self.emit(SessionEvent::ErrorOccurred {
                    network_id: network_id.clone(),
                    kind: ErrorKind::Protocol,
                    message: "Invalid autojoin channel".to_owned(),
                });
            }
        }
    }
}
