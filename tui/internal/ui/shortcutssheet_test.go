package ui

import (
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
)

func shortcutActions() []string {
	var actions []string
	for _, group := range shortcutGroups {
		for _, row := range group.rows {
			actions = append(actions, row.action)
		}
	}
	return actions
}

func shortcutsCardPlain(m *Model) string {
	return ansiPattern.ReplaceAllString(m.shortcutsCard(m.width), "")
}

func shortcutsCardClosedBottom(card string) bool {
	plain := strings.TrimRight(ansiPattern.ReplaceAllString(card, ""), "\n")
	return strings.HasSuffix(plain, "╯")
}

func visibleShortcutActions(m *Model) map[string]bool {
	inner := m.shortcutsInnerWidth()
	plain := shortcutsPlainFold(strings.Join(m.shortcutsCardBody(inner), "\n"))
	seen := make(map[string]bool)
	for _, action := range shortcutActions() {
		if strings.Contains(plain, action) {
			seen[action] = true
		}
	}
	return seen
}

func allShortcutActionsReachable(m *Model) bool {
	inner := m.shortcutsInnerWidth()
	total, visible, maxOffset := m.shortcutsScrollBounds(inner)
	if total <= visible {
		return len(visibleShortcutActions(m)) == len(shortcutActions())
	}
	seen := make(map[string]bool)
	for offset := 0; offset <= maxOffset; offset++ {
		m.shortcutsScroll = offset
		for action, ok := range visibleShortcutActions(m) {
			if ok {
				seen[action] = true
			}
		}
	}
	return len(seen) == len(shortcutActions())
}

func TestShortcutsSheetFitsDefaultSize(t *testing.T) {
	m := resizeModel(t, seededModel(t), 118, 30)
	m.openShortcuts()
	card := m.shortcutsCard(m.width)
	if !shortcutsCardClosedBottom(card) {
		t.Fatalf("shortcuts card must close its bottom border:\n%s", card)
	}
	seen := visibleShortcutActions(m)
	for _, action := range shortcutActions() {
		if !seen[action] {
			t.Fatalf("shortcuts sheet missing action %q at 118x30:\n%s", action, shortcutsCardPlain(m))
		}
	}
}

func TestShortcutsSheetScrollsShortTerminal(t *testing.T) {
	m := resizeModel(t, seededModel(t), 80, 24)
	m.openShortcuts()
	card := m.shortcutsCard(m.width)
	if !shortcutsCardClosedBottom(card) {
		t.Fatalf("shortcuts card must close its bottom border at 80x24:\n%s", card)
	}
	if !allShortcutActionsReachable(m) {
		t.Fatalf("not every shortcut action is reachable by scrolling at 80x24")
	}

	m.shortcutsScroll = 0
	hintTop := shortcutsCardPlain(m)
	if !strings.Contains(hintTop, "↓ more") {
		t.Fatalf("top scroll hint = %q, want ↓ more", hintTop)
	}

	inner := m.shortcutsInnerWidth()
	_, visible, maxOffset := m.shortcutsScrollBounds(inner)
	m.shortcutsScroll = maxOffset / 2
	if maxOffset/2 == 0 && maxOffset > 0 {
		m.shortcutsScroll = 1
	}
	hintMiddle := shortcutsCardPlain(m)
	if !strings.Contains(hintMiddle, " of ") {
		t.Fatalf("middle scroll hint missing range:\n%s", hintMiddle)
	}

	m.shortcutsScroll = maxOffset
	hintEnd := shortcutsCardPlain(m)
	if !strings.Contains(hintEnd, "↑ more") {
		t.Fatalf("end scroll hint = %q, want ↑ more", hintEnd)
	}
	if !strings.Contains(hintEnd, "quit") {
		t.Fatalf("scrolled sheet must show quit at the end:\n%s", hintEnd)
	}
	_ = visible
}

func TestShortcutsSheetScrollKeys(t *testing.T) {
	m := resizeModel(t, seededModel(t), 80, 24)
	m.openShortcuts()
	inner := m.shortcutsInnerWidth()
	_, _, maxOffset := m.shortcutsScrollBounds(inner)
	if maxOffset < 1 {
		t.Fatal("80x24 sheet must need scrolling for this test")
	}

	m, _ = pressCmd(t, m, tea.KeyPressMsg{Code: tea.KeyEnd})
	if m.shortcutsScroll != maxOffset {
		t.Fatalf("End scroll = %d, want %d", m.shortcutsScroll, maxOffset)
	}
	m, _ = pressCmd(t, m, tea.KeyPressMsg{Code: tea.KeyHome})
	if m.shortcutsScroll != 0 {
		t.Fatalf("Home scroll = %d, want 0", m.shortcutsScroll)
	}
	m, _ = pressCmd(t, m, tea.KeyPressMsg{Code: tea.KeyDown})
	if m.shortcutsScroll != 1 {
		t.Fatalf("Down scroll = %d, want 1", m.shortcutsScroll)
	}
}

func TestShortcutsSheetResetsScrollOnClose(t *testing.T) {
	m := resizeModel(t, seededModel(t), 80, 24)
	m.openShortcuts()
	m.shortcutsScroll = 3
	m.closeShortcuts()
	if m.shortcutsScroll != 0 {
		t.Fatalf("closeShortcuts scroll = %d, want 0", m.shortcutsScroll)
	}
	m.openShortcuts()
	if m.shortcutsScroll != 0 {
		t.Fatalf("openShortcuts scroll = %d, want 0", m.shortcutsScroll)
	}
}
