package controller

import (
	"path/filepath"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/session"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

// TestControllerShutdownRefreshesSTSExpiry covers the quit path: Ctrl+Q and
// SIGHUP return from tea.Run without Session.Stop, and main defers
// Controller.Shutdown so STS expiry is rewritten from quit time, not connect
// time. Without that, a session that outlives duration leaves an expired
// on-disk policy and the next start connects in plaintext.
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
