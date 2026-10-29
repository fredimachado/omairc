package ui

// This file covers the jump-to-first-new affordance: the "↓ new" header marker
// that arms when rows arrive while the reader is scrolled up, and the Alt+U
// chord that lands on them. It mirrors TranscriptList's firstUnseenIndex
// jumpArmed, and jumpToUnseen.

import (
	"fmt"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// unseenDemoModel seeds the demo on a frozen clock, pads the selected #omarchy
// transcript past a viewport, and leaves it following the end.
func unseenDemoModel(t *testing.T) (*Model, *demo.DemoServer) {
	t.Helper()
	ctrl := controller.New()
	ctrl.SetClock(session.NewFakeClock(time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)))
	d := demo.New()
	if !d.Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	m := New(ctrl, nil)
	m = resizeModel(t, m, 118, 30)
	// The selected conversation is focused, so these pad lines accrue neither
	// unread nor a mark; they only make the transcript taller than the viewport
	// so Page Up can detach.
	for i := 0; i < 40; i++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(
			"@msgid=pad-%d :nora!u@h PRIVMSG #omarchy :pad line %d\r\n", i, i)))
	}
	m = updateMsg(t, m, NotifyMsg{})
	return m, d
}

// TestDetachedArrivalArmsMarker proves a row that arrives while the reader is
// scrolled up arms the marker, and Alt+U lands on it.
func TestDetachedArrivalArmsMarker(t *testing.T) {
	m, d := unseenDemoModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	before := m.transcriptRowTotal()

	d.InjectOmarchy([]byte("@msgid=arrival-1 :nora!u@h PRIVMSG #omarchy :new while reading\r\n"))
	m = updateMsg(t, m, NotifyMsg{})

	if !m.jumpArmed() {
		t.Fatal("an arrival while scrolled up must arm the jump")
	}
	if m.firstUnseenRow != before {
		t.Fatalf("firstUnseenRow = %d, want %d", m.firstUnseenRow, before)
	}
	if content := m.View().Content; !strings.Contains(content, "↓ new") {
		t.Fatalf("the header must show the jump marker:\n%s", content)
	}

	m = press(t, m, altKey('u'))
	if m.firstUnseenRow != -1 {
		t.Fatalf("Alt+U must consume the marker, firstUnseenRow = %d", m.firstUnseenRow)
	}
	if m.jumpArmed() {
		t.Fatal("the jump must be unarmed after landing on the first new row")
	}
}

// TestAltUReattachesWhenTheTargetIsTheEnd is the short-arrival repro: a few
// rows land while the reader is scrolled up, Alt+U puts the newest row on the
// bottom line, and the view follows the end. A later arrival stays followed
// and does not bring ↓ new back.
func TestAltUReattachesWhenTheTargetIsTheEnd(t *testing.T) {
	m, d := unseenDemoModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	for i := 0; i < 3; i++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(
			"@msgid=few-%d :nora!u@h PRIVMSG #omarchy :arrive %d\r\n", i, i)))
	}
	m = updateMsg(t, m, NotifyMsg{})
	if !m.jumpArmed() {
		t.Fatal("arrivals while scrolled up must arm the jump")
	}
	if !strings.Contains(m.View().Content, "↓ new") {
		t.Fatal("the header must show ↓ new before Alt+U")
	}
	if m.transcriptPlaceLabel() == "" {
		t.Fatal("a detached reader must show the place counter")
	}

	m = press(t, m, altKey('u'))
	if !m.transcriptFollowEnd {
		t.Fatal("Alt+U onto a target at the end must follow the end")
	}
	if m.transcriptScroll != 0 {
		t.Fatalf("scroll = %d, want 0 after reattach", m.transcriptScroll)
	}
	if m.firstUnseenRow != -1 {
		t.Fatalf("firstUnseenRow = %d, want cleared", m.firstUnseenRow)
	}
	if label := m.transcriptPlaceLabel(); label != "" {
		t.Fatalf("place after reattach = %q, want empty", label)
	}
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if m.jumpArmed() || strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must clear once the view follows the end:\n%s", plain)
	}
	if !strings.Contains(plain, "arrive 2") {
		t.Fatal("the newest arrival must be on screen after Alt+U")
	}

	d.InjectOmarchy([]byte("@msgid=few-later :nora!u@h PRIVMSG #omarchy :after reattach\r\n"))
	m = updateMsg(t, m, NotifyMsg{})
	if !m.transcriptFollowEnd || m.firstUnseenRow != -1 || m.jumpArmed() {
		t.Fatalf("later arrival follow=%v unseen=%d armed=%v, want followed and clear",
			m.transcriptFollowEnd, m.firstUnseenRow, m.jumpArmed())
	}
	if label := m.transcriptPlaceLabel(); label != "" {
		t.Fatalf("place after the later arrival = %q, want empty", label)
	}
	plain = ansiPattern.ReplaceAllString(m.View().Content, "")
	if strings.Contains(plain, "↓ new") {
		t.Fatalf("a later arrival must not bring ↓ new back:\n%s", plain)
	}
	if !strings.Contains(plain, "after reattach") {
		t.Fatal("the later arrival must be visible while following the end")
	}
}

// TestAltUKeepsMarkerWhileUnseenRowsRemainBelow is the 180x30 repro: two Page
// Ups, then 35 arrivals. Alt+U lands on the first unseen row with more of
// that run still below the view. ↓ new must stay, each further Alt+U must
// move toward the end, and the view follows the end once a landing reaches it.
func TestAltUKeepsMarkerWhileUnseenRowsRemainBelow(t *testing.T) {
	m, d := unseenDemoModel(t)
	m = resizeModel(t, m, 180, 30)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("two Page Ups must leave follow-the-end")
	}
	before := m.transcriptRowTotal()
	const arrivals = 35
	for i := 0; i < arrivals; i++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(
			"@msgid=tail-%d :nora!u@h PRIVMSG #omarchy :tail %d\r\n", i, i)))
	}
	m = updateMsg(t, m, NotifyMsg{})
	if m.firstUnseenRow != before {
		t.Fatalf("firstUnseenRow = %d, want %d", m.firstUnseenRow, before)
	}
	if !m.jumpArmed() {
		t.Fatal("arrivals while scrolled up must arm the jump")
	}
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(plain, "↓ new") {
		t.Fatal("the header must show ↓ new before Alt+U")
	}

	m = press(t, m, altKey('u'))
	if m.transcriptFollowEnd {
		t.Fatal("Alt+U partway through the unseen run must stay detached")
	}
	if m.firstUnseenRow != -1 {
		t.Fatalf("firstUnseenRow = %d, want the original marker consumed", m.firstUnseenRow)
	}
	if got := m.ctrl.Messages()[m.firstVisibleMessageRow()].Body; got != "tail 0" {
		t.Fatalf("top body = %q, want the first unseen row", got)
	}
	wantPlace := fmt.Sprintf("%d of %d", before+1, before+arrivals)
	if got := m.transcriptPlaceLabel(); got != wantPlace {
		t.Fatalf("place = %q, want %q", got, wantPlace)
	}
	last := fmt.Sprintf("tail %d", arrivals-1)
	plain = ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must stay while unseen rows remain below:\n%s", plain)
	}
	if strings.Contains(plain, last) {
		t.Fatalf("the newest row %q must stay below the fold:\n%s", last, plain)
	}

	top := m.firstVisibleMessageRow()
	for pressN := 0; pressN < 8; pressN++ {
		m = press(t, m, altKey('u'))
		if m.transcriptFollowEnd {
			break
		}
		next := m.firstVisibleMessageRow()
		if next <= top {
			t.Fatalf("Alt+U %d left the top at row %d, want it to advance past %d", pressN+2, next, top)
		}
		plain = ansiPattern.ReplaceAllString(m.View().Content, "")
		if !strings.Contains(plain, "↓ new") {
			t.Fatalf("↓ new cleared before the end, place %s:\n%s", m.transcriptPlaceLabel(), plain)
		}
		if strings.Contains(plain, last) {
			t.Fatalf("Alt+U %d showed %q before reattaching:\n%s", pressN+2, last, plain)
		}
		top = next
	}
	if !m.transcriptFollowEnd || m.transcriptScroll != 0 {
		t.Fatalf("follow=%v scroll=%d, want followed at the end", m.transcriptFollowEnd, m.transcriptScroll)
	}
	if label := m.transcriptPlaceLabel(); label != "" {
		t.Fatalf("place after reattach = %q, want empty", label)
	}
	plain = ansiPattern.ReplaceAllString(m.View().Content, "")
	if m.jumpArmed() || strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must clear once the view follows the end:\n%s", plain)
	}
	if !strings.Contains(plain, last) {
		t.Fatal("the newest arrival must be on screen after the view reattaches")
	}
}

// TestAltUStepsToTheNextRowThatIsNotFullyVisible proves a second Alt+U steps
// to the first row that is not fully on screen, instead of jumping to the end
// or paging by a fixed line count. The tail is more than two viewports, so
// that next row still leaves unseen rows below and the view stays detached
// with ↓ new.
func TestAltUStepsToTheNextRowThatIsNotFullyVisible(t *testing.T) {
	m, d := unseenDemoModel(t)
	m = resizeModel(t, m, 180, 30)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	before := m.transcriptRowTotal()
	extra := m.transcriptRowsHeight()*2 + 8
	if extra < 40 {
		extra = 40
	}
	for i := 0; i < extra; i++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(
			"@msgid=page-%d :nora!u@h PRIVMSG #omarchy :page %d\r\n", i, i)))
	}
	m = updateMsg(t, m, NotifyMsg{})
	if m.firstUnseenRow != before {
		t.Fatalf("firstUnseenRow = %d, want %d", m.firstUnseenRow, before)
	}

	m = press(t, m, altKey('u'))
	if m.transcriptFollowEnd {
		t.Fatal("the first Alt+U must stay detached")
	}
	if got := m.ctrl.Messages()[m.firstVisibleMessageRow()].Body; got != "page 0" {
		t.Fatalf("top body = %q, want the first unseen row", got)
	}
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must stay after the first landing:\n%s", plain)
	}
	continuation := m.firstRowBelowViewport()
	if continuation < 0 {
		t.Fatal("the first landing must leave a row that is not fully visible")
	}
	wantBody := m.ctrl.Messages()[continuation].Body
	top := m.firstVisibleMessageRow()

	m = press(t, m, altKey('u'))
	if m.transcriptFollowEnd {
		t.Fatal("the second Alt+U must stay detached while a viewport of unseen rows remains")
	}
	if m.transcriptScroll == 0 {
		t.Fatal("the second Alt+U must not clamp onto the end")
	}
	next := m.firstVisibleMessageRow()
	if next != continuation {
		t.Fatalf("second landing top = %d, want the row that was not fully visible (%d)", next, continuation)
	}
	if got := m.ctrl.Messages()[next].Body; got != wantBody {
		t.Fatalf("second landing body = %q, want %q", got, wantBody)
	}
	if next <= top {
		t.Fatalf("second landing top = %d, want it past %d", next, top)
	}
	last := fmt.Sprintf("page %d", extra-1)
	plain = ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must stay after the second landing:\n%s", plain)
	}
	if strings.Contains(plain, last) {
		t.Fatalf("the newest row %q must stay below the fold:\n%s", last, plain)
	}

	for pressN := 0; pressN < 8; pressN++ {
		m = press(t, m, altKey('u'))
		if m.transcriptFollowEnd {
			break
		}
		advanced := m.firstVisibleMessageRow()
		if advanced <= next {
			t.Fatalf("Alt+U left the top at row %d, want it past %d", advanced, next)
		}
		next = advanced
	}
	if !m.transcriptFollowEnd || m.transcriptScroll != 0 {
		t.Fatalf("follow=%v scroll=%d, want followed at the end", m.transcriptFollowEnd, m.transcriptScroll)
	}
	plain = ansiPattern.ReplaceAllString(m.View().Content, "")
	if m.jumpArmed() || strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must clear once the view follows the end:\n%s", plain)
	}
	if !strings.Contains(plain, last) {
		t.Fatal("the newest arrival must be on screen after the view reattaches")
	}
}

// TestAltUStaysDetachedWhenUnseenRowsFillTheViewport proves a long unseen tail
// still pins to the first new row and keeps the place counter.
func TestAltUStaysDetachedWhenUnseenRowsFillTheViewport(t *testing.T) {
	m, d := unseenDemoModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	before := m.transcriptRowTotal()
	extra := m.transcriptRowsHeight() + 8
	if extra < 12 {
		extra = 12
	}
	for i := 0; i < extra; i++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(
			"@msgid=many-%d :nora!u@h PRIVMSG #omarchy :buried %d\r\n", i, i)))
	}
	m = updateMsg(t, m, NotifyMsg{})
	if m.firstUnseenRow != before {
		t.Fatalf("firstUnseenRow = %d, want %d", m.firstUnseenRow, before)
	}

	m = press(t, m, altKey('u'))
	if m.transcriptFollowEnd {
		t.Fatal("Alt+U with more unseen rows than the viewport must stay detached")
	}
	if m.transcriptScroll == 0 {
		t.Fatal("the newest row must stay below the viewport")
	}
	if m.firstUnseenRow != -1 {
		t.Fatalf("firstUnseenRow = %d, want the marker consumed", m.firstUnseenRow)
	}
	label := m.transcriptPlaceLabel()
	if label == "" {
		t.Fatal("a detached pin must keep the place counter")
	}
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(plain, "buried 0") {
		t.Fatalf("the first unseen row must be on screen:\n%s", plain)
	}
	last := fmt.Sprintf("buried %d", extra-1)
	if strings.Contains(plain, last) {
		t.Fatalf("the newest unseen row %q must stay below the fold:\n%s", last, plain)
	}
	if !strings.Contains(plain, label) {
		t.Fatalf("composer edge missing the place counter %q:\n%s", label, plain)
	}
}

// TestJumpWithMarkGoesToEnd proves Alt+U falls back to the newest row when the
// only reason to jump is the "New messages" mark.
func TestJumpWithMarkGoesToEnd(t *testing.T) {
	m, d := unreadDemoModel(t)
	fillDesktopUnread(d)
	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "#desktop")
	})
	if m.transcriptFollowEnd {
		t.Fatal("precondition: the conversation must have landed on the mark")
	}
	if !m.jumpArmed() {
		t.Fatal("a New messages mark while detached must arm the jump")
	}

	m = press(t, m, altKey('u'))
	if !m.transcriptFollowEnd {
		t.Fatal("Alt+U with only a mark must land on the newest row")
	}
}

// TestJumpNoOpWhenFollowingEnd proves the chord does nothing when the reader is
// already at the newest row.
func TestJumpNoOpWhenFollowingEnd(t *testing.T) {
	m, _ := unseenDemoModel(t)
	if !m.transcriptFollowEnd {
		t.Fatal("precondition: the pad model starts following the end")
	}
	if m.jumpArmed() {
		t.Fatal("nothing is waiting below a reader who follows the end")
	}
	m = press(t, m, altKey('u'))
	if !m.transcriptFollowEnd || m.firstUnseenRow != -1 {
		t.Fatal("Alt+U must be a no-op while following the end")
	}
}

// TestArrivalWhileFollowingEndDoesNotArm proves the marker stays cleared for a
// reader at the bottom: the new row is already visible.
func TestArrivalWhileFollowingEndDoesNotArm(t *testing.T) {
	m, d := unseenDemoModel(t)
	d.InjectOmarchy([]byte("@msgid=arrival-2 :nora!u@h PRIVMSG #omarchy :at the bottom\r\n"))
	m = updateMsg(t, m, NotifyMsg{})
	if m.firstUnseenRow != -1 {
		t.Fatalf("firstUnseenRow = %d, want -1 while following the end", m.firstUnseenRow)
	}
	if m.jumpArmed() {
		t.Fatal("following the end must not arm the jump")
	}
}

// detachAndLandOnLongUnseen pages up, appends more than two viewports of
// one-line rows, and presses Alt+U once. It returns the model and the row
// where the unseen run continues.
func detachAndLandOnLongUnseen(t *testing.T) (*Model, int) {
	t.Helper()
	m, d := unseenDemoModel(t)
	m = resizeModel(t, m, 180, 30)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	extra := m.transcriptRowsHeight()*2 + 8
	if extra < 40 {
		extra = 40
	}
	for i := 0; i < extra; i++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(
			"@msgid=cont-%d :nora!u@h PRIVMSG #omarchy :cont %d\r\n", i, i)))
	}
	m = updateMsg(t, m, NotifyMsg{})
	m = press(t, m, altKey('u'))
	if m.transcriptFollowEnd || m.transcriptScroll == 0 {
		t.Fatal("the first Alt+U must stay detached")
	}
	continuation := m.firstRowBelowViewport()
	if continuation < 0 {
		t.Fatal("a long tail must leave a row that is not fully visible")
	}
	if m.unseenResumeRow != continuation {
		t.Fatalf("resume = %d, want the continuation row %d", m.unseenResumeRow, continuation)
	}
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(plain, "↓ new") {
		t.Fatal("↓ new must show while the continuation row is not fully visible")
	}
	return m, continuation
}

// TestPageDownPastResumeKeepsUnseenMarker proves a manual Page Down past the
// catch-up row keeps ↓ new while unseen rows remain. Alt+U alone is not enough:
// the arm check has to treat a continuation further down than the resume row as
// still armed.
func TestPageDownPastResumeKeepsUnseenMarker(t *testing.T) {
	m, continuation := detachAndLandOnLongUnseen(t)
	resume := m.unseenResumeRow
	if resume != continuation {
		t.Fatalf("resume = %d, want %d", resume, continuation)
	}

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgDown})
	if m.transcriptFollowEnd {
		t.Fatal("Page Down must stay detached while unseen rows remain")
	}
	below := m.firstRowBelowViewport()
	if below < 0 {
		t.Fatal("Page Down must leave a row that is not fully visible")
	}
	if below <= resume {
		t.Fatalf("first row below = %d, want it past the resume row %d", below, resume)
	}
	if m.unseenResumeRow != resume {
		t.Fatalf("resume = %d, want it kept at %d after Page Down", m.unseenResumeRow, resume)
	}
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if !m.jumpArmed() || !strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must stay after Page Down past the resume row:\n%s", plain)
	}
}

// TestAltUAfterCtrlHomeReturnsToTheUnseenRun proves Ctrl+Home hides ↓ new over
// history already read and the next Alt+U returns to the unseen run instead of
// walking that history from the top.
func TestAltUAfterCtrlHomeReturnsToTheUnseenRun(t *testing.T) {
	m, continuation := detachAndLandOnLongUnseen(t)
	body := m.ctrl.Messages()[continuation].Body

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyHome, Mod: tea.ModCtrl})
	if m.transcriptFollowEnd {
		t.Fatal("Ctrl+Home must leave follow-the-end")
	}
	if m.firstVisibleMessageRow() >= continuation {
		t.Fatalf("Ctrl+Home top = %d, want it above the continuation %d", m.firstVisibleMessageRow(), continuation)
	}
	if m.unseenResumeRow != continuation {
		t.Fatalf("resume = %d, want it kept at %d", m.unseenResumeRow, continuation)
	}
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must hide over history already read:\n%s", plain)
	}

	m = press(t, m, altKey('u'))
	if got := m.ctrl.Messages()[m.firstVisibleMessageRow()].Body; got != body {
		t.Fatalf("top body = %q, want the unseen continuation %q", got, body)
	}
	plain = ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(plain, "↓ new") {
		t.Fatal("↓ new must return once the continuation row is the first row not fully visible")
	}
}

// TestAltUAfterPageUpReturnsToTheUnseenRun proves Page Up hides ↓ new and the
// next Alt+U returns to the unseen run instead of stepping through the page
// the reader just moved into.
func TestAltUAfterPageUpReturnsToTheUnseenRun(t *testing.T) {
	m, continuation := detachAndLandOnLongUnseen(t)
	body := m.ctrl.Messages()[continuation].Body

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	if m.firstVisibleMessageRow() >= continuation {
		t.Fatalf("Page Up top = %d, want it above the continuation %d", m.firstVisibleMessageRow(), continuation)
	}
	if m.unseenResumeRow != continuation {
		t.Fatalf("resume = %d, want it kept at %d", m.unseenResumeRow, continuation)
	}
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must hide over history already read:\n%s", plain)
	}

	m = press(t, m, altKey('u'))
	if got := m.ctrl.Messages()[m.firstVisibleMessageRow()].Body; got != body {
		t.Fatalf("top body = %q, want the unseen continuation %q", got, body)
	}
}

// TestTypingFooterBelowTheViewDoesNotArmUnseenMarker proves ↓ new stays hidden
// when the only lines below the viewport are the direct-message typing footer.
func TestTypingFooterBelowTheViewDoesNotArmUnseenMarker(t *testing.T) {
	m, d := unseenDemoModel(t)
	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "anna")
	})
	for i := 0; i < 40; i++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(
			"@msgid=dm-pad-%d :anna!u@h PRIVMSG fred :dm pad %d\r\n", i, i)))
	}
	m = updateMsg(t, m, NotifyMsg{})
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	for i := 0; i < 35; i++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(
			"@msgid=dm-tail-%d :anna!u@h PRIVMSG fred :dm tail %d\r\n", i, i)))
	}
	// Each of anna's lines clears her typing hint. Put it back so the footer
	// is the only thing that can sit below a viewport full of message rows.
	d.InjectOmarchy([]byte("@+typing=active :anna!u@h TAGMSG fred\r\n"))
	m = updateMsg(t, m, NotifyMsg{})
	m = press(t, m, altKey('u'))
	if m.unseenResumeRow < 0 {
		t.Fatal("Alt+U through a long direct-message tail must arm the catch-up")
	}
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(plain, "↓ new") {
		t.Fatal("↓ new must show while an unseen message row remains below")
	}

	area := m.transcriptArea()
	height := m.transcriptRowsHeight()
	messageEnd := area.end()
	footer := len(area.lines) - messageEnd
	if footer < 1 {
		t.Fatal("the anna direct message must render a typing footer")
	}
	if messageEnd <= height {
		t.Fatalf("message lines = %d, viewport = %d, want the footer to be able to sit below a full viewport", messageEnd, height)
	}
	// Place the viewport by hand. Page Down would follow the end once the last
	// message row arrives, which hides the footer-only case this test locks.
	// Assigning the fields leaves the catch-up index in place, so a sticky
	// flag would still paint ↓ new.
	m.transcriptFollowEnd = false
	m.transcriptScroll = len(area.lines) - messageEnd
	if m.transcriptScroll <= 0 {
		t.Fatalf("scroll = %d, want the footer lines below the viewport", m.transcriptScroll)
	}
	if got := m.firstRowBelowViewport(); got >= 0 {
		t.Fatalf("first row below = %d, want none when only the typing footer is hidden", got)
	}
	plain = ansiPattern.ReplaceAllString(m.View().Content, "")
	if m.jumpArmed() || strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must hide when only the typing footer is below the view:\n%s", plain)
	}
}

// TestBufferSwitchDropsUnseenCatchUp proves leaving a conversation clears the
// catch-up, and returning does not bring ↓ new or the Alt+U walk back.
func TestBufferSwitchDropsUnseenCatchUp(t *testing.T) {
	m, _ := detachAndLandOnLongUnseen(t)
	if m.unseenResumeRow < 0 {
		t.Fatal("precondition: the catch-up must be armed")
	}

	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "#desktop")
	})
	if m.unseenResumeRow != -1 {
		t.Fatalf("unseenResumeRow = %d, want cleared on a buffer switch", m.unseenResumeRow)
	}

	m.switchSelection(func() {
		m.ctrl.SelectConversation("omarchy", "#omarchy")
	})
	if m.unseenResumeRow != -1 {
		t.Fatalf("unseenResumeRow = %d, want the catch-up to stay dropped", m.unseenResumeRow)
	}
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if strings.Contains(plain, "↓ new") {
		t.Fatalf("↓ new must stay off after the catch-up was dropped:\n%s", plain)
	}
	top := m.firstVisibleMessageRow()
	m = press(t, m, altKey('u'))
	if got := m.firstVisibleMessageRow(); got != top {
		t.Fatalf("top = %d, want unchanged %d: Alt+U must not walk a dropped catch-up", got, top)
	}
	if m.unseenResumeRow != -1 {
		t.Fatalf("unseenResumeRow = %d, want it to stay clear", m.unseenResumeRow)
	}
}

// TestAltUStepsPastAWrappedRowTallerThanTheViewport proves a wrapped row that
// is taller than the viewport does not trap Alt+U. The press that lands on it
// arms the following message, and the next press moves there.
func TestAltUStepsPastAWrappedRowTallerThanTheViewport(t *testing.T) {
	m, d := unseenDemoModel(t)
	m = resizeModel(t, m, 100, 30)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	if m.transcriptFollowEnd {
		t.Fatal("Page Up must leave follow-the-end")
	}
	tall := strings.TrimRight(strings.Repeat("word ", 400), " ")
	d.InjectOmarchy([]byte("@msgid=wrap-lead :nora!u@h PRIVMSG #omarchy :lead\r\n"))
	d.InjectOmarchy([]byte("@msgid=wrap-tall :nora!u@h PRIVMSG #omarchy :" + tall + "\r\n"))
	d.InjectOmarchy([]byte("@msgid=wrap-trailer :nora!u@h PRIVMSG #omarchy :trailer\r\n"))
	pad := m.transcriptRowsHeight() + 4
	for i := 0; i < pad; i++ {
		d.InjectOmarchy([]byte(fmt.Sprintf(
			"@msgid=wrap-after-%d :nora!u@h PRIVMSG #omarchy :after %d\r\n", i, i)))
	}
	m = updateMsg(t, m, NotifyMsg{})

	m = press(t, m, altKey('u'))
	if got := m.ctrl.Messages()[m.firstVisibleMessageRow()].Body; got != "lead" {
		t.Fatalf("top body = %q, want the first unseen row", got)
	}
	tallIndex := m.firstRowBelowViewport()
	if tallIndex < 0 {
		t.Fatal("the wrapped row must be the first row that is not fully visible")
	}
	if got := m.ctrl.Messages()[tallIndex].Body; got != tall {
		t.Fatalf("continuation = %q, want the wrapped row", got)
	}
	area := m.transcriptArea()
	if tallIndex >= len(area.heights) || area.heights[tallIndex] <= m.transcriptRowsHeight() {
		height := 0
		if tallIndex >= 0 && tallIndex < len(area.heights) {
			height = area.heights[tallIndex]
		}
		t.Fatalf("wrapped row height = %d, want it taller than the viewport %d", height, m.transcriptRowsHeight())
	}

	m = press(t, m, altKey('u'))
	if got := m.firstVisibleMessageRow(); got != tallIndex {
		t.Fatalf("top = %d, want the wrapped row %d", got, tallIndex)
	}
	if m.unseenResumeRow != tallIndex+1 {
		t.Fatalf("resume = %d, want the row after the wrapped one (%d)", m.unseenResumeRow, tallIndex+1)
	}

	m = press(t, m, altKey('u'))
	if got := m.firstVisibleMessageRow(); got != tallIndex+1 {
		t.Fatalf("top = %d, want the row after the wrapped one (%d), not another pass over the tall row", got, tallIndex+1)
	}
	if got := m.ctrl.Messages()[m.firstVisibleMessageRow()].Body; got != "trailer" {
		t.Fatalf("top body = %q, want trailer", got)
	}
}

// TestSettleUnseenResumeRefreshesAnchorSequence proves Alt+U during an older
// page load does not restore a stale catch-up after the splice. The snapshot
// taken when the page was requested must follow the post-Alt+U landing.
func TestSettleUnseenResumeRefreshesAnchorSequence(t *testing.T) {
	m, continuation := detachAndLandOnLongUnseen(t)
	staleBody := m.ctrl.Messages()[continuation].Body
	m.snapshotTranscriptAnchor()
	if m.unseenResumeSequenceAtAnchor < 0 {
		t.Fatal("the older-page snapshot must record the catch-up sequence")
	}

	m = press(t, m, altKey('u'))
	if m.transcriptFollowEnd || m.unseenResumeRow < 0 {
		t.Fatal("a second Alt+U must stay detached with a new catch-up row")
	}
	landed := m.unseenResumeRow
	if landed <= continuation {
		t.Fatalf("resume = %d, want it past the first continuation %d", landed, continuation)
	}
	landedBody := m.ctrl.Messages()[landed].Body
	if landedBody == staleBody {
		t.Fatal("the post-Alt+U catch-up must be a later row than the snapshotted one")
	}

	key := m.ctrl.Reducer().ConversationKey("omarchy", "#omarchy")
	when := time.Date(2026, 9, 26, 8, 0, 0, 0, time.UTC)
	m.ctrl.Apply(irc.HistoryEvent{
		Conversation: key,
		Target:       "#omarchy",
		Kind:         irc.HistoryChat,
		OlderPage:    true,
		Lines: []irc.ReplayLine{{
			Author:    "bob",
			Body:      "older while alt-u",
			Timestamp: when,
			MsgID:     irc.MsgID{Value: "older-while-alt-u"},
		}},
	})
	m.ctrl.Publish(irc.ViewNotify{Messages: true})
	m.noteTranscriptGrowth()

	if m.unseenResumeRow < 0 || m.unseenResumeRow >= len(m.ctrl.Messages()) {
		t.Fatalf("resume = %d after the prepend, want the post-Alt+U landing", m.unseenResumeRow)
	}
	if got := m.ctrl.Messages()[m.unseenResumeRow].Body; got != landedBody {
		t.Fatalf("resume body = %q, want post-Alt+U landing %q (stale was %q)", got, landedBody, staleBody)
	}
}

// TestStatusResumeArmCheckHeaderHeightParity proves the recursion guard that
// measures ↓ new arming keeps Status header height in line with the painted
// ↓ new band. An empty Status topic must not collapse to a nil header under
// the guard while the view paints two lines. Channel bands stay stable either
// way.
func TestStatusResumeArmCheckHeaderHeightParity(t *testing.T) {
	m := seededModel(t)
	channel := m.ctrl.SelectedTarget()
	m.ctrl.Apply(irc.TopicEvent{NetworkID: "omarchy", Channel: channel, Topic: ""})
	channelHeader := len(m.transcriptHeader())
	m.withinResumeArmCheck = true
	if got := len(m.transcriptHeader()); got != channelHeader {
		t.Fatalf("channel header guard=%d painted=%d, want a stable band", got, channelHeader)
	}
	m.withinResumeArmCheck = false

	m.toggleStatus()
	if !m.ctrl.ConsoleOpen() {
		t.Fatal("Status must be the open pane")
	}
	if topic := strings.TrimSpace(m.ctrl.PlainIrcText(m.ctrl.Topic())); topic != "" {
		t.Fatalf("Status Topic() = %q, want empty", topic)
	}
	now := time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)
	for i := 0; i < 40; i++ {
		m.ctrl.Console().Append(irc.Lifecycle("omarchy", irc.LogSeverityInfo, "test",
			fmt.Sprintf("status pad %d", i), now))
	}
	m.noteTranscriptGrowth()
	if m.transcriptRowTotal() < 20 {
		t.Fatalf("Status rows = %d, want a console taller than the viewport", m.transcriptRowTotal())
	}
	m.pinTranscriptToRow(0)
	if m.transcriptFollowEnd {
		t.Fatal("pinning the top Status row must stay detached when more rows remain")
	}
	m.firstUnseenRow = -1
	// Point the catch-up at the continuation the guarded measure sees so ↓ new
	// arms before the height fix. The painted band then uses a taller header.
	m.withinResumeArmCheck = true
	m.unseenResumeRow = m.firstRowBelowViewport()
	m.withinResumeArmCheck = false
	if m.unseenResumeRow < 0 {
		t.Fatal("Status must leave a row below the viewport")
	}
	if !m.jumpArmed() {
		t.Fatal("Status catch-up must arm ↓ new")
	}
	paintedHeader := len(m.transcriptHeader())
	paintedBelow := m.firstRowBelowViewport()
	if paintedHeader != 2 {
		t.Fatalf("painted Status header = %d, want the ↓ new band and blank", paintedHeader)
	}

	m.withinResumeArmCheck = true
	guardHeader := len(m.transcriptHeader())
	guardBelow := m.firstRowBelowViewport()
	m.withinResumeArmCheck = false
	if guardHeader != paintedHeader {
		t.Fatalf("guard Status header = %d, painted = %d", guardHeader, paintedHeader)
	}
	if guardBelow != paintedBelow {
		t.Fatalf("guard firstRowBelow = %d, painted = %d", guardBelow, paintedBelow)
	}
}

// TestShortcutSheetListsJumpToNew proves the chord is discoverable in the
// shortcuts sheet.
func TestShortcutSheetListsJumpToNew(t *testing.T) {
	m := resizeModel(t, seededModel(t), 140, 70)
	m = press(t, m, ctrlKey('/'))
	content := m.View().Content
	if !strings.Contains(content, "Alt+U") || !strings.Contains(content, "first new message") {
		t.Fatalf("shortcuts sheet missing the Alt+U row:\n%s", content)
	}
}
