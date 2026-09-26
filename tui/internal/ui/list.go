package ui

import (
	"fmt"
	"strings"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"

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

func newChannelListState() channelListState {
	input := textinput.New()
	input.Placeholder = "Filter channels"
	input.Prompt = "› "
	return channelListState{input: input}
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
	if m.ctrl.JoinChannelListRow(index) {
		m.afterSelectionChange()
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

// channelListCardLines renders the overlay into a bordered block exactly width
// cells wide.
func (m *Model) channelListCardLines(width int) []string {
	inner := width - 4
	if inner < 8 {
		inner = 8
	}
	rendered := m.styles.SheetCard.Width(inner).Render(strings.Join(m.channelListCard(inner), "\n"))
	return strings.Split(rendered, "\n")
}

// channelListCard is the overlay's content, before the border.
func (m *Model) channelListCard(inner int) []string {
	snapshot := ChannelListSnapshot{}
	if m.ctrl != nil {
		snapshot = m.ctrl.ChannelListSnapshot()
	}
	title := "Channels"
	if snapshot.Mask != "" {
		title += " · " + snapshot.Mask
	}
	lines := []string{m.styles.SheetTitle.Render(title) + "  " + m.list.input.View()}
	switch {
	case snapshot.Loading:
		lines = append(lines, m.styles.JumpEmpty.Render("Loading..."))
	case snapshot.Error != "":
		lines = append(lines, m.styles.SheetProblem.Render(snapshot.Error))
	case len(snapshot.Rows) == 0:
		lines = append(lines, m.styles.JumpEmpty.Render("No matches"))
	default:
		for index, row := range snapshot.Rows {
			if index >= 200 {
				break
			}
			style := m.styles.JumpRow
			if index == m.list.selected {
				style = m.styles.JumpSelected
			}
			label := fmt.Sprintf("%-24s %5d  %s", row.Channel, row.Users,
				m.ctrl.PlainChannelTopic(row.Topic))
			lines = append(lines, style.Render(truncateLine(label, inner)))
		}
	}
	return lines
}
