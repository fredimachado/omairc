package ui

import (
	"fmt"
	"strings"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// membersView renders the member column. It is shown only for channels (see
// Model.membersVisible); the rows already arrive in PREFIX-rank then nick
// order from the controller, so the view never sorts. A focused member row is
// highlighted and was chosen with Ctrl+Shift+P. The heading reads
// "ONLINE - N" like MembersColumn.qml:43.
func (m *Model) membersView(width, height int) string {
	lines := []string{
		m.styles.MembersHeader.Render(fmt.Sprintf("ONLINE - %d", m.ctrl.PeopleCount())),
	}
	for index, member := range m.ctrl.Members() {
		lines = append(lines, m.memberRow(index, member)...)
	}
	return renderColumn(m.styles.Conversation, width, fitLines(lines, height, false))
}

// toggleMembers is Ctrl+Shift+M: it hides or shows the whole member panel. It
// is a no-op off a channel, on Status, or without a controller, matching the
// Qt window-level toggle mounted on OmaircWindow rather than the panel (a
// direct message has no panel to flip). Hiding clears the member focus so a
// stale cursor does not swallow keys; showing does not steal focus.
func (m *Model) toggleMembers() {
	if m.ctrl == nil || !m.ctrl.IsChannel() || m.ctrl.ConsoleOpen() {
		return
	}
	m.membersHidden = !m.membersHidden
	if m.membersHidden {
		m.memberFocus = false
	}
}

// memberRow renders one member as one or two physical lines: the chrome line
// (presence dot, PREFIX label, bot mark, account, typing dots) and, when the
// network advertises member status, an indented status subline. It mirrors the
// Row plus the status Text in MembersColumn.qml:184-250. Each sub-line is its
// own entry so renderColumn/truncateLine keep them from wrapping.
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
	if presenceShown {
		dot := m.styles.MemberPresenceOnline
		if member.Away {
			dot = m.styles.MemberPresenceAway
		}
		line.WriteString(dot.Render("●"))
		line.WriteString(" ")
	}
	// The focused row wins over the away dim on the first line; an away member
	// keeps its status subline either way.
	labelStyle := m.styles.Conversation
	switch {
	case focused:
		labelStyle = m.styles.MemberSelected
	case away:
		labelStyle = m.styles.MemberAway
	}
	line.WriteString(labelStyle.Render(label))
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
		line.WriteString(m.styles.MemberTyping.Render("..."))
	}

	lines := []string{line.String()}
	if m.ctrl.HasMemberStatus() && member.Status != "" {
		lines = append(lines, m.styles.MemberStatus.Render("  "+member.Status))
	}
	return lines
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
	m.ctrl.OpenDirectMessage(nick)
	m.loadDraft()
	m.afterSelectionChange()
}
