package ui

import (
	"fmt"
	"strconv"
	"strings"

	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// The sidebar's direct-message avatar is two half-block cells, matching the
// 22px QML avatar; internal/avatar quantizes that layout size to a 32px fetch.
const (
	avatarGlyphCols      = 2
	avatarGlyphPixelSize = 22
)

// sidebarNetworkIDs returns the network ids shown in the left sidebar, in
// display order. Stored profiles from the connection model are listed even
// before a session exists, matching ServerListColumn.qml's connection.networks
// repeater. The demo shell (conn == nil) falls back to registered sessions.
func (m *Model) sidebarNetworkIDs() []string {
	if m.conn != nil {
		rows := m.conn.Networks()
		ids := make([]string, 0, len(rows))
		for _, row := range rows {
			ids = append(ids, row.NetworkID)
		}
		return ids
	}
	if m.ctrl == nil {
		return nil
	}
	return m.ctrl.NetworkOrder()
}

// sidebarNetworkDisplayName returns the roster label for one sidebar network.
// It prefers the connection roster, then a live session, then the raw id.
func (m *Model) sidebarNetworkDisplayName(networkID string) string {
	if networkID == "" {
		return ""
	}
	if m.conn != nil {
		for _, row := range m.conn.Networks() {
			if row.NetworkID == networkID {
				return row.DisplayName
			}
		}
	}
	if m.ctrl != nil {
		if name := m.ctrl.NetworkDisplayName(networkID); name != "" {
			return name
		}
	}
	return networkID
}

// syncSidebarNetworkOrder copies the connection roster order into the
// controller so collapse, reorder, and jump overlays stay aligned with the
// sidebar. The demo shell has no connection model to mirror.
func (m *Model) syncSidebarNetworkOrder() {
	if m.conn == nil || m.ctrl == nil {
		return
	}
	m.ctrl.SetNetworkOrder(m.sidebarNetworkIDs())
}

// sidebarView renders the network roster: one header per stored or registered
// network, then a CHANNELS group and a DIRECT MESSAGES group, then the
// identity footer pinned to the bottom row. It is data-driven from the
// connection roster and controller snapshots, never a scan of rendered
// children. A collapsed network keeps its header and hides its groups; the
// focused header is highlighted so Alt+Left / Alt+Right land somewhere visible.
//
// A roster taller than the column scrolls as one window: the focused network
// header, or else the selected conversation, is kept on screen so a walk,
// unread jump, or header focus never lands on a clipped row. It mirrors
// OmaircWindow.qml's revealSidebarRow / revealNamedSidebarItem over the
// sidebarScroll Flickable.
//
// The view returns inner content lines only: the outer panel card (border and
// title) is drawn by the frame that embeds this column.
func (m *Model) sidebarView(width, height int) string {
	grouped := make(map[string][]controller.ConversationSnapshot)
	for _, row := range m.ctrl.Conversations() {
		grouped[row.NetworkID] = append(grouped[row.NetworkID], row)
	}

	roster := make([]string, 0, height)
	// headerSpans and rowSpans record where each revealable element landed in
	// the roster as [start, end) line indexes, so the window can slide to keep
	// the focused header or the selected conversation visible.
	headerSpans := make(map[string][2]int)
	rowSpans := make(map[string][2]int)
	groups := []struct {
		label  string
		direct bool
	}{{"CHANNELS", false}, {"DIRECT MESSAGES", true}}
	for index, networkID := range m.sidebarNetworkIDs() {
		// Networks are separate blocks: a blank row then a rule, above the
		// second onward, so the roster never reads as one continuous list.
		if index > 0 {
			roster = append(roster, "", m.sidebarDivider(width))
		}
		name := m.sidebarNetworkDisplayName(networkID)
		headerStart := len(roster)
		roster = append(roster, m.networkHeader(name, networkID))
		// A refused nick or registration ERROR is that network's status line,
		// the same slot as NetworkSection.qml's liveStatus.
		if reason := m.ctrl.LastErrorFor(networkID); reason != "" {
			roster = append(roster, m.styles.MutedLine.Render(reason))
		}
		headerSpans[networkID] = [2]int{headerStart, len(roster)}
		if m.ctrl.IsNetworkCollapsed(networkID) {
			continue
		}
		// Groups are separate blocks too: a blank row before each heading after
		// the first, so the sections breathe. The network header stays tight
		// against its first group, which then reads as the header's subtitle.
		firstGroup := true
		for _, group := range groups {
			lines, members := m.sidebarGroup(group.label, grouped[networkID], group.direct, width)
			if len(lines) == 0 {
				continue
			}
			if !firstGroup {
				roster = append(roster, "")
			}
			firstGroup = false
			base := len(roster)
			roster = append(roster, lines...)
			// lines[0] is the section heading, so the members start one row
			// past the block's base and follow in render order.
			for offset, member := range members {
				line := base + 1 + offset
				rowSpans[member.ConversationID] = [2]int{line, line + 1}
			}
		}
	}
	// Reserve the bottom rows for the identity footer: clamp the roster to what
	// is left and append the footer block, so the column is exactly height lines.
	// The footer gives the nick its own row and, when the column can spare one
	// more, draws a rule above the nick that mirrors ServerListColumn.qml's
	// identityFooter divider. A sidebar too short for the rule drops it before
	// the presence row; a tiny one keeps only the nick.
	footer := m.identityFooterLines(width)
	if len(footer) > height {
		footer = footer[:clampInt(height, 0, len(footer))]
	} else if len(footer) < height {
		footer = append([]string{m.sidebarDivider(width)}, footer...)
	}
	rosterHeight := height - len(footer)
	if rosterHeight > 0 {
		start := m.sidebarRevealStart(len(roster), rosterHeight, headerSpans, rowSpans)
		end := start + rosterHeight
		if end > len(roster) {
			end = len(roster)
		}
		roster = roster[start:end]
	} else {
		roster = nil
	}
	lines := fitLines(roster, rosterHeight, false)
	lines = append(lines, footer...)
	return renderColumn(m.styles.Conversation, width, lines)
}

// sidebarRevealStart returns the first roster line the sidebar shows, so the
// focused network header, or else the selected conversation, stays on screen.
// It mirrors OmaircWindow.qml's revealSidebarRow: the window slides the least it
// can, resting at the top while the target already fits the first page. With
// neither a focused header nor a rendered selection it starts at the top, the
// same resting state as the QML Flickable at contentY 0.
func (m *Model) sidebarRevealStart(total, height int, headerSpans, rowSpans map[string][2]int) int {
	maxStart := total - height
	if maxStart < 0 {
		maxStart = 0
	}
	top, bottom, found := 0, 0, false
	if m.sidebarNetworkFocusID != "" {
		if span, ok := headerSpans[m.sidebarNetworkFocusID]; ok {
			top, bottom, found = span[0], span[1], true
		}
	}
	if !found {
		if id := m.ctrl.SelectedConversationID(); id != "" {
			if span, ok := rowSpans[id]; ok {
				top, bottom, found = span[0], span[1], true
			}
		}
	}
	if !found {
		return 0
	}
	start := 0
	if bottom > height {
		start = bottom - height
	}
	if top < start {
		start = top
	}
	if start > maxStart {
		start = maxStart
	}
	if start < 0 {
		start = 0
	}
	return start
}

// identityFooterLines renders the sidebar's bottom identity block as two rows:
// the initials chip and the self nick on the first, then the presence word and
// the "inbox N" pill on the second, indented under the nick. Keeping the nick on
// its own row leaves the status room to breathe instead of pushing the inbox pill
// past the column edge. The chip shows "?" for an empty nick, matching the
// direct-message identicon. It mirrors ServerListColumn.qml's identityFooter
// (selfNickLabel, selfPresenceLabel, inboxBadge). The footer is visual only;
// Ctrl+Shift+A reaches the sheet. The build version is not here: it sits at the
// bottom right of the shell footer, after the shortcut list.
func (m *Model) identityFooterLines(width int) []string {
	if m.ctrl == nil {
		return nil
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

	nickLine := chip
	if nick != "" {
		nickLine += " " + m.styles.Conversation.Bold(true).Render(nick)
	}

	// The status row is indented under the nick, past the chip and its space, so
	// it reads as belonging to the nick above it.
	indent := lipgloss.Width(chip) + 1
	available := width - indent
	presenceLabel := m.styles.MutedLine.Render(presence)
	statusSegments := []string{presenceLabel}
	if count := m.ctrl.InboxCount(); count > 0 {
		badge := m.badge(m.styles.BadgeUnread, fmt.Sprintf("inbox %d", count))
		// The presence word is also on the shell footer, but the inbox pill is
		// only here, so drop the word rather than let the column cut the pill in
		// half when both cannot fit.
		if lipgloss.Width(presenceLabel)+1+lipgloss.Width(badge) > available {
			statusSegments = []string{badge}
		} else {
			statusSegments = append(statusSegments, badge)
		}
	}
	statusLine := strings.Repeat(" ", indent) + strings.Join(statusSegments, " ")

	return []string{nickLine, statusLine}
}

// sidebarDivider is the full-width rule shared by the network separators and
// the identity footer's top rule. It reaches the column width, so the dividers
// and the section titles share one visual language, and a sub-cell width yields
// no line so a tiny column never draws a stray glyph.
func (m *Model) sidebarDivider(width int) string {
	if width < 1 {
		return ""
	}
	return m.styles.Divider.Render(strings.Repeat("─", width))
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
// group and renders the section header plus each row. It returns the rendered
// lines and the members in render order, so the caller can map each row back to
// its conversation id for reveal. An empty group renders nothing, so a network
// with only channels has no direct-message heading.
func (m *Model) sidebarGroup(label string, rows []controller.ConversationSnapshot, direct bool, width int) ([]string, []controller.ConversationSnapshot) {
	var members []controller.ConversationSnapshot
	for _, row := range rows {
		if row.Direct == direct {
			members = append(members, row)
		}
	}
	if len(members) == 0 {
		return nil, nil
	}
	lines := []string{m.sectionHeader(label, width)}
	for _, row := range members {
		lines = append(lines, m.conversationRowWidth(row, width))
	}
	return lines, members
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
		line.WriteString(m.typingPulse())
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
