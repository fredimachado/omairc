package ui

import (
	"strings"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
)

// This file holds the Phase 5/6 conversation and network navigation: the walk
// and unread chords, the jump overlay, the Status toggle, the network header
// walk/collapse/reorder, the direct-message close, and the per-conversation
// composer drafts. It reads only controller snapshots, never internal/irc.

// Jump entry kinds.
const (
	jumpKindConversation = iota
	jumpKindStatus
)

// jumpEntry is one row of the jump overlay: a conversation or a network's
// Status surface.
type jumpEntry struct {
	kind           int
	label          string
	networkID      string
	target         string
	conversationID string
}

// jumpState is the Ctrl+K filter overlay.
type jumpState struct {
	open     bool
	input    textinput.Model
	selected int
}

func newJumpState() jumpState {
	input := textinput.New()
	input.Placeholder = "Jump to"
	input.Prompt = "› "
	return jumpState{input: input}
}

// jumpVisible reports whether the jump overlay is open.
func (m *Model) jumpVisible() bool { return m != nil && m.jump.open }

// openJump opens the jump overlay with an empty query and the composer
// blurred, so typed keys reach the filter.
func (m *Model) openJump() {
	if m.ctrl == nil {
		return
	}
	m.closeAllOverlays()
	m.jump.open = true
	m.jump.input.SetValue("")
	m.jump.selected = 0
	m.jump.input.SetWidth(m.overlayInputWidth())
	m.composer.Blur()
	_ = m.jump.input.Focus()
}

// closeJump dismisses the overlay and returns focus to the composer.
func (m *Model) closeJump() {
	if !m.jump.open {
		return
	}
	m.jump.open = false
	m.jump.input.Blur()
	_ = m.composer.Focus()
	m.loadDraft()
}

// jumpEntries builds the filtered overlay rows: every network's conversations
// in sidebar order, then that network's Status row, matching
// OmaircWindow.qml's refreshJumpMatches.
func (m *Model) jumpEntries() []jumpEntry {
	if m.ctrl == nil {
		return nil
	}
	query := strings.ToLower(strings.TrimSpace(m.jump.input.Value()))
	conversations := m.ctrl.Conversations()
	var entries []jumpEntry
	for _, networkID := range m.ctrl.NetworkOrder() {
		networkName := m.ctrl.NetworkDisplayName(networkID)
		for _, row := range conversations {
			if row.NetworkID != networkID {
				continue
			}
			label := m.jumpConversationLabel(row.ConversationName, networkName)
			if !jumpMatches(query, label) {
				continue
			}
			entries = append(entries, jumpEntry{
				kind:           jumpKindConversation,
				label:          label,
				networkID:      networkID,
				target:         row.ConversationName,
				conversationID: row.ConversationID,
			})
		}
		statusLabel := statusJumpLabel(networkName)
		if jumpMatches(query, statusLabel) {
			entries = append(entries, jumpEntry{
				kind:      jumpKindStatus,
				label:     statusLabel,
				networkID: networkID,
			})
		}
	}
	return entries
}

// jumpConversationLabel mirrors OmaircWindow.qml's jumpTargetLabel for a
// conversation: a duplicate channel name carries its network display name.
func (m *Model) jumpConversationLabel(name, networkName string) string {
	if name == "" {
		return ""
	}
	if duplicateTargetName(m.ctrl, name) && networkName != "" {
		return name + " · " + networkName
	}
	return name
}

// jumpMatches reports whether the lower-cased query is a substring of the
// label.
func jumpMatches(query, label string) bool {
	if query == "" {
		return true
	}
	return strings.Contains(strings.ToLower(label), query)
}

// moveJump moves the highlighted row by delta, wrapping at both ends.
func (m *Model) moveJump(delta int) {
	entries := m.jumpEntries()
	if len(entries) == 0 {
		m.jump.selected = 0
		return
	}
	m.jump.selected = ((m.jump.selected+delta)%len(entries) + len(entries)) % len(entries)
}

// activateJump opens the highlighted entry and dismisses the overlay. Landing
// on a conversation under a collapsed network expands that network first.
func (m *Model) activateJump() {
	entries := m.jumpEntries()
	if len(entries) == 0 {
		return
	}
	index := m.jump.selected
	if index < 0 || index >= len(entries) {
		index = 0
	}
	entry := entries[index]
	m.switchSelection(func() {
		if entry.kind == jumpKindStatus {
			m.ctrl.OpenStatus(entry.networkID)
			return
		}
		m.ctrl.SetNetworkCollapsed(entry.networkID, false)
		m.ctrl.SelectConversation(entry.networkID, entry.target)
	})
	m.closeJump()
}

// handleJumpKey folds one key while the overlay is open. Escape dismisses,
// Up/Down highlight, Enter activates, and everything else filters.
func (m *Model) handleJumpKey(key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	switch key {
	case "esc", "escape":
		m.closeJump()
		return m, nil
	case "up":
		m.moveJump(-1)
		return m, nil
	case "down":
		m.moveJump(1)
		return m, nil
	case "enter":
		m.activateJump()
		return m, nil
	}
	var cmd tea.Cmd
	m.jump.input, cmd = m.jump.input.Update(msg)
	m.jump.selected = 0
	return m, cmd
}

// handleOverlayKey routes one key to whichever filter overlay is open. Only
// one overlay ever owns the keys; opening one closes the rest.
func (m *Model) handleOverlayKey(key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	switch {
	case m.nick.open:
		return m.handleNickKey(key, msg)
	case m.link.open:
		return m.handleLinkKey(key, msg)
	case m.inbox.open:
		return m.handleInboxKey(key, msg)
	case m.jump.open:
		return m.handleJumpKey(key, msg)
	}
	return m, nil
}

// walk moves to the next or previous visible conversation over the sidebar
// order, wrapping. Status is not in the walk. Rows under a collapsed network
// are skipped; a hidden current row still has a place in the full order, so
// walking continues from there. It mirrors OmaircWindow.qml's stepConversation.
func (m *Model) walk(delta int) {
	if m.ctrl == nil {
		return
	}
	rows := m.ctrl.Conversations()
	if len(rows) == 0 {
		return
	}
	visible := false
	for _, row := range rows {
		if !m.ctrl.IsNetworkCollapsed(row.NetworkID) {
			visible = true
			break
		}
	}
	if !visible {
		return
	}
	current := -1
	for index, row := range rows {
		if row.ConversationID == m.ctrl.SelectedConversationID() && row.ConversationID != "" {
			current = index
			break
		}
	}
	start := current
	if start < 0 {
		if delta > 0 {
			start = -1
		} else {
			start = len(rows)
		}
	}
	for step := 1; step <= len(rows); step++ {
		index := ((start+delta*step)%len(rows) + len(rows)) % len(rows)
		row := rows[index]
		if m.ctrl.IsNetworkCollapsed(row.NetworkID) {
			continue
		}
		m.switchSelection(func() {
			m.ctrl.SelectConversation(row.NetworkID, row.ConversationName)
		})
		return
	}
}

// jumpUnread selects the next unread conversation, mentions first, skipping
// muted rows while hunting a mention. It mirrors OmaircWindow.qml's
// jumpToNextUnread: rows hidden under a collapsed network still count, and
// landing on one expands that network.
func (m *Model) jumpUnread() {
	if m.ctrl == nil {
		return
	}
	rows := m.ctrl.Conversations()
	if len(rows) == 0 {
		return
	}
	current := -1
	for index, row := range rows {
		if row.ConversationID == m.ctrl.SelectedConversationID() && row.ConversationID != "" {
			current = index
			break
		}
	}
	start := 0
	if current >= 0 {
		start = (current + 1) % len(rows)
	}
	mention := -1
	unread := -1
	for step := 0; step < len(rows); step++ {
		index := (start + step) % len(rows)
		if index == current {
			continue
		}
		row := rows[index]
		if mention < 0 && row.Mention && !row.Muted {
			mention = index
			break
		}
		if unread < 0 && row.Unread > 0 {
			unread = index
		}
	}
	target := mention
	if target < 0 {
		target = unread
	}
	if target < 0 {
		return
	}
	row := rows[target]
	m.switchSelection(func() {
		m.ctrl.SetNetworkCollapsed(row.NetworkID, false)
		m.ctrl.SelectConversation(row.NetworkID, row.ConversationName)
	})
}

// toggleStatus opens or closes the Status console. Opening remembers the
// conversation id so Escape can return to it. It mirrors the Ctrl+` shortcut in
// OmaircWindow.qml.
func (m *Model) toggleStatus() {
	if m.ctrl == nil {
		return
	}
	if m.ctrl.ConsoleOpen() {
		m.switchSelection(func() {
			if m.statusReturnID != "" && m.ctrl.SelectConversationByID(m.statusReturnID) {
				return
			}
			m.ctrl.ClearConversationSelection()
		})
		return
	}
	networkID := m.ctrl.FocusedNetworkID()
	if networkID == "" {
		networkID = m.ctrl.SelectedNetworkID()
	}
	if networkID == "" {
		return
	}
	m.statusReturnID = m.ctrl.SelectedConversationID()
	m.switchSelection(func() {
		m.ctrl.OpenStatus(networkID)
	})
}

// dismissEscape is the Escape handler when no modal or overlay owns the key.
// It leaves find first, then clears member focus, then closes Status.
func (m *Model) dismissEscape() {
	if m.find.active {
		m.leaveFind()
		return
	}
	if m.memberFocus {
		m.memberFocus = false
		return
	}
	if m.ctrl != nil && m.ctrl.ConsoleOpen() {
		m.toggleStatus()
	}
}

// --- Network headers and the server list ----------------------------------

// stepNetwork moves the network header focus with Alt+Left / Alt+Right,
// wrapping. It restores the server list column so the highlight is visible.
// It mirrors OmaircWindow.qml's stepNetwork.
func (m *Model) stepNetwork(delta int) {
	if m.ctrl == nil {
		return
	}
	order := m.ctrl.NetworkOrder()
	if len(order) == 0 {
		return
	}
	current := indexOfString(order, m.sidebarNetworkFocusID)
	if current < 0 {
		current = indexOfString(order, m.ctrl.SelectedNetworkID())
	}
	var next int
	if current < 0 {
		if delta > 0 {
			next = 0
		} else {
			next = len(order) - 1
		}
	} else {
		next = ((current+delta)%len(order) + len(order)) % len(order)
	}
	m.serverListVisible = true
	m.sidebarNetworkFocusID = order[next]
}

// openFocusedNetworkStatus opens the focused header's Status surface and
// clears the header focus, matching openNetworkStatus.
func (m *Model) openFocusedNetworkStatus() {
	if m.sidebarNetworkFocusID == "" || m.ctrl == nil {
		return
	}
	networkID := m.sidebarNetworkFocusID
	m.sidebarNetworkFocusID = ""
	m.switchSelection(func() {
		m.ctrl.OpenStatus(networkID)
	})
}

// collapseFocusedNetwork collapses or expands the focused network. It is a
// no-op without a focused header.
func (m *Model) collapseFocusedNetwork(collapsed bool) {
	if m.sidebarNetworkFocusID == "" || m.ctrl == nil {
		return
	}
	m.ctrl.SetNetworkCollapsed(m.sidebarNetworkFocusID, collapsed)
}

// collapseAllNetworks collapses or expands every network. Header focus is not
// required and is not stolen.
func (m *Model) collapseAllNetworks(collapsed bool) {
	if m.ctrl == nil {
		return
	}
	m.ctrl.SetAllNetworksCollapsed(collapsed)
}

// moveFocusedNetwork reorders the focused network with no wrap. Focus stays on
// the moved network.
func (m *Model) moveFocusedNetwork(delta int) {
	if m.sidebarNetworkFocusID == "" || m.ctrl == nil {
		return
	}
	m.ctrl.MoveNetwork(m.sidebarNetworkFocusID, delta)
}

// clearNetworkFocus is Ctrl+L: it drops the header focus so Enter sends again.
func (m *Model) clearNetworkFocus() {
	m.sidebarNetworkFocusID = ""
}

// toggleServerList collapses or restores the whole left rail. Collapsing drops
// the focused header, so Enter in the composer sends again.
func (m *Model) toggleServerList() {
	m.serverListVisible = !m.serverListVisible
	if !m.serverListVisible {
		m.sidebarNetworkFocusID = ""
	}
}

// closeDirectMessage closes the selected direct message with Ctrl+W. It is a
// no-op on a channel or Status.
func (m *Model) closeDirectMessage() {
	if m.ctrl == nil || m.ctrl.ConsoleOpen() {
		return
	}
	m.saveDraft()
	if !m.ctrl.CloseDirectMessage() {
		return
	}
	m.loadDraft()
	m.afterSelectionChange()
}

// focusMembers focuses the member list with Ctrl+Shift+P, reopening a hidden
// panel by making it visible when the window is wide enough. It is a no-op off
// a channel or on Status.
func (m *Model) focusMembers() {
	if m.ctrl == nil || !m.ctrl.IsChannel() || m.ctrl.ConsoleOpen() {
		return
	}
	m.memberFocus = true
	m.clampMemberIndex()
}

// indexOfString returns the index of needle in values, or -1.
func indexOfString(values []string, needle string) int {
	if needle == "" {
		return -1
	}
	for index, value := range values {
		if value == needle {
			return index
		}
	}
	return -1
}

// --- Per-conversation drafts ----------------------------------------------

// switchSelection stashes the current composer draft, runs apply (which changes
// the selection or Status surface), then restores the draft for the new key. It
// also drops find, the transcript cursor, and the member focus, matching the Qt
// selection side effects.
func (m *Model) switchSelection(apply func()) {
	if m.find.active {
		m.leaveFind()
	}
	m.saveDraft()
	apply()
	m.loadDraft()
	m.afterSelectionChange()
}

// afterSelectionChange resets the view state that does not survive a move to a
// new conversation or Status surface.
func (m *Model) afterSelectionChange() {
	m.transcriptFollowEnd = true
	m.transcriptScroll = 0
	m.transcriptCursor = -1
	m.resetHistoryBrowse()
	m.memberFocus = false
	m.memberIndex = 0
}

// saveDraft records the composer text under the current conversation key.
func (m *Model) saveDraft() {
	if m.draftKey == "" {
		return
	}
	m.drafts[m.draftKey] = m.composer.Value()
}

// loadDraft restores the composer text for the current conversation key.
func (m *Model) loadDraft() {
	key := m.composerDraftKey()
	m.draftKey = key
	m.composer.SetValue(m.drafts[key])
	m.composer.CursorEnd()
}

// composerDraftKey is the stable key unsent text belongs to: the Status key for
// the focused network, else the selected conversation id. It never follows the
// user across targets. It mirrors OmaircWindow.qml's composerHistoryKey.
func (m *Model) composerDraftKey() string {
	if m.ctrl == nil {
		return ""
	}
	if m.ctrl.ConsoleOpen() {
		return "status\n" + m.ctrl.FocusedNetworkID()
	}
	return m.ctrl.SelectedConversationID()
}

// clearCurrentDraft drops the stored draft for the current key, used after the
// composer sends.
func (m *Model) clearCurrentDraft() {
	if m.draftKey == "" {
		return
	}
	m.drafts[m.draftKey] = ""
}

// --- Rendering ------------------------------------------------------------

// jumpCardLines renders the jump overlay into a bordered block exactly width
// cells wide.
func (m *Model) jumpCardLines(width int) []string {
	inner := width - 4
	if inner < 8 {
		inner = 8
	}
	rendered := m.styles.JumpCard.Width(inner).Render(strings.Join(m.jumpCard(inner), "\n"))
	return strings.Split(rendered, "\n")
}

// jumpCard is the overlay's content, before the border.
func (m *Model) jumpCard(inner int) []string {
	lines := []string{m.styles.JumpQuery.Render("Jump") + "  " + m.jump.input.View()}
	entries := m.jumpEntries()
	if len(entries) == 0 {
		return append(lines, m.styles.JumpEmpty.Render("No matches"))
	}
	for index, entry := range entries {
		style := m.styles.JumpRow
		if index == m.jump.selected {
			style = m.styles.JumpSelected
		}
		lines = append(lines, style.Render(truncateLine(entry.label, inner)))
	}
	return lines
}

// overlayInputWidth is the width the composer and the overlay filters use
// inside the current window.
func (m *Model) overlayInputWidth() int {
	width := m.width - 8
	if width < 1 {
		width = 1
	}
	return width
}
