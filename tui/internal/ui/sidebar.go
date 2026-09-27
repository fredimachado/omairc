package ui

import (
	"fmt"
	"strconv"
	"strings"

	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/version"
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
//
// The view returns inner content lines only: the outer panel card (border and
// title) is drawn by the frame that embeds this column.
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
		roster = append(roster, m.sidebarGroup("CHANNELS", grouped[networkID], false, width)...)
		roster = append(roster, m.sidebarGroup("DIRECT MESSAGES", grouped[networkID], true, width)...)
	}
	// Reserve the bottom row for the identity footer: clamp the roster to
	// height-1 and append the footer, so the column is exactly height lines.
	lines := fitLines(roster, height-1, false)
	lines = append(lines, m.identityFooterLine(width))
	return renderColumn(m.styles.Conversation, width, lines)
}

// identityFooterLine renders the sidebar's bottom identity row as an initials
// chip, the self nick, its presence word, and an "inbox N" pill when the
// waiting list is non-empty, with the build version trailing when the column is
// wide enough. The chip shows "?" for an empty nick, matching the direct-message
// identicon. It mirrors ServerListColumn.qml's identityFooter
// (selfNickLabel, selfPresenceLabel, inboxBadge). The footer is visual only;
// Ctrl+Shift+A reaches the sheet.
func (m *Model) identityFooterLine(width int) string {
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
	nick := m.ctrl.CurrentNick()

	chip := lipgloss.NewStyle().
		Foreground(nickColor(nick)).
		Background(avatarFill(nick)).
		Bold(true).
		Render(initials(nick))

	segments := []string{chip}
	if nick != "" {
		segments = append(segments, m.styles.Conversation.Bold(true).Render(nick))
	}
	segments = append(segments, m.styles.MutedLine.Render(presence))
	if count := m.ctrl.InboxCount(); count > 0 {
		segments = append(segments, m.badge(m.styles.BadgeUnread, fmt.Sprintf("inbox %d", count)))
	}

	footer := strings.Join(segments, " ")
	if label := m.styles.FooterHint.Render(version.Value); lipgloss.Width(footer)+lipgloss.Width(label)+1 <= width {
		footer += " " + label
	}
	return footer
}

// networkHeader names a network and carries its collapse chevron, mention pill,
// and unread total pill. The display name is rendered verbatim; the focused
// header uses the focus style.
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
		header += " " + m.badge(m.styles.BadgeMention, "@")
	}
	if unread := m.ctrl.UnreadCountFor(networkID); unread > 0 {
		header += " " + m.badge(m.styles.BadgeUnread, strconv.Itoa(unread))
	}
	return header
}

// sidebarGroup filters one network's rows into a channel or direct-message
// group and renders the section header plus each row. An empty group renders
// nothing, so a network with only channels has no direct-message heading.
func (m *Model) sidebarGroup(label string, rows []controller.ConversationSnapshot, direct bool, width int) []string {
	var members []controller.ConversationSnapshot
	for _, row := range rows {
		if row.Direct == direct {
			members = append(members, row)
		}
	}
	if len(members) == 0 {
		return nil
	}
	lines := []string{m.sectionHeader(label, width)}
	for _, row := range members {
		lines = append(lines, m.conversationRowWidth(row, width))
	}
	return lines
}

// sectionHeader renders an uppercase group label followed by a subtle rule that
// reaches the column width, so the roster reads as titled sections.
func (m *Model) sectionHeader(label string, width int) string {
	header := m.styles.SectionHeader.Render(label)
	rule := width - lipgloss.Width(header) - 1
	if rule <= 0 {
		return header
	}
	return header + " " + m.styles.Divider.Render(strings.Repeat("─", rule))
}

// badge renders a one-line pill: the badge's foreground over the raised
// surface, padded by one cell on each side so it reads as a pill rather than a
// bare number.
func (m *Model) badge(style lipgloss.Style, text string) string {
	return style.Background(m.styles.Colors.SurfaceRaised).Render(" " + text + " ")
}

// conversationRow renders one sidebar row without a width budget. It is the
// test-facing entry point; the roster calls conversationRowWidth so the selected
// row's accent bar and fill reach the column edge.
func (m *Model) conversationRow(row controller.ConversationSnapshot) string {
	return m.conversationRowWidth(row, 0)
}

// conversationRowWidth renders one sidebar row: an accent left bar on the
// selected row, a presence mark, the direct-message avatar, the target, and the
// mention, unread, and muted pills. The selected row wins over every other row
// style and fills the column width; the other rows are padded by the caller.
func (m *Model) conversationRowWidth(row controller.ConversationSnapshot, width int) string {
	selected := row.ConversationID != "" && row.ConversationID == m.ctrl.SelectedConversationID()

	// The selected row wins, then muted, then a mention, then the plain row.
	rowStyle := m.styles.Conversation
	if row.Mention {
		rowStyle = m.styles.MentionRow
	}
	if row.Muted {
		rowStyle = m.styles.MutedLine
	}
	if selected {
		rowStyle = m.styles.Selected.Background(m.styles.Colors.SurfaceRaised)
	}

	var line strings.Builder
	// The accent bar marks the selected conversation; every other row carries a
	// blank gutter so the rows stay aligned with it.
	if selected {
		line.WriteString(m.styles.Selected.
			Foreground(m.styles.Colors.Accent).
			Render("▌"))
	} else {
		line.WriteString(rowStyle.Render(" "))
	}
	// The presence dot stays visible on the selected fill.
	dot := " "
	dotStyle := rowStyle
	switch row.Presence {
	case "online":
		dot = "●"
		dotStyle = m.styles.MemberPresenceOnline
	case "away":
		dot = "○"
		dotStyle = m.styles.MemberPresenceAway
	}
	if selected && dot != " " {
		dotStyle = dotStyle.Background(m.styles.Colors.SurfaceRaised)
	}
	line.WriteString(dotStyle.Render(dot))
	line.WriteString(rowStyle.Render(" "))

	// The direct-message glyph is rendered separately from the row text so its
	// trailing reset cannot clear the row style.
	if glyph := strings.TrimSuffix(m.avatarGlyph(row), " "); glyph != "" {
		line.WriteString(glyph)
		line.WriteString(rowStyle.Render(" "))
	}
	line.WriteString(rowStyle.Render(row.Conversation))

	if row.Typing {
		line.WriteString(rowStyle.Render(" "))
		line.WriteString(m.styles.MemberTyping.Render("…"))
	}
	if row.Mention {
		line.WriteString(rowStyle.Render(" "))
		line.WriteString(m.badge(m.styles.BadgeMention, "@"))
	}
	if row.Unread > 0 {
		line.WriteString(rowStyle.Render(" "))
		badge := m.styles.BadgeUnread
		if row.Muted {
			badge = m.styles.BadgeMuted
		}
		line.WriteString(m.badge(badge, strconv.Itoa(row.Unread)))
	}
	if row.Muted {
		line.WriteString(rowStyle.Render(" "))
		line.WriteString(m.badge(m.styles.BadgeMuted, "muted"))
	}

	rendered := line.String()
	if width > 0 {
		if pad := width - lipgloss.Width(rendered); pad > 0 {
			rendered += rowStyle.Render(strings.Repeat(" ", pad))
		}
	}
	return rendered
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
