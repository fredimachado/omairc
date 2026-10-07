package ui

import (
	"strings"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
)

// TitleMark is an unfocused mention or direct message held in the window
// title. Body is plain IRC text; attentionTitle collapses leftover controls.
// Opening that conversation, or focusing the window, drops the mark. The
// formatted string matches src/qml/TitleMark.qml.
type TitleMark struct {
	Author, Body, NetworkID, Target string
}

// Title reproduces the Qt window title byte-for-byte from OmaircWindow.qml:
// consoleVisible ? statusTitleText() : conversationTitleText(). ConsoleOpen
// carries that derivation, including the fall back to Status when nothing is
// selected, exactly as the Qt shell does on first run. A nil controller has no
// selection and no focused network, so it renders "Omairc".
func Title(ctrl *controller.Controller, conn *connection.Connection) string {
	if ctrl == nil {
		return "Omairc"
	}
	if ctrl.ConsoleOpen() {
		return statusTitleText(ctrl, conn)
	}
	return conversationTitleText(ctrl, conn)
}

// conversationTitleText mirrors OmaircWindow.qml's conversationTitleText. It
// prefixes the focused network display name only when the selected target
// appears on more than one network, so a duplicated #omarchy is never
// ambiguous.
func conversationTitleText(ctrl *controller.Controller, conn *connection.Connection) string {
	current := ctrl.SelectedTarget()
	if current == "" {
		return "Omairc"
	}
	networkName := focusedNetworkDisplayName(ctrl, conn)
	if duplicateTargetName(ctrl, current) && networkName != "" {
		return current + " · " + networkName + " - Omairc"
	}
	return current + " - Omairc"
}

// attentionTitle formats an unfocused mention or direct message. place is the
// conversation, plus the network display name when that target is duplicated.
// An author that already is the place (a direct message) is not repeated.
func attentionTitle(author, body, place string) string {
	who := collapseTitleSpace(author)
	text := collapseTitleSpace(body)
	if who == "" {
		who = place
	}
	lead := who
	if text != "" {
		lead = who + ": " + text
	}
	if place != "" && place != who {
		return lead + " · " + place + " - Omairc"
	}
	return lead + " - Omairc"
}

// collapseTitleSpace drops C0, C1, and DEL (ESC, BEL, newlines) and folds
// the gap into one space. It matches TitleMark.qml's collapse. IRC formatting
// is stripped before this runs.
func collapseTitleSpace(text string) string {
	var out strings.Builder
	pendingSpace := false
	for _, r := range text {
		if r <= 0x20 || r == 0x7f || (r >= 0x80 && r <= 0x9f) {
			if out.Len() > 0 {
				pendingSpace = true
			}
			continue
		}
		if pendingSpace {
			out.WriteByte(' ')
			pendingSpace = false
		}
		out.WriteRune(r)
	}
	return out.String()
}

// noteTitleMark records an unfocused mention or direct message. A focused
// window, or an arrival with no conversation, leaves the title alone. body
// still carries IRC formatting; PlainIrcText strips it before display.
func (m *Model) noteTitleMark(windowActive bool, author, body, networkID, target string) {
	if windowActive || target == "" || m.ctrl == nil {
		return
	}
	m.titleMark = &TitleMark{
		Author:    author,
		Body:      m.ctrl.PlainIrcText(body),
		NetworkID: networkID,
		Target:    target,
	}
}

// clearTitleMark drops the attention line so the title is the open
// conversation or Status again.
func (m *Model) clearTitleMark() {
	m.titleMark = nil
}

// clearTitleMarkIfOpened drops the mark when the open conversation is the one
// it names. Status is not that conversation, so a console stays marked.
func (m *Model) clearTitleMarkIfOpened() {
	if m.titleMark == nil || m.ctrl == nil || m.ctrl.ConsoleOpen() {
		return
	}
	if m.ctrl.FocusedNetworkID() == m.titleMark.NetworkID &&
		m.ctrl.SelectedTarget() == m.titleMark.Target {
		m.titleMark = nil
	}
}

// titleMarkPlace is the conversation label inside the attention title, with
// the network display name when that target exists on more than one network.
func (m *Model) titleMarkPlace() string {
	if m.titleMark == nil {
		return ""
	}
	target := m.titleMark.Target
	networkName := networkDisplayName(m.ctrl, m.conn, m.titleMark.NetworkID)
	if duplicateTargetName(m.ctrl, target) && networkName != "" {
		return target + " · " + networkName
	}
	return target
}

// windowTitle is the OSC title: the attention line while a mark is set, and
// Title otherwise.
func (m *Model) windowTitle() string {
	if m.titleMark == nil {
		return Title(m.ctrl, m.conn)
	}
	return attentionTitle(m.titleMark.Author, m.titleMark.Body, m.titleMarkPlace())
}

// statusTitleText mirrors OmaircWindow.qml's statusTitleText. When no session
// is bound yet (first run, before Connect applies) it falls back to the
// connection's draft display name, so a default launch is titled
// "chat.freenode.net Status".
func statusTitleText(ctrl *controller.Controller, conn *connection.Connection) string {
	if networkName := focusedNetworkDisplayName(ctrl, conn); networkName != "" {
		return networkName + " Status"
	}
	if conn != nil {
		if displayName := conn.DisplayName(); displayName != "" {
			return displayName + " Status"
		}
	}
	return "Status"
}

// statusJumpLabel is the jump overlay's label for a network's Status row. It
// lives beside the window-title helpers so the " Status" literal stays in one
// file (bin/check-conventions gates it to title.go).
func statusJumpLabel(networkName string) string {
	if networkName == "" {
		return "Status"
	}
	return networkName + " Status"
}

// focusedNetworkDisplayName is the display name of the focused network, or ""
// when the controller has no focused network. It falls back to the connection
// roster so a stored profile titles Status before its session is live.
func focusedNetworkDisplayName(ctrl *controller.Controller, conn *connection.Connection) string {
	networkID := ""
	if ctrl != nil {
		networkID = ctrl.FocusedNetworkID()
	}
	return networkDisplayName(ctrl, conn, networkID)
}

// networkDisplayName is the roster name for one network. It falls back to the
// connection roster when the controller has not published one yet.
func networkDisplayName(ctrl *controller.Controller, conn *connection.Connection, networkID string) string {
	if networkID == "" {
		return ""
	}
	if ctrl != nil {
		if name := ctrl.NetworkDisplayName(networkID); name != "" {
			return name
		}
	}
	if conn != nil {
		for _, row := range conn.Networks() {
			if row.NetworkID == networkID {
				return row.DisplayName
			}
		}
	}
	return ""
}

// duplicateTargetName counts the sidebar rows whose ConversationName matches
// name across every network. Like the Qt helper it stops as soon as a second
// match is seen.
func duplicateTargetName(ctrl *controller.Controller, name string) bool {
	if name == "" {
		return false
	}
	seen := 0
	for _, row := range ctrl.Conversations() {
		if row.ConversationName == name {
			seen++
			if seen > 1 {
				return true
			}
		}
	}
	return false
}
