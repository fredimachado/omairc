//! The public outbound writers of `tui/internal/session/session.go` (Go) and
//! `IrcSession::send*` in `src/irc/ircsession.cpp` (C++).
//!
//! Every method is a thin `locked` wrapper over a `Core` method, so the
//! frame-level tests can drive each one against a `LoopbackTransport`. The
//! controller calls these; no UI code writes a frame directly (rule 1 of the
//! master plan). A method returns false when nothing was sent, and it leaves no
//! pending state behind when it refuses.

use chrono::{DateTime, Utc};

use super::events::SessionState;
use super::session::{is_valid_privmsg_target, Session};
use super::typing::TypingPhase;
use crate::irc::capability::IrcCapability;
use crate::irc::channelmode::IrcChannelModeRequest;
use crate::irc::commandbuilder;
use crate::irc::jointarget::IrcJoinTarget;
use crate::irc::message::MAX_CLASSIC_FRAME_BYTES;

impl Session {
    /// Writes a PRIVMSG, split across frames when the body is long
    /// (`IrcSession::sendPrivmsg`). A target must be valid and the body non-empty.
    pub fn send_privmsg(&self, target: &str, body: &str) -> bool {
        self.locked(|core| core.send_privmsg_locked(target, body))
    }

    /// Writes a NOTICE (`IrcSession::sendNotice`).
    pub fn send_notice(&self, target: &str, body: &str) -> bool {
        self.locked(|core| {
            if target.is_empty() || body.is_empty() {
                return false;
            }
            core.send_trailing_body(&format!("NOTICE {target} :"), body, "", "")
        })
    }

    /// Writes a CTCP ACTION (`IrcSession::sendAction`).
    pub fn send_action(&self, target: &str, body: &str) -> bool {
        self.locked(|core| {
            if !is_valid_privmsg_target(target) || body.is_empty() {
                return false;
            }
            let sent = core.send_trailing_body(
                &format!("PRIVMSG {target} :\u{1}ACTION "),
                body,
                "\u{1}",
                "",
            );
            if sent {
                core.note_message_sent(target);
            }
            sent
        })
    }

    /// Writes a CTCP query, optionally under a request label
    /// (`IrcSession::sendCtcp`).
    pub fn send_ctcp(
        &self,
        target: &str,
        command: &str,
        argument: &str,
        request_label: &str,
    ) -> bool {
        self.locked(|core| core.send_ctcp(target, command, argument, request_label))
    }

    /// Writes a `+typing` TAGMSG when `message-tags` is on and the pace allows it
    /// (`IrcSession::sendTyping`).
    pub fn send_typing(&self, target: &str, phase: TypingPhase) -> bool {
        self.locked(|core| core.send_typing(target, phase))
    }

    /// Writes a MODE query or change (`IrcSession::sendChannelMode`).
    pub fn send_channel_mode(&self, request: &IrcChannelModeRequest) -> bool {
        self.locked(|core| {
            if request.query {
                return core.send_command(&format!("MODE {}", request.channel), "");
            }
            let mut parts = vec![
                "MODE".to_owned(),
                request.channel.clone(),
                request.modes.clone(),
            ];
            parts.extend(request.parameters.iter().cloned());
            core.send_command(&parts.join(" "), "")
        })
    }

    /// Writes a JOIN, and remembers the key for autojoin once the join is
    /// confirmed (`IrcSession::join`).
    pub fn join(&self, target: &IrcJoinTarget) -> bool {
        self.locked(|core| core.join_locked(target))
    }

    /// Writes a PART (`IrcSession::part`).
    pub fn part(&self, channel: &str) -> bool {
        self.locked(|core| !channel.is_empty() && core.send_command(&format!("PART {channel}"), ""))
    }

    /// Writes a KICK with an optional reason (`IrcSession::kick`).
    pub fn kick(&self, channel: &str, nick: &str, reason: &str) -> bool {
        self.locked(|core| {
            if channel.is_empty() || nick.is_empty() {
                return false;
            }
            let command = if reason.is_empty() {
                format!("KICK {channel} {nick}")
            } else {
                format!("KICK {channel} {nick} :{reason}")
            };
            core.send_command(&command, "")
        })
    }

    /// Writes an INVITE (`IrcSession::invite`).
    pub fn invite(&self, nick: &str, channel: &str) -> bool {
        self.locked(|core| {
            !nick.is_empty()
                && !channel.is_empty()
                && core.send_command(&format!("INVITE {nick} {channel}"), "")
        })
    }

    /// Writes a TOPIC (`IrcSession::setTopic`).
    pub fn set_topic(&self, channel: &str, topic: &str) -> bool {
        self.locked(|core| {
            !channel.is_empty()
                && !topic.is_empty()
                && core.send_command(&format!("TOPIC {channel} :{topic}"), "")
        })
    }

    /// Writes AWAY with a reason, or clears away with an empty one
    /// (`IrcSession::setAway`).
    pub fn set_away(&self, reason: &str) -> bool {
        self.locked(|core| core.set_away_locked(reason))
    }

    /// Writes `AWAY :` with no reason, which marks the user away with the server's
    /// default text. It differs from `set_away("")`, which clears
    /// (`IrcSession::markAway`).
    pub fn mark_away(&self, reason: &str) -> bool {
        self.locked(|core| {
            let trimmed = reason.trim();
            if trimmed.is_empty() {
                return core.send_command("AWAY :", "");
            }
            core.send_command(&format!("AWAY :{trimmed}"), "")
        })
    }

    /// Clears away (`IrcSession::clearAway`).
    pub fn clear_away(&self) -> bool {
        self.locked(|core| core.set_away_locked(""))
    }

    /// Sets one of this user's metadata values. Returns the value placed on the
    /// wire, `Some("")` for a clear, or `None` when nothing was sent
    /// (`IrcSession::setOwnMetadata`).
    pub fn set_own_metadata(&self, key: &str, value: &str) -> Option<String> {
        self.locked(|core| core.set_own_metadata(key, value))
    }

    /// Clears one of this user's metadata values (`IrcSession::clearOwnMetadata`).
    pub fn clear_own_metadata(&self, key: &str) -> bool {
        self.locked(|core| core.set_own_metadata(key, "").is_some())
    }

    /// Writes NICK (`IrcSession::changeNick`).
    pub fn change_nick(&self, nick: &str) -> bool {
        self.locked(|core| !nick.is_empty() && core.send_command(&format!("NICK {nick}"), ""))
    }

    /// Writes `WHOIS <nick> <nick>`, optionally under a request label
    /// (`IrcSession::whois`).
    pub fn whois(&self, nick: &str, request_label: &str) -> bool {
        self.locked(|core| {
            let trimmed = nick.trim();
            if trimmed.is_empty() {
                return false;
            }
            core.send_command(&format!("WHOIS {trimmed} {trimmed}"), request_label)
        })
    }

    /// Begins a labeled request and returns its label, or `""` when
    /// `labeled-response` is not on (`IrcSession::startLabeledRequest`).
    pub fn start_labeled_request(&self) -> String {
        self.locked(|core| {
            if !core.caps.enabled().contains(IrcCapability::LabeledResponse) {
                return String::new();
            }
            core.begin_labeled_request()
        })
    }

    /// Drops a label without notifying (`IrcSession::cancelRequestLabel`).
    pub fn cancel_request_label(&self, label: &str) {
        self.locked(|core| core.drop_request_label(label));
    }

    /// Writes a MONITOR command. `modifier` is `+`, `-`, `C`, `L` or `S`. Long
    /// lists are split so each line fits one frame (`IrcSession::sendMonitor`).
    pub fn send_monitor(&self, modifier: char, nicks: &[String]) -> bool {
        self.locked(|core| core.send_monitor_locked(modifier, nicks))
    }

    /// Writes `LIST`, or `LIST <mask>` (`IrcSession::list`).
    pub fn list(&self, mask: &str) -> bool {
        self.locked(|core| {
            let trimmed = mask.trim();
            if trimmed.contains(['\r', '\n']) {
                return false;
            }
            if trimmed.is_empty() {
                return core.send_command("LIST", "");
            }
            core.send_command(&format!("LIST {trimmed}"), "")
        })
    }

    /// Writes one already-built command line (`IrcSession::sendRaw`).
    pub fn send_raw(&self, line: &str) -> bool {
        self.locked(|core| core.send_command(line, ""))
    }

    /// Asks for one page older than `msgid`, or older than `timestamp` when no
    /// msgid is given (`requestOlderHistory`).
    pub fn request_older_history(
        &self,
        target: &str,
        msgid: &str,
        timestamp: Option<DateTime<Utc>>,
    ) -> bool {
        self.locked(|core| core.request_older_history(target, msgid, timestamp))
    }

    /// Asks for lines strictly after `after` (`requestHistoryAfter`).
    pub fn request_history_after(&self, target: &str, after: DateTime<Utc>) -> bool {
        self.locked(|core| core.request_history_after(target, after))
    }

    /// Asks for the newest page of `target` (`requestHistoryLatest`).
    pub fn request_history_latest(&self, target: &str) -> bool {
        self.locked(|core| core.request_history_latest(target))
    }

    /// Asks which targets have messages between `from` and `until`
    /// (`requestHistoryTargets`).
    pub fn request_history_targets(&self, from: DateTime<Utc>, until: DateTime<Utc>) -> bool {
        self.locked(|core| core.request_history_targets(from, until))
    }

    /// Asks the server for the stored read marker of `target`.
    pub fn request_read_marker_get(&self, target: &str) {
        self.locked(|core| core.request_read_marker_get(target));
    }

    /// Records a read marker to publish once the target is caught up
    /// (`queueReadMarkerSet`).
    // Chat history and read-marker sync wait for the playback phase; cfg(test) comes off with it.
    #[cfg(test)]
    pub fn queue_read_marker_set(&self, target: &str, when: DateTime<Utc>) {
        self.locked(|core| core.queue_read_marker_set(target, when));
    }
}

impl super::session::Core {
    /// `sendPrivmsgLocked`: validates, sends, and notes the send for typing.
    pub(crate) fn send_privmsg_locked(&mut self, target: &str, body: &str) -> bool {
        if !is_valid_privmsg_target(target) || body.is_empty() {
            return false;
        }
        let sent = self.send_trailing_body(&format!("PRIVMSG {target} :"), body, "", "");
        if sent {
            self.note_message_sent(target);
        }
        sent
    }

    /// `joinLocked`: a keyed JOIN remembers its key until the join is confirmed.
    pub(crate) fn join_locked(&mut self, target: &IrcJoinTarget) -> bool {
        if self.state != SessionState::Registered {
            return false;
        }
        let channel = target.channel().to_owned();
        let key = target.key().unwrap_or("").to_owned();
        let Ok(line) = commandbuilder::join(&channel, &key, &self.features) else {
            return false;
        };
        if target.has_key() {
            let folded = self.fold_channel(&channel);
            self.pending_join_keys.insert(folded, key);
        }
        self.send_line(line.as_bytes());
        true
    }

    /// `setAwayLocked`: AWAY with a trimmed reason, or a bare AWAY to clear.
    // Phase 4: the slash catalog sends these frames
    pub(crate) fn set_away_locked(&mut self, reason: &str) -> bool {
        let trimmed = reason.trim();
        if trimmed.is_empty() {
            return self.send_command("AWAY", "");
        }
        self.send_command(&format!("AWAY :{trimmed}"), "")
    }

    /// `sendMonitorLocked`: packs nicks into MONITOR lines that each fit one frame.
    // Phase 4: the slash catalog sends these frames
    pub(crate) fn send_monitor_locked(&mut self, modifier: char, nicks: &[String]) -> bool {
        let mark = modifier.to_ascii_uppercase();
        match mark {
            'C' | 'L' | 'S' => {
                if !nicks.is_empty() {
                    return false;
                }
                return self.send_command(&format!("MONITOR {mark}"), "");
            }
            '+' | '-' => {}
            _ => return false,
        }
        if nicks.is_empty() {
            return false;
        }

        let prefix = format!("MONITOR {mark} ");
        let mut batch: Vec<String> = Vec::new();
        for nick in nicks {
            if nick.is_empty() {
                return false;
            }
            let mut next = batch.clone();
            next.push(nick.clone());
            let line = format!("{prefix}{}", next.join(","));
            if line.len() + 2 > MAX_CLASSIC_FRAME_BYTES {
                if batch.is_empty() {
                    return false;
                }
                if !self.send_command(&format!("{prefix}{}", batch.join(",")), "") {
                    return false;
                }
                batch = vec![nick.clone()];
                if prefix.len() + nick.len() + 2 > MAX_CLASSIC_FRAME_BYTES {
                    return false;
                }
            } else {
                batch = next;
            }
        }
        !batch.is_empty() && self.send_command(&format!("{prefix}{}", batch.join(",")), "")
    }
}
