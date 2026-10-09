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
	layout := m.shortcutsLayout(inner)
	budget := m.overlayCardRowBudget()
	total = len(layout.body)
	if total <= budget-shortcutsHeaderRows && shortcutsBodyShowsAllActions(layout.columns) {
		return total, total, 0
	}
	visible = budget - shortcutsHeaderRows - shortcutsFooterRows
	if visible < 1 {
		visible = 1
	}
	if total <= visible {
		return total, total, 0
	}
	maxOffset = total - visible
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
	layout := m.shortcutsLayout(inner)
	budget := m.overlayCardRowBudget()
	if len(layout.body) <= budget-shortcutsHeaderRows && shortcutsBodyShowsAllActions(layout.columns) {
		lines := []string{title}
		lines = append(lines, layout.body...)
		return lines
	}
	total, visible, maxOffset := m.shortcutsScrollBounds(inner)
	if total <= visible {
		lines := []string{title}
		lines = append(lines, layout.body...)
		return lines
	}
	offset := clampInt(m.shortcutsScroll, 0, maxOffset)
	hint := m.shortcutsScrollHint(offset, total, visible)
	lines := []string{title}
	lines = append(lines, layout.body[offset:offset+visible]...)
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

// shortcutRenderedColumn is one column's groups and the lines rendered for
// them, before those columns are joined into a row.
type shortcutRenderedColumn struct {
	groups []shortcutGroup
	lines  []string
}

// shortcutsLayout is the sheet body plus the columns that produced it.
type shortcutsLayout struct {
	body    []string
	columns []shortcutRenderedColumn
}

// shortcutsLayout lays groups out in columns when a partition fits the row
// budget. Action presence is judged on each column before the columns are joined.
func (m *Model) shortcutsLayout(inner int) shortcutsLayout {
	singleLines := m.shortcutsGroupsLines(inner, shortcutGroups, true, true)
	single := shortcutsLayout{
		body: singleLines,
		columns: []shortcutRenderedColumn{{
			groups: shortcutGroups,
			lines:  singleLines,
		}},
	}
	budget := m.overlayCardRowBudget() - shortcutsHeaderRows
	if len(singleLines) <= budget && shortcutsBodyShowsAllActions(single.columns) {
		return single
	}
	count := shortcutsColumnCount(inner, len(singleLines))
	if count <= 1 {
		return single
	}
	assignment := m.balanceShortcutGroups(shortcutGroups, count, inner)
	if len(assignment) < 2 {
		return single
	}
	widths := m.fitShortcutColumnWidths(assignment, inner)
	rendered := m.renderShortcutColumns(assignment, widths)
	if !shortcutsBodyShowsAllActions(rendered) {
		return single
	}
	body := joinRenderedShortcutColumns(rendered, widths, inner)
	if len(body) <= budget {
		return shortcutsLayout{body: body, columns: rendered}
	}
	return single
}

// shortcutsSheetBody lays out every group from shortcutGroups, flowing into
// multiple columns when the card is wide enough.
func (m *Model) shortcutsSheetBody(inner int) []string {
	return m.shortcutsLayout(inner).body
}

// renderShortcutColumns renders each column at its fitted width.
func (m *Model) renderShortcutColumns(assignment [][]shortcutGroup, widths []int) []shortcutRenderedColumn {
	if len(widths) != len(assignment) {
		return nil
	}
	columns := make([]shortcutRenderedColumn, len(assignment))
	for index, groups := range assignment {
		columns[index] = shortcutRenderedColumn{
			groups: groups,
			lines:  m.shortcutsGroupsLines(widths[index], groups, false, true),
		}
	}
	return columns
}

// joinRenderedShortcutColumns joins rendered columns at the fitted widths.
func joinRenderedShortcutColumns(columns []shortcutRenderedColumn, widths []int, inner int) []string {
	stacks := make([][]string, len(columns))
	for index, column := range columns {
		stacks[index] = column.lines
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
	plainAction := displayShortcutAction(help.Desc)
	actionIndent := labelWidth + shortcutsMenuGutter
	var actionLines []string
	if wrapActions {
		actionLines = shortcutsWrapPlainAction(plainAction, actionWidth)
	} else {
		actionLines = []string{plainAction}
	}
	firstAction := actionStyle.Render(actionLines[0])
	line := keyColumn + strings.Repeat(" ", shortcutsMenuGutter) + clipShortcutLine(firstAction, actionWidth)
	lines := []string{line}
	for index := 1; index < len(actionLines); index++ {
		continuation := actionStyle.Render(actionLines[index])
		contLine := strings.Repeat(" ", actionIndent) + clipShortcutLine(continuation, colInner-actionIndent)
		lines = append(lines, clipShortcutLine(contLine, colInner))
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

// balanceShortcutGroups assigns whole groups to columns. It keeps the partition
// with the shortest joined height that still shows every action. When heights
// tie, the grouping whose groups follow source order wins, and columns are
// ordered by their first group's place in shortcutGroups.
func (m *Model) balanceShortcutGroups(groups []shortcutGroup, columns, inner int) [][]shortcutGroup {
	if len(groups) == 0 {
		return balanceShortcutGroupsGreedy(groups, columns)
	}
	bestHeight := int(^uint(0) >> 1)
	var best [][]shortcutGroup
	budget := m.overlayCardRowBudget() - shortcutsHeaderRows
	for _, assignment := range collectShortcutAssignments(groups, columns) {
		ordered := sortShortcutAssignmentColumns(assignment)
		widths := m.fitShortcutColumnWidths(ordered, inner)
		if !shortcutWidthsFit(widths, inner) {
			continue
		}
		rendered := m.renderShortcutColumns(ordered, widths)
		if !shortcutsBodyShowsAllActions(rendered) {
			continue
		}
		body := joinRenderedShortcutColumns(rendered, widths, inner)
		if len(body) > budget {
			continue
		}
		if len(body) < bestHeight || (len(body) == bestHeight && (best == nil || shortcutAssignmentSourceLess(ordered, best))) {
			bestHeight = len(body)
			best = ordered
		}
	}
	if best != nil {
		return best
	}
	// No partition fits the row budget; keep groups whole in one scrollable column.
	return [][]shortcutGroup{groups}
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

func shortcutGroupIndex(group shortcutGroup) int {
	for index, candidate := range shortcutGroups {
		if candidate.title == group.title {
			return index
		}
	}
	return len(shortcutGroups)
}

// shortcutAssignmentReadingOrder is the source index of each group, read down
// the left column and then the next.
func shortcutAssignmentReadingOrder(assignment [][]shortcutGroup) []int {
	var order []int
	for _, column := range assignment {
		for _, group := range column {
			order = append(order, shortcutGroupIndex(group))
		}
	}
	return order
}

// shortcutAssignmentSourceLess reports whether left follows shortcutGroups
// more closely than right. Columns are compared after source order, then by
// the group sequence down each column.
func shortcutAssignmentSourceLess(left, right [][]shortcutGroup) bool {
	leftOrder := shortcutAssignmentReadingOrder(sortShortcutAssignmentColumns(left))
	rightOrder := shortcutAssignmentReadingOrder(sortShortcutAssignmentColumns(right))
	for index := 0; index < len(leftOrder) && index < len(rightOrder); index++ {
		if leftOrder[index] != rightOrder[index] {
			return leftOrder[index] < rightOrder[index]
		}
	}
	return len(leftOrder) < len(rightOrder)
}

func sortShortcutAssignmentColumns(assignment [][]shortcutGroup) [][]shortcutGroup {
	if len(assignment) < 2 {
		return assignment
	}
	indexed := make([]struct {
		key    int
		column []shortcutGroup
	}, len(assignment))
	for index, column := range assignment {
		key := len(shortcutGroups)
		if len(column) > 0 {
			key = shortcutGroupIndex(column[0])
		}
		indexed[index] = struct {
			key    int
			column []shortcutGroup
		}{key: key, column: column}
	}
	for left := 1; left < len(indexed); left++ {
		pivot := indexed[left]
		right := left
		for scan := left - 1; scan >= 0; scan-- {
			if indexed[scan].key <= pivot.key {
				break
			}
			indexed[scan+1] = indexed[scan]
			right = scan
		}
		indexed[right] = pivot
	}
	out := make([][]shortcutGroup, len(indexed))
	for index, entry := range indexed {
		out[index] = entry.column
	}
	return out
}

// shortcutWidthsFit reports whether widths plus the column gutters stay inside
// inner. A missing width is not a fit.
func shortcutWidthsFit(widths []int, inner int) bool {
	if len(widths) == 0 {
		return false
	}
	total := (len(widths) - 1) * shortcutsColumnGap
	for _, width := range widths {
		if width < 1 {
			return false
		}
		total += width
	}
	return total <= inner
}

// shortcutsBodyShowsAllActions reports whether every action is present in its
// own column. Each column is folded on its own lines. A joined row is not
// searched, because the other column would sit between a wrapped label.
func shortcutsBodyShowsAllActions(columns []shortcutRenderedColumn) bool {
	if len(columns) == 0 {
		return false
	}
	seen := make(map[string]bool)
	for _, column := range columns {
		plain := shortcutsPlainFold(shortcutsStripAnsi(strings.Join(column.lines, "\n")))
		for _, group := range column.groups {
			for _, row := range group.rows {
				action := displayShortcutAction(row.action)
				if !strings.Contains(plain, action) {
					return false
				}
				seen[action] = true
			}
		}
	}
	for _, group := range shortcutGroups {
		for _, row := range group.rows {
			if !seen[displayShortcutAction(row.action)] {
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

// shortcutsStackMinInner is the narrowest inner width that fits the stack's rows
// on one line without wrapping the action label.
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

// shortcutsStackMinInnerWrap is the narrowest column width that still renders
// every keycap when long actions wrap onto the next line.
func shortcutsStackMinInnerWrap(groups []shortcutGroup) int {
	need := shortcutsMinColumnInner
	keyText := 0
	for _, group := range groups {
		for _, row := range group.rows {
			if width := lipgloss.Width(displayShortcutKeys(row.keys)); width > keyText {
				keyText = width
			}
		}
	}
	column := keyText + 2 + shortcutsMenuGutter + 4
	if column > need {
		need = column
	}
	return need
}

// fitShortcutColumnWidths sizes each column from its groups. Widths plus gaps
// never exceed inner; for two columns it picks the split with the shortest stack.
func (m *Model) fitShortcutColumnWidths(assignment [][]shortcutGroup, inner int) []int {
	mins := make([]int, len(assignment))
	for index, groups := range assignment {
		mins[index] = shortcutsStackMinInnerWrap(groups)
	}
	if len(mins) == 0 {
		return mins
	}
	gaps := (len(mins) - 1) * shortcutsColumnGap
	available := inner - gaps
	if available < len(mins) {
		available = len(mins)
	}
	if len(mins) == 2 {
		return m.fitTwoShortcutColumnWidths(assignment, mins, available)
	}
	widths := append([]int(nil), mins...)
	preferred := 0
	for _, width := range widths {
		preferred += width
	}
	if preferred > available {
		widths = scaleShortcutColumnWidths(widths, available)
	} else if spare := available - preferred; spare > 0 {
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

// fitTwoShortcutColumnWidths searches every legal split for the shortest join
// that still shows every action in its own column. A split that clips an
// action is skipped. There is no unvalidated fallback width.
func (m *Model) fitTwoShortcutColumnWidths(assignment [][]shortcutGroup, mins []int, available int) []int {
	if len(assignment) != 2 || len(mins) != 2 || mins[0] > available-mins[1] {
		return nil
	}
	bestHeight := int(^uint(0) >> 1)
	var best []int
	for left := mins[0]; left <= available-mins[1]; left++ {
		right := available - left
		widths := []int{left, right}
		rendered := m.renderShortcutColumns(assignment, widths)
		if !shortcutsBodyShowsAllActions(rendered) {
			continue
		}
		body := joinRenderedShortcutColumns(rendered, widths, available+shortcutsColumnGap)
		if len(body) < bestHeight {
			bestHeight = len(body)
			best = append([]int(nil), widths...)
		}
	}
	return best
}

// scaleShortcutColumnWidths shrinks preferred widths proportionally to available.
func scaleShortcutColumnWidths(widths []int, available int) []int {
	preferred := 0
	for _, width := range widths {
		preferred += width
	}
	scaled := make([]int, len(widths))
	remaining := available
	for index, width := range widths {
		share := available * width / preferred
		if share < 1 {
			share = 1
		}
		scaled[index] = share
		remaining -= share
	}
	for remaining > 0 {
		widest := 0
		for index, width := range scaled {
			if width > scaled[widest] {
				widest = index
			}
		}
		scaled[widest]++
		remaining--
	}
	for remaining < 0 {
		shrink := 0
		for index, width := range scaled {
			if width > scaled[shrink] {
				shrink = index
			}
		}
		if scaled[shrink] <= 1 {
			break
		}
		scaled[shrink]--
		remaining++
	}
	return scaled
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
		out[row] = clipShortcutLine(line.String(), inner)
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

// shortcutsWrapPlainAction breaks one action label across lines at spaces when
// possible. Each line fits within width terminal cells.
func shortcutsWrapPlainAction(plain string, width int) []string {
	if width < 1 {
		width = 1
	}
	if lipgloss.Width(plain) <= width {
		return []string{plain}
	}
	words := strings.Fields(plain)
	if len(words) == 0 {
		return []string{clipPlainWidth(plain, width)}
	}
	var lines []string
	var current strings.Builder
	for _, word := range words {
		candidate := word
		if current.Len() > 0 {
			candidate = current.String() + " " + word
		}
		if lipgloss.Width(candidate) <= width {
			current.Reset()
			current.WriteString(candidate)
			continue
		}
		if current.Len() > 0 {
			lines = append(lines, current.String())
			current.Reset()
		}
		for lipgloss.Width(word) > width {
			segment := clipPlainWidth(word, width)
			lines = append(lines, segment)
			word = strings.TrimPrefix(word, segment)
		}
		current.WriteString(word)
	}
	if current.Len() > 0 {
		lines = append(lines, current.String())
	}
	return lines
}

func clipPlainWidth(text string, width int) string {
	if width < 1 {
		return ""
	}
	var out strings.Builder
	cells := 0
	for _, runeValue := range text {
		runeWidth := lipgloss.Width(string(runeValue))
		if cells+runeWidth > width {
			break
		}
		out.WriteRune(runeValue)
		cells += runeWidth
	}
	return out.String()
}

var shortcutsPlainSpace = regexp.MustCompile(`\s+`)

// shortcutsPlainFold collapses whitespace so wrapped labels still match.
func shortcutsPlainFold(plain string) string {
	return shortcutsPlainSpace.ReplaceAllString(plain, " ")
}

var shortcutsAnsiPattern = regexp.MustCompile("\x1b\\[[0-9;]*m")

func shortcutsStripAnsi(text string) string {
	return shortcutsAnsiPattern.ReplaceAllString(text, "")
}
