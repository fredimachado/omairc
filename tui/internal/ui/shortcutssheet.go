package ui

import (
	"fmt"
	"regexp"
	"strings"

	"charm.land/bubbles/v2/key"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
)

// This file is the Ctrl+/ shortcuts sheet. It lists the full chord map,
// mirroring src/qml/ShortcutsSheet.qml, and is the only overlay that can open on
// top of the Connect sheet. While it is open every navigation chord is gated.

// shortcutRow is one keys/action pair in the sheet.
type shortcutRow struct {
	keys   string
	action string
}

// shortcutGroup is a titled block of chord rows.
type shortcutGroup struct {
	title string
	rows  []shortcutRow
}

// shortcutGroups is the sheet's content. It mirrors ShortcutsSheet.qml's
// shortcutGroups, including Ctrl+Q as the only quit chord and Ctrl+C as copy.
var shortcutGroups = []shortcutGroup{
	{
		title: "MOVE",
		rows: []shortcutRow{
			{"Alt+Down / Alt+Up", "walk conversations"},
			{"Alt+Left / Alt+Right", "walk networks"},
			{"Alt+Shift+Left / Right", "collapse / expand network"},
			{"Ctrl+Alt+Shift+Left / Right", "collapse / expand all"},
			{"Alt+Shift+Up / Down", "move network"},
			{"Alt+A", "next unread"},
			{"Alt+Shift+A", "mark all read"},
			{"Alt+U", "first new message"},
		},
	},
	{
		title: "JUMP",
		rows: []shortcutRow{
			{"Ctrl+K", "jump to conversation"},
			{"Ctrl+Shift+O", "open link"},
			{"Ctrl+Shift+K", "jump to nick"},
			{"Ctrl+Shift+A", "inbox"},
			{"Ctrl+`", "Status"},
			{"Ctrl+W", "close conversation"},
		},
	},
	{
		title: "WRITE",
		rows: []shortcutRow{
			{"Ctrl+L", "composer"},
			{"Ctrl+Shift+U", "insert a file link"},
			{"Ctrl+C", "copy selection"},
			{"Ctrl+F", "find"},
			{"Enter", "send"},
			{"Page Up / Page Down", "scroll transcript"},
			{"Shift+Page Up / Shift+Page Down", "scroll transcript half page"},
			{"Ctrl+Home / Ctrl+End", "top / bottom"},
			{"Tab / Shift+Tab", "complete nick or channel"},
			{"Up / Down", "history"},
			{"Escape", "dismiss"},
		},
	},
	{
		title: "CONNECT",
		rows: []shortcutRow{
			{"Ctrl+,", "Connect"},
			{"Ctrl+Tab", "Connect tabs"},
			{"Ctrl+N", "add network"},
			{"Ctrl+Shift+Delete", "remove network"},
			{"Ctrl+Enter", "apply selected network"},
		},
	},
	{
		title: "WINDOW",
		rows: []shortcutRow{
			{"Ctrl+Shift+S", "server list"},
			{"Ctrl+Shift+M", "members panel"},
			{"Ctrl+Shift+P", "focus members"},
			{"Page Up / Page Down", "page focused members"},
			{"Shift+Page Up / Shift+Page Down", "page focused members half page"},
			{"Home / End", "first / last focused nick"},
			{"Ctrl+/", "this sheet"},
			{"Ctrl+Shift+/", "About"},
			{"Ctrl+Q", "quit"},
		},
	},
}

// shortcutsHeaderRows is the pinned title row when the sheet scrolls.
const shortcutsHeaderRows = 1

// shortcutsFooterRows is the pinned scroll-position row at the bottom.
const shortcutsFooterRows = 1

// shortcutsColumnGap is the blank gutter between multi-column group stacks.
const shortcutsColumnGap = 4

// shortcutsMinColumnInner is the narrowest inner width that still fits the
// keycap column and a short action label in one column.
const shortcutsMinColumnInner = 44

// openShortcuts shows the sheet. It is allowed on top of the Connect sheet.
func (m *Model) openShortcuts() {
	m.shortcutsOpen = true
	m.shortcutsScroll = 0
	m.composer.Blur()
}

// closeShortcuts hides the sheet and returns focus to the composer or the
// Connect sheet underneath.
func (m *Model) closeShortcuts() {
	if !m.shortcutsOpen {
		return
	}
	m.shortcutsOpen = false
	m.shortcutsScroll = 0
	m.refocusComposer()
}

// handleShortcutsScrollKey scrolls the sheet when it does not fit. It returns
// true when the key was consumed.
func (m *Model) handleShortcutsScrollKey(key string) bool {
	inner := m.shortcutsInnerWidth()
	total, visible, maxOffset := m.shortcutsScrollBounds(inner)
	if total <= visible {
		return false
	}
	page := visible
	if page < 1 {
		page = 1
	}
	switch key {
	case "up":
		m.shortcutsScroll--
	case "down":
		m.shortcutsScroll++
	case "pgup":
		m.shortcutsScroll -= page
	case "pgdown":
		m.shortcutsScroll += page
	case "home":
		m.shortcutsScroll = 0
	case "end":
		m.shortcutsScroll = maxOffset
	default:
		return false
	}
	m.shortcutsScroll = clampInt(m.shortcutsScroll, 0, maxOffset)
	return true
}

// shortcutsCard renders the shortcuts sheet as one card block through the
// shared overlay frame in model.go.
func (m *Model) shortcutsCard(width int) string {
	return m.overlayCardBlock(width, m.shortcutsCardBody)
}

// shortcutsInnerWidth is the content width inside the shared panel border.
func (m *Model) shortcutsInnerWidth() int {
	frameX, _ := m.styles.Panel.GetFrameSize()
	inner := m.width - 2*overlayCardInset - frameX
	if inner < 8 {
		inner = 8
	}
	return inner
}

// shortcutsScrollBounds returns the scrollable body line count, how many body
// lines fit in the middle band, and the largest legal scroll offset.
func (m *Model) shortcutsScrollBounds(inner int) (total, visible, maxOffset int) {
	body := m.shortcutsSheetBody(inner)
	budget := m.overlayCardRowBudget()
	total = len(body)
	if total <= budget-shortcutsHeaderRows {
		return total, total, 0
	}
	visible = budget - shortcutsHeaderRows - shortcutsFooterRows
	if visible < 1 {
		visible = 1
	}
	maxOffset = total - visible
	if maxOffset < 0 {
		maxOffset = 0
	}
	return total, visible, maxOffset
}

// shortcutsMenuGutter is the two-cell gutter between the keycap column and the
// action column.
const shortcutsMenuGutter = 2

// shortcutsCardBody is the sheet's content, before the shared frame. Every chord
// is a key.Binding whose Help drives a keycap chip in the left column with the
// action in the right column, under a per-group header. inner is the content
// width inside the border.
func (m *Model) shortcutsCardBody(inner int) []string {
	title := truncateLine(m.styles.SheetTitle.Render("Shortcuts"), inner)
	body := m.shortcutsSheetBody(inner)
	budget := m.overlayCardRowBudget()
	if len(body) <= budget-shortcutsHeaderRows {
		lines := []string{title}
		lines = append(lines, body...)
		return lines
	}
	total, visible, _ := m.shortcutsScrollBounds(inner)
	offset := clampInt(m.shortcutsScroll, 0, total-visible)
	hint := m.shortcutsScrollHint(offset, total, visible)
	lines := []string{title}
	lines = append(lines, body[offset:offset+visible]...)
	lines = append(lines, truncateLine(hint, inner))
	return lines
}

// shortcutsScrollHint is the pinned footer line while the sheet scrolls.
func (m *Model) shortcutsScrollHint(offset, total, visible int) string {
	style := lipgloss.NewStyle().Foreground(m.styles.Colors.TextMuted)
	end := offset + visible
	if end > total {
		end = total
	}
	if offset == 0 {
		return style.Render("↓ more")
	}
	if end >= total {
		return style.Render("↑ more")
	}
	return style.Render(fmt.Sprintf("%d-%d of %d", offset+1, end, total))
}

// shortcutsSheetBody lays out every group from shortcutGroups, flowing into
// multiple columns when the card is wide enough.
func (m *Model) shortcutsSheetBody(inner int) []string {
	single := m.shortcutsGroupsLines(inner, shortcutGroups, true, false)
	budget := m.overlayCardRowBudget() - shortcutsHeaderRows
	if len(single) <= budget {
		return single
	}
	columns := shortcutsColumnCount(inner, len(single))
	if columns <= 1 {
		return single
	}
	assignment := m.balanceShortcutGroups(shortcutGroups, columns, inner)
	widths := m.fitShortcutColumnWidths(assignment, inner)
	stacks := make([][]string, columns)
	for index, groups := range assignment {
		stacks[index] = m.shortcutsGroupsLines(widths[index], groups, false, true)
	}
	return joinShortcutColumns(stacks, widths, shortcutsColumnGap, inner)
}

// shortcutsGroupsLines renders groups at colInner width. blankBetween inserts a
// spacer row between groups in a single-column stack.
func (m *Model) shortcutsGroupsLines(colInner int, groups []shortcutGroup, blankBetween, wrapActions bool) []string {
	var lines []string
	for index, group := range groups {
		if blankBetween && index > 0 {
			lines = append(lines, "")
		}
		lines = append(lines, truncateLine(m.styles.SectionHeader.Render(group.title), colInner))
		labelWidth, actionWidth := shortcutsKeyActionWidths(colInner, group.rows)
		for _, row := range group.rows {
			lines = append(lines, m.shortcutsRowLines(colInner, labelWidth, actionWidth, row, wrapActions)...)
		}
	}
	return lines
}

// shortcutsKeyActionWidths sizes the keycap and action columns for colInner.
// rows limits the widest key label to the rows rendered in one column.
func shortcutsKeyActionWidths(colInner int, rows []shortcutRow) (labelWidth, actionWidth int) {
	keyText := 0
	for _, candidate := range rows {
		if width := lipgloss.Width(displayShortcutKeys(candidate.keys)); width > keyText {
			keyText = width
		}
	}
	column := keyText + 2 + shortcutsMenuGutter
	if limit := colInner - 4; column > limit {
		column = limit
	}
	if column < 4 {
		column = 4
	}
	if column > colInner {
		column = colInner
	}
	labelWidth = column - shortcutsMenuGutter
	actionWidth = colInner - column
	if actionWidth < 1 {
		actionWidth = 1
	}
	return labelWidth, actionWidth
}

// shortcutsRowLines renders one chord row at colInner width. When wrapActions is
// true, a long action continues on the next line so the full label stays visible.
func (m *Model) shortcutsRowLines(colInner, labelWidth, actionWidth int, row shortcutRow, wrapActions bool) []string {
	actionStyle := lipgloss.NewStyle().Foreground(m.styles.Colors.TextMuted)
	keysLabel := displayShortcutKeys(row.keys)
	help := key.NewBinding(
		key.WithKeys(keysLabel),
		key.WithHelp(keysLabel, row.action),
	).Help()
	chip := clipShortcutLine(m.styles.Keycap.Render(" "+help.Key+" "), labelWidth)
	keyColumn := lipgloss.NewStyle().Width(labelWidth).Render(chip)
	actionText := actionStyle.Render(displayShortcutAction(help.Desc))
	line := keyColumn + strings.Repeat(" ", shortcutsMenuGutter) + clipShortcutLine(actionText, actionWidth)
	lines := []string{clipShortcutLine(line, colInner)}
	if wrapActions && lipgloss.Width(actionText) > actionWidth {
		lines = append(lines, clipShortcutLine(actionText, colInner))
	}
	return lines
}

// shortcutsColumnCount picks how many group columns fit at inner width.
func shortcutsColumnCount(inner int, _ int) int {
	if inner >= 3*shortcutsMinColumnInner+2*shortcutsColumnGap {
		return 3
	}
	if inner >= 2*shortcutsMinColumnInner+shortcutsColumnGap {
		return 2
	}
	return 1
}

// shortcutGroupLineCount is the rendered row count of one group block.
func shortcutGroupLineCount(group shortcutGroup) int {
	return 1 + len(group.rows)
}

// balanceShortcutGroups assigns whole groups to columns. For two columns it
// searches partitions that fit inner width, then picks the flattest stack.
func (m *Model) balanceShortcutGroups(groups []shortcutGroup, columns, inner int) [][]shortcutGroup {
	if len(groups) == 0 {
		return balanceShortcutGroupsGreedy(groups, columns)
	}
	bestHeight := int(^uint(0) >> 1)
	var best [][]shortcutGroup
	for _, assignment := range collectShortcutAssignments(groups, columns) {
		if !m.shortcutsAssignmentFits(inner, assignment) {
			continue
		}
		body := m.shortcutsAssignmentBody(inner, assignment)
		if !m.shortcutsBodyShowsAllActions(body) {
			continue
		}
		if len(body) > m.overlayCardRowBudget()-shortcutsHeaderRows {
			continue
		}
		if len(body) < bestHeight {
			bestHeight = len(body)
			best = assignment
		}
	}
	if best != nil {
		return best
	}
	return balanceShortcutGroupsGreedy(groups, columns)
}

// collectShortcutAssignments returns every way to place groups into columns.
func collectShortcutAssignments(groups []shortcutGroup, columns int) [][][]shortcutGroup {
	if columns < 1 {
		return nil
	}
	stack := make([][]shortcutGroup, columns)
	var out [][][]shortcutGroup
	var visit func(index int)
	visit = func(index int) {
		if index == len(groups) {
			for _, column := range stack {
				if len(column) == 0 {
					return
				}
			}
			out = append(out, cloneShortcutAssignment(stack))
			return
		}
		for column := 0; column < columns; column++ {
			stack[column] = append(stack[column], groups[index])
			visit(index + 1)
			stack[column] = stack[column][:len(stack[column])-1]
		}
	}
	visit(0)
	return out
}

func cloneShortcutAssignment(stack [][]shortcutGroup) [][]shortcutGroup {
	out := make([][]shortcutGroup, len(stack))
	for index, column := range stack {
		out[index] = append([]shortcutGroup(nil), column...)
	}
	return out
}

// shortcutsAssignmentFits reports whether an assignment's minimum widths fit inner.
func (m *Model) shortcutsAssignmentFits(inner int, assignment [][]shortcutGroup) bool {
	total := (len(assignment) - 1) * shortcutsColumnGap
	for _, groups := range assignment {
		total += m.shortcutsStackMinInner(groups)
	}
	return total <= inner
}

// shortcutsAssignmentBody renders one two-column assignment.
func (m *Model) shortcutsAssignmentBody(inner int, assignment [][]shortcutGroup) []string {
	widths := m.fitShortcutColumnWidths(assignment, inner)
	stacks := make([][]string, len(assignment))
	for index, groups := range assignment {
		stacks[index] = m.shortcutsGroupsLines(widths[index], groups, false, true)
	}
	return joinShortcutColumns(stacks, widths, shortcutsColumnGap, inner)
}

// shortcutsBodyShowsAllActions reports whether every action label appears in body.
func (m *Model) shortcutsBodyShowsAllActions(body []string) bool {
	plain := shortcutsPlainFold(strings.Join(body, "\n"))
	for _, group := range shortcutGroups {
		for _, row := range group.rows {
			if !strings.Contains(plain, row.action) {
				return false
			}
		}
	}
	return true
}

func shortcutStackHeight(groups []shortcutGroup) int {
	height := 0
	for _, group := range groups {
		height += shortcutGroupLineCount(group)
	}
	return height
}

func balanceShortcutGroupsGreedy(groups []shortcutGroup, columns int) [][]shortcutGroup {
	stacks := make([][]shortcutGroup, columns)
	heights := make([]int, columns)
	for _, group := range groups {
		rows := shortcutGroupLineCount(group)
		column := 0
		best := heights[0] + rows
		for index := 1; index < columns; index++ {
			if heights[index]+rows < best {
				best = heights[index] + rows
				column = index
			}
		}
		stacks[column] = append(stacks[column], group)
		heights[column] += rows
	}
	return stacks
}

// shortcutsStackMinInner is the narrowest inner width that fits the stack's rows.
func (m *Model) shortcutsStackMinInner(groups []shortcutGroup) int {
	need := shortcutsMinColumnInner
	for _, group := range groups {
		for _, row := range group.rows {
			keys := lipgloss.Width(displayShortcutKeys(row.keys))
			action := lipgloss.Width(displayShortcutAction(row.action))
			rowNeed := keys + 2 + shortcutsMenuGutter + action
			if rowNeed > need {
				need = rowNeed
			}
		}
	}
	return need
}

// fitShortcutColumnWidths sizes each column from its groups. When the preferred
// widths already fit inner, any leftover space goes to the wider column.
func (m *Model) fitShortcutColumnWidths(assignment [][]shortcutGroup, inner int) []int {
	widths := make([]int, len(assignment))
	for index, groups := range assignment {
		widths[index] = m.shortcutsStackMinInner(groups)
	}
	total := (len(widths)-1) * shortcutsColumnGap
	for _, width := range widths {
		total += width
	}
	if spare := inner - total; spare > 0 && len(widths) > 0 {
		widest := 0
		for index, width := range widths {
			if width > widths[widest] {
				widest = index
			}
		}
		widths[widest] += spare
	}
	return widths
}

// joinShortcutColumns lays out column stacks side by side, one terminal row at a
// time, so each column keeps a fixed width and the row never exceeds inner.
func joinShortcutColumns(columns [][]string, widths []int, gap int, inner int) []string {
	if len(columns) == 0 {
		return nil
	}
	maxRows := 0
	for _, column := range columns {
		if len(column) > maxRows {
			maxRows = len(column)
		}
	}
	gutter := strings.Repeat(" ", gap)
	out := make([]string, maxRows)
	for row := 0; row < maxRows; row++ {
		var line strings.Builder
		for index, column := range columns {
			cell := strings.Repeat(" ", widths[index])
			if row < len(column) {
				cell = padShortcutLine(column[row], widths[index])
			}
			line.WriteString(cell)
			if index < len(columns)-1 {
				line.WriteString(gutter)
			}
		}
		out[row] = line.String()
	}
	return out
}

// padShortcutLine fits one rendered row to an exact column width.
func padShortcutLine(line string, width int) string {
	line = clipShortcutLine(line, width)
	if deficit := width - lipgloss.Width(line); deficit > 0 {
		return line + strings.Repeat(" ", deficit)
	}
	return line
}

// clipShortcutLine shortens one styled row to width without wrapping it.
func clipShortcutLine(line string, width int) string {
	if width <= 0 {
		return ""
	}
	return ansi.Truncate(line, width, "")
}

var shortcutsPlainSpace = regexp.MustCompile(`\s+`)

// shortcutsPlainFold collapses whitespace so wrapped labels still match.
func shortcutsPlainFold(plain string) string {
	return shortcutsPlainSpace.ReplaceAllString(plain, " ")
}
