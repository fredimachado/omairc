package ui

import (
	"fmt"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// membersView renders the member column. It is shown only for channels (see
// Model.membersVisible); the rows already arrive in PREFIX-rank then nick
// order from the controller, so the view never sorts.
func (m *Model) membersView(width, height int) string {
	lines := []string{
		m.styles.MembersHeader.Render(fmt.Sprintf("Members (%d)", m.ctrl.PeopleCount())),
	}
	for _, member := range m.ctrl.Members() {
		lines = append(lines, m.memberRow(member))
	}
	return renderColumn(m.styles.Conversation, width, fitLines(lines, height, false))
}

// memberRow renders one member: the PREFIX label (falling back to the nick), an
// away mark, and any metadata status.
func (m *Model) memberRow(member controller.MemberSnapshot) string {
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
	if member.Away {
		return m.styles.MemberAway.Render(text)
	}
	return m.styles.Conversation.Render(text)
}
