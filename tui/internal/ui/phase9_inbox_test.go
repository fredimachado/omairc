package ui

// This file ports the Phase 9 session-inbox sheet subset of the Qt
// test_unfocusedMentionNotifiesOnce / inbox feature fences: the Ctrl+Shift+A
// toggle, the identity footer badge, Enter revealing a mention or joining an
// invite, selecting a conversation consuming its rows, and Delete/dismiss
// keeping the highlight on the same logical row.

import (
	"fmt"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// phase9DemoModel seeds the demo on a frozen clock and returns the DemoServer
// so a test can inject inbound frames. transcriptPhase8Model does not expose
// the server, and the waiting list needs injected arrivals.
func phase9DemoModel(t *testing.T) (*Model, *demo.DemoServer) {
	t.Helper()
	ctrl := controller.New()
	ctrl.SetClock(session.NewFakeClock(time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)))
	d := demo.New()
	if !d.Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	m := New(ctrl, nil)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	return updated.(*Model), d
}

// selectPhase9Ricing moves the selection off the seeded #omarchy so a mention
// aimed at #omarchy plants an inbox row.
func selectPhase9Ricing(t *testing.T, m *Model) {
	t.Helper()
	m.ctrl.SelectConversation("omarchy", "#ricing")
	if got := m.ctrl.SelectedTarget(); got != "#ricing" {
		t.Fatalf("selected target = %q, want #ricing", got)
	}
}

// writtenOmarchyFramesContain reports whether the primary transport wrote a
// frame containing text.
func writtenOmarchyFramesContain(d *demo.DemoServer, text string) bool {
	for _, frame := range d.OmarchyTransport().WrittenFrames() {
		if strings.Contains(string(frame), text) {
			return true
		}
	}
	return false
}

func TestInboxSheetTogglesWithShortcut(t *testing.T) {
	m := seededModel(t)

	m = press(t, m, ctrlShiftKey('a'))
	if !m.inboxVisible() {
		t.Fatal("Ctrl+Shift+A must open the inbox sheet")
	}
	m = press(t, m, ctrlShiftKey('a'))
	if m.inboxVisible() {
		t.Fatal("a second Ctrl+Shift+A must close the inbox sheet")
	}
	if !m.composer.Focused() {
		t.Fatal("closing the inbox must return focus to the composer")
	}

	m = press(t, m, ctrlShiftKey('a'))
	if !m.inboxVisible() {
		t.Fatal("Ctrl+Shift+A must reopen the inbox sheet")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.inboxVisible() {
		t.Fatal("Escape must close the inbox sheet")
	}
	if !m.composer.Focused() {
		t.Fatal("Escape must return focus to the composer")
	}

	// Connect is a window-level modal: the inbox chord must not open on top of
	// it, and Connect must stay visible.
	ctrl := controller.New()
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	conn := connection.New(ctrl, nil)
	mc := New(ctrl, conn)
	mc = resizeModel(t, mc, 118, 30)
	mc = press(t, mc, ctrlKey(','))
	if !mc.connectVisible() {
		t.Fatal("first-run Connect must be visible")
	}
	mc = press(t, mc, ctrlShiftKey('a'))
	if mc.inboxVisible() {
		t.Fatal("Ctrl+Shift+A must be gated while Connect is visible")
	}
	if !mc.connectVisible() {
		t.Fatal("the gated chord must not close Connect")
	}
}

func TestInboxMarkShowsCountAndOpensSheet(t *testing.T) {
	m, d := phase9DemoModel(t)
	selectPhase9Ricing(t, m)
	// Selecting #ricing consumed its seeded mention, so the baseline is the
	// anna direct only.
	before := m.ctrl.InboxCount()

	d.InjectOmarchy([]byte("@msgid=inbox-mark-1 :anna!u@h PRIVMSG #omarchy :fred: inbox ping\r\n"))

	if got := m.ctrl.InboxCount(); got != before+1 {
		t.Fatalf("InboxCount = %d, want %d", got, before+1)
	}
	if badge := fmt.Sprintf("inbox %d", before+1); !strings.Contains(m.View().Content, badge) {
		t.Fatalf("rendered view missing the identity badge %q:\n%s", badge, m.View().Content)
	}

	m = press(t, m, ctrlShiftKey('a'))
	if !m.inboxVisible() {
		t.Fatal("Ctrl+Shift+A must open the inbox sheet")
	}
	if got := len(m.inboxEntries()); got != before+1 {
		t.Fatalf("inboxEntries = %d, want %d", got, before+1)
	}
	row := m.ctrl.InboxItems()[0]
	if row.Kind != "mention" {
		t.Fatalf("Kind = %q, want mention", row.Kind)
	}
	if row.Target != "#omarchy" {
		t.Fatalf("Target = %q, want #omarchy", row.Target)
	}
}

func TestInboxMentionEnterJumpsToMsgid(t *testing.T) {
	m, d := phase9DemoModel(t)
	selectPhase9Ricing(t, m)

	d.InjectOmarchy([]byte("@msgid=inbox-mention-1 :anna!u@h PRIVMSG #omarchy :fred: inbox scroll\r\n"))

	m = press(t, m, ctrlShiftKey('a'))
	if !m.inboxVisible() {
		t.Fatal("Ctrl+Shift+A must open the inbox sheet")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if m.inboxVisible() {
		t.Fatal("Enter must close the inbox sheet")
	}
	if got := m.View().WindowTitle; got != "#omarchy · irc.example · fred - Omairc" {
		t.Fatalf("after Enter title = %q, want %q", got, "#omarchy · irc.example · fred - Omairc")
	}
	if got := m.ctrl.SelectedTarget(); got != "#omarchy" {
		t.Fatalf("SelectedTarget = %q, want #omarchy", got)
	}
	row := m.msgidRow("inbox-mention-1")
	if row < 0 {
		t.Fatal("msgidRow must resolve the activated mention")
	}
	if got := m.ctrl.Messages()[row].Body; got != "fred: inbox scroll" {
		t.Fatalf("row body = %q, want %q", got, "fred: inbox scroll")
	}
	if !m.composer.Focused() {
		t.Fatal("activating a mention must focus the composer")
	}
}

func TestInboxMentionWithoutMsgidKeepsSavedPlace(t *testing.T) {
	m, d := phase9DemoModel(t)
	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "#desktop")
	})
	for index := 0; index < 40; index++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(":dax!u@h PRIVMSG #desktop :desktop filler %d\r\n", index)))
	}
	m.noteTranscriptGrowth()
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	body := m.ctrl.Messages()[m.firstVisibleMessageRow()].Body
	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "#ricing")
	})
	d.InjectOmarchy([]byte(":anna!u@h PRIVMSG #desktop :fred: inbox without id\r\n"))
	if m.ctrl.InboxCount() == 0 {
		t.Fatal("a mention while away must enter the inbox")
	}
	m = press(t, m, ctrlShiftKey('a'))
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if got := m.ctrl.SelectedTarget(); got != "#desktop" {
		t.Fatalf("SelectedTarget = %q, want #desktop", got)
	}
	if got := m.ctrl.Messages()[m.firstVisibleMessageRow()].Body; got != body {
		t.Fatalf("top body = %q, want the saved line %q", got, body)
	}
}

func TestInboxInviteEnterJoinsChannel(t *testing.T) {
	m, d := phase9DemoModel(t)
	before := m.ctrl.InboxCount()

	d.InjectOmarchy([]byte(":alice!u@h INVITE fred :#invited\r\n"))
	if got := m.ctrl.InboxCount(); got != before+1 {
		t.Fatalf("InboxCount = %d, want %d", got, before+1)
	}
	row := m.ctrl.InboxItems()[0]
	if row.Kind != "invite" {
		t.Fatalf("Kind = %q, want invite", row.Kind)
	}
	if row.Target != "#invited" {
		t.Fatalf("Target = %q, want #invited", row.Target)
	}

	m = press(t, m, ctrlShiftKey('a'))
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if !writtenOmarchyFramesContain(d, "JOIN #invited") {
		t.Fatalf("no JOIN #invited frame: %v", d.OmarchyTransport().WrittenFrames())
	}

	// The self-join consumes the invite row if activation had not already.
	d.InjectOmarchy([]byte(":fred!u@h JOIN :#invited\r\n"))
	if got := m.ctrl.InboxCount(); got != before {
		t.Fatalf("InboxCount after self-join = %d, want baseline %d", got, before)
	}
}

func TestInboxSelectingConversationClearsMatchingRows(t *testing.T) {
	m, d := phase9DemoModel(t)
	selectPhase9Ricing(t, m)
	before := m.ctrl.InboxCount()

	d.InjectOmarchy([]byte("@msgid=inbox-select-1 :anna!u@h PRIVMSG #omarchy :fred: select me\r\n"))
	if got := m.ctrl.InboxCount(); got != before+1 {
		t.Fatalf("InboxCount = %d, want %d", got, before+1)
	}

	m.ctrl.SelectConversation("omarchy", "#omarchy")
	if got := m.ctrl.InboxCount(); got != before {
		t.Fatalf("InboxCount after select = %d, want baseline %d", got, before)
	}
	if m.inboxVisible() {
		t.Fatal("selecting a conversation must leave the sheet closed")
	}
}

func TestInboxDeleteDismissesSelectedRow(t *testing.T) {
	m, d := phase9DemoModel(t)
	selectPhase9Ricing(t, m)

	d.InjectOmarchy([]byte("@msgid=inbox-delete-1 :anna!u@h PRIVMSG #omarchy :fred: delete me\r\n"))
	m = press(t, m, ctrlShiftKey('a'))
	before := m.ctrl.InboxCount()
	if before == 0 {
		t.Fatal("the inbox must have a row to dismiss")
	}

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyDelete})
	if got := m.ctrl.InboxCount(); got != before-1 {
		t.Fatalf("InboxCount after Delete = %d, want %d", got, before-1)
	}
	if !m.inboxVisible() {
		t.Fatal("Delete must keep the inbox sheet open")
	}
	if got := m.ctrl.SelectedTarget(); got != "#ricing" {
		t.Fatalf("SelectedTarget = %q, want #ricing (Delete must not activate)", got)
	}
}

func TestInboxDismissAboveSelectionKeepsHighlight(t *testing.T) {
	m, d := phase9DemoModel(t)
	selectPhase9Ricing(t, m)

	d.InjectOmarchy([]byte("@msgid=inbox-keep-1 :anna!u@h PRIVMSG #omarchy :fred: keep one\r\n"))
	d.InjectOmarchy([]byte("@msgid=inbox-keep-2 :anna!u@h PRIVMSG #omarchy :fred: keep two\r\n"))
	d.InjectOmarchy([]byte("@msgid=inbox-keep-3 :anna!u@h PRIVMSG #omarchy :fred: keep three\r\n"))

	m = press(t, m, ctrlShiftKey('a'))
	// The waiting list is newest first: Down moves from keep-3 to keep-2.
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyDown})
	if m.inbox.selected != 1 {
		t.Fatalf("selected = %d, want 1", m.inbox.selected)
	}

	m.dismissInboxRow(0)
	if m.inbox.selected != 0 {
		t.Fatalf("selected after dismissing above it = %d, want 0", m.inbox.selected)
	}
	items := m.ctrl.InboxItems()
	if items[0].MsgID != "inbox-keep-2" {
		t.Fatalf("items[0].MsgID = %q, want inbox-keep-2", items[0].MsgID)
	}
	if items[0].Target != "#omarchy" {
		t.Fatalf("items[0].Target = %q, want #omarchy", items[0].Target)
	}

	// A newer arrival lands at the front of the waiting list; the highlight
	// stays at its index (the list has no stable row identity across arrivals).
	d.InjectOmarchy([]byte("@msgid=inbox-keep-4 :anna!u@h PRIVMSG #omarchy :fred: keep four\r\n"))
	items = m.ctrl.InboxItems()
	if items[0].MsgID != "inbox-keep-4" {
		t.Fatalf("items[0].MsgID = %q, want inbox-keep-4", items[0].MsgID)
	}
	if m.inbox.selected != 0 {
		t.Fatalf("selected = %d, want 0 after the new arrival", m.inbox.selected)
	}
}

func TestIdentityFooterShowsNickPresenceAndInboxBadge(t *testing.T) {
	m, d := phase9DemoModel(t)
	// The seeded world waits on a #ricing mention and the anna direct.
	// Selecting #ricing consumes its row; dismiss clears the rest so the badge
	// starts at a known zero.
	selectPhase9Ricing(t, m)
	for m.ctrl.InboxCount() > 0 {
		m.ctrl.DismissInboxItem(0)
	}

	sidebar := m.sidebarView(sidebarWidth(m.width), m.bodyHeight())
	if !strings.Contains(sidebar, "fred") {
		t.Fatalf("identity footer must show the self nick fred:\n%s", sidebar)
	}
	presence := "offline"
	if m.ctrl.ConnectionStatus() == "Connected" {
		if m.ctrl.SelfAway() {
			presence = "away"
		} else {
			presence = "available"
		}
	}
	if !strings.Contains(sidebar, presence) {
		t.Fatalf("identity footer must show presence %q:\n%s", presence, sidebar)
	}
	if presence != "available" {
		t.Fatalf("seeded demo presence = %q, want available (fred is not away)", presence)
	}

	d.InjectOmarchy([]byte("@msgid=inbox-footer-1 :anna!u@h PRIVMSG #omarchy :fred: footer ping\r\n"))
	if got := m.ctrl.InboxCount(); got != 1 {
		t.Fatalf("InboxCount = %d, want 1", got)
	}
	sidebar = m.sidebarView(sidebarWidth(m.width), m.bodyHeight())
	if !strings.Contains(sidebar, "inbox 1") {
		t.Fatalf("identity footer missing inbox badge:\n%s", sidebar)
	}
	// The badge sits on the status row, below the nick's own row.
	rows := strings.Split(ansiPattern.ReplaceAllString(sidebar, ""), "\n")
	if last := rows[len(rows)-1]; !strings.Contains(last, "inbox 1") {
		t.Fatalf("the inbox pill must sit on the status row: %q", last)
	}
}
