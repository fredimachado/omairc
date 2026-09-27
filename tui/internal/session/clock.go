package session

import (
	"sync"
	"time"
)

// Clock is the time seam for the session state machine. Every timer the session
// needs — reconnect backoff, capability negotiation timeout, ping watchdog, the
// 45000ms labeled-response timeout, and the typing refresh — is armed through
// AfterFunc so tests can drive it from a FakeClock instead of sleeping.
//
// It mirrors the QTimer/clock injection in src/irc/ircsession.h, which takes
// its timers as constructor arguments and reads the wall clock through one
// monotonic source.
type Clock interface {
	Now() time.Time
	AfterFunc(d time.Duration, f func()) Timer
}

// Timer is a one-shot timer handle. Stop reports whether it prevented the
// callback from firing; it returns false once the timer has fired or was
// already stopped, matching *time.Timer.Stop.
type Timer interface {
	Stop() bool
}

// RealClock is the production Clock backed by the time package.
type RealClock struct{}

// NewRealClock returns the real-time Clock.
func NewRealClock() RealClock {
	return RealClock{}
}

// Now returns time.Now.
func (RealClock) Now() time.Time {
	return time.Now()
}

// AfterFunc schedules f on a real *time.Timer. *time.Timer already implements
// Timer, so it is returned directly.
func (RealClock) AfterFunc(d time.Duration, f func()) Timer {
	return time.AfterFunc(d, f)
}

// FakeClock is a deterministic Clock for tests in this package and in packages
// that build on it. Advance moves virtual time forward and fires every timer
// whose deadline has arrived, in deadline order (ties broken by scheduling
// order). Timers are one-shot.
//
// It is safe for one goroutine to call Now/AfterFunc while another calls
// Advance: a mutex guards the schedule and callbacks are invoked with the lock
// released, so a callback may call back into the clock (for example to re-arm a
// watchdog).
type FakeClock struct {
	mu     sync.Mutex
	now    time.Time
	nextID uint64
	timers []*fakeTimer
}

type fakeTimer struct {
	clock   *FakeClock
	id      uint64
	due     time.Time
	fire    func()
	stopped bool
	fired   bool
}

// NewFakeClock returns a FakeClock whose virtual time starts at start.
func NewFakeClock(start time.Time) *FakeClock {
	return &FakeClock{now: start}
}

// Now returns the current virtual time.
func (c *FakeClock) Now() time.Time {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.now
}

// AfterFunc schedules f to fire once d of virtual time has elapsed.
func (c *FakeClock) AfterFunc(d time.Duration, f func()) Timer {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.nextID++
	timer := &fakeTimer{
		clock: c,
		id:    c.nextID,
		due:   c.now.Add(d),
		fire:  f,
	}
	c.timers = append(c.timers, timer)
	return timer
}

// Advance moves virtual time forward by d and fires the timers that come due,
// earliest deadline first. A timer scheduled by a firing callback is eligible
// to fire in the same Advance when its deadline also falls within d.
func (c *FakeClock) Advance(d time.Duration) {
	if d < 0 {
		d = 0
	}

	c.mu.Lock()
	target := c.now.Add(d)
	for {
		timer := c.nextDueLocked(target)
		if timer == nil {
			break
		}
		if timer.due.After(c.now) {
			c.now = timer.due
		}
		timer.fired = true
		c.removeLocked(timer)
		fire := timer.fire
		c.mu.Unlock()
		if fire != nil {
			fire()
		}
		c.mu.Lock()
	}
	if target.After(c.now) {
		c.now = target
	}
	c.mu.Unlock()
}

// Pending reports how many timers are still scheduled. Fired and stopped timers
// are removed, so Pending counts only live timers.
func (c *FakeClock) Pending() int {
	c.mu.Lock()
	defer c.mu.Unlock()
	return len(c.timers)
}

// nextDueLocked returns the live timer with the earliest deadline at or before
// target, breaking ties by scheduling order.
func (c *FakeClock) nextDueLocked(target time.Time) *fakeTimer {
	var best *fakeTimer
	for _, timer := range c.timers {
		if timer.stopped || timer.fired || timer.due.After(target) {
			continue
		}
		if best == nil || timer.due.Before(best.due) ||
			(timer.due.Equal(best.due) && timer.id < best.id) {
			best = timer
		}
	}
	return best
}

func (c *FakeClock) removeLocked(timer *fakeTimer) {
	for index, candidate := range c.timers {
		if candidate == timer {
			c.timers = append(c.timers[:index], c.timers[index+1:]...)
			return
		}
	}
}

// Stop prevents the timer from firing and reports whether it was still pending.
func (t *fakeTimer) Stop() bool {
	clock := t.clock
	clock.mu.Lock()
	defer clock.mu.Unlock()
	if t.stopped || t.fired {
		return false
	}
	t.stopped = true
	clock.removeLocked(t)
	return true
}
