//! Port of `tui/internal/controller/autoawayruntime.go` (Go) and
//! `src/irc/ircautoawayruntime.{h,cpp}` (C++): the idle and grace timers, the tripped
//! state, the per-network auto and manual away sets, and the `/autoaway` settings.
//!
//! The idle and grace steps run on the `Clock` seam. A timer's callback cannot reach
//! the controller, so it queues an `Expiry` that `Controller::pump` runs on the UI
//! thread, in order. Each expiry carries the timer generation it was armed under, and
//! a stop bumps the generation, so a callback that fired just before a stop (local
//! activity, a disable) is dropped instead of tripping away. Tests advance the fake
//! clock and then pump, which is what the UI loop does on each tick.
//!
//! Away decisions stay here. The controller records the Status lines and the unaway
//! bookkeeping the runtime asks for. A changed setting is also saved through the preference
//! store (`persistence.rs`).

use std::cell::RefCell;
use std::collections::{BTreeSet, VecDeque};
use std::rc::Rc;
use std::time::Duration;

use crate::irc::autoaway::{
    autoaway_grace_seconds, format_autoaway_cleared_status, format_autoaway_confirmation,
    format_autoaway_query, format_autoaway_tripped_status, AutoawayConfig, AutoawayKind,
    AutoawayRequest, AUTOAWAY_MAX_TIMEOUT_SECONDS, AUTOAWAY_MIN_TIMEOUT_SECONDS,
};
use crate::irc::command::{spec_for, Verb};
use crate::irc::statusentry;
use crate::session::clock::ClockTimer;
use crate::session::events::SessionState;
use crate::session::session::Session;

use super::controller::Controller;

/// Which timer fired.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expiry {
    Idle,
    Grace,
}

/// The queue the timer callbacks write to, with the generation each was armed under.
type ExpiryQueue = Rc<RefCell<VecDeque<(Expiry, u64)>>>;

/// The auto-away state the controller holds (`AutoawayRuntime`'s members).
pub(super) struct AutoawayState {
    pub(super) config: AutoawayConfig,
    idle_timer: Option<Box<dyn ClockTimer>>,
    grace_timer: Option<Box<dyn ClockTimer>>,
    grace_armed: bool,
    tripped: bool,
    /// Bumped by every stop; a queued expiry from an older generation is dropped.
    generation: u64,
    expiries: ExpiryQueue,
    /// The networks auto-away marked away.
    auto_away: BTreeSet<String>,
    /// The networks the user took away by hand with `/away`.
    manual_away: BTreeSet<String>,
    /// The networks an unaway (`AWAY` with no reason) was sent to. A 305 or 306, or a
    /// fresh registration, lifts the latch (`apply`'s selfAway and welcome branches).
    pub(super) unaway_sent: BTreeSet<String>,
    /// The last idle and grace intervals armed, for the timer tests.
    #[cfg(test)]
    pub(super) idle_interval: Duration,
    #[cfg(test)]
    pub(super) grace_interval: Duration,
}

impl Default for AutoawayState {
    fn default() -> Self {
        Self {
            config: AutoawayConfig::default(),
            idle_timer: None,
            grace_timer: None,
            grace_armed: false,
            tripped: false,
            generation: 0,
            expiries: Rc::new(RefCell::new(VecDeque::new())),
            auto_away: BTreeSet::new(),
            manual_away: BTreeSet::new(),
            unaway_sent: BTreeSet::new(),
            #[cfg(test)]
            idle_interval: Duration::ZERO,
            #[cfg(test)]
            grace_interval: Duration::ZERO,
        }
    }
}

impl AutoawayState {
    /// Whether auto-away has tripped and not been cleared.
    // A test seam: the auto-away tests read the timer state; the allow comes off with a consumer.
    #[allow(dead_code)]
    pub(super) fn tripped(&self) -> bool {
        self.tripped
    }

    /// Whether `network_id` is one auto-away marked away.
    pub(super) fn is_auto_away(&self, network_id: &str) -> bool {
        self.auto_away.contains(network_id)
    }

    /// Whether the idle timer is scheduled.
    // A test seam: the auto-away tests read the timer state; the allow comes off with a consumer.
    #[allow(dead_code)]
    pub(super) fn idle_scheduled(&self) -> bool {
        self.idle_timer.is_some()
    }

    /// Whether the grace timer is scheduled.
    // A test seam: the auto-away tests read the timer state; the allow comes off with a consumer.
    #[allow(dead_code)]
    pub(super) fn grace_scheduled(&self) -> bool {
        self.grace_timer.is_some()
    }

    /// Whether `network_id` is in the manual-away set.
    pub(super) fn is_manual_away(&self, network_id: &str) -> bool {
        self.manual_away.contains(network_id)
    }

    /// Whether the unaway line was sent to `network_id` since it was last marked away.
    pub(super) fn unaway_was_sent(&self, network_id: &str) -> bool {
        self.unaway_sent.contains(network_id)
    }

    /// Cancels both timers and invalidates whatever they had queued.
    fn stop_timers(&mut self) {
        self.generation += 1;
        if let Some(timer) = self.idle_timer.take() {
            timer.stop();
        }
        if let Some(timer) = self.grace_timer.take() {
            timer.stop();
        }
        self.grace_armed = false;
    }

    /// The reason auto-away gives: the one-shot reason when set, else the default.
    fn reason(&self) -> String {
        if self.config.one_shot_reason.is_empty() {
            self.config.default_reason.clone()
        } else {
            self.config.one_shot_reason.clone()
        }
    }

    fn within_range(&self) -> bool {
        self.config.enabled
            && self.config.timeout_seconds >= AUTOAWAY_MIN_TIMEOUT_SECONDS
            && self.config.timeout_seconds <= AUTOAWAY_MAX_TIMEOUT_SECONDS
    }
}

impl Controller {
    /// Runs the `/autoaway` argument against the settings and returns the reply text
    /// (`DispatchAutoaway`). The caller routes the text to the selected conversation or
    /// to Status. An unparseable argument, or an `on` with no usable timeout, gives the
    /// usage line.
    pub fn autoaway_command(&mut self, request: AutoawayRequest) -> String {
        self.pump();
        let previous_reason = self.autoaway.reason();
        let text = match request.kind {
            AutoawayKind::Usage => return spec_for(Verb::Autoaway).usage.to_owned(),
            AutoawayKind::Query => format_autoaway_query(&self.autoaway.config),
            AutoawayKind::Disable => {
                self.autoaway.config.enabled = false;
                self.autoaway.stop_timers();
                self.autoaway.tripped = false;
                self.autoaway_clear_networks(false);
                format_autoaway_confirmation(&self.autoaway.config)
            }
            AutoawayKind::EnableOn => {
                if self.autoaway.config.timeout_seconds < AUTOAWAY_MIN_TIMEOUT_SECONDS {
                    return spec_for(Verb::Autoaway).usage.to_owned();
                }
                self.autoaway.config.enabled = true;
                if !self.autoaway.tripped {
                    self.autoaway_arm_idle();
                }
                format_autoaway_confirmation(&self.autoaway.config)
            }
            AutoawayKind::SetTimeout => {
                self.autoaway.config.enabled = true;
                self.autoaway.config.timeout_seconds = request.timeout_seconds;
                if !request.text.is_empty() {
                    self.autoaway.config.one_shot_reason = request.text.clone();
                }
                if !self.autoaway.tripped {
                    self.autoaway_arm_idle();
                }
                format_autoaway_confirmation(&self.autoaway.config)
            }
            AutoawayKind::SetDefaultReason => {
                self.autoaway.config.default_reason = request.text.clone();
                format_autoaway_confirmation(&self.autoaway.config)
            }
            AutoawayKind::ClearDefaultReason => {
                self.autoaway.config.default_reason.clear();
                format_autoaway_confirmation(&self.autoaway.config)
            }
        };
        if !matches!(request.kind, AutoawayKind::Usage | AutoawayKind::Query) {
            self.persist_autoaway();
        }
        if self.autoaway.tripped
            && self.autoaway.config.enabled
            && self.autoaway.reason() != previous_reason
        {
            self.autoaway_refresh_reason();
        }
        self.pump();
        text
    }

    /// One input event is activity: it clears a tripped auto-away and re-arms the idle
    /// timer (`noteLocalActivity`).
    pub fn note_local_activity(&mut self) {
        self.pump();
        let was_tripped = self.autoaway.tripped;
        self.autoaway.tripped = false;
        if !self.autoaway.auto_away.is_empty() {
            self.autoaway_clear_networks(true);
        } else if was_tripped {
            self.autoaway.config.one_shot_reason.clear();
        }
        if self.autoaway.config.enabled {
            self.autoaway_arm_idle();
        }
        self.pump();
    }

    /// The user ran `/away` on `network_id`, so auto-away leaves it alone
    /// (`noteManualAway`).
    pub fn note_manual_away(&mut self, network_id: &str) {
        self.autoaway.auto_away.remove(network_id);
        self.autoaway.manual_away.insert(network_id.to_owned());
    }

    /// The user ran `/back` on `network_id` (`noteAwayCleared`). A one-shot reason is
    /// dropped once the last auto-away network comes back.
    pub fn note_away_cleared(&mut self, network_id: &str) {
        let was_auto = self.autoaway.auto_away.remove(network_id);
        self.autoaway.manual_away.remove(network_id);
        if was_auto && self.autoaway.auto_away.is_empty() {
            self.autoaway.config.one_shot_reason.clear();
        }
    }

    /// A network reached Registered: if auto-away has tripped, it is marked away too
    /// (`onSessionRegistered`).
    pub(super) fn autoaway_on_registered(&mut self, network_id: &str) {
        if self.autoaway.tripped && self.autoaway.config.enabled {
            if let Some(session) = self.session(network_id) {
                self.autoaway_mark_session(&session);
            }
        }
    }

    /// Drops one network from both away sets (`forgetNetwork`).
    pub(super) fn autoaway_forget_network(&mut self, network_id: &str) {
        self.autoaway.auto_away.remove(network_id);
        self.autoaway.manual_away.remove(network_id);
        self.autoaway.unaway_sent.remove(network_id);
    }

    /// Records that the unaway line went to `network_id` (`MarkUnawaySent`).
    pub(super) fn autoaway_mark_unaway_sent(&mut self, network_id: &str) {
        self.autoaway.unaway_sent.insert(network_id.to_owned());
    }

    /// A chat line went out on `network_id`. A user who is away is brought back once
    /// per away period, so the unaway line is sent at most once (`unawayAfterChat`).
    pub(super) fn unaway_after_chat(&mut self, network_id: &str) {
        if !self.reducer.self_away(network_id) || self.autoaway.unaway_was_sent(network_id) {
            return;
        }
        let Some(session) = self.session(network_id) else {
            return;
        };
        if session.clear_away() {
            self.autoaway_mark_unaway_sent(network_id);
            self.note_away_cleared(network_id);
        }
    }

    /// Runs one queued timer expiry if it is still current. Returns whether it ran.
    pub(super) fn run_autoaway_expiry(&mut self) -> bool {
        let next = self.autoaway.expiries.borrow_mut().pop_front();
        let Some((expiry, generation)) = next else {
            return false;
        };
        if generation != self.autoaway.generation {
            return true;
        }
        match expiry {
            Expiry::Idle => self.autoaway_on_idle(),
            Expiry::Grace => self.autoaway_on_grace(),
        }
        true
    }

    /// Arms the idle timer for the configured timeout, or stops everything when
    /// auto-away is off or out of range (`armAutoawayIdle`).
    fn autoaway_arm_idle(&mut self) {
        self.autoaway.stop_timers();
        if !self.autoaway.within_range() {
            return;
        }
        let interval = Duration::from_secs(self.autoaway.config.timeout_seconds as u64);
        #[cfg(test)]
        {
            self.autoaway.idle_interval = interval;
        }
        let generation = self.autoaway.generation;
        let queue = Rc::clone(&self.autoaway.expiries);
        self.autoaway.idle_timer = Some(self.clock.after(
            interval,
            Box::new(move || queue.borrow_mut().push_back((Expiry::Idle, generation))),
        ));
    }

    /// The idle timer fired: open the grace window (`onAutoawayIdle`).
    fn autoaway_on_idle(&mut self) {
        if !self.autoaway.config.enabled
            || self.autoaway.config.timeout_seconds < AUTOAWAY_MIN_TIMEOUT_SECONDS
        {
            return;
        }
        self.autoaway.stop_timers();
        self.autoaway.grace_armed = true;
        let grace_seconds = autoaway_grace_seconds(self.autoaway.config.timeout_seconds);
        if grace_seconds <= 0 {
            self.autoaway_on_grace();
            return;
        }
        let interval = Duration::from_secs(grace_seconds as u64);
        #[cfg(test)]
        {
            self.autoaway.grace_interval = interval;
        }
        let generation = self.autoaway.generation;
        let queue = Rc::clone(&self.autoaway.expiries);
        self.autoaway.grace_timer = Some(self.clock.after(
            interval,
            Box::new(move || queue.borrow_mut().push_back((Expiry::Grace, generation))),
        ));
    }

    /// The grace window closed: trip (`onAutoawayGrace`).
    fn autoaway_on_grace(&mut self) {
        if !self.autoaway.grace_armed {
            return;
        }
        self.autoaway.grace_armed = false;
        self.autoaway.stop_timers();
        self.autoaway_trip();
    }

    /// Marks every eligible registered network away and records one Status line for
    /// each (`tripAutoaway`).
    fn autoaway_trip(&mut self) {
        if !self.autoaway.config.enabled {
            return;
        }
        self.autoaway.tripped = true;
        let status = format_autoaway_tripped_status(&self.autoaway.reason());
        for network_id in self.manager.network_ids() {
            if self.autoaway.is_auto_away(&network_id)
                || self.autoaway.is_manual_away(&network_id)
                || self.reducer.self_away(&network_id)
            {
                continue;
            }
            let Some(session) = self.session(&network_id) else {
                continue;
            };
            if self.autoaway_mark_session(&session) {
                self.autoaway_record_status(&network_id, &status);
            }
        }
        self.autoaway.stop_timers();
    }

    /// Marks one registered session away and records it in the auto-away set
    /// (`markSessionAutoAway`).
    fn autoaway_mark_session(&mut self, session: &Session) -> bool {
        if session.state() != SessionState::Registered {
            return false;
        }
        if !session.mark_away(&self.autoaway.reason()) {
            return false;
        }
        self.autoaway.auto_away.insert(session.network_id());
        true
    }

    /// Sends AWAY with the current reason to every auto-away network again
    /// (`refreshAutoAwayReason`).
    fn autoaway_refresh_reason(&mut self) {
        if !self.autoaway.tripped || !self.autoaway.config.enabled {
            return;
        }
        let reason = self.autoaway.reason();
        let networks: Vec<String> = self.autoaway.auto_away.iter().cloned().collect();
        for network_id in networks {
            let Some(session) = self.session(&network_id) else {
                continue;
            };
            if session.state() == SessionState::Registered {
                session.mark_away(&reason);
            }
        }
    }

    /// Brings every auto-away network back and clears the tripped state
    /// (`clearAutoAwayNetworks`). `log_cleared` writes the cleared line to Status.
    fn autoaway_clear_networks(&mut self, log_cleared: bool) {
        self.autoaway.tripped = false;
        let networks: Vec<String> = std::mem::take(&mut self.autoaway.auto_away)
            .into_iter()
            .collect();
        let status = format_autoaway_cleared_status();
        for network_id in networks {
            let Some(session) = self.session(&network_id) else {
                continue;
            };
            if session.state() == SessionState::Registered && session.clear_away() {
                self.autoaway_mark_unaway_sent(&network_id);
                if log_cleared {
                    self.autoaway_record_status(&network_id, &status);
                }
            }
        }
        self.autoaway.config.one_shot_reason.clear();
    }

    /// Writes one non-empty line to a network's Status (`recordAutoawayStatus`).
    fn autoaway_record_status(&mut self, network_id: &str, text: &str) {
        if network_id.is_empty() || text.is_empty() {
            return;
        }
        let now = self.clock.now();
        self.append_status(statusentry::outcome(network_id, text, now));
    }

    /// `/away reason` on `network_id`: an empty reason clears away, anything else marks
    /// the user away by hand. Returns false when the session refuses the line
    /// (`VerbAway`).
    // The product CLI (Phase 9) calls these; the dispatcher writes the same frames for /away and /back.
    #[allow(dead_code)]
    pub fn away(&mut self, network_id: &str, reason: &str) -> bool {
        self.pump();
        let Some(session) = self.session(network_id) else {
            return false;
        };
        if !session.set_away(reason) {
            return false;
        }
        if reason.trim().is_empty() {
            self.note_away_cleared(network_id);
        } else {
            self.note_manual_away(network_id);
        }
        self.pump();
        true
    }

    /// `/back` on `network_id`: clears away. Returns false when the session refuses the
    /// line (`VerbBack`).
    // The product CLI (Phase 9) calls these; the dispatcher writes the same frames for /away and /back.
    #[allow(dead_code)]
    pub fn back(&mut self, network_id: &str) -> bool {
        self.pump();
        let Some(session) = self.session(network_id) else {
            return false;
        };
        if !session.clear_away() {
            return false;
        }
        self.note_away_cleared(network_id);
        self.pump();
        true
    }

    /// A chat line went out on `network_id`. Input counts as activity, and a user who
    /// is away is brought back once per away period (`sendSelectedMessageOutcome`). The
    /// chat dispatch calls this after the PRIVMSG is written.
    pub fn note_chat_sent(&mut self, network_id: &str) {
        self.pump();
        self.note_local_activity();
        self.unaway_after_chat(network_id);
        self.pump();
    }
}
