//! Bouncer playback and chathistory catch-up (Gap 5). Port of
//! `tui/internal/controller/playback.go` and `src/irc/ircplaybackcoordinator.cpp`.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use chrono::{DateTime, Duration, Utc};

use crate::irc::conversation::target_looks_like_service;
use crate::irc::servicenick::nick_is_routable;
use crate::irc::ctcp::parse_ctcp_request;
use crate::irc::event::{HistoryEvent, IrcConversationCause};
use crate::irc::message::IrcMessage;
use crate::irc::prefixnick::prefix_nick;
use crate::irc::reducer::EventReducer;
use crate::irc::translator::{conversation_for, server_time_of, HistoryTarget};
use crate::irc::wiretext::wire_text;
use crate::session::events::SessionState;
use crate::session::session::Session;
use crate::storage::playbacktime::{play_stamp_at, PlaybackTargetTime, PlaybackTimeStore};

use super::controller::Controller;

type SendZnc = Box<dyn Fn(&Session, &str, &str) -> bool>;

#[derive(Default)]
struct CatchUpSent {
    targets: bool,
    targets_retried: bool,
    asked: HashMap<String, bool>,
    discovered: HashMap<String, bool>,
    targets_lower: Option<DateTime<Utc>>,
    targets_extra_pages: u32,
}

#[derive(Default)]
struct ZncPlaybackSent {
    all: bool,
    queries: bool,
    targets: HashSet<String>,
}

/// Per-connection playback state (`IrcPlaybackCoordinator`).
pub struct PlaybackCoordinator {
    times: Rc<RefCell<PlaybackTimeStore>>,
    playback_snapshot: HashMap<String, Vec<PlaybackTargetTime>>,
    znc_playback_sent: HashMap<String, ZncPlaybackSent>,
    znc_autojoin: HashMap<String, Vec<String>>,
    znc_joined_channels: HashMap<String, Vec<String>>,
    catch_up_sent: HashMap<String, CatchUpSent>,
    send_znc: SendZnc,
}

impl PlaybackCoordinator {
    /// An empty coordinator tied to the persisted playback-time store.
    pub fn new(times: Rc<RefCell<PlaybackTimeStore>>) -> Self {
        let send_znc: SendZnc = Box::new(|session, target, from| {
            session.send_raw(&format!("ZNC *playback PLAY {target} {from}"))
        });
        Self {
            times,
            playback_snapshot: HashMap::new(),
            znc_playback_sent: HashMap::new(),
            znc_autojoin: HashMap::new(),
            znc_joined_channels: HashMap::new(),
            catch_up_sent: HashMap::new(),
            send_znc,
        }
    }

    #[cfg(test)]
    pub(crate) fn set_send_znc(&mut self, hook: SendZnc) {
        self.send_znc = hook;
    }

    pub fn on_registered(&mut self, network_id: &str, autojoin_channels: &[String]) {
        let rows = self.times.borrow().targets(network_id);
        self.playback_snapshot.insert(network_id.to_owned(), rows);
        self.znc_autojoin
            .insert(network_id.to_owned(), autojoin_channels.to_vec());
        self.znc_joined_channels.remove(network_id);
    }

    pub fn on_left_registration(&mut self, network_id: &str) {
        self.znc_playback_sent.remove(network_id);
        self.playback_snapshot.remove(network_id);
        self.znc_autojoin.remove(network_id);
        self.znc_joined_channels.remove(network_id);
        self.catch_up_sent.remove(network_id);
    }

    pub fn drop_snapshot(&mut self, network_id: &str) {
        self.playback_snapshot.remove(network_id);
    }

    pub fn rekey(&mut self, reducer: &EventReducer, network_id: &str, old_target: &str, new_target: &str) {
        let rows = match self.playback_snapshot.get(network_id) {
            Some(rows) if !old_target.is_empty() && !new_target.is_empty() => rows.clone(),
            _ => return,
        };
        let mapping = reducer.server_features(network_id).case_mapping();
        let old_index = rows
            .iter()
            .position(|row| mapping.equals(&row.target, old_target));
        let old_index = match old_index {
            Some(index) => index,
            None => return,
        };
        let when = rows[old_index].when;
        let mut rows = rows;
        if mapping.equals(old_target, new_target) {
            rows[old_index].target = new_target.to_owned();
            self.playback_snapshot.insert(network_id.to_owned(), rows);
            return;
        }
        rows.remove(old_index);
        if let Some(index) = rows.iter().position(|row| mapping.equals(&row.target, new_target)) {
            if when != DateTime::<Utc>::MIN_UTC
                && (rows[index].when == DateTime::<Utc>::MIN_UTC || when > rows[index].when)
            {
                rows[index].when = when;
            }
        } else {
            rows.push(PlaybackTargetTime {
                target: new_target.to_owned(),
                when,
            });
        }
        self.playback_snapshot.insert(network_id.to_owned(), rows);
    }

    pub fn note_kept_replay(
        &self,
        reducer: &mut EventReducer,
        znc_playback_cap: impl Fn(&str) -> bool,
    ) {
        for line in reducer.take_kept_replay() {
            if !self.may_note_playback_time(reducer, &line.network_id, &line.target, znc_playback_cap(&line.network_id)) {
                continue;
            }
            let mapping = reducer.server_features(&line.network_id).case_mapping();
            self.times.borrow_mut().note(
                &line.network_id,
                &line.target,
                line.server_time,
                mapping,
            );
        }
    }

    pub fn note_playback_clock(
        &self,
        reducer: &EventReducer,
        network_id: &str,
        message: &IrcMessage,
        current_nick: &str,
        znc_playback_cap: bool,
    ) {
        if network_id.is_empty() || message.command != "PRIVMSG" || message.params.len() < 2 {
            return;
        }
        let when = match server_time_of(message) {
            Some(when) => when,
            None => return,
        };
        if let Some(ctcp) = parse_ctcp_request(&wire_text(&message.params[1])) {
            if !ctcp.command.eq_ignore_ascii_case("ACTION") {
                return;
            }
        }
        let features = reducer.server_features(network_id);
        let wire_target = wire_text(&message.params[0]);
        if conversation_for(network_id, &wire_target, message, current_nick, &features).is_none() {
            return;
        }
        let sender = prefix_nick(message);
        let channel = features.is_channel(&wire_target);
        let self_line = features.case_mapping().equals(&sender, current_nick);
        let display_target = if channel || self_line {
            wire_target.clone()
        } else {
            sender.clone()
        };
        if display_target.is_empty() {
            return;
        }
        if !channel
            && (!nick_is_routable(&display_target) || target_looks_like_service(&display_target, &features))
        {
            return;
        }
        if self_line {
            let key = reducer.conversation_key(network_id, &display_target);
            if reducer.find(&key).is_none() {
                return;
            }
        }
        if !self.may_note_playback_time(reducer, network_id, &display_target, znc_playback_cap) {
            return;
        }
        let mapping = features.case_mapping();
        self.times
            .borrow_mut()
            .note(network_id, &display_target, when, mapping);
    }

    pub fn trim_bouncer_batch(&self, network_id: &str, reducer: &EventReducer, event: &mut HistoryEvent) {
        let sent = match self.znc_playback_sent.get(network_id) {
            Some(sent) if sent.queries => sent,
            _ => return,
        };
        let bound = match self.playback_snapshot_time(reducer, network_id, &event.target) {
            Some(when) => when,
            None => return,
        };
        let bound_ms = bound.timestamp_millis();
        let mut kept = Vec::new();
        for line in &event.lines {
            let line_ms = line.server_time.map(|t| t.timestamp_millis());
            match line_ms {
                None => kept.push(line.clone()),
                Some(ms) if ms > bound_ms => kept.push(line.clone()),
                Some(ms) if ms < bound_ms => {}
                Some(_) if !line.msgid.value.is_empty() => kept.push(line.clone()),
                Some(_) => {}
            }
        }
        event.lines = kept;
        let _ = sent;
    }

    pub fn note_joined_channel(&mut self, reducer: &EventReducer, network_id: &str, channel: &str) {
        if network_id.is_empty() || channel.is_empty() {
            return;
        }
        let mapping = reducer.server_features(network_id).case_mapping();
        let channels = self.znc_joined_channels.entry(network_id.to_owned()).or_default();
        if channels.iter().any(|existing| mapping.equals(existing, channel)) {
            return;
        }
        channels.push(channel.to_owned());
    }

    pub fn request(
        &mut self,
        session: &Session,
        reducer: &EventReducer,
        znc_playback_cap: bool,
        motd_seen: bool,
        restored_directs: &[String],
    ) {
        if session.state() != SessionState::Registered || !motd_seen || !znc_playback_cap {
            return;
        }
        let network_id = session.network_id();
        let sent = self.znc_playback_sent.get(&network_id);
        if sent.is_some_and(|sent| sent.all) {
            return;
        }
        let features = reducer.server_features(&network_id);
        let stored = self
            .playback_snapshot
            .get(&network_id)
            .cloned()
            .unwrap_or_default();
        if stored.is_empty() {
            let already_specific = sent.is_some_and(|sent| !sent.targets.is_empty());
            if !already_specific {
                if !(self.send_znc)(session, "*", "0") {
                    return;
                }
                self.mark_all(&network_id);
            }
        } else {
            for row in &stored {
                let normalized = reducer
                    .conversation_key(&network_id, &row.target)
                    .normalized_target;
                if self.znc_playback_covers(&network_id, &normalized) {
                    continue;
                }
                if !(self.send_znc)(session, &row.target, &play_stamp_at(row.when, true)) {
                    return;
                }
                self.mark_target(&network_id, &normalized);
            }
            for nick in restored_directs {
            if !persistable_direct_target(reducer, &network_id, nick) {
                continue;
            }
            if self
                    .playback_snapshot_time(reducer, &network_id, nick)
                    .is_some()
                {
                    continue;
                }
                let key = reducer.conversation_key(&network_id, nick);
                if reducer.find(&key).is_none() {
                    continue;
                }
                if self.znc_playback_covers(&network_id, &key.normalized_target) {
                    continue;
                }
                if !(self.send_znc)(session, nick, "0") {
                    return;
                }
                self.mark_target(&network_id, &key.normalized_target);
            }
            for channel in self.znc_autojoin.get(&network_id).cloned().unwrap_or_default() {
                if channel.is_empty() || !features.is_channel(&channel) {
                    continue;
                }
                if self
                    .playback_snapshot_time(reducer, &network_id, &channel)
                    .is_some()
                {
                    continue;
                }
                let normalized = reducer
                    .conversation_key(&network_id, &channel)
                    .normalized_target;
                if self.znc_playback_covers(&network_id, &normalized) {
                    continue;
                }
                if !(self.send_znc)(session, &channel, "0") {
                    return;
                }
                self.mark_target(&network_id, &normalized);
            }
            if !self
                .znc_playback_sent
                .get(&network_id)
                .is_some_and(|sent| sent.queries)
            {
                if !(self.send_znc)(session, "*", "0") {
                    return;
                }
                self.mark_queries(&network_id);
            }
        }
        for channel in self.znc_joined_channels.get(&network_id).cloned().unwrap_or_default() {
            self.request_channel_playback(session, reducer, &channel, znc_playback_cap, motd_seen);
        }
    }

    pub fn request_channel_playback(
        &mut self,
        session: &Session,
        reducer: &EventReducer,
        channel: &str,
        znc_playback_cap: bool,
        motd_seen: bool,
    ) {
        if channel.is_empty() || session.state() != SessionState::Registered || !motd_seen || !znc_playback_cap {
            return;
        }
        let network_id = session.network_id();
        let features = reducer.server_features(&network_id);
        if !features.is_channel(channel) {
            return;
        }
        if reducer.playback_batch_kept(&network_id, channel) {
            return;
        }
        let (when, ok) = match self.playback_snapshot_time(reducer, &network_id, channel) {
            Some(when) => (when, true),
            None => (Utc::now(), false),
        };
        if !(self.send_znc)(session, channel, &play_stamp_at(when, ok)) {
            return;
        }
        let normalized = reducer.conversation_key(&network_id, channel).normalized_target;
        self.mark_target(&network_id, &normalized);
    }

    #[cfg(test)]
    pub(crate) fn resume_time(
        &self,
        reducer: &EventReducer,
        network_id: &str,
        target: &str,
    ) -> Option<DateTime<Utc>> {
        self.playback_snapshot_time(reducer, network_id, target)
    }

    /// Self-JOIN CHATHISTORY resume: snapshot first, then persisted times before 001.
    pub fn history_resume_instant(
        &self,
        network_id: &str,
        target: &str,
        mapping: crate::irc::casemapping::IrcCaseMapping,
        times: &PlaybackTimeStore,
    ) -> Option<DateTime<Utc>> {
        if let Some(when) = self.playback_snapshot_time_with_mapping(network_id, target, mapping) {
            return Some(when);
        }
        if self.snapshot_taken(network_id) {
            return None;
        }
        times.noted(network_id, target, mapping)
    }

    pub fn snapshot_taken(&self, network_id: &str) -> bool {
        self.playback_snapshot.contains_key(network_id)
    }

    pub fn request_catch_up(
        &mut self,
        session: &Session,
        reducer: &EventReducer,
        chat_history: bool,
        motd_seen: bool,
        restored_directs: &[String],
        now: DateTime<Utc>,
    ) {
        if session.state() != SessionState::Registered || !motd_seen || !chat_history {
            return;
        }
        let network_id = session.network_id();
        if self.catch_up_sent.get(&network_id).is_some_and(|sent| sent.targets) {
            return;
        }
        for nick in restored_directs {
            if !persistable_direct_target(reducer, &network_id, nick) {
                continue;
            }
            let key = reducer.conversation_key(&network_id, nick);
            if reducer.find(&key).is_none() {
                continue;
            }
            if self.catch_up_asked(&network_id, &key.normalized_target) {
                continue;
            }
            let sent = if let Some(when) = self.playback_snapshot_time(reducer, &network_id, nick) {
                session.request_history_after(nick, when)
            } else {
                session.request_history_latest(nick)
            };
            if !sent {
                continue;
            }
            self.mark_catch_up(&network_id, &key.normalized_target);
        }
        let newest = self.newest_snapshot(&network_id);
        if newest.is_none() || now == DateTime::<Utc>::MIN_UTC {
            self.mark_catch_up_targets(&network_id);
            return;
        }
        let newest = newest.unwrap();
        let lower = newest - Duration::seconds(1);
        let upper = now + Duration::seconds(10);
        let upper = if upper > lower {
            upper
        } else {
            DateTime::parse_from_rfc3339("9999-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc)
        };
        if !session.request_history_targets(lower, upper) {
            return;
        }
        let sent = self.catch_up_sent.entry(network_id).or_default();
        sent.targets = true;
        sent.targets_lower = Some(lower);
    }

    pub fn note_discovered_targets(
        &mut self,
        session: &Session,
        reducer: &mut EventReducer,
        targets: &[HistoryTarget],
        current_nick: &str,
        restored_directs: &[String],
        dismissed_directs: &[String],
    ) -> usize {
        if session.state() != SessionState::Registered {
            return 0;
        }
        let network_id = session.network_id();
        let features = reducer.server_features(&network_id).clone();
        let mapping = features.case_mapping();
        let newest = self.newest_snapshot(&network_id);
        let mut created = 0;
        for row in targets {
            let name = &row.name;
            if name.is_empty() || features.is_channel(name) {
                continue;
            }
            if !nick_is_routable(name) || target_looks_like_service(name, &features) {
                continue;
            }
            if !current_nick.is_empty() && mapping.equals(name, current_nick) {
                continue;
            }
            if list_contains(dismissed_directs, name, mapping) {
                continue;
            }
            let key = reducer.conversation_key(&network_id, name);
            if self.catch_up_asked(&network_id, &key.normalized_target) {
                continue;
            }
            let open_now = reducer.find(&key).is_some();
            let restored = list_contains(restored_directs, name, mapping);
            if !open_now && !restored {
                if !persistable_direct_target(reducer, &network_id, name) || newest.is_none() {
                    continue;
                }
                if reducer
                    .ensure_conversation(&key, name, IrcConversationCause::InboundOther)
                    .is_none()
                {
                    continue;
                }
                let bound = newest.unwrap() - Duration::seconds(1);
                let bound = self
                    .playback_snapshot_time(reducer, &network_id, name)
                    .unwrap_or(bound);
                if !session.request_history_after(name, bound) {
                    if reducer.find(&key).is_some_and(|c| c.messages.is_empty()) {
                        reducer.drop_direct_message(&key);
                    }
                    continue;
                }
                self.mark_catch_up(&network_id, &key.normalized_target);
                self.catch_up_sent
                    .entry(network_id.clone())
                    .or_default()
                    .discovered
                    .insert(key.normalized_target.clone(), true);
                created += 1;
                continue;
            }
            if !open_now {
                continue;
            }
            let sent = if let Some(when) = self.playback_snapshot_time(reducer, &network_id, name) {
                session.request_history_after(name, when)
            } else {
                session.request_history_latest(name)
            };
            if sent {
                self.mark_catch_up(&network_id, &key.normalized_target);
            }
        }
        created
    }

    pub fn note_targets_page(
        &mut self,
        session: &Session,
        targets: &[HistoryTarget],
        history_ended: bool,
        limit: usize,
    ) {
        if limit == 0 || history_ended || targets.len() < limit {
            return;
        }
        let network_id = session.network_id();
        let sent = self.catch_up_sent.get(&network_id);
        let (lower, extra) = match sent {
            Some(sent) if sent.targets_lower.is_some() && sent.targets_extra_pages < 2 => {
                (sent.targets_lower.unwrap(), sent.targets_extra_pages)
            }
            _ => return,
        };
        let mut oldest = None;
        for row in targets {
            if let Some(latest) = row.latest {
                oldest = match oldest {
                    Some(current) if latest < current => Some(latest),
                    None => Some(latest),
                    _ => oldest,
                };
            }
        }
        let oldest = match oldest {
            Some(oldest) if oldest > lower => oldest,
            _ => return,
        };
        if !session.request_history_targets(lower, oldest) {
            return;
        }
        let sent = self.catch_up_sent.entry(network_id).or_default();
        sent.targets_extra_pages = extra + 1;
    }

    pub fn retry_targets(&mut self, network_id: &str) -> bool {
        let sent = match self.catch_up_sent.get_mut(network_id) {
            Some(sent) => sent,
            None => return false,
        };
        if sent.targets_retried {
            return false;
        }
        sent.targets_retried = true;
        sent.targets = false;
        true
    }

    pub fn was_discovered(&self, reducer: &EventReducer, network_id: &str, target: &str) -> bool {
        if target.is_empty() {
            return false;
        }
        let sent = match self.catch_up_sent.get(network_id) {
            Some(sent) => sent,
            None => return false,
        };
        let key = reducer.conversation_key(network_id, target);
        sent.discovered.get(&key.normalized_target).is_some_and(|v| *v)
    }

    fn newest_snapshot(&self, network_id: &str) -> Option<DateTime<Utc>> {
        let rows = self.playback_snapshot.get(network_id)?;
        rows.iter()
            .filter(|row| row.when != DateTime::<Utc>::MIN_UTC)
            .map(|row| row.when)
            .max()
    }

    fn catch_up_asked(&self, network_id: &str, normalized_target: &str) -> bool {
        self.catch_up_sent
            .get(network_id)
            .and_then(|sent| sent.asked.get(normalized_target))
            .is_some_and(|asked| *asked)
    }

    fn mark_catch_up(&mut self, network_id: &str, normalized_target: &str) {
        self.catch_up_sent
            .entry(network_id.to_owned())
            .or_default()
            .asked
            .insert(normalized_target.to_owned(), true);
    }

    fn mark_catch_up_targets(&mut self, network_id: &str) {
        self.catch_up_sent.entry(network_id.to_owned()).or_default().targets = true;
    }

    fn mark_all(&mut self, network_id: &str) {
        self.znc_playback_sent.entry(network_id.to_owned()).or_default().all = true;
    }

    fn mark_queries(&mut self, network_id: &str) {
        self.znc_playback_sent
            .entry(network_id.to_owned())
            .or_default()
            .queries = true;
    }

    fn mark_target(&mut self, network_id: &str, normalized_target: &str) {
        self.znc_playback_sent
            .entry(network_id.to_owned())
            .or_default()
            .targets
            .insert(normalized_target.to_owned());
    }

    fn znc_playback_covers(&self, network_id: &str, normalized_target: &str) -> bool {
        match self.znc_playback_sent.get(network_id) {
            Some(sent) => sent.all || sent.queries || sent.targets.contains(normalized_target),
            None => false,
        }
    }

    fn playback_snapshot_time(
        &self,
        reducer: &EventReducer,
        network_id: &str,
        target: &str,
    ) -> Option<DateTime<Utc>> {
        let mapping = reducer.server_features(network_id).case_mapping();
        self.playback_snapshot_time_with_mapping(network_id, target, mapping)
    }

    fn playback_snapshot_time_with_mapping(
        &self,
        network_id: &str,
        target: &str,
        mapping: crate::irc::casemapping::IrcCaseMapping,
    ) -> Option<DateTime<Utc>> {
        let rows = self.playback_snapshot.get(network_id)?;
        if target.is_empty() {
            return None;
        }
        rows.iter()
            .find(|row| mapping.equals(&row.target, target))
            .map(|row| row.when)
    }

    fn may_note_playback_time(
        &self,
        reducer: &EventReducer,
        network_id: &str,
        target: &str,
        znc_playback_cap: bool,
    ) -> bool {
        if network_id.is_empty() || target.is_empty() {
            return false;
        }
        if !znc_playback_cap {
            return true;
        }
        let normalized = reducer.conversation_key(network_id, target).normalized_target;
        self.znc_playback_covers(network_id, &normalized)
    }
}

fn persistable_direct_target(reducer: &EventReducer, network_id: &str, target: &str) -> bool {
    if network_id.is_empty() || target.is_empty() {
        return false;
    }
    let features = reducer.server_features(network_id);
    if features.is_channel(target) {
        return false;
    }
    !target_looks_like_service(target, &features)
}

fn list_contains(rows: &[String], target: &str, mapping: crate::irc::casemapping::IrcCaseMapping) -> bool {
    rows.iter().any(|row| mapping.equals(row, target))
}

impl Controller {
    pub(super) fn znc_playback_cap(&self, network_id: &str) -> bool {
        self.capabilities
            .get(network_id)
            .is_some_and(|caps| caps.contains(crate::irc::capability::IrcCapability::ZncPlayback))
    }

    pub(super) fn chat_history_cap(&self, network_id: &str) -> bool {
        self.capabilities.get(network_id).is_some_and(|caps| {
            caps.contains(crate::irc::capability::IrcCapability::ChatHistory)
                && caps.contains(crate::irc::capability::IrcCapability::Batch)
        })
    }

    pub(super) fn motd_seen(&self, network_id: &str) -> bool {
        self.open_directs_motd_seen.contains(network_id)
    }

    pub(super) fn request_znc_playback(&mut self, session: &Session) {
        let network_id = session.network_id();
        let features = self.reducer.server_features(&network_id);
        let mapping = features.case_mapping();
        let restored = self.open_directs.listed(&network_id, mapping);
        self.playback.borrow_mut().request(
            session,
            &self.reducer,
            self.znc_playback_cap(&network_id),
            self.motd_seen(&network_id),
            &restored,
        );
    }

    pub(super) fn request_chat_history_catch_up(&mut self, session: &Session) {
        let network_id = session.network_id();
        let features = self.reducer.server_features(&network_id);
        let mapping = features.case_mapping();
        let restored = self.open_directs.listed(&network_id, mapping);
        let now = self.clock.now();
        self.playback.borrow_mut().request_catch_up(
            session,
            &self.reducer,
            self.chat_history_cap(&network_id),
            self.motd_seen(&network_id),
            &restored,
            now,
        );
    }

    pub(super) fn request_channel_playback(&mut self, session: &Session, channel: &str) {
        let network_id = session.network_id();
        self.playback.borrow_mut().request_channel_playback(
            session,
            &self.reducer,
            channel,
            self.znc_playback_cap(&network_id),
            self.motd_seen(&network_id),
        );
    }

    pub(super) fn note_kept_replay(&mut self) {
        let caps = self.capabilities.clone();
        self.playback.borrow().note_kept_replay(&mut self.reducer, |network_id| {
            caps.get(network_id)
                .is_some_and(|set| set.contains(crate::irc::capability::IrcCapability::ZncPlayback))
        });
        for query in self.reducer.take_remembered_queries() {
            let key = self.reducer.conversation_key(&query.network_id, &query.target);
            if self.reducer.find(&key).is_some() {
                self.remember_open_direct(&query.network_id, &query.target);
            }
        }
    }

    pub(super) fn note_playback_clock(&self, network_id: &str, message: &IrcMessage) {
        let nick = self.current_nick(network_id);
        self.playback.borrow().note_playback_clock(
            &self.reducer,
            network_id,
            message,
            &nick,
            self.znc_playback_cap(network_id),
        );
    }
}
