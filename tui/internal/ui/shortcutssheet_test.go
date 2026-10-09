package ui

import (
	"fmt"
	"regexp"
	"runtime"
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

func shortcutGroupTitles(groups []shortcutGroup) string {
	titles := make([]string, len(groups))
	for index, group := range groups {
		titles[index] = group.title
	}
	return strings.Join(titles, "+")
}

// twoColumnPartitionFits reports whether some whole-group two-column partition
// shows every action inside the row budget. Heights are wrap counts, matching
// the layout search, so a wrapped label is judged inside its own column.
func twoColumnPartitionFits(inner, bodyBudget int) bool {
	available := inner - shortcutsColumnGap
	if available < 2 {
		return false
	}
	measure := newShortcutMeasure()
	for _, assignment := range collectShortcutAssignments(shortcutGroups, 2) {
		ordered := sortShortcutAssignmentColumns(assignment)
		mins := []int{
			shortcutsStackMinInnerWrap(ordered[0]),
			shortcutsStackMinInnerWrap(ordered[1]),
		}
		if mins[0] > available-mins[1] {
			continue
		}
		for left := mins[0]; left <= available-mins[1]; left++ {
			height, ok := measure.stack(ordered, []int{left, available - left})
			if ok && height <= bodyBudget {
				return true
			}
		}
	}
	return false
}

// balancedShortcutPartitionFits reports whether the sheet's own layout keeps
// more than one column inside the row budget. At the two-column width it asks
// the wrap-count search directly, so a one-column fallback cannot hide a fit.
func balancedShortcutPartitionFits(m *Model) bool {
	inner := m.shortcutsInnerWidth()
	budget := m.overlayCardRowBudget() - shortcutsHeaderRows
	if shortcutsColumnCount(inner, 1) == 2 {
		return twoColumnPartitionFits(inner, budget)
	}
	layout := m.shortcutsLayout(inner)
	return len(layout.columns) > 1 && len(layout.body) <= budget && shortcutsBodyShowsAllActions(layout.columns)
}

func allShortcutActionsReachableInView(m *Model) bool {
	inner := m.shortcutsInnerWidth()
	layout := m.shortcutsLayout(inner)
	if !shortcutsBodyShowsAllActions(layout.columns) {
		return false
	}
	_, visible, maxOffset := m.shortcutsScrollBounds(inner)
	seen := make(map[string]bool)
	for offset := 0; offset <= maxOffset; offset++ {
		for _, column := range layout.columns {
			start := offset
			if start > len(column.lines) {
				start = len(column.lines)
			}
			end := offset + visible
			if end > len(column.lines) {
				end = len(column.lines)
			}
			plain := shortcutsPlainFold(shortcutsStripAnsi(strings.Join(column.lines[start:end], "\n")))
			for _, group := range column.groups {
				for _, row := range group.rows {
					if strings.Contains(plain, row.action) {
						seen[row.action] = true
					}
				}
			}
		}
	}
	return len(seen) == len(shortcutActions())
}

// viewShowsEveryShortcutAction reports whether scroll 0 paints every action.
// Column lines are matched in the view so a wrapped label still counts when
// the other column sits between its lines. quit is required in the plain view.
func viewShowsEveryShortcutAction(m *Model) bool {
	plain := shortcutsViewPlain(m)
	if !strings.Contains(plain, "quit") {
		return false
	}
	layout := m.shortcutsLayout(m.shortcutsInnerWidth())
	if !shortcutsBodyShowsAllActions(layout.columns) {
		return false
	}
	for _, column := range layout.columns {
		for _, line := range column.lines {
			text := strings.TrimSpace(shortcutsStripAnsi(line))
			if text == "" {
				continue
			}
			if !strings.Contains(plain, text) {
				return false
			}
		}
	}
	return true
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
	layout := m.shortcutsLayout(m.shortcutsInnerWidth())
	for _, column := range layout.columns {
		assertNoWrappedActionPrefixDuplicates(t, strings.Join(column.lines, "\n"))
	}

	if runtime.GOOS == "windows" {
		inner := m.shortcutsInnerWidth()
		total, visible, maxOffset := m.shortcutsScrollBounds(inner)
		if balancedShortcutPartitionFits(m) {
			if m.shortcutsScroll != 0 || maxOffset != 0 || total != visible {
				t.Fatalf("118x30 shortcuts scroll = %d bounds total %d visible %d max %d, want scroll 0 when a balanced partition fits", m.shortcutsScroll, total, visible, maxOffset)
			}
			if !viewShowsEveryShortcutAction(m) {
				t.Fatalf("View() at scroll 0 is missing a shortcut action, including quit:\n%s", shortcutsViewPlain(m))
			}
			if hint := shortcutsViewScrollHint(m); hint != "" {
				t.Fatalf("scroll hint = %q, want none when a balanced partition fits at 118x30", hint)
			}
		} else {
			if maxOffset < 1 {
				t.Fatalf("118x30 shortcuts scroll bounds = total %d visible %d max %d, want scrolling when no balanced partition fits", total, visible, maxOffset)
			}
			if !allShortcutActionsReachableInView(m) {
				t.Fatalf("not every shortcut action is reachable at 118x30")
			}
		}
		return
	}

	inner := m.shortcutsInnerWidth()
	layout = m.shortcutsLayout(inner)
	if len(layout.columns) != 2 {
		t.Fatalf("118x30 columns = %d, want 2", len(layout.columns))
	}
	if got := shortcutGroupTitles(layout.columns[0].groups); got != "MOVE+JUMP+CONNECT" {
		t.Fatalf("left column = %s, want MOVE+JUMP+CONNECT", got)
	}
	if got := shortcutGroupTitles(layout.columns[1].groups); got != "WRITE+WINDOW" {
		t.Fatalf("right column = %s, want WRITE+WINDOW", got)
	}
	if len(layout.widths) != 2 || layout.widths[0] != 48 || layout.widths[1] != 60 {
		t.Fatalf("column widths = %v, want [48 60]", layout.widths)
	}
	budget := m.overlayCardRowBudget() - shortcutsHeaderRows
	if len(layout.body) != 25 || len(layout.body) > budget {
		t.Fatalf("body lines = %d, budget %d, want 25 lines that fit", len(layout.body), budget)
	}
	total, visible, maxOffset := m.shortcutsScrollBounds(inner)
	if m.shortcutsScroll != 0 || maxOffset != 0 || total != visible {
		t.Fatalf("118x30 shortcuts scroll = %d bounds total %d visible %d max %d, want no scroll", m.shortcutsScroll, total, visible, maxOffset)
	}
	if !strings.Contains(shortcutsViewPlain(m), "quit") {
		t.Fatalf("quit missing from View() at scroll 0:\n%s", shortcutsViewPlain(m))
	}
	if !viewShowsEveryShortcutAction(m) {
		t.Fatalf("View() at scroll 0 is missing a shortcut action, including quit:\n%s", shortcutsViewPlain(m))
	}
	if hint := shortcutsViewScrollHint(m); hint != "" {
		t.Fatalf("scroll hint = %q, want none at 118x30", hint)
	}
}

func TestShortcutsSheetWideFallsBackToTwoColumns(t *testing.T) {
	m := resizeModel(t, seededModel(t), 148, 30)
	m.openShortcuts()
	inner := m.shortcutsInnerWidth()
	if got := shortcutsColumnCount(inner, 0); got < 3 {
		t.Fatalf("column count at inner %d = %d, want at least 3", inner, got)
	}
	budget := m.overlayCardRowBudget() - shortcutsHeaderRows
	if !twoColumnPartitionFits(inner, budget) {
		if runtime.GOOS != "windows" {
			t.Fatal("a two-column partition should fit at 148x30")
		}
		return
	}
	layout := m.shortcutsLayout(inner)
	_, _, maxOffset := m.shortcutsScrollBounds(inner)
	if len(layout.columns) < 2 || maxOffset != 0 {
		t.Fatalf("148x30 columns = %d maxOffset = %d, want a fitting multi-column layout and no scroll", len(layout.columns), maxOffset)
	}
	if len(layout.body) > budget {
		t.Fatalf("148x30 body lines = %d, budget %d", len(layout.body), budget)
	}
}

func TestShortcutsSheetScrollClampsBeforeStep(t *testing.T) {
	up, _, maxOffset := shortcutsScrolledThenResized(t)
	up, _ = pressCmd(t, up, tea.KeyPressMsg{Code: tea.KeyUp})
	if up.shortcutsScroll != maxOffset-1 {
		t.Fatalf("Up scroll = %d, want %d after clamping to %d", up.shortcutsScroll, maxOffset-1, maxOffset)
	}

	pg, page, maxOffset := shortcutsScrolledThenResized(t)
	pg, _ = pressCmd(t, pg, tea.KeyPressMsg{Code: tea.KeyPgUp})
	want := maxOffset - page
	if want < 0 {
		want = 0
	}
	if pg.shortcutsScroll != want {
		t.Fatalf("PgUp scroll = %d, want %d after clamping to %d then stepping %d", pg.shortcutsScroll, want, maxOffset, page)
	}
}

// shortcutsScrolledThenResized scrolls to the end of a short sheet, then grows
// the terminal so the stored offset is past the new maximum.
func shortcutsScrolledThenResized(t *testing.T) (m *Model, visible, maxOffset int) {
	t.Helper()
	m = resizeModel(t, seededModel(t), 80, 24)
	m.openShortcuts()
	m, _ = pressCmd(t, m, tea.KeyPressMsg{Code: tea.KeyEnd})
	stale := m.shortcutsScroll
	if stale < 1 {
		t.Fatalf("80x24 end scroll = %d, want a scrolled sheet", stale)
	}
	m = resizeModel(t, m, 80, 30)
	if m.shortcutsScroll != stale {
		t.Fatalf("resize stored scroll = %d, want the stale offset %d", m.shortcutsScroll, stale)
	}
	_, visible, maxOffset = m.shortcutsScrollBounds(m.shortcutsInnerWidth())
	if maxOffset < 1 || maxOffset >= stale {
		t.Fatalf("resized maxOffset = %d, stale %d, want a smaller positive maximum", maxOffset, stale)
	}
	return m, visible, maxOffset
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

func withWindowsShortcutKeyLabels(t *testing.T, fn func()) {
	t.Helper()
	prev := shortcutSheetKeyLabels
	shortcutSheetKeyLabels = ExpandShortcutKeyLabels
	t.Cleanup(func() { shortcutSheetKeyLabels = prev })
	fn()
}

func TestShortcutsSheetTallNarrowWindowsLabels(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("covered by TestShortcutsSheetTallNarrowNoPanic on Windows")
	}
	withWindowsShortcutKeyLabels(t, func() {
		m := resizeModel(t, seededModel(t), 60, 60)
		m.openShortcuts()
		if !allShortcutActionsReachableInView(m) {
			t.Fatalf("not every shortcut action is reachable at 60x60 with Windows key labels")
		}
		assertNoWrappedActionPrefixDuplicates(t, shortcutsViewPlain(m))
	})
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
