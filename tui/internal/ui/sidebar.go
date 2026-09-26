package ui

import (
	"fmt"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// sidebarView renders the network roster: one header per registered network,
// then a CHANNELS group and a DIRECT MESSAGES group. It is data-driven from
// the controller snapshots, never a scan of rendered children. A collapsed
// network keeps its header and hides its groups; the focused header is
// highlighted so Alt+Left / Alt+Right land somewhere visible.
func (m *Model) sidebarView(width, height int) string {
	grouped := make(map[string][]controller.ConversationSnapshot)
	for _, row := range m.ctrl.Conversations() {
		grouped[row.NetworkID] = append(grouped[row.NetworkID], row)
	}

	lines := make([]string, 0, height)
	for _, networkID := range m.ctrl.NetworkOrder() {
		name := m.ctrl.NetworkDisplayName(networkID)
		if name == "" {
			name = networkID
		}
		lines = append(lines, m.networkHeader(name, networkID))
		if m.ctrl.IsNetworkCollapsed(networkID) {
			continue
		}
		lines = append(lines, m.sidebarGroup("CHANNELS", grouped[networkID], false)...)
		lines = append(lines, m.sidebarGroup("DIRECT MESSAGES", grouped[networkID], true)...)
	}
	return renderColumn(m.styles.Conversation, width, fitLines(lines, height, false))
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
	return style.Render(text)
}
