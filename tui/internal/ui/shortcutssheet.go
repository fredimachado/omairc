package ui

import (
	"fmt"
	"strings"
)

// This file is the Ctrl+/ shortcuts sheet. It lists the full chord map,
// mirroring src/qml/ShortcutsSheet.qml, and is the only overlay that can open on
// top of the Connect sheet. While it is open every navigation chord is gated.

// shortcutRow is one keys/action pair in the sheet.
type shortcutRow struct {
	keys   string
	action string
}

// shortcutGroup is a titled block of chord rows.
type shortcutGroup struct {
	title string
	rows  []shortcutRow
}

// shortcutGroups is the sheet's content. It mirrors ShortcutsSheet.qml's
// shortcutGroups, including Ctrl+Q as the only quit chord and Ctrl+C as copy.
var shortcutGroups = []shortcutGroup{
	{
		title: "MOVE",
		rows: []shortcutRow{
			{"Alt+Down / Alt+Up", "walk conversations"},
			{"Alt+Left / Alt+Right", "walk networks"},
			{"Alt+Shift+Left / Right", "collapse / expand network"},
			{"Ctrl+Alt+Shift+Left / Right", "collapse / expand all"},
			{"Alt+Shift+Up / Down", "move network"},
			{"Alt+A", "next unread"},
		},
	},
	{
		title: "JUMP",
		rows: []shortcutRow{
			{"Ctrl+K", "jump to conversation"},
			{"Ctrl+Shift+O", "open link"},
			{"Ctrl+Shift+K", "jump to nick"},
			{"Ctrl+Shift+A", "inbox"},
			{"Ctrl+`", "Status"},
			{"Ctrl+W", "close direct message"},
		},
	},
	{
		title: "WRITE",
		rows: []shortcutRow{
			{"Ctrl+L", "composer"},
			{"Ctrl+C", "copy selection"},
			{"Ctrl+F", "find"},
			{"Enter", "send"},
			{"Page Up / Page Down", "scroll transcript"},
			{"Shift+Page Up / Shift+Page Down", "scroll transcript half page"},
			{"Ctrl+Home / Ctrl+End", "top / bottom"},
			{"Tab", "nick complete"},
			{"Up / Down", "history"},
			{"Escape", "dismiss"},
		},
	},
	{
		title: "CONNECT",
		rows: []shortcutRow{
			{"Ctrl+,", "Connect"},
			{"Ctrl+Tab", "Connect tabs"},
			{"Ctrl+N", "add network"},
			{"Ctrl+Shift+Delete", "remove network"},
			{"Ctrl+Enter", "apply selected network"},
		},
	},
	{
		title: "WINDOW",
		rows: []shortcutRow{
			{"Ctrl+Shift+S", "server list"},
			{"Ctrl+Shift+M", "members panel"},
			{"Ctrl+Shift+P", "focus members"},
			{"Page Up / Page Down", "page focused members"},
			{"Shift+Page Up / Shift+Page Down", "page focused members half page"},
			{"Home / End", "first / last focused nick"},
			{"Ctrl+/", "this sheet"},
			{"Ctrl+Q", "quit"},
		},
	},
}

// openShortcuts shows the sheet. It is allowed on top of the Connect sheet.
func (m *Model) openShortcuts() {
	m.shortcutsOpen = true
	m.composer.Blur()
}

// closeShortcuts hides the sheet and returns focus to the composer or the
// Connect sheet underneath.
func (m *Model) closeShortcuts() {
	if !m.shortcutsOpen {
		return
	}
	m.shortcutsOpen = false
	m.refocusComposer()
}

// shortcutsCardLines renders the sheet into a bordered block exactly width
// cells wide.
func (m *Model) shortcutsCardLines(width int) []string {
	inner := width - 4
	if inner < 8 {
		inner = 8
	}
	keyWidth := inner / 2
	if keyWidth < 12 {
		keyWidth = 12
	}
	lines := []string{m.styles.JumpQuery.Render("Shortcuts")}
	for _, group := range shortcutGroups {
		lines = append(lines, m.styles.JumpEmpty.Render(group.title))
		for _, row := range group.rows {
			label := fmt.Sprintf("%-*s %s", keyWidth, row.keys, row.action)
			lines = append(lines, m.styles.JumpRow.Render(truncateLine(label, inner)))
		}
	}
	rendered := m.styles.JumpCard.Width(inner).Render(strings.Join(lines, "\n"))
	return strings.Split(rendered, "\n")
}
