package controller

import (
	"runtime"
	"sync/atomic"
)

// The shell goroutine and the session read loops both call into a Controller.
// Qt runs the equivalent on one thread. These two sides share the reducer's
// maps, so a walk (SelectConversation -> MemberView) overlapping an inbound
// line (MessageReceived -> Apply) is a concurrent map read and map write.
//
// Lock is reentrant for the goroutine that already holds it. A shell turn
// that calls into the session can be re-entered by a handler on that same
// goroutine (Start emits StateChanged before it returns); a non-reentrant
// mutex would deadlock there. Callbacks that block on the shell (p.Send) are
// queued and run after the outermost Unlock, so they do not wait on this
// mutex while the shell waits on them.

func goroutineID() uint64 {
	var buf [48]byte
	n := runtime.Stack(buf[:], false)
	// First line: "goroutine 123 [running]:"
	const prefix = "goroutine "
	if n < len(prefix) {
		return 0
	}
	var id uint64
	for i := len(prefix); i < n; i++ {
		c := buf[i]
		if c < '0' || c > '9' {
			break
		}
		id = id*10 + uint64(c-'0')
	}
	return id
}

// Lock serializes the shell with session handlers. Model.Update and Model.View
// hold it for the whole turn. Session handler methods take it themselves.
// Re-locking from the holding goroutine nests; the matching Unlock releases.
func (c *Controller) Lock() { c.lock() }

// Unlock releases one Lock. The outermost Unlock runs callbacks queued with
// deferAfterUnlock. Unlock without a matching Lock panics.
func (c *Controller) Unlock() { c.unlock() }

func (c *Controller) lock() {
	id := goroutineID()
	if id != 0 && atomic.LoadUint64(&c.lockOwner) == id {
		c.lockDepth++
		return
	}
	c.mu.Lock()
	atomic.StoreUint64(&c.lockOwner, id)
	c.lockDepth = 1
}

func (c *Controller) unlock() {
	if c.lockDepth == 0 {
		panic("controller: unlock without lock")
	}
	c.lockDepth--
	if c.lockDepth > 0 {
		return
	}
	pending := c.afterUnlock
	c.afterUnlock = nil
	atomic.StoreUint64(&c.lockOwner, 0)
	c.mu.Unlock()
	for _, fn := range pending {
		fn()
	}
}

// deferAfterUnlock runs fn after the outermost Unlock. With no lock held it
// runs immediately, so tests that call Apply on one goroutine still see
// mention and monitor callbacks before Apply returns.
func (c *Controller) deferAfterUnlock(fn func()) {
	if fn == nil {
		return
	}
	if c.lockDepth == 0 {
		fn()
		return
	}
	c.afterUnlock = append(c.afterUnlock, fn)
}
