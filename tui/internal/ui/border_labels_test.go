package ui

// Border titles and position counters live on the rounded frame. These tests
// pin the strings the panels and cards paint on those edges, and that the
// transcript place never repeats the "↓ new" marker.

import (
	"fmt"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
)

func plainLines(block string) []string {
	return strings.Split(ansiPattern.ReplaceAllString(block, ""), "\n")
}

func assertEdge(t *testing.T, block, title, label string) {
	t.Helper()
	lines := plainLines(block)
	if len(lines) < 2 {
		t.Fatalf("frame has %d lines, want a closed border:\n%s", len(lines), block)
	}
	top, bottom := lines[0], lines[len(lines)-1]
	if !strings.Contains(top, "╭") || !strings.Contains(top, "╮") {
		t.Fatalf("top edge = %q, want a closed rule", top)
	}
	if !strings.Contains(bottom, "╰") || !strings.Contains(bottom, "╯") {
		t.Fatalf("bottom edge = %q, want a closed rule", bottom)
	}
	if title != "" && !strings.Contains(top, title) {
		t.Fatalf("top edge = %q, want title %q", top, title)
	}
	if label == "" {
		if strings.Contains(bottom, " of ") {
			t.Fatalf("bottom edge = %q, want a solid rule", bottom)
		}
		return
	}
	if !strings.Contains(bottom, label) {
		t.Fatalf("bottom edge = %q, want label %q", bottom, label)
	}
	width := lipgloss.Width(lines[0])
	for index, line := range lines {
		if got := lipgloss.Width(line); got != width {
			t.Fatalf("line %d width = %d, want %d", index, got, width)
		}
	}
}

func TestMemberBorderShowsCountAndFocusedPosition(t *testing.T) {
	m := seededModel(t)
	if got, want := m.ctrl.PeopleCount(), len(m.ctrl.Members()); got != want || got != 12 {
		t.Fatalf("PeopleCount = %d, members = %d, want both 12", got, want)
	}
	outerW, outerH := m.membersColumnWidth(), m.bodyHeight()
	unfocused := m.framedColumnLabeled(m.membersView, outerW, outerH, false, m.memberFrameTitle(), m.memberFrameLabel())
	assertEdge(t, unfocused, "Online 12", "")
	if lipgloss.Width(unfocused) != outerW || lipgloss.Height(unfocused) != outerH {
		t.Fatalf("unfocused panel = %dx%d, want %dx%d", lipgloss.Width(unfocused), lipgloss.Height(unfocused), outerW, outerH)
	}

	m = press(t, m, ctrlShiftKey('p'))
	if !m.memberFocus {
		t.Fatal("Ctrl+Shift+P must focus the member list")
	}
	m.memberIndex = phase8MemberIndex(t, m, "anna")
	focused := m.framedColumnLabeled(m.membersView, outerW, outerH, m.memberFocus, m.memberFrameTitle(), m.memberFrameLabel())
	assertEdge(t, focused, "Online 12", "2 of 12")
	if lipgloss.Width(focused) != outerW || lipgloss.Height(focused) != outerH {
		t.Fatalf("focused panel = %dx%d, want the same outer size %dx%d", lipgloss.Width(focused), lipgloss.Height(focused), outerW, outerH)
	}

	m.ctrl.SelectConversation("omarchy", "#desktop")
	if title := m.memberFrameTitle(); title != fmt.Sprintf("Online %d", m.ctrl.PeopleCount()) {
		t.Fatalf("desktop title = %q, PeopleCount = %d", title, m.ctrl.PeopleCount())
	}
}

func TestOverlayCardsPutCountsOnTheBorder(t *testing.T) {
	m := seededModel(t)

	m.openJump()
	entries := m.jumpEntries()
	if len(entries) < 2 {
		t.Fatalf("jump entries = %d, want at least 2", len(entries))
	}
	assertEdge(t, m.jumpCard(m.width), overlayCountTitle("Jump", len(entries)), "1 of "+fmt.Sprint(len(entries)))
	m.jump.selected = 1
	assertEdge(t, m.jumpCard(m.width), overlayCountTitle("Jump", len(entries)), "2 of "+fmt.Sprint(len(entries)))
	m.jump.input.SetValue("zzz-no-match")
	assertEdge(t, m.jumpCard(m.width), "Jump 0", "")
	m.closeJump()

	m.openNickJump()
	nicks := m.nickEntries()
	if len(nicks) == 0 {
		t.Fatal("nick entries are empty")
	}
	assertEdge(t, m.nickCard(m.width), overlayCountTitle("Nick", len(nicks)), "1 of "+fmt.Sprint(len(nicks)))
	m.closeNickJump()

	m.toggleLink()
	links := len(m.linkMatches())
	label := ""
	if links > 0 {
		label = "1 of " + fmt.Sprint(links)
	}
	assertEdge(t, m.linkCard(m.width), overlayCountTitle("Links", links), label)
	m.closeLink()

	m.toggleInbox()
	inbox := len(m.inboxEntries())
	label = ""
	if inbox > 0 {
		label = "1 of " + fmt.Sprint(inbox)
	}
	assertEdge(t, m.inboxCard(m.width), overlayCountTitle("Inbox", inbox), label)
	m.closeInbox()

	m.composer.Reset()
	m.composer.SetValue("/list")
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	rows := len(m.channelListSnapshot().Rows)
	if rows == 0 {
		t.Fatal("/list returned no rows")
	}
	assertEdge(t, m.channelListCard(m.width), channelListFrameTitle(m.channelListSnapshot()), "1 of "+fmt.Sprint(rows))
}

func TestTranscriptPlaceSitsOnTheComposerAndSkipsTheNewMarker(t *testing.T) {
	m, d := unseenDemoModel(t)
	if label := m.transcriptPlaceLabel(); label != "" {
		t.Fatalf("place while following = %q, want empty", label)
	}
	following := plainLines(m.composerView())
	if strings.Contains(following[0], " of ") || strings.Contains(strings.Join(following, "\n"), "↓ new") {
		t.Fatalf("composer while following = %q, want a solid top edge", following[0])
	}

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgUp})
	total := len(m.ctrl.Messages())
	want := fmt.Sprintf("%d of %d", m.firstVisibleMessageRow()+1, total)
	if got := m.transcriptPlaceLabel(); got != want {
		t.Fatalf("place = %q, want %q", got, want)
	}
	scrolled := plainLines(m.composerView())
	if !strings.Contains(scrolled[0], want) {
		t.Fatalf("composer top = %q, want %q", scrolled[0], want)
	}
	if strings.Contains(strings.Join(scrolled, "\n"), "↓ new") {
		t.Fatal("the composer place must not repeat the ↓ new marker")
	}

	d.InjectOmarchy([]byte("@msgid=place-1 :nora!u@h PRIVMSG #omarchy :new while reading\r\n"))
	m = updateMsg(t, m, NotifyMsg{})
	if !m.jumpArmed() {
		t.Fatal("an arrival while scrolled up must arm ↓ new")
	}
	content := ansiPattern.ReplaceAllString(m.View().Content, "")
	if !strings.Contains(content, "↓ new") {
		t.Fatal("the header must still show ↓ new")
	}
	place := m.transcriptPlaceLabel()
	if place == "" || place == "↓ new" || strings.Contains(place, "↓") {
		t.Fatalf("place beside the marker = %q, want a numeric position", place)
	}
	if !strings.Contains(plainLines(m.composerView())[0], place) {
		t.Fatal("the composer top edge lost the place after the arrival")
	}
}
