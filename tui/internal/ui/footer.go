package ui

import (
	"strings"

	"charm.land/bubbles/v2/help"
	"charm.land/lipgloss/v2"
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

// footerVisible reports whether the footer fits: the composer block, the footer
// row, and at least one row of body above them. A short terminal drops the
// footer before it starves the columns. It is false without a controller, so
// the empty render stays a single line.
func (m *Model) footerVisible() bool {
	if m == nil || m.ctrl == nil {
		return false
	}
	return m.height-len(m.slashLines())-composerFieldHeight-footerHeight >= 1
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
			network = m.ctrl.NetworkDisplayName(id)
			if network == "" {
				network = id
			}
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
// right, separated out to the window width. The help is rendered from
// footerKeyMap, so the hint set is the chord map, never a hand-written string.
func (m *Model) footerView() string {
	left := m.footerStatusView()
	available := m.width - lipgloss.Width(left) - 1
	if available < 0 {
		available = 0
	}
	// Copy the help model so a render never mutates the shell state.
	h := m.help
	h.SetWidth(available)
	right := h.View(footerKeyMap{m: m})
	gap := m.width - lipgloss.Width(left) - lipgloss.Width(right)
	if gap < 1 {
		gap = 1
	}
	return truncateLine(left+strings.Repeat(" ", gap)+right, m.width)
}
