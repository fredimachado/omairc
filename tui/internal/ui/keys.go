package ui

import (
	"charm.land/bubbles/v2/key"
	tea "charm.land/bubbletea/v2"
)

// This file is the Phase 6 chord map: every window chord from
// `.cursor/skills/verify-omairc/features/keyboard.md`, plus the modal gating
// that disables the chords while Connect or the shortcuts sheet is open.
//
// The pure navigation chords live in chordTable, keyed by bubbletea's
// key.Keystroke() (lowercase arrows and special keys, "ctrl+" / "alt+" /
// "shift+" prefixes in that order). Chords that need the raw key press, the
// composer, or a mode check are handled in dispatchChord directly.

// chordFunc applies one navigation chord. It returns an optional tea.Cmd (the
// copy chord emits the clipboard OSC 52 sequence); most chords return nil.
type chordFunc func(*Model) tea.Cmd

// chordTable is the explicit chord map. Every entry changes selection, network
// display state, an overlay, or focus; none of them touch the composer text.
var chordTable = map[string]chordFunc{
	"alt+down":  func(m *Model) tea.Cmd { m.walk(1); return nil },
	"alt+up":    func(m *Model) tea.Cmd { m.walk(-1); return nil },
	"alt+right": func(m *Model) tea.Cmd { m.stepNetwork(1); return nil },
	"alt+left":  func(m *Model) tea.Cmd { m.stepNetwork(-1); return nil },

	"alt+shift+left":       func(m *Model) tea.Cmd { m.collapseFocusedNetwork(true); return nil },
	"alt+shift+right":      func(m *Model) tea.Cmd { m.collapseFocusedNetwork(false); return nil },
	"ctrl+alt+shift+left":  func(m *Model) tea.Cmd { m.collapseAllNetworks(true); return nil },
	"ctrl+alt+shift+right": func(m *Model) tea.Cmd { m.collapseAllNetworks(false); return nil },
	"alt+shift+up":         func(m *Model) tea.Cmd { m.moveFocusedNetwork(-1); return nil },
	"alt+shift+down":       func(m *Model) tea.Cmd { m.moveFocusedNetwork(1); return nil },

	"ctrl+l":       func(m *Model) tea.Cmd { m.clearNetworkFocus(); return nil },
	"ctrl+k":       func(m *Model) tea.Cmd { m.openJump(); return nil },
	"alt+a":        func(m *Model) tea.Cmd { m.jumpUnread(); return nil },
	"alt+shift+a":  func(m *Model) tea.Cmd { m.markAllRead(); return nil },
	"alt+u":        func(m *Model) tea.Cmd { m.jumpToUnseen(); return nil },
	"ctrl+shift+k": func(m *Model) tea.Cmd { m.openNickJump(); return nil },
	"ctrl+shift+p": func(m *Model) tea.Cmd { m.focusMembers(); return nil },
	"ctrl+shift+m": func(m *Model) tea.Cmd { m.toggleMembers(); return nil },
	"ctrl+shift+s": func(m *Model) tea.Cmd { m.toggleServerList(); return nil },
	"ctrl+w":       func(m *Model) tea.Cmd { m.closeDirectMessage(); return nil },
	"ctrl+/":       func(m *Model) tea.Cmd { m.openShortcuts(); return nil },
	"ctrl+shift+/": func(m *Model) tea.Cmd { m.openAbout(); return nil },
	"ctrl+,":       func(m *Model) tea.Cmd { m.openConnect(); return nil },
	"ctrl+shift+o": func(m *Model) tea.Cmd { m.toggleLink(); return nil },
	"ctrl+shift+a": func(m *Model) tea.Cmd { m.toggleInbox(); return nil },
	"ctrl+`":       func(m *Model) tea.Cmd { m.toggleStatus(); return nil },
	"ctrl+f":       func(m *Model) tea.Cmd { m.beginOrAdvanceFind(); return nil },
	"ctrl+c":       func(m *Model) tea.Cmd { return m.copySelection() },
	"esc":          func(m *Model) tea.Cmd { m.dismissEscape(); return nil },
	"escape":       func(m *Model) tea.Cmd { m.dismissEscape(); return nil },
}

// dispatchChord folds one key through the chord table. It reports whether the
// chord consumed the key; an unhandled key falls through to the composer.
func (m *Model) dispatchChord(key string, msg tea.KeyPressMsg) (bool, tea.Cmd) {
	if m.modalBlocksChord(key) {
		return true, nil
	}
	// The slash-completion session owns Tab/Up/Down/Escape/Enter while it is
	// open (and the composer, not the member panel or the sidebar, has focus).
	// It runs before the chord table so Escape dismisses the list instead of
	// the selection, and before nick-complete and history.
	if m.slash.open() && !m.memberFocus && m.sidebarNetworkFocusID == "" {
		if handled, cmd := m.routeSlashKey(key); handled {
			return true, cmd
		}
	}
	if fn, ok := chordTable[key]; ok {
		cmd := fn(m)
		m.refocusComposer()
		return true, cmd
	}
	switch key {
	case "enter", "return":
		if m.memberFocus {
			m.activateFocusedMember()
			return true, nil
		}
		if m.sidebarNetworkFocusID != "" {
			m.openFocusedNetworkStatus()
			m.refocusComposer()
			return true, nil
		}
		m.sendComposer()
		return true, nil
	case "tab":
		m.completeNick(false)
		m.syncSlash()
		return true, nil
	case "shift+tab":
		m.completeNick(true)
		m.syncSlash()
		return true, nil
	case "up":
		if m.memberFocus {
			m.moveMember(-1)
			return true, nil
		}
		m.recallHistory(-1)
		m.syncSlash()
		return true, nil
	case "down":
		if m.memberFocus {
			m.moveMember(1)
			return true, nil
		}
		m.recallHistory(1)
		m.syncSlash()
		return true, nil
	case "home":
		if m.memberFocus {
			m.jumpMembers(false)
			return true, nil
		}
	case "end":
		if m.memberFocus {
			m.jumpMembers(true)
			return true, nil
		}
	case "pgup":
		if m.memberFocus {
			m.pageMembers(-1, 1)
		} else {
			m.pageTranscript(-1, 1)
		}
		return true, nil
	case "pgdown":
		if m.memberFocus {
			m.pageMembers(1, 1)
		} else {
			m.pageTranscript(1, 1)
		}
		return true, nil
	case "shift+pgup":
		if m.memberFocus {
			m.pageMembers(-1, 0.35)
		} else {
			m.pageTranscript(-1, 0.35)
		}
		return true, nil
	case "shift+pgdown":
		if m.memberFocus {
			m.pageMembers(1, 0.35)
		} else {
			m.pageTranscript(1, 0.35)
		}
		return true, nil
	case "ctrl+home":
		m.jumpTranscript(false)
		return true, nil
	case "ctrl+end":
		m.jumpTranscript(true)
		return true, nil
	}
	return false, nil
}

// modalBlocksChord reports whether the open Connect sheet or shortcuts sheet
// must swallow key. Every navigation chord is disabled under those modals;
// only the keys the modal itself owns, plus Ctrl+Q (quit), Ctrl+/ (toggle the
// shortcuts sheet), Ctrl+C (copy), and Escape, are let through. About is
// handled before this runs, but it is included so the gate stays total.
func (m *Model) modalBlocksChord(key string) bool {
	if !m.connectVisible() && !m.shortcutsOpen && !m.aboutOpen {
		return false
	}
	switch key {
	case "ctrl+q", "ctrl+/", "ctrl+c", "esc", "escape":
		return false
	}
	return true
}

// footerShortcutBinding is the tail of the footer's help: the Ctrl+/ toggle
// that opens the shortcuts sheet. Every ShortHelp ends with it so the way to
// learn the rest of the map is always visible.
var footerShortcutBinding = key.NewBinding(
	key.WithKeys("ctrl+/"),
	key.WithHelp("Ctrl+/", "shortcuts"),
)

// footerKeyMap is the bubbles/help KeyMap behind the footer's right side. It
// mirrors the chords in chordTable and dispatchChord so the footer can never
// drift into a parallel hand-written hint string. ShortHelp is contextual: it
// lists the chords that apply where focus sits and always ends with the Ctrl+/
// shortcuts binding.
type footerKeyMap struct{ m *Model }

// binding is a tiny constructor so the context table below stays one line per
// chord.
func binding(keys []string, helpKey, helpDesc string) key.Binding {
	return key.NewBinding(key.WithKeys(keys...), key.WithHelp(helpKey, helpDesc))
}

// ShortHelp returns the contextual single-line help. It always appends the
// Ctrl+/ shortcuts binding last.
func (k footerKeyMap) ShortHelp() []key.Binding {
	return append(k.contextBindings(), footerShortcutBinding)
}

// FullHelp groups the same context bindings next to the shortcuts toggle.
func (k footerKeyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{k.contextBindings(), {footerShortcutBinding}}
}

// contextBindings is the chord set for the current focus, read from the same
// state dispatchChord routes on.
func (k footerKeyMap) contextBindings() []key.Binding {
	m := k.m
	switch {
	case m == nil || m.ctrl == nil:
		return nil
	case m.shortcutsOpen:
		return []key.Binding{
			binding([]string{"esc", "escape", "ctrl+/"}, "Esc", "close"),
		}
	case m.aboutOpen:
		return []key.Binding{
			binding([]string{"enter", "return", "esc", "escape"}, "Esc", "close"),
		}
	case m.connectVisible():
		return []key.Binding{
			binding([]string{"tab"}, "Tab", "next field"),
			binding([]string{"enter", "return"}, "Enter", "next"),
			binding([]string{"ctrl+enter", "ctrl+return"}, "Ctrl+Enter", "apply"),
		}
	case m.find.active:
		return []key.Binding{
			binding([]string{"ctrl+f"}, "Ctrl+F", "next match"),
			binding([]string{"esc", "escape"}, "Esc", "dismiss"),
		}
	case m.overlaysVisible():
		return []key.Binding{
			binding([]string{"enter", "return"}, "Enter", "select"),
			binding([]string{"esc", "escape"}, "Esc", "dismiss"),
		}
	case m.memberFocus:
		return []key.Binding{
			binding([]string{"enter", "return"}, "Enter", "open DM"),
			binding([]string{"up", "down"}, "↑/↓", "move"),
			binding([]string{"pgup", "pgdown"}, "PgUp/PgDn", "page"),
		}
	case m.sidebarNetworkFocusID != "":
		return []key.Binding{
			binding([]string{"enter", "return"}, "Enter", "Status"),
			binding([]string{"alt+left", "alt+right"}, "Alt+←/→", "networks"),
			binding([]string{"alt+up", "alt+down"}, "Alt+↑/↓", "walk"),
		}
	default:
		return []key.Binding{
			binding([]string{"enter", "return"}, "Enter", "send"),
			binding([]string{"tab"}, "Tab", "complete nick"),
			binding([]string{"ctrl+k"}, "Ctrl+K", "jump"),
		}
	}
}
