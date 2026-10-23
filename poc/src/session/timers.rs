//! The session's timers: the reconnect backoff, the capability deadline, the ping
//! watchdog and the label expiry. Each slot holds at most one armed timer. Arming a
//! slot cancels its predecessor, and a callback whose id is no longer the slot's
//! current one is ignored. `timer_fired` routes an expiry to the handler that owns
//! the slot.

use std::time::Duration;

use super::session::{Core, Session};

/// The timers the session owns. Each slot holds at most one armed timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TimerSlot {
    Reconnect,
    Capability,
    Ping,
    Label,
}

impl TimerSlot {
    pub(crate) const COUNT: usize = 4;

    fn index(self) -> usize {
        self as usize
    }
}

pub(crate) struct ArmedTimer {
    id: u64,
    handle: Box<dyn super::clock::ClockTimer>,
}

impl Core {
    /// Records the one-shot handle for `slot`, cancelling any armed predecessor.
    /// The callback is ignored when a newer arm replaced it.
    pub(crate) fn arm_timer(&mut self, slot: TimerSlot, delay: Duration) {
        self.cancel_timer(slot);
        self.timer_sequence += 1;
        let id = self.timer_sequence;
        let weak = self.weak.clone();
        let handle = self.clock.after(
            delay,
            Box::new(move || {
                if let Some(shared) = weak.upgrade() {
                    Session::from_shared(shared).locked(|core| core.timer_fired(slot, id));
                }
            }),
        );
        self.timers[slot.index()] = Some(ArmedTimer { id, handle });
    }

    pub(crate) fn cancel_timer(&mut self, slot: TimerSlot) {
        if let Some(armed) = self.timers[slot.index()].take() {
            armed.handle.stop();
        }
    }

    pub(crate) fn timer_fired(&mut self, slot: TimerSlot, id: u64) {
        let current = matches!(&self.timers[slot.index()], Some(armed) if armed.id == id);
        if !current {
            return;
        }
        self.timers[slot.index()] = None;
        match slot {
            TimerSlot::Reconnect => self.begin_reconnect_attempt(),
            TimerSlot::Capability => self.on_capability_timeout(),
            TimerSlot::Ping => self.on_ping_watchdog_fired(),
            TimerSlot::Label => self.on_label_timer_fired(),
        }
    }
}
