package ui

// This file covers the transcript "New messages" boundary and the
// open-at-unread landing that the Preferences toggle controls. It mirrors
// OmaircWindow.qml's placeTranscriptAfterSelect / pinTranscriptOnFocusReturn
// and MessageListModel::unreadMarkRow.

import (
	"fmt"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// updateMsg folds a non-key message (Blur/Focus) into the shell and returns the
// updated model.
func updateMsg(t *testing.T, m *Model, msg tea.Msg) *Model {
	t.Helper()
	updated, _ := m.Update(msg)
	model, ok := updated.(*Model)
	if !ok {
		t.Fatalf("Update returned %T, want *Model", updated)
	}
	return model
}

// unreadDemoModel seeds the demo on a frozen clock and returns the DemoServer
// so a test can inject a live line into a conversation other than the selected
// one and plant an unread mark.
func unreadDemoModel(t *testing.T) (*Model, *demo.DemoServer) {
	t.Helper()
	ctrl := controller.New()
	ctrl.SetClock(session.NewFakeClock(time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)))
	d := demo.New()
	if !d.Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	m := New(ctrl, nil)
	// A short viewport keeps the packed #omarchy transcript taller than the
	// window, so pinning to the unread mark has rows to scroll past.
	m = resizeModel(t, m, 118, 20)
	return m, d
}

// plantOmarchyUnread moves the selection to #ricing and injects one live line
// into #omarchy, so #omarchy carries an unread mark. It returns the model with
// #ricing still selected.
func plantOmarchyUnread(t *testing.T, m *Model, d *demo.DemoServer) *Model {
	t.Helper()
	m = press(t, m, altKey(tea.KeyDown))
	if got := m.ctrl.SelectedTarget(); got != "#ricing" {
		t.Fatalf("walk selection = %q, want #ricing", got)
	}
	d.InjectOmarchy([]byte("@msgid=unread-1 :anna!u@h PRIVMSG #omarchy :a fresh line\r\n"))
	return m
}

// TestUnreadMarkRowSkipsFirstRow proves the boundary needs a row above it: a
// conversation whose first message is unread renders no boundary and reports -1,
// mirroring the !view.empty() guard in MessageListModel::buildView.
func TestUnreadMarkRowSkipsFirstRow(t *testing.T) {
	m, d := unreadDemoModel(t)
	m = plantOmarchyUnread(t, m, d)
	// An inbound message for a channel that is not open invents the channel
	// with a single row, and that first row is the unread mark.
	d.InjectOmarchy([]byte("@msgid=fresh-1 :anna!u@h PRIVMSG #fresh :first and only\r\n"))
	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "#fresh")
	})
	if got := m.ctrl.UnreadMarkRow(); got != -1 {
		t.Fatalf("UnreadMarkRow = %d, want -1 for a first-row mark", got)
	}
	if content := m.View().Content; strings.Contains(content, "New messages") {
		t.Fatalf("a first-row mark must not render a boundary:\n%s", content)
	}
}

// fillDesktopUnread injects enough lines into the never-selected #desktop that
// a first visit can pin its unread mark away from the bottom.
func fillDesktopUnread(d *demo.DemoServer) {
	for index := 0; index < 40; index++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(
			"@msgid=desk-%d :anna!u@h PRIVMSG #desktop :desk line %d\r\n", index, index)))
	}
}

// TestOpenAtUnreadLandsOnMark renders the boundary and lands the viewport on it
// when the preference is on and the conversation has no saved scroll place.
func TestOpenAtUnreadLandsOnMark(t *testing.T) {
	m, d := unreadDemoModel(t)
	if !m.ctrl.OpenAtUnread() {
		t.Fatal("open-at-unread must default on")
	}
	fillDesktopUnread(d)
	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "#desktop")
	})
	if got := m.ctrl.SelectedTarget(); got != "#desktop" {
		t.Fatalf("selection = %q, want #desktop", got)
	}
	mark := m.ctrl.UnreadMarkRow()
	if mark < 0 {
		t.Fatal("#desktop must carry an unread mark after the injected lines")
	}
	if m.transcriptFollowEnd {
		t.Fatal("opening a never-visited conversation at unread must leave follow-the-end")
	}
	if content := m.View().Content; !strings.Contains(content, "New messages") {
		t.Fatalf("transcript must render the New messages boundary:\n%s", content)
	}
}

// TestOpenAtUnreadOffPinsToEnd pins to the end when the preference is off.
func TestOpenAtUnreadOffPinsToEnd(t *testing.T) {
	m, d := unreadDemoModel(t)
	m.ctrl.SetOpenAtUnread(false)
	m = plantOmarchyUnread(t, m, d)

	m = press(t, m, altKey(tea.KeyUp))
	if got := m.ctrl.SelectedTarget(); got != "#omarchy" {
		t.Fatalf("walk selection = %q, want #omarchy", got)
	}
	if !m.transcriptFollowEnd {
		t.Fatal("with the preference off the transcript must follow the end")
	}
}

// TestBoundaryKeepsRowCountAligned proves the boundary is attached to the
// marked row rather than inserted as its own row, so find and copy, which index
// the message count, stay aligned with the rendered row area.
func TestBoundaryKeepsRowCountAligned(t *testing.T) {
	m, d := unreadDemoModel(t)
	m = plantOmarchyUnread(t, m, d)
	m = press(t, m, altKey(tea.KeyUp))

	if mark := m.ctrl.UnreadMarkRow(); mark < 0 {
		t.Fatal("#omarchy must carry an unread mark")
	}
	area := m.transcriptArea()
	if got, want := area.count(), m.transcriptRowTotal(); got != want {
		t.Fatalf("rendered rows = %d, want %d message rows", got, want)
	}
}

// TestReopenSameConversationKeepsViewport proves re-selecting the current
// conversation does not yank the reader, mirroring the openConversationsAtUnread
// guard in placeTranscriptAfterSelect.
func TestReopenSameConversationKeepsViewport(t *testing.T) {
	m, d := unreadDemoModel(t)
	m = plantOmarchyUnread(t, m, d)
	m = press(t, m, altKey(tea.KeyUp))

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	scroll := m.transcriptScroll
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "#omarchy")
	})
	if m.transcriptScroll != scroll {
		t.Fatalf("re-select moved the viewport: scroll = %d, want %d", m.transcriptScroll, scroll)
	}
}

// TestStatusFollowsEndWithMark proves the Status console still follows the end
// even when the conversation behind it has a New messages mark.
func TestStatusFollowsEndWithMark(t *testing.T) {
	m, d := unreadDemoModel(t)
	fillDesktopUnread(d)
	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "#desktop")
	})
	if m.transcriptFollowEnd {
		t.Fatal("precondition: the conversation must have landed on the mark")
	}

	m = press(t, m, ctrlKey('`'))
	if !m.ctrl.ConsoleOpen() {
		t.Fatal("Ctrl+` must open Status")
	}
	if !m.transcriptFollowEnd {
		t.Fatal("Status must follow the end regardless of the mark")
	}
}

// TestFocusRegainLandsOnMark proves a focus regain pins the transcript to the
// mark, mirroring pinTranscriptOnFocusReturn.
func TestFocusRegainLandsOnMark(t *testing.T) {
	m, d := unreadDemoModel(t)
	m = plantOmarchyUnread(t, m, d)
	m = press(t, m, altKey(tea.KeyUp))

	// Simulate scrolling away, then blur and regain focus while unfocused chat
	// arrives so the controller consumes unread and keeps the mark.
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	m = updateMsg(t, m, tea.BlurMsg{})
	d.InjectOmarchy([]byte("@msgid=unread-2 :anna!u@h PRIVMSG #omarchy :while away\r\n"))
	m = updateMsg(t, m, tea.FocusMsg{})
	if m.transcriptFollowEnd {
		t.Fatal("focus regain must land on the mark, not the end")
	}
	if content := m.View().Content; !strings.Contains(content, "New messages") {
		t.Fatalf("focus regain must show the boundary:\n%s", content)
	}
}
