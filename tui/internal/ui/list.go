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

// channelListState is the /list overlay: a filter input, a highlighted row,
// and the rows streamed by the controller. It is modal while open.
type channelListState struct {
	open     bool
	input    textinput.Model
	selected int
}

// channelListRowCap bounds how many streamed rows the overlay renders. The
// controller can report thousands of channels; the cap keeps one render cheap
// while the visible window stays the real limiter (the shared card frame
// clips it).
const channelListRowCap = 200

func newChannelListState() channelListState {
	// The filter adopts the shared text input. Its styles are re-applied from
	// the live palette when the overlay opens, because model.go owns
	// construction and does not know this input's styles.
	return channelListState{input: newTextInput(defaultStyles(), "Filter channels", "› ")}
}

// channelListVisible reports whether the /list overlay is open.
func (m *Model) channelListVisible() bool { return m != nil && m.list.open }

// openChannelList shows the overlay with the controller's current filter.
func (m *Model) openChannelList() {
	if m.ctrl == nil {
		return
	}
	m.closeAllOverlays()
	m.list.open = true
	m.list.selected = 0
	m.list.input.SetStyles(m.styles.Input)
	m.list.input.SetValue(m.ctrl.ChannelListSnapshot().Filter)
	m.list.input.SetWidth(m.overlayInputWidth())
	m.composer.Blur()
	_ = m.list.input.Focus()
}

// closeChannelList dismisses the overlay and returns focus to the composer.
func (m *Model) closeChannelList() {
	if !m.list.open {
		return
	}
	m.list.open = false
	m.list.input.Blur()
	if m.ctrl != nil {
		m.ctrl.DismissChannelList()
	}
	_ = m.composer.Focus()
	m.loadDraft()
}

// syncChannelList opens the overlay when the controller reports an active
// /list, and closes it when the controller cleared it.
func (m *Model) syncChannelList() {
	if m.ctrl == nil {
		return
	}
	snapshot := m.ctrl.ChannelListSnapshot()
	if snapshot.Open && !m.list.open {
		m.openChannelList()
		return
	}
	if !snapshot.Open && m.list.open {
		m.list.open = false
		m.list.input.Blur()
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
	count := len(m.channelListRows())
	if count == 0 {
		m.list.selected = 0
		return
	}
	m.list.selected = ((m.list.selected+delta)%count + count) % count
}

// activateChannelList opens the highlighted channel and dismisses the overlay.
func (m *Model) activateChannelList() {
	index := m.list.selected
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
	var cmd tea.Cmd
	m.list.input, cmd = m.list.input.Update(msg)
	if m.ctrl != nil {
		m.ctrl.SetChannelListFilter(m.list.input.Value())
	}
	m.list.selected = 0
	return m, cmd
}

// channelListCard renders the /list overlay as one card block through the
// shared overlay frame in model.go.
func (m *Model) channelListCard(width int) string {
	return m.overlayCardBlock(width, m.channelListCardBody)
}

// channelListCardBody is the /list overlay's content, before the shared frame.
// inner is the content width inside the border.
func (m *Model) channelListCardBody(inner int) []string {
	snapshot := ChannelListSnapshot{}
	if m.ctrl != nil {
		snapshot = m.ctrl.ChannelListSnapshot()
	}
	title := "Channels"
	if snapshot.Mask != "" {
		title += " · " + snapshot.Mask
	}
	titleRendered := m.styles.SheetTitle.Render(title)
	input := m.list.input.View()
	const gap = "  "
	header := titleRendered + gap + input
	if lipgloss.Width(header) > inner {
		// The filter is an input view, so it stays a hard cut. The title is the
		// label and takes an ellipsis when the row cannot hold both.
		titleBudget := inner - lipgloss.Width(gap) - lipgloss.Width(input)
		if titleBudget < 1 {
			header = truncateLine(input, inner)
		} else {
			header = ellipsizeLine(titleRendered, titleBudget) + gap + input
			if lipgloss.Width(header) > inner {
				header = truncateLine(header, inner)
			}
		}
	}
	lines := []string{header}
	switch {
	case snapshot.Loading:
		lines = append(lines, ellipsizeLine(m.styles.Empty.Render("Loading..."), inner))
	case snapshot.Error != "":
		lines = append(lines, ellipsizeLine(m.styles.StatusErr.Render(snapshot.Error), inner))
	case len(snapshot.Rows) == 0:
		lines = append(lines, ellipsizeLine(m.styles.Empty.Render("No matches"), inner))
	default:
		lines = append(lines, m.channelListTableLines(snapshot.Rows, inner)...)
	}
	return lines
}

// channelListTableLines lays the streamed rows into a lipgloss table: one
// CHANNEL / USERS / TOPIC header row, a hairline rule under it, zebra-striped
// rows, right-aligned user counts, and a full-width fill on the highlighted
// row. Each element is one rendered line, truncated to inner so the shared card
// frame never widens. It mirrors the Qt list's columns and keeps the
// controller's 200-row cap.
func (m *Model) channelListTableLines(rows []controller.ChannelListRow, inner int) []string {
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
		case row == m.list.selected:
			return channelListCellStyle(col, m.channelListSelectedStyle())
		case row%2 == 1:
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
