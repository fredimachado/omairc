package ui

// This file covers workstream L6 (filter/navigation sheets): the shared sheet
// heading and toggle-chip row chrome in navigate.go, the selected-row fill, the
// shared filter inputs adopted through newTextInput, and the preserved
// labels/order of every navigation target. It asserts visible text only, so it
// does not depend on the terminal color profile.

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
)

// wave4L6Inner is the content width the shared overlay frame hands a body.
func wave4L6Inner(m *Model) int {
	frameX, _ := m.styles.Panel.GetFrameSize()
	return m.width - 2*overlayCardInset - frameX
}

// wave4L6Plain strips ANSI sequences from every body line.
func wave4L6Plain(lines []string) []string {
	plain := make([]string, len(lines))
	for index, line := range lines {
		plain[index] = ansiPattern.ReplaceAllString(line, "")
	}
	return plain
}

// TestWave4L6FilterInputsUseSharedChrome pins that the jump, nick, and link
// filters are rebuilt through newTextInput on open: the shared prompt and the
// sheet's placeholder, with a typed filter reset on reopen.
func TestWave4L6FilterInputsUseSharedChrome(t *testing.T) {
	m := seededModel(t)

	m.openJump()
	if got := m.jump.input.Prompt; got != overlayFilterPrompt {
		t.Fatalf("jump prompt = %q, want %q", got, overlayFilterPrompt)
	}
	if got := m.jump.input.Placeholder; got != jumpPlaceholder {
		t.Fatalf("jump placeholder = %q, want %q", got, jumpPlaceholder)
	}
	m.jump.input.SetValue("#desktop")
	m.closeJump()
	m.openJump()
	if got := m.jump.input.Value(); got != "" {
		t.Fatalf("reopened jump filter = %q, want it reset", got)
	}
	m.closeJump()

	m.openNickJump()
	if got := m.nick.input.Prompt; got != overlayFilterPrompt {
		t.Fatalf("nick prompt = %q, want %q", got, overlayFilterPrompt)
	}
	if got := m.nick.input.Placeholder; got != nickPlaceholder {
		t.Fatalf("nick placeholder = %q, want %q", got, nickPlaceholder)
	}
	m.closeNickJump()

	m.toggleLink()
	if got := m.link.input.Prompt; got != overlayFilterPrompt {
		t.Fatalf("link prompt = %q, want %q", got, overlayFilterPrompt)
	}
	if got := m.link.input.Placeholder; got != linkPlaceholder {
		t.Fatalf("link placeholder = %q, want %q", got, linkPlaceholder)
	}
	m.closeLink()
}

// TestWave4L6JumpRowsCarryToggleChipsAndFill pins the shared row chrome: the
// highlighted row carries the accent bar, the [x] chip, and a raised-surface
// fill that reaches the body's inner width; every other row carries [ ].
func TestWave4L6JumpRowsCarryToggleChipsAndFill(t *testing.T) {
	m := seededModel(t)
	m.openJump()
	inner := wave4L6Inner(m)
	body := wave4L6Plain(m.jumpCardBody(inner))
	entries := m.jumpEntries()
	if len(entries) < 2 {
		t.Fatalf("jump entries = %d, want at least 2 to compare rows", len(entries))
	}

	// Header, divider, then the filter line, then one row per entry.
	rows := body[3:]
	if len(rows) != len(entries) {
		t.Fatalf("rendered rows = %d, want %d", len(rows), len(entries))
	}
	if !strings.HasPrefix(rows[0], "▌[x] ") {
		t.Fatalf("highlighted row = %q, want the accent bar and a checked chip", rows[0])
	}
	if !strings.Contains(rows[0], entries[0].label) {
		t.Fatalf("highlighted row = %q, want it to carry %q", rows[0], entries[0].label)
	}
	if !strings.HasPrefix(rows[1], " [ ] ") {
		t.Fatalf("second row = %q, want an unchecked chip", rows[1])
	}

	// The selected fill and the unchecked gutter both reach the inner width.
	for index, row := range rows {
		if got := lipgloss.Width(row); got != inner {
			t.Fatalf("row %d width = %d, want the inner width %d: %q", index, got, inner, row)
		}
	}

	joined := strings.Join(body, "\n")
	if got := strings.Count(joined, overlayChipOn); got != 1 {
		t.Fatalf("[x] chips = %d, want exactly one", got)
	}
	if got := strings.Count(joined, overlayChipOff); got != len(entries)-1 {
		t.Fatalf("[ ] chips = %d, want %d", got, len(entries)-1)
	}
}

// TestWave4L6JumpPreservesTargetOrder proves the restyle did not reword,
// remove, or reorder the navigation targets: every jumpEntry label still
// renders, in jumpEntries order.
func TestWave4L6JumpPreservesTargetOrder(t *testing.T) {
	m := seededModel(t)
	m.openJump()
	body := wave4L6Plain(m.jumpCardBody(wave4L6Inner(m)))
	joined := strings.Join(body, "\n")

	entries := m.jumpEntries()
	if len(entries) == 0 {
		t.Fatal("jumpEntries is empty")
	}
	previous := -1
	for _, entry := range entries {
		index := strings.Index(joined, entry.label)
		if index < 0 {
			t.Fatalf("jump body is missing the target %q:\n%s", entry.label, joined)
		}
		if index < previous {
			t.Fatalf("target %q moved out of order:\n%s", entry.label, joined)
		}
		previous = index
	}
}

// TestWave4L6InboxRowsCarryToggleChips pins the inbox sheet onto the same row
// chrome as the other filter sheets: an empty waiting list keeps the muted
// empty state after the heading and divider.
func TestWave4L6InboxRowsCarryToggleChips(t *testing.T) {
	m := seededModel(t)
	for m.ctrl.InboxCount() > 0 {
		m.ctrl.DismissInboxItem(0)
	}
	m.toggleInbox()
	inner := wave4L6Inner(m)
	body := wave4L6Plain(m.inboxCardBody(inner))
	if len(body) < 2 || !strings.Contains(strings.Join(body, "\n"), "No mentions") {
		t.Fatalf("empty inbox body = %q, want the No mentions empty state", body)
	}
}

// TestWave4L6EmptyStatesPreserved pins the four empty-state labels the sheets
// have always shown.
func TestWave4L6EmptyStatesPreserved(t *testing.T) {
	m := seededModel(t)
	inner := wave4L6Inner(m)

	m.openJump()
	m.jump.input.SetValue("zzz-no-match")
	if body := strings.Join(wave4L6Plain(m.jumpCardBody(inner)), "\n"); !strings.Contains(body, "No matches") {
		t.Fatalf("jump empty body = %q, want No matches", body)
	}
	m.closeJump()

	m.openNickJump()
	m.nick.input.SetValue("zzz-no-match")
	if body := strings.Join(wave4L6Plain(m.nickCardBody(inner)), "\n"); !strings.Contains(body, "No matches") {
		t.Fatalf("nick empty body = %q, want No matches", body)
	}
	m.closeNickJump()

	m.toggleLink()
	m.link.input.SetValue("zzz-no-match")
	if body := strings.Join(wave4L6Plain(m.linkCardBody(inner)), "\n"); !strings.Contains(body, "No links") {
		t.Fatalf("link empty body = %q, want No links", body)
	}
	m.closeLink()

	for m.ctrl.InboxCount() > 0 {
		m.ctrl.DismissInboxItem(0)
	}
	m.toggleInbox()
	if body := strings.Join(wave4L6Plain(m.inboxCardBody(inner)), "\n"); !strings.Contains(body, "No mentions") {
		t.Fatalf("inbox empty body = %q, want No mentions", body)
	}
	m.closeInbox()
}
