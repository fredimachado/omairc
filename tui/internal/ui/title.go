package ui

import "github.com/fredimachado/omairc/tui/internal/controller"

// Title reproduces the Qt window title byte-for-byte from OmaircWindow.qml:
// consoleVisible ? statusTitleText() : conversationTitleText(). A nil or empty
// controller has no selection and no focused network, so it renders "Omairc".
func Title(ctrl *controller.Controller) string {
	if ctrl == nil {
		return "Omairc"
	}
	if ctrl.ConsoleOpen() {
		return statusTitleText(ctrl)
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

// statusTitleText mirrors OmaircWindow.qml's statusTitleText. The Qt
// connection.displayName fallback collapses into the focused network display
// name here, so an empty name falls straight through to "Status".
func statusTitleText(ctrl *controller.Controller) string {
	networkName := focusedNetworkDisplayName(ctrl)
	if networkName != "" {
		return networkName + " Status"
	}
	return "Status"
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
