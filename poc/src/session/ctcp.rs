//! Port of the CTCP paths of `tui/internal/session/session.go` (Go) and
//! `src/irc/ircsession.cpp` (C++): answering a CTCP query sent to this user, and
//! sending a CTCP query.
//!
//! Only a direct query is answered. A channel query gets nothing, and neither
//! does a query from a service identity or a server-prefixed source. Replies go
//! out as NOTICE, and at most one per nick every 5 seconds, so a flood of queries
//! cannot make the client a reflector.

use chrono::Local;

use super::session::{Core, CTCP_PING_PAYLOAD_MAX_BYTES, CTCP_REPLY_INTERVAL};
use super::wire;
use crate::irc::ctcp::{ctcp_payload, ctcp_version_reply_text, parse_ctcp_request, CtcpRequest};
use crate::irc::message::IrcMessage;
use crate::irc::prefixnick::prefix_nick;
use crate::irc::servicenick::is_service_identity;

impl Core {
    /// Answers a CTCP query sent to this user. Returns true when the message was
    /// a non-ACTION CTCP and has been consumed, even when no reply went out
    /// (`handleMessageLocked`, the CTCP block).
    pub(crate) fn answer_ctcp(&mut self, message: &IrcMessage) -> bool {
        let body = wire::param(message, 1);
        let Some(request) = parse_ctcp_request(&body) else {
            return false;
        };
        if request.command == "ACTION" {
            return false;
        }
        let sender = prefix_nick(message);
        if sender.is_empty()
            || is_service_identity(
                &sender,
                &wire::ctcp_reply_host(message),
                &self.channel_types,
            )
        {
            return true;
        }
        let payload = match request.command.as_str() {
            "PING" => {
                if request.argument.len() > CTCP_PING_PAYLOAD_MAX_BYTES {
                    return true;
                }
                ctcp_payload(&CtcpRequest {
                    command: "PING".to_owned(),
                    argument: request.argument.clone(),
                })
            }
            "TIME" => ctcp_payload(&CtcpRequest {
                command: "TIME".to_owned(),
                argument: self
                    .now()
                    .with_timezone(&Local)
                    .format("%a, %d %b %Y %H:%M:%S %z")
                    .to_string(),
            }),
            "VERSION" => ctcp_payload(&CtcpRequest {
                command: "VERSION".to_owned(),
                argument: ctcp_version_reply_text().to_owned(),
            }),
            _ => return true,
        };
        if !self.allow_ctcp_reply(&sender) {
            return true;
        }
        self.send_trailing_body(&format!("NOTICE {sender} :"), &payload, "", "");
        true
    }

    /// Whether a CTCP reply may go to `nick` now. Records the reply time when it
    /// may (`allowCtcpReplyLocked`).
    fn allow_ctcp_reply(&mut self, nick: &str) -> bool {
        let key = nick.to_lowercase();
        let now = self.now();
        if let Some(last) = self.ctcp_reply_clock.get(&key) {
            let interval =
                chrono::TimeDelta::from_std(CTCP_REPLY_INTERVAL).unwrap_or(chrono::TimeDelta::MAX);
            if now - *last < interval {
                return false;
            }
        }
        self.ctcp_reply_clock.insert(key, now);
        true
    }

    /// Sends a CTCP query, optionally labeled (`sendCtcpLocked`). The verb is
    /// upper-cased and may not contain a space.
    pub(crate) fn send_ctcp(
        &mut self,
        target: &str,
        command: &str,
        argument: &str,
        request_label: &str,
    ) -> bool {
        let verb = command.trim().to_uppercase();
        if !super::session::is_valid_privmsg_target(target) || verb.is_empty() || verb.contains(' ')
        {
            return false;
        }
        let payload = ctcp_payload(&CtcpRequest {
            command: verb,
            argument: argument.to_owned(),
        });
        self.send_trailing_body(&format!("PRIVMSG {target} :"), &payload, "", request_label)
    }
}
