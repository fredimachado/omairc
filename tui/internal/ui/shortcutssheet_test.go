package ui

import (
	"fmt"
	"regexp"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
)

var shortcutsScrollRangePattern = regexp.MustCompile(`\d+-\d+ of \d+`)

func shortcutActions() []string {
	var actions []string
	for _, group := range shortcutGroups {
		for _, row := range group.rows {
			actions = append(actions, row.action)
		}
	}
	return actions
}

func shortcutsViewPlain(m *Model) string {
	return ansiPattern.ReplaceAllString(m.View().Content, "")
}

func shortcutsViewLines(m *Model) []string {
	return strings.Split(strings.TrimRight(shortcutsViewPlain(m), "\n"), "\n")
}

func shortcutsViewClosedBottom(m *Model) bool {
	lines := shortcutsViewLines(m)
	limit := m.bodyHeight()
	if limit > len(lines) {
		limit = len(lines)
	}
	for index := 0; index < limit; index++ {
		end := strings.LastIndex(lines[index], "╯")
		if end < 0 {
			continue
		}
		start := strings.LastIndex(lines[index][:end], "╰")
		if start >= overlayCardInset {
			return true
		}
	}
	return false
}

func visibleShortcutActionsInView(m *Model) map[string]bool {
	plain := shortcutsPlainFold(shortcutsViewPlain(m))
	seen := make(map[string]bool)
	for _, action := range shortcutActions() {
		if strings.Contains(plain, action) {
			seen[action] = true
		}
	}
	return seen
}

func shortcutsViewScrollHint(m *Model) string {
	plain := shortcutsViewPlain(m)
	for _, line := range strings.Split(plain, "\n") {
		if strings.Contains(line, "↓ more") {
			return "↓ more"
		}
		if strings.Contains(line, "↑ more") {
			return "↑ more"
		}
		if match := shortcutsScrollRangePattern.FindString(line); match != "" {
			return match
		}
	}
	return ""
}

func shortcutsSheetFitsWithoutScroll(m *Model) bool {
	inner := m.shortcutsInnerWidth()
	body := m.shortcutsSheetBody(inner)
	budget := m.overlayCardRowBudget() - shortcutsHeaderRows
	return len(body) <= budget && m.shortcutsBodyShowsAllActions(body)
}

func allShortcutActionsReachableInView(m *Model) bool {
	inner := m.shortcutsInnerWidth()
	body := m.shortcutsSheetBody(inner)
	if !m.shortcutsBodyShowsAllActions(body) {
		return false
	}
	_, visible, maxOffset := m.shortcutsScrollBounds(inner)
	seen := make(map[string]bool)
	for offset := 0; offset <= maxOffset; offset++ {
		end := offset + visible
		if end > len(body) {
			end = len(body)
		}
		plain := shortcutsPlainFold(shortcutsStripAnsi(strings.Join(body[offset:end], "\n")))
		for _, action := range shortcutActions() {
			if strings.Contains(plain, action) {
				seen[action] = true
			}
		}
	}
	return len(seen) == len(shortcutActions())
}

func assertNoWrappedActionPrefixDuplicates(t *testing.T, plain string) {
	t.Helper()
	folded := shortcutsPlainFold(shortcutsStripAnsi(plain))
	for _, pair := range []struct {
		truncated string
		full      string
	}{
		{"walk conversation", "walk conversations"},
		{"scroll transcript half pa", "scroll transcript half page"},
		{"page focused members half", "page focused members half page"},
	} {
		if !strings.Contains(folded, pair.full) {
			continue
		}
		rest := strings.ReplaceAll(folded, pair.full, "")
		if strings.Contains(rest, pair.truncated) {
			t.Fatalf("wrapped action left duplicate prefix %q for %q in view:\n%s", pair.truncated, pair.full, plain)
		}
	}
}

func TestShortcutsSheetFitsDefaultSize(t *testing.T) {
	m := resizeModel(t, seededModel(t), 118, 30)
	m.openShortcuts()
	if lipgloss.Height(m.View().Content) != m.height {
		t.Fatalf("view height = %d, want terminal height %d", lipgloss.Height(m.View().Content), m.height)
	}
	if !shortcutsViewClosedBottom(m) {
		t.Fatalf("shortcuts sheet must close its bottom border in View() at 118x30:\n%s", shortcutsViewPlain(m))
	}
	if !allShortcutActionsReachableInView(m) {
		t.Fatalf("not every shortcut action is reachable at 118x30")
	}
	assertNoWrappedActionPrefixDuplicates(t, shortcutsViewPlain(m))

	inner := m.shortcutsInnerWidth()
	total, visible, maxOffset := m.shortcutsScrollBounds(inner)
	fits := shortcutsSheetFitsWithoutScroll(m)
	if fits {
		if maxOffset != 0 || total != visible {
			t.Fatalf("118x30 shortcuts scroll bounds = total %d visible %d max %d, want no scrolling when body fits", total, visible, maxOffset)
		}
		if hint := shortcutsViewScrollHint(m); hint != "" {
			t.Fatalf("scroll hint = %q, want none when body fits at 118x30", hint)
		}
	} else {
		if maxOffset < 1 {
			t.Fatalf("118x30 shortcuts scroll bounds = total %d visible %d max %d, want scrolling when body does not fit", total, visible, maxOffset)
		}
	}
}

func TestShortcutsSheetScrollsShortTerminal(t *testing.T) {
	m := resizeModel(t, seededModel(t), 80, 24)
	m.openShortcuts()
	if lipgloss.Height(m.View().Content) != m.height {
		t.Fatalf("view height = %d, want terminal height %d", lipgloss.Height(m.View().Content), m.height)
	}
	if !shortcutsViewClosedBottom(m) {
		t.Fatalf("shortcuts sheet must close its bottom border in View() at 80x24:\n%s", shortcutsViewPlain(m))
	}
	if !allShortcutActionsReachableInView(m) {
		t.Fatalf("not every shortcut action is reachable by scrolling at 80x24")
	}

	inner := m.shortcutsInnerWidth()
	total, visible, maxOffset := m.shortcutsScrollBounds(inner)
	if maxOffset < 1 {
		t.Fatalf("80x24 scroll bounds = total %d visible %d max %d, want scrolling", total, visible, maxOffset)
	}

	m.shortcutsScroll = 0
	if hint := shortcutsViewScrollHint(m); hint != "↓ more" {
		t.Fatalf("top scroll hint = %q, want ↓ more", hint)
	}

	m, _ = pressCmd(t, m, tea.KeyPressMsg{Code: tea.KeyPgDown})
	if m.shortcutsScroll != visible {
		t.Fatalf("PgDown scroll = %d, want %d", m.shortcutsScroll, visible)
	}
	offset := m.shortcutsScroll
	end := offset + visible
	if end > total {
		end = total
	}
	wantMiddle := fmt.Sprintf("%d-%d of %d", offset+1, end, total)
	if hint := shortcutsViewScrollHint(m); hint != wantMiddle {
		t.Fatalf("middle scroll hint = %q, want %s", hint, wantMiddle)
	}
	if !shortcutsViewClosedBottom(m) {
		t.Fatalf("shortcuts bottom border must stay visible after PgDown:\n%s", shortcutsViewPlain(m))
	}

	m.shortcutsScroll = maxOffset
	if hint := shortcutsViewScrollHint(m); hint != "↑ more" {
		t.Fatalf("end scroll hint = %q, want ↑ more", hint)
	}
	if !strings.Contains(shortcutsViewPlain(m), "quit") {
		t.Fatalf("scrolled sheet must show quit at the end:\n%s", shortcutsViewPlain(m))
	}
	if !shortcutsViewClosedBottom(m) {
		t.Fatalf("shortcuts bottom border must stay visible at end scroll:\n%s", shortcutsViewPlain(m))
	}
}

func TestShortcutsSheetTallNarrowNoPanic(t *testing.T) {
	m := resizeModel(t, seededModel(t), 60, 60)
	m.openShortcuts()
	if lipgloss.Height(m.View().Content) != m.height {
		t.Fatalf("view height = %d, want terminal height %d", lipgloss.Height(m.View().Content), m.height)
	}
	if !allShortcutActionsReachableInView(m) {
		t.Fatalf("not every shortcut action is reachable at 60x60")
	}
	assertNoWrappedActionPrefixDuplicates(t, shortcutsViewPlain(m))
}

func TestShortcutsWrapPlainActionNoDuplicatePrefix(t *testing.T) {
	lines := shortcutsWrapPlainAction("walk conversations", 17)
	if len(lines) < 2 {
		t.Fatalf("expected wrapped lines, got %v", lines)
	}
	joined := strings.Join(lines, " ")
	if joined != "walk conversations" {
		t.Fatalf("wrapped lines = %q, want full action preserved", joined)
	}
	if lines[0] == "walk conversation" {
		t.Fatalf("first line must not be the truncated prefix %q", lines[0])
	}
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
