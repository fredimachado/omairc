//! Port of `tui/internal/session/clock.go` (Go) and the QTimer injection in
//! `src/irc/ircsession.h` (C++).
//!
//! `Clock` is the only place a session reads time or arms a timer. Every
//! timer the state machine needs (reconnect backoff, capability timeout, ping
//! watchdog, labeled-response timeout, typing refresh) goes through `after`, so
//! tests drive them from a `FakeClock` without sleeping. The production
//! `RealClock` never spawns a thread: the UI loop calls `fire_due`, which keeps
//! the session on the UI thread and needs no `Send`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

/// The time seam for the session state machine.
pub trait Clock {
    /// The current wall-clock time, in UTC. Used for timestamps and CTCP rate
    /// limiting, never for scheduling.
    fn now(&self) -> DateTime<Utc>;

    /// Schedules `callback` to run once `delay` has elapsed. The returned handle
    /// cancels the callback when stopped.
    fn after(&self, delay: Duration, callback: Box<dyn FnOnce()>) -> Box<dyn ClockTimer>;

    /// Runs the timers that are due now and returns how many ran. The UI loop calls
    /// it on each tick. The real clock fires from here; the fake clock fires only
    /// on `advance`, so its default runs nothing.
    fn fire_due(&self) -> usize {
        0
    }
}

/// A one-shot timer handle, the Rust shape of Go `Timer`.
pub trait ClockTimer {
    /// Prevents the callback from running. Returns false once the callback has
    /// fired or was already stopped, matching `*time.Timer::Stop`.
    fn stop(&self) -> bool;
}

/// Shared state of one scheduled callback.
#[derive(Default)]
struct TimerState {
    fired: Cell<bool>,
    stopped: Cell<bool>,
}

impl TimerState {
    fn pending(&self) -> bool {
        !self.fired.get() && !self.stopped.get()
    }
}

/// The handle returned by both clocks.
struct Handle(Rc<TimerState>);

impl ClockTimer for Handle {
    fn stop(&self) -> bool {
        if !self.0.pending() {
            return false;
        }
        self.0.stopped.set(true);
        true
    }
}

/// The production clock. Wall time comes from `Utc::now`; timers fire when the
/// UI loop calls `fire_due`, so no background thread is needed.
#[derive(Default)]
pub struct RealClock {
    pending: RefCell<Vec<RealEntry>>,
}

struct RealEntry {
    due: Instant,
    state: Rc<TimerState>,
    callback: Box<dyn FnOnce()>,
}

// test seam: FakeClock drives the timers in the tests
#[allow(dead_code)]
impl RealClock {
    /// A clock with no timers armed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Runs every callback whose deadline has passed, earliest first, and
    /// returns how many ran. The caller is the UI loop; a callback may arm new
    /// timers, which fire on a later call.
    pub fn fire_due(&self) -> usize {
        let mut fired = 0;
        loop {
            let now = Instant::now();
            let next = {
                let mut pending = self.pending.borrow_mut();
                pending.retain(|entry| entry.state.pending());
                let index = pending
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| entry.due <= now)
                    .min_by_key(|(_, entry)| entry.due)
                    .map(|(index, _)| index);
                index.map(|index| pending.remove(index))
            };
            let Some(entry) = next else {
                break;
            };
            entry.state.fired.set(true);
            (entry.callback)();
            fired += 1;
        }
        fired
    }

    /// How many timers are still armed.
    // Phase 8: the reconnect schedule shows the pending timers
    #[allow(dead_code)]
    pub fn pending(&self) -> usize {
        self.pending
            .borrow()
            .iter()
            .filter(|entry| entry.state.pending())
            .count()
    }
}

impl Clock for RealClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn fire_due(&self) -> usize {
        RealClock::fire_due(self)
    }

    fn after(&self, delay: Duration, callback: Box<dyn FnOnce()>) -> Box<dyn ClockTimer> {
        let state = Rc::new(TimerState::default());
        self.pending.borrow_mut().push(RealEntry {
            due: Instant::now() + delay,
            state: Rc::clone(&state),
            callback,
        });
        Box::new(Handle(state))
    }
}

/// A deterministic clock for tests. `advance` moves virtual time forward and
/// runs the timers that come due, earliest deadline first, ties broken by
/// scheduling order. A callback may arm another timer, which fires in the same
/// `advance` when its deadline also falls inside the window.
// test seam: FakeClock drives the timers in the tests
#[cfg(test)]
pub struct FakeClock {
    start: DateTime<Utc>,
    elapsed: Cell<Duration>,
    sequence: Cell<u64>,
    timers: RefCell<Vec<FakeEntry>>,
}

// test seam: FakeClock drives the timers in the tests
#[cfg(test)]
struct FakeEntry {
    due: Duration,
    sequence: u64,
    state: Rc<TimerState>,
    callback: Box<dyn FnOnce()>,
}

// test seam: FakeClock drives the timers in the tests
#[cfg(test)]
impl FakeClock {
    /// A fake clock whose virtual time starts at `start`.
    // test seam: FakeClock drives the timers in the tests
    pub fn new(start: DateTime<Utc>) -> Self {
        Self {
            start,
            elapsed: Cell::new(Duration::ZERO),
            sequence: Cell::new(0),
            timers: RefCell::new(Vec::new()),
        }
    }

    /// Moves virtual time forward by `delta`, running each timer that comes due.
    // test seam: FakeClock drives the timers in the tests
    pub fn advance(&self, delta: Duration) {
        let target = self.elapsed.get() + delta;
        loop {
            let next = {
                let mut timers = self.timers.borrow_mut();
                timers.retain(|entry| entry.state.pending());
                let index = timers
                    .iter()
                    .enumerate()
                    .filter(|(_, entry)| entry.due <= target)
                    .min_by_key(|(_, entry)| (entry.due, entry.sequence))
                    .map(|(index, _)| index);
                index.map(|index| timers.remove(index))
            };
            let Some(entry) = next else {
                break;
            };
            if entry.due > self.elapsed.get() {
                self.elapsed.set(entry.due);
            }
            entry.state.fired.set(true);
            (entry.callback)();
        }
        if target > self.elapsed.get() {
            self.elapsed.set(target);
        }
    }

    /// How many timers are still armed. Fired and stopped timers do not count.
    // Phase 8: the reconnect schedule shows the pending timers
    pub fn pending(&self) -> usize {
        self.timers
            .borrow()
            .iter()
            .filter(|entry| entry.state.pending())
            .count()
    }
}

#[cfg(test)]
impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        let elapsed =
            chrono::TimeDelta::from_std(self.elapsed.get()).unwrap_or(chrono::TimeDelta::MAX);
        self.start + elapsed
    }

    fn after(&self, delay: Duration, callback: Box<dyn FnOnce()>) -> Box<dyn ClockTimer> {
        let state = Rc::new(TimerState::default());
        let sequence = self.sequence.get() + 1;
        self.sequence.set(sequence);
        self.timers.borrow_mut().push(FakeEntry {
            due: self.elapsed.get() + delay,
            sequence,
            state: Rc::clone(&state),
            callback,
        });
        Box::new(Handle(state))
    }
}
