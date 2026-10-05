package ui

import (
	"strings"

	"charm.land/bubbles/v2/key"
	"charm.land/lipgloss/v2"
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
			{"Alt+Shift+A", "mark all read"},
			{"Alt+U", "first new message"},
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
			{"Ctrl+W", "close conversation"},
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
			{"Tab / Shift+Tab", "complete nick or channel"},
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
			{"Ctrl+Shift+/", "About"},
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

// shortcutsCard renders the shortcuts sheet as one card block through the
// shared overlay frame in model.go.
func (m *Model) shortcutsCard(width int) string {
	return m.overlayCardBlock(width, m.shortcutsCardBody)
}

// shortcutsMenuGutter is the two-cell gutter between the keycap column and the
// action column.
const shortcutsMenuGutter = 2

// shortcutsCardBody is the sheet's content, before the shared frame. Every chord
// is a key.Binding whose Help drives a keycap chip in the left column with the
// action in the right column, under a per-group header. inner is the content
// width inside the border.
func (m *Model) shortcutsCardBody(inner int) []string {
	lines := []string{truncateLine(m.styles.SheetTitle.Render("Shortcuts"), inner)}

	// Size the keycap column to the widest key label (chip padding included),
	// capped so the action column always keeps a few cells of its own.
	keyText := 0
	for _, group := range shortcutGroups {
		for _, row := range group.rows {
			if width := lipgloss.Width(displayShortcutKeys(row.keys)); width > keyText {
				keyText = width
			}
		}
	}
	column := keyText + 2 + shortcutsMenuGutter
	if limit := inner - 4; column > limit {
		column = limit
	}
	if column < 4 {
		column = 4
	}
	if column > inner {
		column = inner
	}
	labelWidth := column - shortcutsMenuGutter
	actionWidth := inner - column
	if actionWidth < 1 {
		actionWidth = 1
	}
	actionStyle := lipgloss.NewStyle().Foreground(m.styles.Colors.TextMuted)

	for groupIndex, group := range shortcutGroups {
		if groupIndex > 0 {
			lines = append(lines, "")
		}
		lines = append(lines, truncateLine(m.styles.SectionHeader.Render(group.title), inner))
		for _, row := range group.rows {
			// The binding is the single source for both cells, so the chip and
			// its action can never drift from the chord map.
			keysLabel := displayShortcutKeys(row.keys)
			help := key.NewBinding(
				key.WithKeys(keysLabel),
				key.WithHelp(keysLabel, row.action),
			).Help()
			chip := truncateLine(m.styles.Keycap.Render(" "+help.Key+" "), labelWidth)
			keyColumn := lipgloss.NewStyle().Width(labelWidth).Render(chip)
			action := truncateLine(actionStyle.Render(displayShortcutAction(help.Desc)), actionWidth)
			line := keyColumn + strings.Repeat(" ", shortcutsMenuGutter) + action
			lines = append(lines, truncateLine(line, inner))
		}
	}
	return lines
}
