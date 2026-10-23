//! Port of `tui/internal/controller/monitorcoordinator.go` (Go) and
//! `src/irc/ircmonitorcoordinator.{h,cpp}` (C++): the MONITOR subscription behind
//! `/monitor`, `/unmonitor` and `/monitored`, and the session-scoped presence the
//! 730, 731 and 734 numerics fold into.
//!
//! Presence is session-scoped. It clears when a network is forgotten or discarded,
//! and at each registration, so a fresh subscription starts from unknown. The nick
//! list belongs to the `MonitorStore` and survives a reconnect.
//!
//! The intents take the network id and the command argument, so the slash dispatcher
//! only resolves the surface and the verb. It maps `MonitorOutcome` to the command
//! outcome text. A refused command keeps the composer text.

use std::collections::{BTreeMap, BTreeSet};

use crate::irc::event::ConversationKey;
use crate::irc::inbox::{InboxItem, InboxKind};
use crate::irc::message::IrcMessage;
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::irc::statusentry;
use crate::irc::wiretext::wire_text;
use crate::session::events::SessionState;

use super::controller::Controller;
use super::notify::Arrival;
use super::stores::ignore_nick_is_usable;

/// One watched nick's last observed state (`IrcMonitorCoordinator::Presence`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MonitorPresence {
    #[default]
    Unknown,
    Online,
    Offline,
}

impl MonitorPresence {
    /// The word `/monitored` shows for this state.
    pub fn label(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Online => "online",
            Self::Offline => "offline",
        }
    }
}

/// What a MONITOR command did (the sent, refused and not-connected arms of
/// `irc::CommandOutcome` that the coordinator can produce).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MonitorOutcome {
    /// The command was handled. Its result, or refusal of the list, is on Status.
    Sent,
    /// The argument or the network cannot take the command.
    Refused,
    /// The network has no registered session.
    NotConnected,
}

/// The session-scoped MONITOR state: which networks sent their list, and each nick's
/// presence per network, keyed by the folded nick.
#[derive(Default)]
pub(super) struct MonitorState {
    subscribed: BTreeSet<String>,
    presence: BTreeMap<String, BTreeMap<String, MonitorPresence>>,
}

impl MonitorState {
    /// Clears the subscription and presence of one network (`ForgetPresence`).
    fn forget(&mut self, network_id: &str) {
        self.subscribed.remove(network_id);
        self.presence.remove(network_id);
    }
}

impl Controller {
    /// Sends the stored nick list once `network_id` is registered and advertises
    /// MONITOR. A network is subscribed once per registration; the list is cut to the
    /// advertised limit (`SubscribeMonitors`).
    pub fn monitor_subscribe(&mut self, network_id: &str) {
        if self.monitor.subscribed.contains(network_id) {
            return;
        }
        let features = self.reducer.server_features(network_id).clone();
        if !features.monitor_advertised() {
            return;
        }
        let Some(session) = self.session(network_id) else {
            return;
        };
        if session.state() != SessionState::Registered {
            return;
        }

        self.monitor.subscribed.insert(network_id.to_owned());
        self.monitor.presence.remove(network_id);

        let mut nicks = self
            .monitors
            .listed(network_id, features.case_mapping());
        if let Some(limit) = features.monitor_limit() {
            nicks.truncate(limit);
        }
        if nicks.is_empty() {
            return;
        }
        session.send_monitor('+', &nicks);
        self.pump();
    }

    /// Clears the session-scoped MONITOR state of `network_id` (`ForgetPresence`).
    pub(super) fn monitor_forget_presence(&mut self, network_id: &str) {
        self.monitor.forget(network_id);
    }

    /// The presence of `nick` on `network_id`, as last reported. Unknown until a 730
    /// or 731 names the nick.
    pub fn monitor_presence(&self, network_id: &str, nick: &str) -> MonitorPresence {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        self.monitor
            .presence
            .get(network_id)
            .and_then(|states| states.get(&mapping.normalize(nick)))
            .copied()
            .unwrap_or_default()
    }

    /// Folds one 730 (online) or 731 (offline) numeric into presence. Only a change
    /// from a known state records a Status line and an inbox row (`handleMonitorPresence`).
    pub fn handle_monitor_presence(&mut self, network_id: &str, message: &IrcMessage, online: bool) {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        for nick in monitor_target_nicks(message) {
            if !self.monitors.contains(network_id, &nick, mapping) {
                continue;
            }
            let key = mapping.normalize(&nick);
            let next = if online {
                MonitorPresence::Online
            } else {
                MonitorPresence::Offline
            };
            let states = self.monitor.presence.entry(network_id.to_owned()).or_default();
            let previous = states.insert(key, next).unwrap_or_default();
            if previous == MonitorPresence::Unknown || previous == next {
                continue;
            }
            let display = self.monitor_display_nick(network_id, &nick);
            let body = if online { "is online" } else { "is offline" };
            let now = self.clock.now();
            self.append_status(statusentry::outcome(
                network_id,
                &format!("{display} {body}"),
                now,
            ));
            if self.monitor_notify_muted(network_id, &nick) {
                continue;
            }
            self.arrivals.push(Arrival::Monitor {
                display: display.clone(),
                body: body.to_owned(),
                network_id: network_id.to_owned(),
            });
            if online && previous != MonitorPresence::Online {
                self.append_inbox(InboxItem {
                    kind: InboxKind::MonitorOnline,
                    timestamp: now,
                    network_id: network_id.to_owned(),
                    actor: display.clone(),
                    target: display,
                    preview: body.to_owned(),
                    msgid: Default::default(),
                });
            }
        }
    }

    /// 734: the server refused the subscription because the list is full, so every
    /// named target is dropped from the store and from presence (`handleMonitorListFull`).
    pub fn handle_monitor_list_full(&mut self, network_id: &str, message: &IrcMessage) {
        let limit = param_text(message, 1);
        let targets = param_text(message, 2);
        let mapping = self.reducer.server_features(network_id).case_mapping();
        for target in targets.split(',') {
            let trimmed = monitor_target_nick(target);
            if trimmed.is_empty() {
                continue;
            }
            self.monitors.remove(network_id, &trimmed, mapping);
            if let Some(states) = self.monitor.presence.get_mut(network_id) {
                states.remove(&mapping.normalize(&trimmed));
            }
        }
        let now = self.clock.now();
        self.append_status(statusentry::outcome(
            network_id,
            &monitor_list_full_text(&limit, &targets),
            now,
        ));
    }

    /// `/monitor nick`: adds `nick` to the stored list and sends `MONITOR + nick`
    /// (`DispatchMonitor`, the add arm). A nick already watched, or a full list, is
    /// reported on Status without a frame.
    pub fn monitor_add(&mut self, network_id: &str, argument: &str) -> MonitorOutcome {
        self.pump();
        let outcome = self.add_monitored(network_id, argument);
        self.pump();
        outcome
    }

    fn add_monitored(&mut self, network_id: &str, argument: &str) -> MonitorOutcome {
        let Some(features) = self.monitor_context(network_id) else {
            return MonitorOutcome::Refused;
        };
        let Some(session) = self.session(network_id) else {
            return MonitorOutcome::NotConnected;
        };
        if session.state() != SessionState::Registered {
            return MonitorOutcome::NotConnected;
        }
        let mapping = features.case_mapping();
        let nick = first_token(argument);
        if !rest_after_first_token(argument).is_empty() || !ignore_nick_is_usable(nick, &features) {
            return MonitorOutcome::Refused;
        }
        if !features.monitor_advertised() {
            self.monitor_status(network_id, "This network does not support MONITOR.");
            return MonitorOutcome::Sent;
        }

        let text = if self.monitors.contains(network_id, nick, mapping) {
            format!("Already watching {nick}")
        } else if features
            .monitor_limit()
            .is_some_and(|limit| self.monitors.listed(network_id, mapping).len() >= limit)
        {
            let limit = features.monitor_limit().unwrap_or_default();
            self.monitor_status(
                network_id,
                &monitor_list_full_text(&limit.to_string(), nick),
            );
            return MonitorOutcome::Sent;
        } else if self.monitors.add(network_id, nick, mapping) {
            if !session.send_monitor('+', &[nick.to_owned()]) {
                self.monitors.remove(network_id, nick, mapping);
                return MonitorOutcome::Refused;
            }
            format!("Watching {nick}")
        } else {
            format!("Already watching {nick}")
        };
        self.monitor_status(network_id, &text);
        MonitorOutcome::Sent
    }

    /// `/unmonitor nick`: drops `nick` from the stored list and sends `MONITOR - nick`
    /// when it was watched (`DispatchMonitor`, the remove arm).
    pub fn monitor_remove(&mut self, network_id: &str, argument: &str) -> MonitorOutcome {
        self.pump();
        let outcome = self.remove_monitored(network_id, argument);
        self.pump();
        outcome
    }

    fn remove_monitored(&mut self, network_id: &str, argument: &str) -> MonitorOutcome {
        let Some(features) = self.monitor_context(network_id) else {
            return MonitorOutcome::Refused;
        };
        let Some(session) = self.session(network_id) else {
            return MonitorOutcome::NotConnected;
        };
        if session.state() != SessionState::Registered {
            return MonitorOutcome::NotConnected;
        }
        let mapping = features.case_mapping();
        let nick = first_token(argument);
        if !rest_after_first_token(argument).is_empty() || !ignore_nick_is_usable(nick, &features) {
            return MonitorOutcome::Refused;
        }
        let text = if self.monitors.remove(network_id, nick, mapping) {
            if let Some(states) = self.monitor.presence.get_mut(network_id) {
                states.remove(&mapping.normalize(nick));
            }
            session.send_monitor('-', &[nick.to_owned()]);
            format!("No longer watching {nick}")
        } else {
            format!("Not watching {nick}")
        };
        self.monitor_status(network_id, &text);
        MonitorOutcome::Sent
    }

    /// `/monitored`: reports every stored nick with its presence (`DispatchMonitor`,
    /// the list arm). An argument makes it refused.
    pub fn monitor_list(&mut self, network_id: &str, argument: &str) -> MonitorOutcome {
        self.pump();
        let Some(features) = self.monitor_context(network_id) else {
            return MonitorOutcome::Refused;
        };
        if self.session(network_id).map(|s| s.state()) != Some(SessionState::Registered) {
            return MonitorOutcome::NotConnected;
        }
        if !argument.is_empty() {
            return MonitorOutcome::Refused;
        }
        let mapping = features.case_mapping();
        let nicks = self.monitors.listed(network_id, mapping);
        let text = if nicks.is_empty() {
            "Not watching anyone".to_owned()
        } else {
            let parts: Vec<String> = nicks
                .iter()
                .map(|nick| {
                    let state = self.monitor_presence(network_id, nick);
                    format!("{nick} ({})", state.label())
                })
                .collect();
            format!("Watching: {}", parts.join(", "))
        };
        self.monitor_status(network_id, &text);
        MonitorOutcome::Sent
    }

    /// The features of a network a MONITOR command may run on. `None` for an empty
    /// network id, which the dispatcher reports as its own scope error.
    fn monitor_context(&self, network_id: &str) -> Option<IrcServerFeatures> {
        if network_id.is_empty() {
            return None;
        }
        Some(self.reducer.server_features(network_id).clone())
    }

    /// Writes one line on the network's Status.
    fn monitor_status(&mut self, network_id: &str, text: &str) {
        let now = self.clock.now();
        self.append_status(statusentry::outcome(network_id, text, now));
    }

    /// The stored spelling of `nick` under the network's mapping, or `nick` itself.
    fn monitor_display_nick(&self, network_id: &str, nick: &str) -> String {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        self.monitors
            .entries(network_id)
            .into_iter()
            .find(|stored| mapping.equals(stored, nick))
            .unwrap_or_else(|| nick.to_owned())
    }

    /// Whether a presence change for `nick` is muted, through the mute list or a muted
    /// conversation (`monitorNotifyMuted`).
    fn monitor_notify_muted(&self, network_id: &str, nick: &str) -> bool {
        let mapping = self.reducer.server_features(network_id).case_mapping();
        if self.mutes.contains(network_id, nick, mapping) {
            return true;
        }
        let key: ConversationKey = self.reducer.conversation_key(network_id, nick);
        self.reducer
            .find(&key)
            .is_some_and(|conversation| conversation.muted)
    }

}

/// The first whitespace-free token of an argument, or the whole argument
/// (`firstToken`).
fn first_token(argument: &str) -> &str {
    argument.split_once(' ').map_or(argument, |(token, _)| token)
}

/// The trimmed text after the first space, or "" (`restAfterFirstToken`).
fn rest_after_first_token(argument: &str) -> String {
    argument
        .split_once(' ')
        .map_or_else(String::new, |(_, rest)| rest.trim().to_owned())
}

/// One message parameter as display text, or "" when absent (`parameterText`).
fn param_text(message: &IrcMessage, index: usize) -> String {
    message.param(index).map(wire_text).unwrap_or_default()
}

/// The nicks a 730, 731 or 734 names in its trailing parameter, with any `!user@host`
/// stripped (`monitorTargetNicks`).
fn monitor_target_nicks(message: &IrcMessage) -> Vec<String> {
    if message.params.len() < 2 {
        return Vec::new();
    }
    let trailing = param_text(message, message.params.len() - 1);
    trailing
        .split(',')
        .map(monitor_target_nick)
        .filter(|nick| !nick.is_empty())
        .collect()
}

/// The nick before any `!`, trimmed (`monitorTargetNick`).
fn monitor_target_nick(target: &str) -> String {
    let before_bang = target.split_once('!').map_or(target, |(nick, _)| nick);
    before_bang.trim().to_owned()
}

/// The "Monitor list is full" line (`monitorListFullText`).
fn monitor_list_full_text(limit: &str, targets: &str) -> String {
    let mut text = "Monitor list is full".to_owned();
    if !limit.is_empty() {
        text.push_str(&format!(" ({limit})"));
    }
    if !targets.is_empty() {
        text.push_str(&format!(": {}", targets.replace(',', ", ")));
    }
    text.push('.');
    text
}
