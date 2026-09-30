package ui

// This file is the Phase 8 member-panel parity suite for the Bubble Tea shell:
// the ONLINE heading, the Ctrl+Shift+M toggle, presence dots, away dimming,
// the bot mark, typing ellipsis, status sublines, and Enter-on-member opening a
// direct message. It reuses the demo-seeded harness in phase5_test.go and the
// key helpers in phase6_test.go.

import (
	"strings"
	"testing"

	"charm.land/bubbles/v2/spinner"
	tea "charm.land/bubbletea/v2"
)

// phase8MemberIndex returns the member list index for nick, or fails the test.
func phase8MemberIndex(t *testing.T, m *Model, nick string) int {
	t.Helper()
	for index, member := range m.ctrl.Members() {
		if member.Nick == nick {
			return index
		}
	}
	t.Fatalf("%s not found in the member list", nick)
	return -1
}

// phase8MemberLine renders one member's row through the view (not the whole
// column), so a test can make style claims without ANSI-matching the terminal.
func phase8MemberLine(t *testing.T, m *Model, nick string) []string {
	t.Helper()
	index := phase8MemberIndex(t, m, nick)
	return m.memberRow(index, m.ctrl.Members()[index])
}

func TestMembersHeadingShowsOnlineCount(t *testing.T) {
	m := seededModel(t)
	if !strings.Contains(m.View().Content, "ONLINE - 12") {
		t.Fatalf("seeded #omarchy panel missing ONLINE - 12:\n%s", m.View().Content)
	}
}

func TestToggleMembersHidesAndReopens(t *testing.T) {
	m := seededModel(t)
	if !strings.Contains(m.View().Content, "ONLINE - 12") {
		t.Fatalf("panel must start open:\n%s", m.View().Content)
	}

	m = press(t, m, ctrlShiftKey('m'))
	if strings.Contains(m.View().Content, "ONLINE - 12") {
		t.Fatal("Ctrl+Shift+M must hide the member column")
	}

	m = press(t, m, ctrlShiftKey('m'))
	if !strings.Contains(m.View().Content, "ONLINE - 12") {
		t.Fatal("a second Ctrl+Shift+M must restore the member column")
	}

	// Hiding clears the member focus so a stale cursor does not swallow keys.
	m = press(t, m, ctrlShiftKey('p'))
	if !m.memberFocus {
		t.Fatal("Ctrl+Shift+P must focus the member list")
	}
	m = press(t, m, ctrlShiftKey('m'))
	if m.memberFocus {
		t.Fatal("hiding the panel must clear the member focus")
	}
}

func TestFocusMembersReopensHiddenPanel(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, ctrlShiftKey('m'))
	if !m.membersHidden {
		t.Fatal("Ctrl+Shift+M must set the hidden state")
	}
	m = press(t, m, ctrlShiftKey('p'))
	if m.membersHidden {
		t.Fatal("Ctrl+Shift+P must reopen a hidden member panel")
	}
	if !m.memberFocus {
		t.Fatal("Ctrl+Shift+P must focus the member list")
	}
}

func TestToggleMembersNoOpOnDirectMessage(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, ctrlShiftKey('k'))
	m.nick.input.SetValue("anna")
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if got := m.View().WindowTitle; got != "anna - Omairc" {
		t.Fatalf("after selecting anna title = %q, want %q", got, "anna - Omairc")
	}

	m = press(t, m, ctrlShiftKey('m'))
	if m.membersHidden {
		t.Fatal("Ctrl+Shift+M must be a no-op on a direct message")
	}
	if got := m.View().WindowTitle; got != "anna - Omairc" {
		t.Fatalf("title after the DM toggle = %q, want it unchanged", got)
	}
	if strings.Contains(m.View().Content, "ONLINE -") {
		t.Fatal("a direct message must not render a member column")
	}
}

func TestMemberOrderFollowsPrefixRank(t *testing.T) {
	m := seededModel(t)
	panel := m.membersView(membersWidth, m.bodyHeight())
	labels := []string{
		"~fred", "&anna", "@dax", "@mira", "%kai", "+teo",
		"ivy", "lena", "max", "nora", "sam", "sol",
	}
	previous := -1
	for _, label := range labels {
		at := strings.Index(panel, label)
		if at < 0 {
			t.Fatalf("member %q missing from the panel:\n%s", label, panel)
		}
		if at <= previous {
			t.Fatalf("member %q rendered out of rank order (at %d, previous %d):\n%s",
				label, at, previous, panel)
		}
		previous = at
	}
}

func TestMemberChromePresenceBotTypingAndStatus(t *testing.T) {
	m := seededModel(t)

	annaLines := phase8MemberLine(t, m, "anna")
	daxLines := phase8MemberLine(t, m, "dax")
	teoLines := phase8MemberLine(t, m, "teo")
	if len(annaLines) == 0 || len(daxLines) == 0 {
		t.Fatal("anna and dax must each render a member row")
	}

	// Presence dots: anna is available (green), teo is away (amber). Both are
	// shown because the demo negotiated away-notify.
	if !strings.Contains(annaLines[0], m.styles.MemberPresenceOnline.Render("●")) {
		t.Fatalf("anna's presence dot is not the online style: %q", annaLines[0])
	}
	if !strings.Contains(teoLines[0], m.styles.MemberPresenceAway.Render("●")) {
		t.Fatalf("teo's presence dot is not the away style: %q", teoLines[0])
	}

	// Bot mark: dax is a bot (seeded PacketBot), anna is not.
	if !strings.Contains(daxLines[0], m.styles.MemberBot.Render("⌬")) {
		t.Fatalf("dax must show the bot mark: %q", daxLines[0])
	}
	if strings.Contains(annaLines[0], "⌬") {
		t.Fatalf("anna must not show a bot mark: %q", annaLines[0])
	}

	// Typing dots: only anna is typing on #omarchy.
	if !strings.Contains(annaLines[0], m.typingDots()) {
		t.Fatalf("anna must show the typing dots: %q", annaLines[0])
	}
	if strings.Contains(daxLines[0], m.typingDots()) {
		t.Fatalf("dax must not show typing dots: %q", daxLines[0])
	}

	// Status subline: anna has "writing docs"; teo has none.
	if len(annaLines) < 2 || !strings.Contains(annaLines[1], "writing docs") {
		t.Fatalf("anna's status subline missing writing docs: %#v", annaLines)
	}
	if len(daxLines) < 2 || !strings.Contains(daxLines[1], "on #desktop") {
		t.Fatalf("dax's status subline missing on #desktop: %#v", daxLines)
	}
	if len(teoLines) != 1 {
		t.Fatalf("teo has no status, want a single line, got %#v", teoLines)
	}
}

// TestTypingIndicatorAnimatesWithTheSpinner proves the old static ellipsis is
// now a Bubbles Points frame: one tick advances the member-panel dots, and the
// sidebar pulse follows as the frame's first cell, so all three surfaces share
// one beat.
func TestTypingIndicatorAnimatesWithTheSpinner(t *testing.T) {
	m := seededModel(t)
	if !m.typingSpinnerRunning() {
		t.Fatal("the seeded #omarchy has anna typing, so the typing spinner must run")
	}
	first := m.typingDots()

	updated, cmd := m.Update(spinner.TickMsg{ID: m.typingSpinner.ID()})
	m = updated.(*Model)
	if cmd == nil {
		t.Fatal("a typing tick must schedule the next frame")
	}
	second := m.typingDots()
	if second == first {
		t.Fatalf("typing dots did not advance past %q", first)
	}

	anna := phase8MemberLine(t, m, "anna")
	if !strings.Contains(anna[0], second) {
		t.Fatalf("anna's row = %q, want the advanced frame %q", anna[0], second)
	}
	if got, want := m.typingPulse(), truncateLine(second, 1); got != want {
		t.Fatalf("sidebar pulse = %q, want the frame's first cell %q", got, want)
	}
}

// TestTypingDotsFollowThePointsSpinner walks the Bubbles Points frames and
// proves the member/transcript glyph is exactly the library frame, while the
// sidebar pulse is that frame's first cell.
func TestTypingDotsFollowThePointsSpinner(t *testing.T) {
	m := seededModel(t)
	for index, frame := range spinner.Points.Frames {
		stamped := m.styles.MemberTyping.Render(frame)
		if got := m.typingDots(); got != stamped {
			t.Fatalf("frame %d dots = %q, want %q", index, got, stamped)
		}
		if got, want := m.typingPulse(), truncateLine(stamped, 1); got != want {
			t.Fatalf("frame %d pulse = %q, want the first cell %q", index, got, want)
		}
		updated, _ := m.Update(spinner.TickMsg{ID: m.typingSpinner.ID()})
		m = updated.(*Model)
	}
}

// TestSidebarDirectTypingRendersThePulse pins the third typing surface: the
// seeded anna direct row carries the animated one-cell pulse.
func TestSidebarDirectTypingRendersThePulse(t *testing.T) {
	m := seededModel(t)
	rendered := ""
	for _, row := range m.ctrl.Conversations() {
		if row.Direct && row.Typing {
			rendered = m.conversationRow(row)
			break
		}
	}
	if rendered == "" {
		t.Fatal("the seeded demo must mark a direct row as typing")
	}
	if !strings.Contains(rendered, m.typingPulse()) {
		t.Fatalf("sidebar row = %q, want the typing pulse %q", rendered, m.typingPulse())
	}
}

// TestMemberNicksUseTheTranscriptPalette pins the feature: a nick keeps one
// palette color everywhere. The member panel's label carries the same nickColor
// hash as the transcript byline, so "mira" in the member list is the same color
// as "mira" in the transcript. The label's PREFIX ("@" ) is colored with the
// nick too, because the row renders the member's whole label.
func TestMemberNicksUseTheTranscriptPalette(t *testing.T) {
	m := seededModel(t)
	for _, nick := range []string{"mira", "anna", "dax"} {
		if got, want := m.memberNickStyle(nick, false, false).GetForeground(),
			m.nickStyle(nick).GetForeground(); got != want {
			t.Fatalf("member %q color = %v, want the transcript byline color %v", nick, got, want)
		}
		member := m.ctrl.Members()[phase8MemberIndex(t, m, nick)]
		line := phase8MemberLine(t, m, nick)[0]
		if want := m.memberNickStyle(nick, false, false).Render(member.Label); !strings.Contains(line, want) {
			t.Fatalf("member %q label is not rendered in its nick color: %q", nick, line)
		}
	}
}

// TestAwayMemberIsDimmed pins that an away member keeps its nick hue but is
// washed toward the page background, while an available member keeps the full
// palette color.
func TestAwayMemberIsDimmed(t *testing.T) {
	m := seededModel(t)

	teoLine := phase8MemberLine(t, m, "teo")[0]
	if !strings.Contains(teoLine, m.memberNickStyle("teo", false, true).Render("+teo")) {
		t.Fatalf("away member teo is not dimmed: %q", teoLine)
	}
	// The dim is still the nick's hue, not a flat gray.
	if dimmed, full := m.memberNickStyle("teo", false, true).GetForeground(),
		m.memberNickStyle("teo", false, false).GetForeground(); dimmed == full {
		t.Fatal("an away nick must be mixed away from its full palette color")
	}

	annaLine := phase8MemberLine(t, m, "anna")[0]
	if strings.Contains(annaLine, m.memberNickStyle("anna", false, true).Render("&anna")) {
		t.Fatalf("available member anna must not be dimmed: %q", annaLine)
	}
	if !strings.Contains(annaLine, m.memberNickStyle("anna", false, false).Render("&anna")) {
		t.Fatalf("anna's label is not her full nick color: %q", annaLine)
	}
}

func TestMemberActivationOpensDirectMessageAndIgnoresSelf(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, ctrlShiftKey('p'))
	m.memberIndex = phase8MemberIndex(t, m, "mira")
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if got := m.View().WindowTitle; got != "mira - Omairc" {
		t.Fatalf("Enter on mira title = %q, want %q", got, "mira - Omairc")
	}

	// Enter on our own row is a no-op.
	self := seededModel(t)
	self = press(t, self, ctrlShiftKey('p'))
	self.memberIndex = phase8MemberIndex(t, self, "fred")
	self = press(t, self, tea.KeyPressMsg{Code: tea.KeyEnter})
	if got := self.View().WindowTitle; got != "#omarchy · irc.example · fred - Omairc" {
		t.Fatalf("Enter on fred title = %q, want the channel unchanged", got)
	}
}

// TestMembersPanelHeadingRule covers the restyled card content: the
// "ONLINE - N" heading is followed by a hairline rule before the member rows.
func TestMembersPanelHeadingRule(t *testing.T) {
	m := seededModel(t)
	panel := m.membersView(membersWidth, m.bodyHeight())
	rule := m.styles.Divider.Render(strings.Repeat("─", membersWidth))
	if !strings.Contains(panel, rule) {
		t.Fatalf("member panel missing the heading rule %q:\n%s", rule, panel)
	}
}

// TestMemberRowFocusAccentsOnlyTheFocusedRow covers the focus accent: the
// focused member's first line starts with the accent bar, every other row
// leaves that gutter blank so the rows never shift when the cursor moves.
func TestMemberRowFocusAccentsOnlyTheFocusedRow(t *testing.T) {
	m := seededModel(t)
	if line := phase8MemberLine(t, m, "anna")[0]; strings.Contains(line, "▌") {
		t.Fatalf("unfocused anna row must not carry an accent bar: %q", line)
	}

	m = press(t, m, ctrlShiftKey('p'))
	m.memberIndex = phase8MemberIndex(t, m, "anna")
	focused := phase8MemberLine(t, m, "anna")[0]
	if !strings.HasPrefix(focused, m.memberFocusBar()) {
		t.Fatalf("focused anna row must start with the accent bar, got %q", focused)
	}
	// Every non-focused row keeps a blank gutter of the same width.
	if other := phase8MemberLine(t, m, "dax")[0]; strings.Contains(other, "▌") {
		t.Fatalf("unfocused dax row must not carry an accent bar: %q", other)
	}
}

// TestMemberStatusSublineAlignsUnderTheLabel covers the status subline restyle:
// it stays a separate muted line, blanked past the focus gutter and (with
// away-notify on) the presence dot, so it hangs under the nick.
func TestMemberStatusSublineAlignsUnderTheLabel(t *testing.T) {
	m := seededModel(t)
	anna := phase8MemberLine(t, m, "anna")
	if len(anna) < 2 {
		t.Fatalf("anna must render a status subline: %#v", anna)
	}
	want := m.styles.MemberStatus.Render("    · writing docs")
	if anna[1] != want {
		t.Fatalf("anna's status subline = %q, want the indented muted line %q", anna[1], want)
	}
}

// shortMemberPanel renders the member column through a short viewport, so the
// seeded twelve-nick roster overflows and the window has to scroll.
func shortMemberPanel(t *testing.T, m *Model) string {
	t.Helper()
	return m.membersView(membersWidth, 6)
}

// TestMemberListRevealsFocusedMember covers the member window: a roster taller
// than the panel shows the tail once End moves the cursor there, instead of
// leaving the focused row off-screen.
func TestMemberListRevealsFocusedMember(t *testing.T) {
	m := seededModel(t)
	last := m.ctrl.Members()[len(m.ctrl.Members())-1].Nick
	if got := shortMemberPanel(t, m); strings.Contains(got, last) {
		t.Fatalf("the tail of the roster should start below the fold:\n%s", got)
	}

	m = press(t, m, ctrlShiftKey('p'))
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnd})
	if m.memberIndex != len(m.ctrl.Members())-1 {
		t.Fatalf("End member index = %d, want the last index", m.memberIndex)
	}
	panel := shortMemberPanel(t, m)
	if !strings.Contains(panel, last) {
		t.Fatalf("End must scroll the last member %q into view:\n%s", last, panel)
	}
	if !strings.Contains(panel, m.memberFocusBar()) {
		t.Fatalf("the focused member's accent bar must be on screen:\n%s", panel)
	}
}

// TestMemberListPageDownScrollsWindow covers the paging path: Page Down moves
// the cursor and the window follows, so the focused row never scrolls away.
func TestMemberListPageDownScrollsWindow(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, ctrlShiftKey('p'))
	before := shortMemberPanel(t, m)

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyPgDown})
	after := shortMemberPanel(t, m)
	if before == after {
		t.Fatalf("Page Down must scroll the member window:\n%s", before)
	}
	if !strings.Contains(after, m.memberFocusBar()) {
		t.Fatalf("the focused member must stay visible after paging:\n%s", after)
	}
}

// TestMemberListShortRosterStaysAtTop pins the resting state: when every member
// fits, moving the cursor does not introduce blank rows or clip the heading.
func TestMemberListShortRosterStaysAtTop(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, ctrlShiftKey('p'))
	m.memberIndex = phase8MemberIndex(t, m, "anna")
	panel := m.membersView(membersWidth, 40)
	if !strings.Contains(panel, "ONLINE - 12") {
		t.Fatalf("the heading must stay pinned:\n%s", panel)
	}
	for _, nick := range []string{"fred", "anna", "max"} {
		if !strings.Contains(panel, nick) {
			t.Fatalf("a roster that fits must show %q:\n%s", nick, panel)
		}
	}
}
