package ui

import (
	"strings"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
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

// Model is the Bubble Tea shell state over one controller and the Connect
// sheet's connection model. conn is nil in the seeded demo, which has no
// profile to edit.
type Model struct {
	ctrl   *controller.Controller
	conn   *connection.Connection
	styles Styles

	width  int
	height int
	focus  focusArea

	composer textinput.Model

	// connectOpen reopens the sheet after a profile exists. On first run the
	// sheet is visible because the connection still requires setup.
	connectOpen bool
	sheet       connectSheetState

	jump jumpState

	// drafts keeps unsent composer text per conversation id, or per Status
	// surface ("status\n<networkID>").
	drafts   map[string]string
	draftKey string

	// statusReturnID is the conversation Escape returns to after Status.
	statusReturnID string
}

// focusArea names where keyboard input goes. The Connect sheet and the jump
// overlay are modal and own the keys while they are open.
type focusArea int

const (
	focusComposer focusArea = iota
	focusConnect
	focusJump
)

// New returns a shell over ctrl with fixed width defaults and the composer
// focused. A nil controller renders the empty state. conn may be nil (the
// seeded demo), in which case the Connect sheet is unavailable.
func New(ctrl *controller.Controller, conn *connection.Connection) *Model {
	composer := textinput.New()
	composer.Placeholder = "Message"
	composer.Prompt = ""
	m := &Model{
		ctrl:     ctrl,
		conn:     conn,
		styles:   defaultStyles(),
		width:    defaultWidth,
		height:   defaultHeight,
		focus:    focusComposer,
		composer: composer,
		sheet:    newConnectSheetState(),
		jump:     newJumpState(),
		drafts:   make(map[string]string),
	}
	m.resize()
	m.draftKey = m.composerDraftKey()
	if m.connectVisible() {
		m.focus = focusConnect
		m.composer.Blur()
		m.syncSheetField()
	} else {
		_ = m.composer.Focus()
	}
	return m
}

// Init starts the shell. Phase 4 has no startup command.
func (m *Model) Init() tea.Cmd { return nil }

// Update folds one Bubble Tea message. Quit chords leave the program; the
// Connect sheet and the jump overlay are modal; otherwise the navigation
// chords run before the rest reaches the composer.
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
		return m.handleKey(msg)
	}
	if m.connectVisible() {
		var cmd tea.Cmd
		m.sheet.input, cmd = m.sheet.input.Update(msg)
		return m, cmd
	}
	if m.jumpVisible() {
		var cmd tea.Cmd
		m.jump.input, cmd = m.jump.input.Update(msg)
		return m, cmd
	}
	var cmd tea.Cmd
	m.composer, cmd = m.composer.Update(msg)
	return m, cmd
}

// handleKey routes one key press to the open overlay or the navigation chords.
func (m *Model) handleKey(msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	key := msg.String()
	if key == "ctrl+q" || key == "ctrl+c" {
		return m, tea.Quit
	}
	if m.connectVisible() {
		m.focus = focusConnect
		return m.handleConnectKey(key, msg)
	}
	if m.jumpVisible() {
		m.focus = focusJump
		return m.handleJumpKey(key, msg)
	}
	m.focus = focusComposer
	switch key {
	case "enter":
		m.sendComposer()
		return m, nil
	case "alt+down":
		m.walk(1)
		m.refocusComposer()
		return m, nil
	case "alt+up":
		m.walk(-1)
		m.refocusComposer()
		return m, nil
	case "alt+a":
		m.jumpUnread()
		m.refocusComposer()
		return m, nil
	case "ctrl+k":
		m.openJump()
		return m, nil
	case "ctrl+`":
		m.toggleStatus()
		m.refocusComposer()
		return m, nil
	case "ctrl+,":
		m.openConnect()
		return m, nil
	case "esc", "escape":
		m.dismissOverlay()
		m.refocusComposer()
		return m, nil
	}
	var cmd tea.Cmd
	m.composer, cmd = m.composer.Update(msg)
	m.saveDraft()
	return m, cmd
}

// sendComposer sends the composer text to the selected conversation and clears
// it, along with its stored draft.
func (m *Model) sendComposer() {
	value := m.composer.Value()
	if strings.TrimSpace(value) != "" && m.ctrl != nil {
		m.ctrl.SendMessage(value)
	}
	m.composer.Reset()
	m.clearCurrentDraft()
}

// refocusComposer restores composer focus after a chord when no overlay is
// open.
func (m *Model) refocusComposer() {
	if m.connectVisible() || m.jumpVisible() {
		return
	}
	_ = m.composer.Focus()
}

// View renders the shell into the declarative View: the alternate screen and
// the Qt-equivalent window title.
func (m *Model) View() tea.View {
	v := tea.NewView(m.render())
	v.AltScreen = true
	v.WindowTitle = Title(m.ctrl, m.conn)
	return v
}

// resize keeps the composer, sheet, and jump input widths in step with the
// window.
func (m *Model) resize() {
	width := m.width - composerPrefixWidth
	if width < 1 {
		width = 1
	}
	m.composer.SetWidth(width)
	m.sheet.input.SetWidth(m.overlayInputWidth())
	m.jump.input.SetWidth(m.overlayInputWidth())
}

// render composes the three columns, the composer, and any open overlay. It
// never indexes a slice unguarded, so a tiny or empty terminal renders a short
// line instead of panicking.
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
	bodyLines := strings.Split(body, "\n")

	if m.connectVisible() {
		bodyLines = m.overlayLines(bodyLines, m.connectCardLines(m.width))
	} else if m.jumpVisible() {
		bodyLines = m.overlayLines(bodyLines, m.jumpCardLines(m.width))
	}

	body = strings.Join(bodyLines, "\n")
	return lipgloss.JoinVertical(lipgloss.Left, body, m.composerView())
}

// overlayLines dims the body and drops the card into its vertical middle. The
// card lines are already exact-width bordered rows, so no ANSI compositing is
// needed.
func (m *Model) overlayLines(body, card []string) []string {
	if len(card) == 0 {
		return body
	}
	out := make([]string, len(body))
	for index, line := range body {
		out[index] = m.styles.Dimmer.Render(line)
	}
	start := (len(out) - len(card)) / 2
	if start < 0 {
		start = 0
	}
	for index, line := range card {
		if start+index >= len(out) {
			break
		}
		out[start+index] = line
	}
	return out
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
