package controller

import (
	"path/filepath"
	"sync"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/session"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

// TestControllerShutdownRefreshesSTSExpiry covers the quit path: Ctrl+Q and
// SIGHUP return from tea.Run without Session.Stop, and main calls
// Controller.Shutdown before close(wake) so STS expiry is rewritten from quit
// time, not connect time. Without that, a session that outlives duration
// leaves an expired on-disk policy and the next start connects in plaintext.
func TestControllerShutdownRefreshesSTSExpiry(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	c, clock := newController(t)
	c.AttachSTSDisk(path)

	config := baseConfig("net-1", "omairc")
	config.AutojoinChannels = nil
	transport := addAndStart(t, c, clock, config)
	transport.InjectBytes([]byte(":server CAP omairc LS :sts=duration=60\r\n"))

	start := clock.Now().Unix()
	entry, ok := storage.NewIniSTSStore(path).Lookup("irc.example", start)
	if !ok {
		t.Fatal("learned policy must be on disk after CAP")
	}
	if entry.ExpiryEpochSeconds != start+60 {
		t.Fatalf("connect-time expiry = %d, want %d", entry.ExpiryEpochSeconds, start+60)
	}

	clock.Advance(30 * time.Second)
	c.Shutdown()

	after := clock.Now().Unix()
	refreshed, ok := storage.NewIniSTSStore(path).Lookup("irc.example", after)
	if !ok {
		t.Fatal("Shutdown must leave a still-valid policy on disk")
	}
	if refreshed.ExpiryEpochSeconds != after+60 {
		t.Fatalf("quit-time expiry = %d, want %d", refreshed.ExpiryEpochSeconds, after+60)
	}

	// A restart that only saw connect-time expiry would already have dropped
	// the policy by now+31. Shutdown moved expiry forward, so Lookup still hits.
	if _, ok := storage.NewIniSTSStore(path).Lookup("irc.example", start+61); !ok {
		t.Fatal("refreshed expiry must still apply past the original connect+duration")
	}
}

// TestQuitOrderShutdownBeforeCloseWake follows main's real quit sequence with
// more than one live session: wire notify onto a wake channel the way run
// does, Shutdown while wake is open (Stop → StateChanged → notify), then
// close wake. Every network's STS expiry must refresh and notify must not
// panic. Closing wake first used to panic and abort StopAll after one network.
func TestQuitOrderShutdownBeforeCloseWake(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	c, clock := newController(t)
	c.AttachSTSDisk(path)

	wake := make(chan struct{}, 1)
	var wakeMu sync.Mutex
	wakeOpen := true
	notify := func() {
		wakeMu.Lock()
		defer wakeMu.Unlock()
		if !wakeOpen {
			return
		}
		select {
		case wake <- struct{}{}:
		default:
		}
	}
	closeWake := func() {
		wakeMu.Lock()
		defer wakeMu.Unlock()
		if !wakeOpen {
			return
		}
		wakeOpen = false
		close(wake)
	}
	c.OnStatusChanged = notify
	c.OnSelectionChanged = notify
	c.OnCapabilitiesChanged = notify
	c.OnViewChanged = notify

	hosts := []string{"alpha.example", "beta.example"}
	for i, host := range hosts {
		config := baseConfig("net-"+host, "omairc")
		config.Host = host
		config.AutojoinChannels = nil
		transport := addAndStart(t, c, clock, config)
		transport.InjectBytes([]byte(":server CAP omairc LS :sts=duration=60\r\n"))
		_ = i
	}

	start := clock.Now().Unix()
	for _, host := range hosts {
		entry, ok := storage.NewIniSTSStore(path).Lookup(host, start)
		if !ok || entry.ExpiryEpochSeconds != start+60 {
			t.Fatalf("%s connect-time entry = %+v ok=%v", host, entry, ok)
		}
	}

	clock.Advance(30 * time.Second)
	after := clock.Now().Unix()

	// Real quit order from run after tea.Run returns.
	done := make(chan struct{})
	go func() {
		defer close(done)
		c.Shutdown()
		closeWake()
	}()
	select {
	case <-done:
	case <-time.After(5 * time.Second):
		t.Fatal("Shutdown+closeWake hung or panicked")
	}

	for _, host := range hosts {
		entry, ok := storage.NewIniSTSStore(path).Lookup(host, after)
		if !ok {
			t.Fatalf("%s missing after quit-order Shutdown", host)
		}
		if entry.ExpiryEpochSeconds != after+60 {
			t.Fatalf("%s quit-time expiry = %d, want %d", host, entry.ExpiryEpochSeconds, after+60)
		}
	}

	// A late status notify after close must not panic.
	notify()
}

// TestManagerStopAllStopsEverySession pins the seam Shutdown uses.
func TestManagerStopAllStopsEverySession(t *testing.T) {
	manager := session.NewManager()
	clock := session.NewFakeClock(time.Unix(0, 0))
	for _, id := range []string{"a", "b"} {
		transport := session.NewLoopbackTransport()
		config := baseConfig(id, "omairc")
		config.AutojoinChannels = nil
		if _, err := manager.Create(config, transport, clock); err != nil {
			t.Fatal(err)
		}
		if !manager.Activate(id) {
			t.Fatalf("Activate(%s) failed", id)
		}
		transport.CompleteConnect()
	}
	manager.StopAll()
	for _, id := range []string{"a", "b"} {
		s := manager.Find(id)
		if s == nil {
			t.Fatalf("Find(%s) = nil", id)
		}
		if state := s.State(); state != session.StateIdle && state != session.StateClosing {
			t.Fatalf("after StopAll state(%s) = %v, want Idle or Closing", id, state)
		}
	}
}
