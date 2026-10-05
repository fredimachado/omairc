package ui

import tea "charm.land/bubbletea/v2"

// channelPromptState is the ask-before-open sheet for a channel that is not
// already a sidebar row. Enter joins it. Escape cancels and sends nothing.
type channelPromptState struct {
	open bool
	name string
}

// openChannelName switches to channel when that buffer is already open.
// Otherwise it asks, and does not join until the prompt is confirmed.
func (m *Model) openChannelName(channel string) {
	if m == nil || m.ctrl == nil || channel == "" {
		return
	}
	networkID := m.ctrl.FocusedNetworkID()
	if m.ctrl.HasConversation(networkID, channel) {
		m.switchSelection(func() {
			m.ctrl.SelectConversation(networkID, channel)
		})
		return
	}
	m.channelPrompt.open = true
	m.channelPrompt.name = channel
	m.composer.Blur()
}

// confirmChannelOpen joins the channel the prompt is asking about, then
// dismisses the prompt. It sends nothing when the prompt is empty.
func (m *Model) confirmChannelOpen() {
	if m == nil {
		return
	}
	channel := m.channelPrompt.name
	m.channelPrompt.open = false
	m.channelPrompt.name = ""
	if channel != "" && m.ctrl != nil {
		m.switchSelection(func() {
			m.ctrl.ConsoleSubmit("/join " + channel)
		})
	}
	m.refocusComposer()
}

// cancelChannelOpen dismisses the prompt without joining.
func (m *Model) cancelChannelOpen() {
	if m == nil || !m.channelPrompt.open {
		return
	}
	m.channelPrompt.open = false
	m.channelPrompt.name = ""
	m.refocusComposer()
}

// handleChannelPromptKey owns the keys while the prompt is open. Enter joins.
// Escape cancels. Every other key is swallowed.
func (m *Model) handleChannelPromptKey(key string) (tea.Model, tea.Cmd) {
	switch key {
	case "enter", "return":
		m.confirmChannelOpen()
	case "esc", "escape":
		m.cancelChannelOpen()
	}
	return m, nil
}

// channelPromptCard renders the ask as one overlay card.
func (m *Model) channelPromptCard(width int) string {
	return m.overlayCardBlock(width, m.channelPromptCardBody)
}

func (m *Model) channelPromptCardBody(inner int) []string {
	lines := m.overlaySheetHeader(inner, "Open channel", 0)
	question := "Open " + m.channelPrompt.name + "?"
	lines = append(lines, truncateLine(m.styles.SheetRow.Render(question), inner))
	lines = append(lines, truncateLine(m.styles.MutedLine.Render("Enter opens it. Escape cancels."), inner))
	return lines
}
