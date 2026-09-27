package ui

import (
	"fmt"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// membersView renders the member column. It is shown only for channels (see
// Model.membersVisible); the rows already arrive in PREFIX-rank then nick
// order from the controller, so the view never sorts. A focused member row is
// highlighted and was chosen with Ctrl+Shift+P.
func (m *Model) membersView(width, height int) string {
	lines := []string{
		m.styles.MembersHeader.Render(fmt.Sprintf("Members (%d)", m.ctrl.PeopleCount())),
	}
	for index, member := range m.ctrl.Members() {
		lines = append(lines, m.memberRow(index, member))
	}
	return renderColumn(m.styles.Conversation, width, fitLines(lines, height, false))
}

// memberRow renders one member: the PREFIX label (falling back to the nick), an
// away mark, and any metadata status. The focused row wins over the away style.
func (m *Model) memberRow(index int, member controller.MemberSnapshot) string {
	label := member.Label
	if label == "" {
		label = member.Nick
	}
	text := label
	if member.Away {
		text += " (away)"
	}
	if member.Status != "" {
		text += " — " + member.Status
	}
	if m.memberFocus && index == m.memberIndex {
		return m.styles.MemberSelected.Render(text)
	}
	if member.Away {
		return m.styles.MemberAway.Render(text)
	}
	return m.styles.Conversation.Render(text)
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
