package ui

import (
	"strings"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// Layout constants. The column widths are proportional with clamps, so a
// narrow window shrinks the transcript before it drops a column.
const (
	defaultWidth  = 80
	defaultHeight = 24
	minWidth      = 24
	minHeight     = 5

	sidebarMinWidth = 16
	sidebarMaxWidth = 30
	membersWidth    = 22
	membersMinTotal = 72
)

// NotifyMsg tells the shell a controller callback fired and it should
// re-render. The main program sends it with p.Send(ui.NotifyMsg{}); Update
// treats it as a no-op that leaves the snapshots already rebuilt.
type NotifyMsg struct{}

// Model is the Bubble Tea shell state over one controller.
type Model struct {
	ctrl   *controller.Controller
	styles Styles

	width  int
	height int
	focus  focusArea

	composer textinput.Model
}

// focusArea names the column that owns keyboard focus. Phase 4 keeps it on the
// composer; later phases move it between columns.
type focusArea int

const (
	focusComposer focusArea = iota
)

// New returns a shell over ctrl with fixed width defaults and the composer
// focused. A nil controller renders the empty state.
func New(ctrl *controller.Controller) *Model {
	composer := textinput.New()
	composer.Placeholder = "Message"
	composer.Prompt = ""
	m := &Model{
		ctrl:     ctrl,
		styles:   defaultStyles(),
		width:    defaultWidth,
		height:   defaultHeight,
		focus:    focusComposer,
		composer: composer,
	}
	m.resize()
	_ = m.composer.Focus()
	return m
}

// Init starts the shell. Phase 4 has no startup command.
func (m *Model) Init() tea.Cmd { return nil }

// Update folds one Bubble Tea message. Quit chords leave the program, Enter
// sends and clears the composer, a resize relayouts, and everything else goes
// to the composer.
func (m *Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.resize()
		return m, nil
	case NotifyMsg:
		return m, nil
	case tea.KeyPressMsg:
		switch msg.String() {
		case "ctrl+q", "ctrl+c":
			return m, tea.Quit
		case "enter":
			value := m.composer.Value()
			if strings.TrimSpace(value) != "" && m.ctrl != nil {
				m.ctrl.SendMessage(value)
			}
			m.composer.Reset()
			return m, nil
		}
	}
	var cmd tea.Cmd
	m.composer, cmd = m.composer.Update(msg)
	return m, cmd
}

// View renders the shell into the declarative View: the alternate screen and
// the Qt-equivalent window title.
func (m *Model) View() tea.View {
	v := tea.NewView(m.render())
	v.AltScreen = true
	v.WindowTitle = Title(m.ctrl)
	return v
}

// resize keeps the composer input width in step with the window.
func (m *Model) resize() {
	width := m.width - composerPrefixWidth
	if width < 1 {
		width = 1
	}
	m.composer.SetWidth(width)
}

// render composes the three columns and the composer. It never indexes a slice
// unguarded, so a tiny or empty terminal renders a short line instead of
// panicking.
func (m *Model) render() string {
	if m == nil || m.ctrl == nil {
		return "Omairc"
	}
	if m.width < minWidth || m.height < minHeight {
		return m.composerView()
	}
	bodyHeight := m.height - 1
	if bodyHeight < 1 {
		bodyHeight = 1
	}

	columns := []string{
		m.sidebarView(sidebarWidth(m.width), bodyHeight),
		m.transcriptView(bodyHeight),
	}
	if m.membersVisible() {
		columns = append(columns, m.membersView(membersWidth, bodyHeight))
	}
	body := lipgloss.JoinHorizontal(lipgloss.Top, columns...)
	return lipgloss.JoinVertical(lipgloss.Left, body, m.composerView())
}

// membersVisible reports whether the member column fits and applies. It is
// shown only for channels, matching the Qt panel.
func (m *Model) membersVisible() bool {
	if m.ctrl == nil || !m.ctrl.IsChannel() {
		return false
	}
	return m.width >= membersMinTotal
}

// sidebarWidth clamps the sidebar's share of the window.
func sidebarWidth(width int) int {
	value := width / 4
	if value < sidebarMinWidth {
		value = sidebarMinWidth
	}
	if value > sidebarMaxWidth {
		value = sidebarMaxWidth
	}
	return value
}

// transcriptWidth is whatever the sidebar and member column leave behind.
func (m *Model) transcriptWidth() int {
	width := m.width - sidebarWidth(m.width)
	if m.membersVisible() {
		width -= membersWidth
	}
	if width < 1 {
		width = 1
	}
	return width
}

// fitLines clamps lines to limit, keeping the tail (the newest transcript
// rows) or the head (the top of a list), then pads with blanks so every column
// is exactly limit lines tall.
func fitLines(lines []string, limit int, tail bool) []string {
	if limit < 0 {
		limit = 0
	}
	if len(lines) > limit {
		if tail {
			lines = lines[len(lines)-limit:]
		} else {
			lines = lines[:limit]
		}
	}
	out := make([]string, 0, limit)
	out = append(out, lines...)
	for len(out) < limit {
		out = append(out, "")
	}
	return out
}

// renderColumn truncates each line to width, joins them, and pads every line
// to width so columns align. Truncating before the join keeps lipgloss from
// wrapping a long row into the next terminal line.
func renderColumn(style lipgloss.Style, width int, lines []string) string {
	trimmed := make([]string, len(lines))
	for index, line := range lines {
		trimmed[index] = truncateLine(line, width)
	}
	return style.Width(width).Render(strings.Join(trimmed, "\n"))
}

// truncateLine cuts one rendered line to width display cells without adding a
// style, so ANSI sequences survive.
func truncateLine(line string, width int) string {
	if width <= 0 {
		return ""
	}
	return lipgloss.NewStyle().MaxWidth(width).Inline(true).Render(line)
}

// composerPrefixWidth is the visible width of the composer's prompt glyph.
const composerPrefixWidth = 2
