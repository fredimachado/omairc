package ui

import (
	"fmt"

	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// The sidebar's direct-message avatar is two half-block cells, matching the
// 22px QML avatar; internal/avatar quantizes that layout size to a 32px fetch.
const (
	avatarGlyphCols      = 2
	avatarGlyphPixelSize = 22
)

// sidebarView renders the network roster: one header per registered network,
// then a CHANNELS group and a DIRECT MESSAGES group, then the identity footer
// pinned to the bottom row. It is data-driven from the controller snapshots,
// never a scan of rendered children. A collapsed network keeps its header and
// hides its groups; the focused header is highlighted so Alt+Left / Alt+Right
// land somewhere visible.
func (m *Model) sidebarView(width, height int) string {
	grouped := make(map[string][]controller.ConversationSnapshot)
	for _, row := range m.ctrl.Conversations() {
		grouped[row.NetworkID] = append(grouped[row.NetworkID], row)
	}

	roster := make([]string, 0, height)
	for _, networkID := range m.ctrl.NetworkOrder() {
		name := m.ctrl.NetworkDisplayName(networkID)
		if name == "" {
			name = networkID
		}
		roster = append(roster, m.networkHeader(name, networkID))
		if m.ctrl.IsNetworkCollapsed(networkID) {
			continue
		}
		roster = append(roster, m.sidebarGroup("CHANNELS", grouped[networkID], false)...)
		roster = append(roster, m.sidebarGroup("DIRECT MESSAGES", grouped[networkID], true)...)
	}
	// Reserve the bottom row for the identity footer: clamp the roster to
	// height-1 and append the footer, so the column is exactly height lines.
	lines := fitLines(roster, height-1, false)
	lines = append(lines, m.identityFooterLine())
	return renderColumn(m.styles.Conversation, width, lines)
}

// identityFooterLine renders the sidebar's bottom identity row: the focused
// network's self nick, its presence word, and an "inbox N" badge when the
// waiting list is non-empty. It mirrors ServerListColumn.qml's identityFooter
// (selfNickLabel, selfPresenceLabel, inboxBadge) as one muted line. The footer
// is visual only; Ctrl+Shift+A reaches the sheet.
func (m *Model) identityFooterLine() string {
	if m.ctrl == nil {
		return ""
	}
	presence := "offline"
	if m.ctrl.ConnectionStatus() == "Connected" {
		if m.ctrl.SelfAway() {
			presence = "away"
		} else {
			presence = "available"
		}
	}
	text := presence
	if nick := m.ctrl.CurrentNick(); nick != "" {
		text = nick + " · " + presence
	}
	if count := m.ctrl.InboxCount(); count > 0 {
		text += fmt.Sprintf(" · inbox %d", count)
	}
	return m.styles.MutedLine.Render(text)
}

// networkHeader names a network and carries its collapse chevron, unread total,
// and mention marker. The focused header uses the focus style.
func (m *Model) networkHeader(name, networkID string) string {
	chevron := "▾"
	if m.ctrl.IsNetworkCollapsed(networkID) {
		chevron = "▸"
	}
	style := m.styles.NetworkName
	if m.sidebarNetworkFocusID == networkID {
		style = m.styles.NetworkNameFocused
	}
	header := style.Render(chevron + " " + name)
	if m.ctrl.MentionFor(networkID) {
		header += " " + m.styles.MentionRow.Render("@")
	}
	if unread := m.ctrl.UnreadCountFor(networkID); unread > 0 {
		header += " " + m.styles.Unread.Render(fmt.Sprintf("%d", unread))
	}
	return header
}

// sidebarGroup filters one network's rows into a channel or direct-message
// group and renders the group label plus each row. An empty group renders
// nothing, so a network with only channels has no direct-message heading.
func (m *Model) sidebarGroup(label string, rows []controller.ConversationSnapshot, direct bool) []string {
	var members []controller.ConversationSnapshot
	for _, row := range rows {
		if row.Direct == direct {
			members = append(members, row)
		}
	}
	if len(members) == 0 {
		return nil
	}
	lines := []string{m.styles.GroupLabel.Render(label)}
	for _, row := range members {
		lines = append(lines, m.conversationRow(row))
	}
	return lines
}

// conversationRow renders one sidebar row: a presence mark, the target, and
// the unread, mention, typing, and muted markers. The selected row wins over
// every other row style.
func (m *Model) conversationRow(row controller.ConversationSnapshot) string {
	mark := " "
	switch row.Presence {
	case "online":
		mark = "●"
	case "away":
		mark = "○"
	}

	text := mark + " " + row.Conversation
	if row.Mention {
		text += " •"
	}
	if row.Unread > 0 {
		text += " " + fmt.Sprintf("%d", row.Unread)
	}
	if row.Typing {
		text += " …"
	}
	if row.Muted {
		text += " muted"
	}

	style := m.styles.Conversation
	if row.Mention {
		style = m.styles.MentionRow
	}
	if row.Muted {
		style = m.styles.MutedLine
	}
	if row.ConversationID != "" && row.ConversationID == m.ctrl.SelectedConversationID() {
		style = m.styles.Selected
	}
	return m.avatarGlyph(row) + style.Render(text)
}

// avatarGlyph is the sidebar's direct-message avatar: two half-block cells
// rasterized from the cached peer image, or the nick identicon when avatars are
// off, the row carries no image, or the image is not cached yet. A channel row
// has no glyph. The glyph is rendered separately from the row text so its
// trailing reset cannot clear the row style.
func (m *Model) avatarGlyph(row controller.ConversationSnapshot) string {
	if !row.Direct {
		return ""
	}
	if m.ctrl != nil && m.ctrl.PrefAvatarsEnabled() && row.Avatar != "" && m.avatars != nil {
		if block, ok := m.avatars.Block(row.Avatar, avatarGlyphPixelSize, avatarGlyphCols); ok {
			return block + " "
		}
	}
	return m.identiconGlyph(row.Conversation) + " "
}

// identiconGlyph is the fallback avatar: one cell showing the nick initial,
// nickColor on an avatarFill background, followed by a separating space.
func (m *Model) identiconGlyph(nick string) string {
	return lipgloss.NewStyle().
		Foreground(nickColor(nick)).
		Background(avatarFill(nick)).
		Bold(true).
		Render(initials(nick))
}
