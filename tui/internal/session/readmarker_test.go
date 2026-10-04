package session

import (
	"strings"
	"testing"
	"time"
)

type readMarkerHandler struct {
	sessionTestHandler
	markers []struct {
		target string
		marker *time.Time
	}
}

func (h *readMarkerHandler) ReadMarkerReceived(networkID, target string, marker *time.Time) {
	h.markers = append(h.markers, struct {
		target string
		marker *time.Time
	}{target, marker})
}

func registerWithReadMarker(fixture *sessionFixture, tokens string) {
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :" + tokens + "\r\n")
	fixture.inject(":server CAP omairc ACK :" + tokens + "\r\n")
	fixture.inject(":server 001 omairc :Welcome\r\n")
}

func TestReadMarkerCapAbsentSendsNothing(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	handler := &readMarkerHandler{}
	fixture.session.SetHandler(handler)
	registerWithReadMarker(fixture, "multi-prefix")
	before := len(fixture.frames())
	fixture.session.QueueReadMarkerSet("#omarchy", time.Unix(0, 0).UTC())
	if len(fixture.frames()) != before {
		t.Fatalf("wrote frames without read-marker cap: %v", fixture.frames())
	}
}

func TestReadMarkerCaughtUpSendsMarkreadOnce(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	registerWithReadMarker(fixture, "draft/read-marker multi-prefix")
	when := time.Date(2024, 6, 1, 12, 0, 0, 123000000, time.UTC)
	fixture.session.QueueReadMarkerSet("#omarchy", when)
	want := "MARKREAD #omarchy timestamp=2024-06-01T12:00:00.123Z\r\n"
	if !fixture.wrote(want) {
		t.Fatalf("frames = %q, want %q", fixture.frames(), want)
	}
	fixture.session.QueueReadMarkerSet("#omarchy", when)
	if countReadMarkerFrame(fixture.frames(), want) != 1 {
		t.Fatalf("duplicate MARKREAD sent: %v", fixture.frames())
	}
}

func TestReadMarkerSojuUsesReadCommand(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	registerWithReadMarker(fixture, "soju.im/read multi-prefix")
	when := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	fixture.session.QueueReadMarkerSet("alice", when)
	if !fixture.wrote("READ alice timestamp=2024-06-01T12:00:00.000Z\r\n") {
		t.Fatalf("frames = %q", fixture.frames())
	}
}

func TestReadMarkerOlderSetNotQueued(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	registerWithReadMarker(fixture, "draft/read-marker")
	older := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	newer := older.Add(time.Minute)
	fixture.session.QueueReadMarkerSet("#omarchy", newer)
	fixture.session.QueueReadMarkerSet("#omarchy", older)
	if countReadMarkerFrame(fixture.frames(), "MARKREAD") != 1 {
		t.Fatalf("older timestamp must not send again: %v", fixture.frames())
	}
}

func TestReadMarkerNewerTimestampSendsAgainAfterFail(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	registerWithReadMarker(fixture, "draft/read-marker")
	first := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	second := first.Add(time.Minute)
	fixture.session.QueueReadMarkerSet("#omarchy", first)
	fixture.inject(":server FAIL MARKREAD RATE_LIMITED #omarchy :slow down\r\n")
	fixture.session.QueueReadMarkerSet("#omarchy", second)
	if countReadMarkerFrame(fixture.frames(), "MARKREAD") != 2 {
		t.Fatalf("expected two MARKREAD frames after FAIL, got %v", fixture.frames())
	}
}

func TestReadMarkerFailDoesNotDisconnect(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	registerWithReadMarker(fixture, "draft/read-marker")
	fixture.session.QueueReadMarkerSet("#omarchy", time.Unix(0, 0).UTC())
	fixture.inject(":server FAIL MARKREAD INVALID_TARGET #omarchy :nope\r\n")
	if fixture.session.State() != StateRegistered {
		t.Fatalf("state = %v, want Registered after FAIL MARKREAD", fixture.session.State())
	}
}

func TestReadMarkerGetOnDirect(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	registerWithReadMarker(fixture, "draft/read-marker")
	fixture.session.RequestReadMarkerGet("alice")
	if !fixture.wrote("MARKREAD alice\r\n") {
		t.Fatalf("frames = %q", fixture.frames())
	}
}

func TestReadMarkerEchoDoesNotResendEqualPending(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	registerWithReadMarker(fixture, "draft/read-marker")
	when := time.Date(2024, 6, 1, 12, 0, 0, 123000000, time.UTC)
	fixture.session.QueueReadMarkerSet("#omarchy", when)
	want := "MARKREAD #omarchy timestamp=2024-06-01T12:00:00.123Z\r\n"
	if !fixture.wrote(want) {
		t.Fatalf("frames = %q", fixture.frames())
	}
	fixture.session.QueueReadMarkerSet("#omarchy", when)
	fixture.inject(":server MARKREAD #omarchy :timestamp=2024-06-01T12:00:00.123Z\r\n")
	if countReadMarkerFrame(fixture.frames(), want) != 1 {
		t.Fatalf("echo must not resend equal timestamp: %v", fixture.frames())
	}
}

func TestReadMarkerFailWithoutTargetClearsInFlight(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	registerWithReadMarker(fixture, "draft/read-marker")
	when := time.Date(2024, 6, 1, 12, 0, 0, 0, time.UTC)
	fixture.session.QueueReadMarkerSet("#omarchy", when)
	fixture.inject(":server FAIL MARKREAD NEED_MORE_PARAMS :Missing parameters\r\n")
	later := when.Add(time.Minute)
	fixture.session.QueueReadMarkerSet("#omarchy", later)
	if countReadMarkerFrame(fixture.frames(), "MARKREAD") != 2 {
		t.Fatalf("expected resend after untargeted FAIL, got %v", fixture.frames())
	}
}

func countReadMarkerFrame(frames []string, needle string) int {
	count := 0
	for _, frame := range frames {
		if strings.Contains(frame, needle) {
			count++
		}
	}
	return count
}
