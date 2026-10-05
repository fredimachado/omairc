package ui

import (
	"sort"
	"strconv"
	"strings"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
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

// Overlay filter chrome. Every filter sheet shares one prompt glyph, so the
// fields line up inside the card, and one placeholder per sheet. The state
// constructors run before the model has a palette, so each open* rebuilds its
// filter through newTextInput with the live styles; defaultStyles seeds the
// never-rendered pre-open value.
const (
	overlayFilterPrompt = "› "
	jumpPlaceholder     = "Jump to"
)

func newJumpState() jumpState {
	return jumpState{input: newTextInput(defaultStyles(), jumpPlaceholder, overlayFilterPrompt)}
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
	m.jump.input = newTextInput(m.styles, jumpPlaceholder, overlayFilterPrompt)
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
// OmaircWindow.qml's refreshJumpMatches. A channel also matches its topic and
// a direct message matches a meaningful real name. Name matches stay above
// detail-only matches, and sidebar order holds inside each group. The list
// stays short.
func (m *Model) jumpEntries() []jumpEntry {
	if m.ctrl == nil {
		return nil
	}
	query := strings.TrimSpace(m.jump.input.Value())
	conversations := m.ctrl.Conversations()
	type ranked struct {
		entry jumpEntry
		score int
		order int
	}
	var rows []ranked
	order := 0
	for _, networkID := range m.sidebarNetworkIDs() {
		networkName := m.sidebarNetworkDisplayName(networkID)
		for _, row := range conversations {
			if row.NetworkID != networkID {
				continue
			}
			label := m.jumpConversationLabel(row.ConversationName, networkName)
			detail := ""
			if row.Direct {
				detail = m.ctrl.PeerRealname(row.NetworkID, row.ConversationName)
			} else {
				detail = m.ctrl.ConversationTopic(row.NetworkID, row.ConversationName)
			}
			score := controller.JumpScore(query, label, detail)
			if query != "" && score <= 0 {
				continue
			}
			rows = append(rows, ranked{
				entry: jumpEntry{
					kind:           jumpKindConversation,
					label:          label,
					networkID:      networkID,
					target:         row.ConversationName,
					conversationID: row.ConversationID,
				},
				score: score,
				order: order,
			})
			order++
		}
		statusLabel := statusJumpLabel(networkName)
		statusScore := controller.JumpScore(query, statusLabel, "")
		if query != "" && statusScore <= 0 {
			continue
		}
		rows = append(rows, ranked{
			entry: jumpEntry{
				kind:      jumpKindStatus,
				label:     statusLabel,
				networkID: networkID,
			},
			score: statusScore,
			order: order,
		})
		order++
	}
	// A name hit scores 2 or 3. A detail-only hit scores 1. The extra point
	// for a topic or real name must not reorder two name matches.
	sort.SliceStable(rows, func(left, right int) bool {
		leftName := rows[left].score >= 2
		rightName := rows[right].score >= 2
		if leftName != rightName {
			return leftName
		}
		return rows[left].order < rows[right].order
	})
	limit := controller.JumpResultLimit()
	if len(rows) > limit {
		rows = rows[:limit]
	}
	entries := make([]jumpEntry, 0, len(rows))
	for _, row := range rows {
		entries = append(entries, row.entry)
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
	case m.list.open:
		return m.handleChannelListKey(key, msg)
	case m.jump.open:
		return m.handleJumpKey(key, msg)
	case m.file.open:
		return m.handleFilePickKey(key, msg)
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
// landing on one expands that network. When nothing is unread, Alt+A opens
// Status for the selected conversation's network. When Status is already
// open, it opens that Status network.
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
		m.openCurrentNetworkStatus()
		return
	}
	row := rows[target]
	m.switchSelection(func() {
		m.ctrl.SetNetworkCollapsed(row.NetworkID, false)
		m.ctrl.SelectConversation(row.NetworkID, row.ConversationName)
	})
}

// markAllRead marks every conversation read and drops the attention title.
// It mirrors the Alt+Shift+A shortcut in OmaircWindow.qml.
func (m *Model) markAllRead() {
	m.clearTitleMark()
	if m.ctrl == nil {
		return
	}
	m.ctrl.MarkAllRead()
}

// openCurrentNetworkStatus opens Status for the selected conversation's
// network. When Status is already open, it opens that Status network. It
// mirrors the empty-unread landing in OmaircWindow.qml's jumpToNextUnread.
func (m *Model) openCurrentNetworkStatus() {
	if m.ctrl == nil {
		return
	}
	networkID := m.ctrl.FocusedNetworkID()
	if networkID == "" {
		networkID = m.ctrl.SelectedNetworkID()
	}
	if networkID == "" {
		return
	}
	m.switchSelection(func() {
		m.ctrl.OpenStatus(networkID)
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
	order := m.sidebarNetworkIDs()
	if len(order) == 0 {
		return
	}
	current := indexOfString(order, m.sidebarNetworkFocusID)
	if current < 0 {
		current = indexOfString(order, m.ctrl.FocusedNetworkID())
		if current < 0 && m.conn != nil {
			current = indexOfString(order, m.conn.SelectedNetworkID())
		}
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
	for _, networkID := range m.sidebarNetworkIDs() {
		m.ctrl.SetNetworkCollapsed(networkID, collapsed)
	}
}

// moveFocusedNetwork reorders the focused network with no wrap. Focus stays on
// the moved network.
func (m *Model) moveFocusedNetwork(delta int) {
	if m.sidebarNetworkFocusID == "" {
		return
	}
	if m.conn != nil {
		if m.conn.MoveNetwork(m.sidebarNetworkFocusID, delta) {
			m.syncSidebarNetworkOrder()
		}
		return
	}
	if m.ctrl != nil {
		m.ctrl.MoveNetwork(m.sidebarNetworkFocusID, delta)
	}
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

// closeDirectMessage closes the selected direct message, or a channel you
// have left, with Ctrl+W. It is a no-op on a channel you are in, or on Status.
func (m *Model) closeDirectMessage() {
	if m.ctrl == nil || m.ctrl.ConsoleOpen() {
		return
	}
	m.saveDraft()
	previousID := m.selectedConversationID()
	if !m.ctrl.CloseDirectMessage() {
		return
	}
	m.loadDraft()
	m.afterSelectionChange(previousID)
}

// focusMembers focuses the member list with Ctrl+Shift+P, reopening a hidden
// panel by making it visible when the window is wide enough. It is a no-op off
// a channel or on Status.
func (m *Model) focusMembers() {
	if m.ctrl == nil || !m.ctrl.IsChannel() || !m.ctrl.ChannelJoined() || m.ctrl.ConsoleOpen() {
		return
	}
	// The focus path reopens the panel if Ctrl+Shift+M hid it, mirroring the
	// feature map's "Press Ctrl+Shift+P to focus the list (and reopen it if it
	// was hidden)".
	m.membersHidden = false
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
// selection side effects. The previous conversation id is captured before apply
// so the landing step can tell a real move from a re-select.
//
// It clears the network-header focus first. Qt's selectConversation and
// openNetworkStatus both do this (src/OmaircWindow.qml), so the composer's Enter
// sends again after a walk. Leaving the header armed made Enter open Status
// instead: the walk was a no-op for the header focus, so the typed line stayed
// in the composer as a draft while the transcript jumped away.
func (m *Model) switchSelection(apply func()) {
	if m.find.active {
		m.leaveFind()
	}
	m.sidebarNetworkFocusID = ""
	previousID := m.selectedConversationID()
	m.saveDraft()
	apply()
	m.loadDraft()
	m.afterSelectionChange(previousID)
}

// selectedConversationID is the selected conversation's stable id, or "" for
// Status or no selection.
func (m *Model) selectedConversationID() string {
	if m.ctrl == nil {
		return ""
	}
	return m.ctrl.SelectedConversationID()
}

// afterSelectionChange resets the view state that does not survive a move to a
// new conversation or Status surface, then places the transcript. previousID is
// the conversation id before the change: re-selecting the same conversation
// while "Open conversations at unread" is on keeps the reader's viewport. It
// mirrors OmaircWindow.qml's placeTranscriptAfterSelect.
func (m *Model) afterSelectionChange(previousID string) {
	// Every selection change, including a return from Status onto the
	// conversation that was already selected. clearTitleMarkIfOpened itself
	// keeps the mark while Status is open.
	m.clearTitleMarkIfOpened()
	if previousID != "" && m.ctrl != nil && previousID != m.ctrl.SelectedConversationID() {
		m.ctrl.ClearHistoryPageCapTailForConversationID(previousID)
	}
	m.firstUnseenRow = -1
	m.transcriptCount = m.transcriptRowTotal()
	if m.keepsViewportOnReselect(previousID) {
		m.transcriptCursor = -1
		m.resetHistoryBrowse()
		m.memberFocus = false
		m.memberIndex = 0
		return
	}
	m.setTranscriptFollowEnd(true)
	m.transcriptScroll = 0
	m.transcriptCursor = -1
	m.clearTranscriptAnchor()
	m.resetHistoryBrowse()
	m.memberFocus = false
	m.memberIndex = 0
	m.placeTranscriptAfterSelect()
	if m.ctrl == nil || !m.ctrl.ConsoleOpen() {
		m.syncReadMarkerViewport()
	}
}

// keepsViewportOnReselect reports whether re-selecting the current conversation
// must leave the transcript where it was. It mirrors the openConversationsAtUnread
// guard at the head of OmaircWindow.qml's placeTranscriptAfterSelect; Status
// always repositions.
func (m *Model) keepsViewportOnReselect(previousID string) bool {
	if m.ctrl == nil || !m.ctrl.OpenAtUnread() || m.ctrl.ConsoleOpen() {
		return false
	}
	return previousID != "" && previousID == m.ctrl.SelectedConversationID()
}

// placeTranscriptAfterSelect lands the transcript for a new selection: the
// Status console follows the end, and a conversation follows the end unless
// "Open conversations at unread" is on and a "New messages" mark exists, in
// which case it lands on that mark. It mirrors OmaircWindow.qml's
// placeTranscriptAfterSelect and pinTranscriptToUnreadOr.
func (m *Model) placeTranscriptAfterSelect() {
	if m.ctrl == nil {
		return
	}
	if m.ctrl.ConsoleOpen() || !m.ctrl.OpenAtUnread() {
		return
	}
	if row := m.ctrl.UnreadMarkRow(); row >= 0 {
		m.pinTranscriptToRow(row)
	}
}

// pinTranscriptOnFocusReturn lands the transcript when the window regains
// focus: Status follows the end, and a conversation with a "New messages" mark
// lands on it. A conversation without a mark keeps its place, so a focus regain
// never yanks a reader who is scrolled up mid-history. It mirrors
// OmaircWindow.qml's pinTranscriptOnFocusReturn.
func (m *Model) pinTranscriptOnFocusReturn() {
	if m.ctrl == nil {
		return
	}
	if m.ctrl.ConsoleOpen() {
		m.jumpTranscript(true)
		return
	}
	if row := m.ctrl.UnreadMarkRow(); row >= 0 {
		m.pinTranscriptToRow(row)
	}
}

// saveDraft records the composer text under the current conversation key.
func (m *Model) saveDraft() {
	if m.draftKey == "" {
		return
	}
	m.drafts[m.draftKey] = m.composer.Value()
}

// followControllerSelection keeps an unsent line on the conversation that was
// showing when the controller moves the selection on its own. Apply opens
// Status and loadDraft records that key; autojoin then selects the channel
// without another loadDraft, so a later save would keep writing under Status
// while a queued file snapshots the channel. Save the visible composer under
// the old key, then load the draft for the key the controller is on now.
func (m *Model) followControllerSelection() {
	if m.ctrl == nil {
		return
	}
	next := m.composerDraftKey()
	if next == m.draftKey {
		return
	}
	if m.find.active {
		m.leaveFind()
	}
	m.saveDraft()
	m.loadDraft()
}

// loadDraft restores the composer text for the current conversation key.
func (m *Model) loadDraft() {
	key := m.composerDraftKey()
	m.draftKey = key
	m.composer.SetValue(m.drafts[key])
	m.composer.CursorEnd()
	m.resetNickComplete()
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

// jumpCard renders the jump overlay as one card block through the shared
// overlay frame in model.go.
func (m *Model) jumpCard(width int) string {
	return m.overlayCardBlock(width, m.jumpCardBody)
}

// jumpCardBody is the jump overlay's content, before the shared frame. inner is
// the content width inside the border.
func (m *Model) jumpCardBody(inner int) []string {
	entries := m.jumpEntries()
	lines := m.overlaySheetHeader(inner, "Jump", len(entries))
	lines = append(lines, truncateLine(m.jump.input.View(), inner))
	if len(entries) == 0 {
		return append(lines, m.overlaySheetEmpty("No matches"))
	}
	for index, entry := range entries {
		lines = append(lines, m.overlayToggleRow(inner, index == m.jump.selected, entry.label))
	}
	return lines
}

// --- Shared overlay chrome ------------------------------------------------

// The sheet rows read as a check list: the highlighted row carries the accent
// bar, a checked [x] chip, and a raised-surface fill that reaches the card's
// inner width, while every other row carries an unchecked [ ] chip.
const (
	overlayChipOn  = "[x]"
	overlayChipOff = "[ ]"
)

// overlaySheetHeader renders the heading every filter sheet opens with: a
// title, a muted count pill when the sheet has rows, and a divider rule that
// reaches the card's inner width.
func (m *Model) overlaySheetHeader(inner int, title string, count int) []string {
	head := m.styles.PanelTitle.Render(title)
	if count > 0 {
		head += " " + m.badge(m.styles.BadgeMuted, strconv.Itoa(count))
	}
	lines := []string{head}
	if inner > 0 {
		lines = append(lines, m.styles.Divider.Render(strings.Repeat("─", inner)))
	}
	return lines
}

// overlaySheetEmpty renders one sheet's empty state in the shared muted style.
func (m *Model) overlaySheetEmpty(text string) string {
	return m.styles.JumpEmpty.Render(text)
}

// overlayToggleRow renders one sheet row through the shared chrome: a leading
// accent bar on the highlighted row, an [x]/[ ] toggle chip, the label, and a
// raised-surface fill that reaches inner. label is plain text; the caller never
// passes pre-styled text, whose reset would clear the row fill.
func (m *Model) overlayToggleRow(inner int, selected bool, label string) string {
	bar := " "
	barStyle := m.styles.Divider
	chip := overlayChipOff
	chipStyle := m.styles.SheetLabel
	rowStyle := m.styles.JumpRow
	if selected {
		fill := m.styles.Colors.SurfaceRaised
		bar = "▌"
		barStyle = m.styles.JumpQuery.Background(fill)
		chip = overlayChipOn
		chipStyle = m.styles.SheetToggleOn.Background(fill)
		rowStyle = m.styles.JumpSelected.Background(fill)
	}
	prefix := barStyle.Render(bar) + chipStyle.Render(chip) + rowStyle.Render(" ")
	line := prefix + rowStyle.Render(truncateLine(label, inner-lipgloss.Width(prefix)))
	if pad := inner - lipgloss.Width(line); pad > 0 {
		line += rowStyle.Render(strings.Repeat(" ", pad))
	}
	return line
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
