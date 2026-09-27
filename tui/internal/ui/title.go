package ui

import (
	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
)

// Title reproduces the Qt window title byte-for-byte from OmaircWindow.qml:
// consoleVisible ? statusTitleText() : conversationTitleText(). Nothing
// selected lands on the Status surface, exactly as the Qt shell does on first
// run. A nil controller has no selection and no focused network, so it renders
// "Omairc".
func Title(ctrl *controller.Controller, conn *connection.Connection) string {
	if ctrl == nil {
		return "Omairc"
	}
	if ctrl.ConsoleOpen() || ctrl.SelectedTarget() == "" {
		return statusTitleText(ctrl, conn)
	}
	return conversationTitleText(ctrl)
}

// conversationTitleText mirrors OmaircWindow.qml's conversationTitleText. It
// prefixes the focused network display name only when the selected target
// appears on more than one network, so a duplicated #omarchy is never
// ambiguous.
func conversationTitleText(ctrl *controller.Controller) string {
	current := ctrl.SelectedTarget()
	if current == "" {
		return "Omairc"
	}
	networkName := focusedNetworkDisplayName(ctrl)
	if duplicateTargetName(ctrl, current) && networkName != "" {
		return current + " · " + networkName + " - Omairc"
	}
	return current + " - Omairc"
}

// statusTitleText mirrors OmaircWindow.qml's statusTitleText. When no session
// is bound yet (first run, before Connect applies) it falls back to the
// connection's draft display name, so a default launch is titled
// "irc.libera.chat Status".
func statusTitleText(ctrl *controller.Controller, conn *connection.Connection) string {
	if networkName := focusedNetworkDisplayName(ctrl); networkName != "" {
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
// when the controller has no focused network.
func focusedNetworkDisplayName(ctrl *controller.Controller) string {
	return ctrl.NetworkDisplayName(ctrl.FocusedNetworkID())
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
