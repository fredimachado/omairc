package ui

// This file is the Phase 8 member-panel parity suite for the Bubble Tea shell:
// the ONLINE heading, the Ctrl+Shift+M toggle, presence dots, away dimming,
// the bot mark, typing ellipsis, status sublines, and Enter-on-member opening a
// direct message. It reuses the demo-seeded harness in phase5_test.go and the
// key helpers in phase6_test.go.

import (
	"strings"
	"testing"

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

	// Typing ellipsis: only anna is typing on #omarchy.
	if !strings.Contains(annaLines[0], m.styles.MemberTyping.Render("...")) {
		t.Fatalf("anna must show the typing ellipsis: %q", annaLines[0])
	}
	if strings.Contains(daxLines[0], m.styles.MemberTyping.Render("...")) {
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

func TestAwayMemberIsDimmed(t *testing.T) {
	m := seededModel(t)

	teoLine := phase8MemberLine(t, m, "teo")[0]
	if !strings.Contains(teoLine, m.styles.MemberAway.Render("+teo")) {
		t.Fatalf("away member teo is not dimmed: %q", teoLine)
	}

	annaLine := phase8MemberLine(t, m, "anna")[0]
	if strings.Contains(annaLine, m.styles.MemberAway.Render("&anna")) {
		t.Fatalf("available member anna must not be dimmed: %q", annaLine)
	}
	if !strings.Contains(annaLine, m.styles.Conversation.Render("&anna")) {
		t.Fatalf("anna's label is not the plain style: %q", annaLine)
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
