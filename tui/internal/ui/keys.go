package ui

import tea "charm.land/bubbletea/v2"

// This file is the Phase 6 chord map: every window chord from
// `.cursor/skills/verify-omairc/features/keyboard.md`, plus the modal gating
// that disables the chords while Connect or the shortcuts sheet is open.
//
// The pure navigation chords live in chordTable, keyed by bubbletea's
// key.String() (lowercase arrows and special keys, "ctrl+" / "alt+" /
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
	"ctrl+shift+k": func(m *Model) tea.Cmd { m.openNickJump(); return nil },
	"ctrl+shift+p": func(m *Model) tea.Cmd { m.focusMembers(); return nil },
	"ctrl+shift+s": func(m *Model) tea.Cmd { m.toggleServerList(); return nil },
	"ctrl+w":       func(m *Model) tea.Cmd { m.closeDirectMessage(); return nil },
	"ctrl+/":       func(m *Model) tea.Cmd { m.openShortcuts(); return nil },
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
		m.completeNick()
		return true, nil
	case "up":
		if m.memberFocus {
			m.moveMember(-1)
			return true, nil
		}
		m.recallHistory(-1)
		return true, nil
	case "down":
		if m.memberFocus {
			m.moveMember(1)
			return true, nil
		}
		m.recallHistory(1)
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
// shortcuts sheet), Ctrl+C (copy), and Escape, are let through.
func (m *Model) modalBlocksChord(key string) bool {
	if !m.connectVisible() && !m.shortcutsOpen {
		return false
	}
	switch key {
	case "ctrl+q", "ctrl+/", "ctrl+c", "esc", "escape":
		return false
	}
	return true
}
