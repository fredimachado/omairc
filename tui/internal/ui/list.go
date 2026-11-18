package ui

import (
	"fmt"
	"strings"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"charm.land/lipgloss/v2/table"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// ChannelListRowView and ChannelListSnapshot alias the controller's channel
// list types so the shell renders them without importing internal/irc.
type ChannelListRowView = controller.ChannelListRow
type ChannelListSnapshot = controller.ChannelListSnapshot

// channelListState is the /list overlay: a filter input, a highlighted row
// and the rows streamed by the controller. It is modal while open.
type channelListState struct {
	input    textinput.Model
	selected int
	scroll   int
}

// channelListRowCap bounds how many streamed rows the overlay renders. The
// controller can report thousands of channels; the cap keeps one render cheap
// while the visible window stays the real limiter (the shared card frame
// clips it).
const channelListRowCap = 200

// channelListVisible reports whether the /list overlay is open.
func (m *Model) channelListVisible() bool { return m.listSheet() != nil }

// openChannelList shows the overlay with the controller's current filter.
func (m *Model) openChannelList() {
	if m.ctrl == nil {
		return
	}
	sheet := &channelListState{input: newTextInput(m.styles, "Filter channels", overlayFilterPrompt)}
	sheet.input.SetValue(m.ctrl.ChannelListSnapshot().Filter)
	sheet.input.SetWidth(m.overlayInputWidth())
	m.setOverlay(sheet)
	m.composer.Blur()
	_ = sheet.input.Focus()
}

// closeChannelList dismisses the overlay, drops the controller's list request
// (channelListState.discard), and returns focus to the composer.
func (m *Model) closeChannelList() {
	if m.listSheet() == nil {
		return
	}
	m.clearOverlay()
	_ = m.composer.Focus()
	m.loadDraft()
}

// syncChannelList opens the overlay when the controller reports an active
// list, and closes it when the controller cleared it.
func (m *Model) syncChannelList() {
	if m.ctrl == nil {
		return
	}
	snapshot := m.ctrl.ChannelListSnapshot()
	if snapshot.Open && m.listSheet() == nil {
		m.openChannelList()
		return
	}
	if !snapshot.Open && m.listSheet() != nil {
		// The controller already closed the list, so drop the sheet without
		// clearOverlay: its discard would dismiss the list a second time.
		m.overlay = nil
		_ = m.composer.Focus()
	}
}

// channelListRows returns the controller's visible rows, or nil.
func (m *Model) channelListRows() []controller.ChannelListRow {
	if m.ctrl == nil {
		return nil
	}
	return m.ctrl.ChannelListSnapshot().Rows
}

// moveChannelList moves the highlight by delta, wrapping.
func (m *Model) moveChannelList(delta int) {
	sheet := m.listSheet()
	if sheet == nil {
		return
	}
	count := len(m.channelListRows())
	if count == 0 {
		sheet.selected = 0
		return
	}
	sheet.selected = ((sheet.selected+delta)%count + count) % count
	m.revealList(&sheet.scroll, sheet.selected, count, m.channelListHeaderLines())
}

// activateChannelList opens the highlighted channel and dismisses the overlay.
func (m *Model) activateChannelList() {
	sheet := m.listSheet()
	if sheet == nil {
		return
	}
	index := sheet.selected
	m.closeChannelList()
	if m.ctrl == nil {
		return
	}
	previousID, wasConsole := m.beginTranscriptSwitch()
	if m.ctrl.JoinChannelListRow(index) {
		m.afterSelectionChange(previousID, wasConsole)
	} else {
		m.suspendScrollMemory = false
	}
}

// handleChannelListKey folds one key while the overlay is open. Escape
// dismisses to the composer, Up/Down highlight, Enter joins, and everything
// else filters.
func (m *Model) handleChannelListKey(key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	sheet := m.listSheet()
	if sheet == nil {
		return m, nil
	}
	switch key {
	case "esc", "escape":
		m.closeChannelList()
		return m, nil
	case "up":
		m.moveChannelList(-1)
		return m, nil
	case "down":
		m.moveChannelList(1)
		return m, nil
	case "enter":
		m.activateChannelList()
		return m, nil
	}
	cmd := sheet.updateInput(msg)
	if m.ctrl != nil {
		m.ctrl.SetChannelListFilter(sheet.input.Value())
	}
	sheet.selected = 0
	sheet.scroll = 0
	return m, cmd
}

// channelListCard renders the /list overlay as one card block through the
// shared overlay frame in model.go. The name, mask, and match count sit on the
// top edge and the highlighted row's position sits on the bottom edge.
func (m *Model) channelListCard(width int) string {
	sheet := m.listSheet()
	if sheet == nil {
		return ""
	}
	snapshot := m.channelListSnapshot()
	return m.overlayCardFramed(width,
		channelListFrameTitle(snapshot),
		overlayCountLabel(sheet.selected, len(snapshot.Rows)),
		m.channelListCardBody)
}

// channelListSnapshot is the controller's current /list snapshot, or an empty
// one when no controller is bound.
func (m *Model) channelListSnapshot() ChannelListSnapshot {
	if m == nil || m.ctrl == nil {
		return ChannelListSnapshot{}
	}
	return m.ctrl.ChannelListSnapshot()
}

// channelListFrameTitle is the card title: "Channels", plus the list mask when
// one was requested, plus the match count once the reply is no longer the
// empty loading state.
func channelListFrameTitle(snapshot ChannelListSnapshot) string {
	title := "Channels"
	if snapshot.Mask != "" {
		title += " · " + snapshot.Mask
	}
	if snapshot.Loading && len(snapshot.Rows) == 0 {
		return title
	}
	return fmt.Sprintf("%s %d", title, len(snapshot.Rows))
}

// channelListCardBody is the /list overlay's content, before the shared frame.
// inner is the content width inside the border.
func (m *Model) channelListCardBody(inner int) []string {
	return overlayTexts(m.listOverlayLines(inner))
}

// channelListHeaderLines is the filter line plus the table's header and rule.
// Data rows start after it. The table renders those two chrome lines whenever
// it has rows; the constant matches BorderHeader(true) with the other borders off.
func (m *Model) channelListHeaderLines() int { return 1 + channelListTableChrome }

const channelListTableChrome = 2

// listOverlayLines is the /list card body. The filter and the table header are
// not rows. Each data line is one channel.
func (m *Model) listOverlayLines(inner int) []overlayLine {
	sheet := m.listSheet()
	if sheet == nil {
		return nil
	}
	snapshot := m.channelListSnapshot()
	lines := []overlayLine{{text: truncateLine(sheet.input.View(), inner), row: -1}}
	switch {
	case snapshot.Loading:
		return append(lines, overlayLine{text: ellipsizeLine(m.styles.Empty.Render("Loading..."), inner), row: -1})
	case snapshot.Error != "":
		return append(lines, overlayLine{text: ellipsizeLine(m.styles.StatusErr.Render(snapshot.Error), inner), row: -1})
	case len(snapshot.Rows) == 0:
		return append(lines, overlayLine{text: ellipsizeLine(m.styles.Empty.Render("No matches"), inner), row: -1})
	}
	rows := snapshot.Rows
	if len(rows) > channelListRowCap {
		rows = rows[:channelListRowCap]
	}
	visible := m.overlayRowBudget(1) - channelListTableChrome
	if visible < 1 {
		visible = 1
	}
	start, end := listWindow(len(rows), sheet.scroll, visible)
	table := m.channelListTableLines(rows[start:end], inner, start)
	chrome := len(table) - (end - start)
	if chrome < 0 {
		chrome = 0
	}
	for index := 0; index < chrome && index < len(table); index++ {
		lines = append(lines, overlayLine{text: table[index], row: -1})
	}
	for index := chrome; index < len(table); index++ {
		lines = append(lines, overlayLine{text: table[index], row: start + (index - chrome)})
	}
	return lines
}

// channelListTableLines lays the streamed rows into a lipgloss table: one
// CHANNEL / USERS / TOPIC header row, a hairline rule under it, zebra-striped
// rows, right-aligned user counts, and a full-width fill on the highlighted
// row. Each element is one rendered line, truncated to inner so the shared card
// frame never widens. It mirrors the Qt list's columns and keeps the
// controller's 200-row cap.
// channelListTableLines lays the visible rows. origin is the absolute index
// of rows[0], so highlight and zebra stay aligned with the full list when the
// window is not at the top. origin 0 is the unwindowed table.
func (m *Model) channelListTableLines(rows []controller.ChannelListRow, inner, origin int) []string {
	selected := -1
	if sheet := m.listSheet(); sheet != nil {
		selected = sheet.selected
	}
	t := table.New().
		Headers("CHANNEL", "USERS", "TOPIC").
		Border(lipgloss.NormalBorder()).
		BorderStyle(m.styles.Divider).
		BorderTop(false).
		BorderBottom(false).
		BorderLeft(false).
		BorderRight(false).
		BorderColumn(false).
		BorderHeader(true).
		Wrap(false).
		Width(inner)
	for index, row := range rows {
		if index >= channelListRowCap {
			break
		}
		t.Row(row.Channel, fmt.Sprintf("%d", row.Users),
			m.ctrl.PlainChannelTopic(row.Topic))
	}
	t.StyleFunc(func(row, col int) lipgloss.Style {
		switch {
		case row == table.HeaderRow:
			return channelListCellStyle(col, m.styles.SectionHeader)
		case row+origin == selected:
			return channelListCellStyle(col, m.channelListSelectedStyle())
		case (row+origin)%2 == 1:
			return channelListCellStyle(col, m.channelListZebraStyle())
		default:
			return channelListCellStyle(col, m.channelListRowStyle())
		}
	})
	// The shared card frame is the one visible box, so the table keeps only its
	// header rule and every rendered line is clamped to the content width.
	rendered := strings.Split(t.String(), "\n")
	for index, line := range rendered {
		rendered[index] = truncateLine(line, inner)
	}
	return rendered
}

// channelListCellStyle gives every cell a one-cell gutter (so adjacent columns
// never touch) and right-aligns the USERS column.
func channelListCellStyle(col int, style lipgloss.Style) lipgloss.Style {
	style = style.Padding(0, 1)
	if col == 1 {
		style = style.Align(lipgloss.Right)
	}
	return style
}

// channelListRowStyle is a plain table cell: the theme's foreground on its
// default surface.
func (m *Model) channelListRowStyle() lipgloss.Style {
	return lipgloss.NewStyle().Foreground(m.styles.Colors.Foreground)
}

// channelListZebraStyle is the alternating row fill.
func (m *Model) channelListZebraStyle() lipgloss.Style {
	return lipgloss.NewStyle().
		Foreground(m.styles.Colors.Foreground).
		Background(m.styles.Colors.Surface)
}

// channelListSelectedStyle is the highlighted row: the selection surface with
// the window background as ink, so the whole row reads as one filled band.
func (m *Model) channelListSelectedStyle() lipgloss.Style {
	return lipgloss.NewStyle().
		Bold(true).
		Foreground(m.styles.Colors.Background).
		Background(m.styles.Colors.Selection)
}
