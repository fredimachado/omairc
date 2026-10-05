package ui

import (
	"fmt"
	"strings"

	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// memberGutter is the fixed-width focus column at the left of every member
// row. A focused row paints an accent bar in it; every other row leaves it
// blank, so the rows never shift when the member cursor moves.
const memberGutter = 2

// membersView renders the member column's inner content: the "ONLINE - N"
// heading, a hairline rule, then one row per member. It is shown only for
// channels (see Model.membersVisible); the rows already arrive in PREFIX-rank
// then nick order from the controller, so the view never sorts. A focused
// member row carries an accent bar and was chosen with Ctrl+Shift+P. The
// heading reads "ONLINE - N" like MembersColumn.qml:43 and stays pinned above
// the rows, the way the QML ListView sits under its heading. The outer card
// frame is drawn around these lines by the panel chrome, so this returns inner
// content only.
//
// A roster taller than the panel scrolls as one window: Page Up/Down, Home/End,
// and the arrow keys move the member cursor, and the window slides to keep the
// focused row visible, mirroring the QML ListView's highlightFollowsCurrentItem.
func (m *Model) membersView(width, height int) string {
	heading := []string{
		m.styles.MembersHeader.Render(fmt.Sprintf("ONLINE - %d", m.ctrl.PeopleCount())),
		m.styles.Divider.Render(strings.Repeat("─", width)),
	}
	if height <= len(heading) {
		// No room for rows: show as much of the heading as fits rather than
		// letting the pinned block push the grid over its budget.
		return renderColumn(m.styles.Conversation, width, fitLines(heading, height, false))
	}
	rowsHeight := height - len(heading)

	// A member block is one line, or two once the network advertises member
	// status, so the window is measured in lines and each block records its
	// span. That keeps the focused row fully visible instead of a line short.
	members := m.ctrl.Members()
	rows := make([]string, 0, len(members)*2)
	spans := make([][2]int, len(members))
	for index, member := range members {
		start := len(rows)
		rows = append(rows, m.memberRow(index, member)...)
		spans[index] = [2]int{start, len(rows)}
	}

	start := memberWindowStart(len(rows), spans, m.memberIndex, m.memberFocus, rowsHeight)
	end := start + rowsHeight
	if end > len(rows) {
		end = len(rows)
	}
	lines := make([]string, 0, len(heading)+rowsHeight)
	lines = append(lines, heading...)
	lines = append(lines, fitLines(rows[start:end], rowsHeight, false)...)
	return renderColumn(m.styles.Conversation, width, lines)
}

// memberWindowStart returns the first member line the panel shows. It keeps the
// focused member's block in view: while the block fits in the first page the
// list stays at the top, then the window slides so the block sits on the bottom
// edge. A block taller than the viewport keeps its first line on screen. Without
// member focus there is no cursor, so the list shows from the top (the same
// resting state as the QML list at currentIndex 0).
func memberWindowStart(total int, spans [][2]int, focusIndex int, focused bool, height int) int {
	maxStart := total - height
	if maxStart < 0 {
		maxStart = 0
	}
	if !focused || focusIndex < 0 || focusIndex >= len(spans) {
		return 0
	}
	start, end := spans[focusIndex][0], spans[focusIndex][1]
	window := 0
	if end > height {
		window = end - height
	}
	if start < window {
		window = start
	}
	if window > maxStart {
		window = maxStart
	}
	return window
}

// toggleMembers is Ctrl+Shift+M: it hides or shows the whole member panel. It
// is a no-op off a channel, on Status, or without a controller, matching the
// Qt window-level toggle mounted on OmaircWindow rather than the panel (a
// direct message has no panel to flip). Hiding clears the member focus so a
// stale cursor does not swallow keys; showing does not steal focus.
func (m *Model) toggleMembers() {
	if m.ctrl == nil || !m.ctrl.IsChannel() || !m.ctrl.ChannelJoined() || m.ctrl.ConsoleOpen() {
		return
	}
	m.membersHidden = !m.membersHidden
	if m.membersHidden {
		m.memberFocus = false
	}
}

// memberRow renders one member as one or two physical lines: the chrome line
// (focus bar, presence dot, PREFIX label, bot mark, account, typing dots) and,
// when the network advertises member status, a muted status subline indented
// under the label. It mirrors the Row plus the status Text in
// MembersColumn.qml:184-250. Each sub-line is its own entry so
// renderColumn/truncateLine keep them from wrapping.
func (m *Model) memberRow(index int, member controller.MemberSnapshot) []string {
	label := member.Label
	if label == "" {
		label = member.Nick
	}
	// Our own away state arrives as the 305/306 numerics, so it is always
	// shown; another member's needs away-notify. This is the exact compare
	// MembersColumn.qml's isSelf uses for the presence dot.
	self := member.Nick == m.ctrl.CurrentNick()
	presenceShown := m.ctrl.HasAwayPresence() || self
	away := member.Away && presenceShown
	focused := m.memberFocus && index == m.memberIndex

	var line strings.Builder
	if focused {
		line.WriteString(m.memberFocusBar())
	} else {
		line.WriteString(strings.Repeat(" ", memberGutter))
	}
	if presenceShown {
		dot := m.styles.MemberPresenceOnline
		if member.Away {
			dot = m.styles.MemberPresenceAway
		}
		line.WriteString(dot.Render("●"))
		line.WriteString(" ")
	}
	// The label carries the nick's fixed palette color — the same hash the
	// transcript byline uses — so one nick reads as the same person in both
	// places. An away member keeps the hue but is washed toward the page, and the
	// focused row is bold; the accent bar already marks focus.
	line.WriteString(m.memberNickStyle(member.Nick, focused, away).Render(label))
	if member.Bot {
		// The mark sits beside the name so the presence dot stays visible.
		line.WriteString(" ")
		line.WriteString(m.styles.MemberBot.Render("⌬"))
	}
	if member.Account != "" {
		line.WriteString(" ")
		line.WriteString(m.styles.MemberAccount.Render(member.Account))
	}
	if m.ctrl.HasTyping() && m.ctrl.NickIsTyping(member.Nick) {
		line.WriteString(" ")
		line.WriteString(m.typingDots())
	}

	lines := []string{line.String()}
	if m.ctrl.HasMemberStatus() && member.Status != "" {
		lines = append(lines, m.memberStatusLine(member.Status, presenceShown))
	}
	// The focused row is the keyboard tooltip: the same facts the Qt hover
	// tip shows, without adding them to the unfocused chrome.
	if focused {
		if tip := m.memberTooltip(member, away); tip != "" {
			lines = append(lines, m.memberFactLine(tip, presenceShown))
		}
	}
	return lines
}

// memberTooltip joins the presence word, meaningful real name, and short
// labels. A listed member is online or away, never offline.
func (m *Model) memberTooltip(member controller.MemberSnapshot, away bool) string {
	parts := []string{"online"}
	if away {
		parts[0] = "away"
	}
	if name := strings.TrimSpace(m.ctrl.PlainIrcText(member.Realname)); name != "" {
		parts = append(parts, name)
	}
	for _, label := range member.Labels {
		if label != "" {
			parts = append(parts, label)
		}
	}
	return strings.Join(parts, " · ")
}

// memberFactLine indents the tooltip under the nick, matching the status
// subline's gutter without the status marker.
func (m *Model) memberFactLine(text string, presenceShown bool) string {
	indent := memberGutter
	if presenceShown {
		indent += 2
	}
	return m.styles.MemberAccount.Render(strings.Repeat(" ", indent) + text)
}

// memberAwayNickMix is how far an away member's nick color is mixed toward the
// page background. The member panel keeps the nick's palette color and washes it
// out for away, instead of dropping to a flat muted gray, so the hue still reads.
const memberAwayNickMix = 0.5

// memberNickStyle is the member panel's label style: the nick's fixed palette
// color, mixed toward the page background when the member is away, and bold when
// the row holds the member focus. It is the member-panel counterpart of the
// transcript's nickStyle, so a nick keeps one color everywhere. The color hashes
// on the nick, not the PREFIX label the row renders, so "@mira" and the
// transcript's "mira" agree.
func (m *Model) memberNickStyle(nick string, focused, away bool) lipgloss.Style {
	ink := nickColor(nick)
	if away {
		ink = mixColors(ink, m.styles.Colors.Background, memberAwayNickMix)
	}
	style := lipgloss.NewStyle().Foreground(ink)
	if focused {
		style = style.Bold(true)
	}
	return style
}

// memberFocusBar is the focused member's gutter cell: an accent block plus the
// gutter's trailing space. It is a one-off style derived from the live theme
// accent, matching the other focus affordances, instead of a fixed constant.
func (m *Model) memberFocusBar() string {
	bar := lipgloss.NewStyle().Bold(true).Foreground(m.styles.Colors.Accent).Render("▌")
	return bar + " "
}

// memberStatusLine renders a member's status as a muted subline aligned under
// the label. The focus gutter and, when shown, the presence dot's cells are
// blanked first and the text gets a quiet marker, so the status reads as
// belonging to the nick above it rather than as a second member row.
func (m *Model) memberStatusLine(status string, presenceShown bool) string {
	indent := memberGutter
	if presenceShown {
		indent += 2
	}
	return m.styles.MemberStatus.Render(strings.Repeat(" ", indent) + "· " + status)
}

// moveMember moves the focused member row by delta, wrapping at both ends.
func (m *Model) moveMember(delta int) {
	count := len(m.ctrl.Members())
	if count == 0 {
		m.memberIndex = 0
		return
	}
	m.memberIndex = ((m.memberIndex+delta)%count + count) % count
}

// pageMembers pages the focused member list by a page (or a fraction of one).
func (m *Model) pageMembers(direction int, fraction float64) {
	count := len(m.ctrl.Members())
	if count == 0 {
		return
	}
	page := int(float64(m.bodyHeight())*fraction + 0.5)
	if page < 1 {
		page = 1
	}
	if direction < 0 {
		m.memberIndex -= page
	} else {
		m.memberIndex += page
	}
	m.clampMemberIndex()
}

// jumpMembers jumps the focused member list to the first or last nick.
func (m *Model) jumpMembers(toEnd bool) {
	count := len(m.ctrl.Members())
	if count == 0 {
		m.memberIndex = 0
		return
	}
	if toEnd {
		m.memberIndex = count - 1
		return
	}
	m.memberIndex = 0
}

// clampMemberIndex keeps the focused member index inside the member list.
func (m *Model) clampMemberIndex() {
	count := len(m.ctrl.Members())
	if count == 0 {
		m.memberIndex = 0
		return
	}
	if m.memberIndex < 0 {
		m.memberIndex = 0
	}
	if m.memberIndex >= count {
		m.memberIndex = count - 1
	}
}

// focusedMemberNick returns the nick under the member cursor, or "".
func (m *Model) focusedMemberNick() string {
	members := m.ctrl.Members()
	if m.memberIndex < 0 || m.memberIndex >= len(members) {
		return ""
	}
	return members[m.memberIndex].Nick
}

// activateFocusedMember opens or creates a direct message with the focused
// member. Entering on yourself is a no-op, matching the Qt list.
func (m *Model) activateFocusedMember() {
	nick := m.focusedMemberNick()
	if nick == "" || nick == m.ctrl.CurrentNick() {
		return
	}
	m.memberFocus = false
	m.saveDraft()
	previousID := m.selectedConversationID()
	m.ctrl.OpenDirectMessage(nick)
	m.loadDraft()
	m.afterSelectionChange(previousID)
}
