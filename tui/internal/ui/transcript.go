package ui

import (
	"fmt"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// transcriptView renders the selected conversation: its topic, then either the
// Status console lines when the console is open or the live transcript. The
// console is read through the controller's text accessor so this package never
// imports internal/irc.
func (m *Model) transcriptView(height int) string {
	lines := make([]string, 0, height)
	if topic := m.ctrl.Topic(); topic != "" {
		header := topic
		if m.ctrl.IsChannel() {
			header = fmt.Sprintf("%s  (%d)", topic, m.ctrl.PeopleCount())
		}
		lines = append(lines, m.styles.Topic.Render(header))
		lines = append(lines, "")
	}

	if m.ctrl.ConsoleOpen() {
		for _, text := range m.ctrl.ConsoleText(m.ctrl.FocusedNetworkID()) {
			lines = append(lines, m.styles.ConsoleLine.Render(text))
		}
	} else {
		for _, message := range m.ctrl.Messages() {
			lines = append(lines, m.messageRow(message))
		}
	}

	// Keep the newest rows visible; the topic scrolls off first.
	return renderColumn(m.styles.Conversation, m.transcriptWidth(), fitLines(lines, height, true))
}

// messageRow renders one transcript line. Actions, events, and notices get
// their own shape; a mention is emphasized.
func (m *Model) messageRow(message controller.MessageSnapshot) string {
	stamp := message.Time.Format("15:04")
	switch message.Kind {
	case "action":
		return m.styles.Action.Render("* " + message.Author + " " + message.Body)
	case "event":
		return m.styles.Event.Render(message.Body)
	case "notice":
		return m.styles.Notice.Render("-" + message.Author + "- " + message.Body)
	}
	line := stamp + " " + message.Author + " " + message.Body
	if message.Mentioned {
		return m.styles.MentionBody.Render(line)
	}
	return line
}
