package ui

import (
	"strings"

	"charm.land/bubbles/v2/help"
	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/version"
)

// footerHeight is the single row the status/help footer occupies below the
// composer. Model.bodyHeight subtracts it whenever footerVisible reports there
// is room.
const footerHeight = 1

// helpStyles maps the theme-driven shell palette onto the bubbles/help view so
// the footer's hint reads like the rest of the interface instead of the
// library defaults.
func helpStyles(styles Styles) help.Styles {
	return help.Styles{
		ShortKey:       styles.Keycap,
		ShortDesc:      styles.FooterHint,
		ShortSeparator: styles.Divider,
		Ellipsis:       styles.FooterHint,
		FullKey:        styles.Keycap,
		FullDesc:       styles.FooterHint,
		FullSeparator:  styles.Divider,
	}
}

// footerVisible reports whether the footer fits: the footer row, the composer
// block inside the middle column, and at least one transcript row above it. A
// short terminal drops the footer before it starves the transcript. The slash
// menu does not affect it — the menu floats over the transcript instead of
// stacking above the composer. It is false without a controller, so the empty
// render stays a single line.
func (m *Model) footerVisible() bool {
	if m == nil || m.ctrl == nil {
		return false
	}
	return m.height-composerFieldHeight-footerHeight >= 1
}

// footerStatusView renders the footer's left side: a spinner while the focused
// network is disconnected (a steady dot once connected), the connection state,
// and the focused network's display name.
func (m *Model) footerStatusView() string {
	state := "Offline"
	network := ""
	if m.ctrl != nil {
		if got := m.ctrl.ConnectionStatus(); got != "" {
			state = got
		}
		if id := m.ctrl.FocusedNetworkID(); id != "" {
			network = m.sidebarNetworkDisplayName(id)
		}
	}
	mark := m.spinner.View()
	if state == "Connected" {
		mark = m.styles.StatusOK.Render("●")
	}
	text := state
	if network != "" {
		text += " · " + network
	}
	return mark + " " + m.styles.Footer.Render(text)
}

// footerView lays the status line on the left and the contextual help on the
// right, separated out to the window width, with the build version trailing the
// help at the far right. The help is rendered from footerKeyMap, so the hint set
// is the chord map, never a hand-written string.
func (m *Model) footerView() string {
	left := m.footerStatusView()
	// The build version sits at the bottom right, after the shortcut list. It is
	// dropped only when the status line and two gutters already fill the row, so
	// it is never rendered cut in half on a narrow terminal.
	label := m.styles.FooterHint.Render(version.Value)
	version := ""
	if m.width-lipgloss.Width(left)-2 >= lipgloss.Width(label) {
		version = label
	}
	// The help gets whatever is left once the status, the version, and the
	// separating space are reserved, so the help model can never crowd the
	// version off the right edge.
	reserved := lipgloss.Width(version)
	if version != "" {
		reserved++ // the space between the shortcut list and the version
	}
	available := m.width - lipgloss.Width(left) - 1 - reserved
	if available < 0 {
		available = 0
	}
	// Copy the help model so a render never mutates the shell state.
	h := m.help
	h.SetWidth(available)
	right := h.View(footerKeyMap{m: m})
	gap := m.width - lipgloss.Width(left) - lipgloss.Width(right) - reserved
	if gap < 1 {
		gap = 1
	}
	line := left + strings.Repeat(" ", gap) + right
	if version != "" {
		line += " " + version
	}
	return truncateLine(line, m.width)
}
