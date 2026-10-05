package ui

// This file is the Phase 9 desktop-notification glue and the notification
// activation handler. It mirrors OmaircWindow.qml's notifyMentionIfUnfocused,
// msgidRow, and activateNotifiedConversation. It reads only controller
// snapshots and plain strings, never internal/irc.

// notificationRecord mirrors OmaircWindow.qml's lastNotification object: the
// most recent arrival the shell recorded, plain-text body included.
type notificationRecord struct {
	Author, Body, NetworkID, Target, MsgID string
}

// notifyMentionIfUnfocused records the arrival and, when the window is
// unfocused and the desktop latch is clear, sends a desktop notification. It
// mirrors OmaircWindow.qml's notifyMentionIfUnfocused: a focused window is a
// no-op, and the recorded body is the IRC-formatting-stripped text.
func (m *Model) notifyMentionIfUnfocused(windowActive bool, author, body, networkID, target, msgid string) {
	if windowActive {
		return
	}
	text := body
	if m.ctrl != nil {
		text = m.ctrl.PlainIrcText(body)
	}
	m.lastNotification = &notificationRecord{
		Author:    author,
		Body:      text,
		NetworkID: networkID,
		Target:    target,
		MsgID:     msgid,
	}
	if m.suppressDesktopNotification {
		return
	}
	if m.notifier != nil {
		m.notifier.Notify(author, text, networkID, target, msgid)
	}
}

// msgidRow returns the transcript row index for msgid, or -1. It mirrors
// OmaircWindow.qml's msgidRow: an empty id or an unmatched id never resolves.
func (m *Model) msgidRow(msgid string) int {
	if m.ctrl == nil || msgid == "" {
		return -1
	}
	for index, message := range m.ctrl.Messages() {
		if message.MsgID == msgid {
			return index
		}
	}
	return -1
}

// activateNotifiedConversation reveals the notified conversation, scrolls to
// the exact message when the row resolves, and focuses the composer. It
// mirrors OmaircWindow.qml's activateNotifiedConversation minus the window
// raise, which a terminal cannot do.
func (m *Model) activateNotifiedConversation(networkID, target, msgid string) {
	if m.ctrl == nil || networkID == "" || target == "" {
		return
	}
	m.sidebarNetworkFocusID = ""
	previousID := m.selectedConversationID()
	m.switchSelection(func() {
		m.ctrl.RevealConversation(networkID, target)
	})
	// The notification's own landing wins over a saved scroll place, and it
	// matches the Qt window: open-at-unread lands on the mark when the
	// conversation changed, and a resolved msgid is revealed only when that
	// setting is off, including when the conversation is already open.
	if m.ctrl != nil && !m.ctrl.ConsoleOpen() {
		same := previousID != "" && previousID == m.ctrl.SelectedConversationID()
		if !same && m.ctrl.OpenAtUnread() {
			m.setTranscriptFollowEnd(true)
			m.transcriptScroll = 0
			m.placeTranscriptAfterSelect()
			m.rememberOpenTranscript()
		} else if !m.ctrl.OpenAtUnread() {
			if row := m.msgidRow(msgid); row >= 0 {
				m.revealTranscriptRow(row)
				m.rememberOpenTranscript()
			} else if !same {
				m.setTranscriptFollowEnd(true)
				m.transcriptScroll = 0
				m.rememberOpenTranscript()
			}
		}
	}
	_ = m.composer.Focus()
}
