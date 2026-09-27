package ui

// This file ports the Phase 9 desktop-notification subset of the Qt
// test_unfocusedMentionNotifiesOnce fence: an unfocused arrival records the
// plain-text notification and calls the notifier once, a focused arrival is a
// no-op, the test latch records without notifying, and an activation reveals
// the conversation and scrolls to the exact msgid.

import (
	"testing"

	"github.com/fredimachado/omairc/tui/internal/notify"
)

// notifyCall records one desktop-notification call.
type notifyCall struct {
	Summary   string
	Body      string
	NetworkID string
	Target    string
	MsgID     string
}

// fakeNotifier is the desktop seam under test: it records every Notify call.
type fakeNotifier struct {
	calls []notifyCall
}

func (f *fakeNotifier) Notify(summary, body, networkID, target, msgid string) {
	f.calls = append(f.calls, notifyCall{
		Summary:   summary,
		Body:      body,
		NetworkID: networkID,
		Target:    target,
		MsgID:     msgid,
	})
}

func (f *fakeNotifier) Close() {}

var _ notify.Notifier = (*fakeNotifier)(nil)

func TestNotifyMentionIfUnfocused(t *testing.T) {
	m := seededModel(t)
	fake := &fakeNotifier{}
	m.SetNotifier(fake)

	m.lastNotification = nil
	m.notifyMentionIfUnfocused(false, "alice", "hey \x02fred", "", "", "")
	if m.lastNotification == nil {
		t.Fatal("an unfocused arrival must record lastNotification")
	}
	if got := m.lastNotification.Author; got != "alice" {
		t.Fatalf("Author = %q, want alice", got)
	}
	if got := m.lastNotification.Body; got != "hey fred" {
		t.Fatalf("Body = %q, want %q (formatting stripped)", got, "hey fred")
	}
	if len(fake.calls) != 1 {
		t.Fatalf("Notify calls = %d, want 1", len(fake.calls))
	}
	if got := fake.calls[0].Body; got != "hey fred" {
		t.Fatalf("notified body = %q, want %q", got, "hey fred")
	}

	// A focused window is a no-op: no record, no notification.
	m.lastNotification = nil
	m.notifyMentionIfUnfocused(true, "alice", "hey fred", "", "", "")
	if m.lastNotification != nil {
		t.Fatal("a focused arrival must not record lastNotification")
	}
	if len(fake.calls) != 1 {
		t.Fatalf("Notify calls after the focused arrival = %d, want 1", len(fake.calls))
	}

	m.notifyMentionIfUnfocused(false, "alice", "hello", "", "", "")
	if m.lastNotification == nil || m.lastNotification.Author != "alice" {
		t.Fatalf("lastNotification = %+v, want an alice record", m.lastNotification)
	}
	if got := m.lastNotification.Body; got != "hello" {
		t.Fatalf("Body = %q, want hello", got)
	}
	if len(fake.calls) != 2 {
		t.Fatalf("Notify calls = %d, want 2", len(fake.calls))
	}

	// The test latch records the arrival but never reaches the notifier.
	m.suppressDesktopNotification = true
	m.lastNotification = nil
	m.notifyMentionIfUnfocused(false, "bob", "secret", "", "", "")
	if m.lastNotification == nil || m.lastNotification.Author != "bob" {
		t.Fatalf("lastNotification = %+v, want a bob record", m.lastNotification)
	}
	if len(fake.calls) != 2 {
		t.Fatalf("suppressed Notify calls = %d, want 2", len(fake.calls))
	}
}

func TestActivateNotifiedConversationRevealsAndScrolls(t *testing.T) {
	m, d := phase9DemoModel(t)
	selectPhase9Ricing(t, m)

	d.InjectOmarchy([]byte("@msgid=mention-1 :anna!u@h PRIVMSG #omarchy :fred: ping\r\n"))
	m.activateNotifiedConversation("omarchy", "#omarchy", "mention-1")

	if got := m.View().WindowTitle; got != "#omarchy · irc.example · fred - Omairc" {
		t.Fatalf("channel title = %q, want %q", got, "#omarchy · irc.example · fred - Omairc")
	}
	row := m.msgidRow("mention-1")
	if row < 0 {
		t.Fatal("msgidRow must resolve the notified mention")
	}
	if got := m.ctrl.Messages()[row].Body; got != "fred: ping" {
		t.Fatalf("channel row body = %q, want %q", got, "fred: ping")
	}
	if !m.composer.Focused() {
		t.Fatal("activating a channel notification must focus the composer")
	}

	// The direct-message branch reveals the DM and lands on its row.
	d.InjectOmarchy([]byte("@msgid=dm-7 :anna!u@h PRIVMSG fred :secret\r\n"))
	m.activateNotifiedConversation("omarchy", "anna", "dm-7")

	if got := m.View().WindowTitle; got != "anna - Omairc" {
		t.Fatalf("direct title = %q, want %q", got, "anna - Omairc")
	}
	row = m.msgidRow("dm-7")
	if row < 0 {
		t.Fatal("msgidRow must resolve the notified direct message")
	}
	if got := m.ctrl.Messages()[row].Body; got != "secret" {
		t.Fatalf("direct row body = %q, want %q", got, "secret")
	}
}
