package ui

import (
	"strings"

	tea "charm.land/bubbletea/v2"
)

// MouseOptIn reports whether the pointer path is on. The flag and the
// environment variable both enable it; neither is a /pref toggle. Those
// toggles are shared with the Qt client, so a TUI-only preference would split
// the contract. Accepted environment values are 1, true, yes, and on.
func MouseOptIn(flag bool, env string) bool {
	if flag {
		return true
	}
	switch strings.ToLower(strings.TrimSpace(env)) {
	case "1", "true", "yes", "on":
		return true
	default:
		return false
	}
}

// SetMouseEnabled turns the pointer path on or off. The shell reads it when
// it builds the view, and ignores mouse messages while it is off.
func (m *Model) SetMouseEnabled(on bool) {
	if m == nil {
		return
	}
	m.mouseEnabled = on
}

// MouseEnabled reports whether the pointer path is on.
func (m *Model) MouseEnabled() bool { return m != nil && m.mouseEnabled }

// handleMouseClick runs the chord-equivalent for a left click. Other buttons
// are ignored so a right click cannot change selection.
func (m *Model) handleMouseClick(x, y int, button tea.MouseButton) {
	if m == nil || !m.mouseEnabled || button != tea.MouseLeft {
		return
	}
	switch hit := m.hitTest(x, y); hit.kind {
	case hitSidebarHeader:
		m.clickNetworkHeader(hit.networkID)
	case hitSidebarRow:
		m.clickConversation(hit.networkID, hit.target)
	case hitMember:
		m.clickMember(hit.index)
	case hitOverlayRow:
		m.activateOverlayRow(hit.index)
	case hitOverlayOutside:
		m.dismissPointerOverlay()
	}
}

// handleMouseWheel scrolls the region under the pointer. The transcript uses
// pageTranscript, the same step as Page Up/Down. The sidebar, the member list
// and an open overlay list page by one window of that region.
func (m *Model) handleMouseWheel(x, y int, button tea.MouseButton) {
	if m == nil || !m.mouseEnabled {
		return
	}
	direction := 0
	switch button {
	case tea.MouseWheelUp:
		direction = -1
	case tea.MouseWheelDown:
		direction = 1
	default:
		return
	}
	layout := m.screenLayout()
	if layout.overlayOn {
		if layout.overlay.contains(x, y) {
			m.wheelOverlay(direction)
		}
		return
	}
	switch {
	case layout.sidebar.contains(x, y):
		m.wheelSidebar(direction)
	case layout.members.contains(x, y):
		m.wheelMembers(direction)
	case layout.transcript.contains(x, y):
		m.pageTranscript(direction, 1)
	}
}

// clickNetworkHeader toggles the network the way Alt+Shift+Left and
// Alt+Shift+Right do, through SetNetworkCollapsed. The chord passes an
// explicit collapsed flag; a click flips the current value.
func (m *Model) clickNetworkHeader(networkID string) {
	if m == nil || m.ctrl == nil || networkID == "" {
		return
	}
	m.sidebarNetworkFocusID = networkID
	m.ctrl.SetNetworkCollapsed(networkID, !m.ctrl.IsNetworkCollapsed(networkID))
	m.sidebarScrollPinned = false
}

// clickConversation selects a sidebar row through the same switchSelection
// and SelectConversation path walk uses.
func (m *Model) clickConversation(networkID, target string) {
	if m == nil || m.ctrl == nil || target == "" {
		return
	}
	m.switchSelection(func() {
		m.ctrl.SelectConversation(networkID, target)
	})
}

// clickMember opens or creates the direct message for that row. It arms the
// member cursor and then calls activateFocusedMember, which is what Enter
// does after Ctrl+Shift+P, including the self-nick no-op and the UserOpen cause.
func (m *Model) clickMember(index int) {
	if m == nil || m.ctrl == nil {
		return
	}
	members := m.ctrl.Members()
	if index < 0 || index >= len(members) {
		return
	}
	m.memberIndex = index
	m.memberFocus = true
	m.activateFocusedMember()
}

// activateOverlayRow selects the row and runs the overlay's Enter path.
func (m *Model) activateOverlayRow(index int) {
	if m == nil {
		return
	}
	switch {
	case m.jump.open:
		m.jump.selected = index
		m.activateJump()
	case m.nick.open:
		m.nick.selected = index
		m.activateNick()
	case m.link.open:
		m.link.selected = index
		m.activateLink()
	case m.inbox.open:
		m.inbox.selected = index
		m.activateInboxSelection()
		m.closeInbox()
	case m.list.open:
		m.list.selected = index
		m.activateChannelList()
	}
}

// dismissPointerOverlay closes the list the way Escape does.
func (m *Model) dismissPointerOverlay() {
	if m == nil {
		return
	}
	switch {
	case m.nick.open:
		m.closeNickJump()
	case m.link.open:
		m.closeLink()
	case m.inbox.open:
		m.closeInbox()
	case m.list.open:
		m.closeChannelList()
	case m.jump.open:
		m.closeJump()
	}
}

// wheelSidebar pages the roster. direction < 0 shows earlier lines.
func (m *Model) wheelSidebar(direction int) {
	if m == nil {
		return
	}
	sidebar, _, _ := m.columnRects()
	_, _, w, h := m.framedInner(sidebar)
	if w < 1 || h < 1 {
		return
	}
	m.sidebarVisibleSlots(w, h)
	if m.sidebarWindowPage < 1 || m.sidebarWindowMax < 1 {
		return
	}
	next := m.sidebarWindowLine
	if direction < 0 {
		next -= m.sidebarWindowPage
	} else {
		next += m.sidebarWindowPage
	}
	m.sidebarScroll = clampInt(next, 0, m.sidebarWindowMax)
	m.sidebarScrollPinned = true
}

// wheelMembers pages the member list without moving the keyboard cursor.
func (m *Model) wheelMembers(direction int) {
	if m == nil {
		return
	}
	_, _, members := m.columnRects()
	_, _, w, h := m.framedInner(members)
	if w < 1 || h < 1 {
		return
	}
	m.memberVisibleSlots(w, h)
	if m.memberWindowPage < 1 || m.memberWindowMax < 1 {
		return
	}
	next := m.memberWindowLine
	if direction < 0 {
		next -= m.memberWindowPage
	} else {
		next += m.memberWindowPage
	}
	m.memberScroll = clampInt(next, 0, m.memberWindowMax)
	m.memberScrollPinned = true
}

// wheelOverlay pages the open list, or the shortcuts sheet. direction follows
// pageTranscript: negative moves toward the start.
func (m *Model) wheelOverlay(direction int) {
	if m == nil {
		return
	}
	if m.shortcutsOpen {
		key := "pgdown"
		if direction < 0 {
			key = "pgup"
		}
		m.handleShortcutsScrollKey(key)
		return
	}
	switch {
	case m.jump.open:
		wheelList(&m.jump.scroll, len(m.jumpEntries()), m.overlayRowBudget(1), direction)
	case m.nick.open:
		wheelList(&m.nick.scroll, len(m.nickEntries()), m.overlayRowBudget(1), direction)
	case m.link.open:
		wheelList(&m.link.scroll, len(m.linkMatches()), m.overlayRowBudget(1), direction)
	case m.inbox.open:
		wheelList(&m.inbox.scroll, len(m.inboxEntries()), m.overlayRowBudget(0), direction)
	case m.list.open:
		rows := m.channelListRows()
		if len(rows) > channelListRowCap {
			rows = rows[:channelListRowCap]
		}
		visible := m.overlayRowBudget(1) - channelListTableChrome
		if visible < 1 {
			visible = 1
		}
		wheelList(&m.list.scroll, len(rows), visible, direction)
	}
}
