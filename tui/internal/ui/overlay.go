package ui

import (
	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
)

// filterOverlay is the one exclusive filter sheet: jump, nick, link, inbox,
// search, list, or file. Model.overlay holds at most one, so two sheets cannot
// be open by construction, and opening one replaces whichever was there
// (setOverlay). Only the seven pointer types in this package implement it. A new sheet must
// implement every method below, so every dispatch (card, key, paste, pointer,
// resize, restyle) handles it or the package does not compile.
//
// This is a separate axis from the layer stack (channel prompt, About,
// shortcuts, Connect). overlayCard checks those layers first, in that order,
// and the filter sheet last. Connect and the shortcuts sheet are never a
// filterOverlay.
type filterOverlay interface {
	// card renders the sheet as one framed block.
	card(m *Model, width int) string
	// handleKey folds one key while the sheet owns the keys.
	handleKey(m *Model, key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd)
	// updateInput folds a non-key message (a paste) into the sheet's filter
	// input. A sheet without an input ignores it.
	updateInput(msg tea.Msg) tea.Cmd
	// dismiss closes the sheet the way Escape does and refocuses the composer.
	dismiss(m *Model)
	// pointerDismissable reports whether a click outside the card dismisses it.
	pointerDismissable() bool
	// overlayLines is the clickable content, or nil when the card has no rows.
	overlayLines(m *Model, inner int) []overlayLine
	// activateRow runs the sheet's Enter path on a clicked row.
	activateRow(m *Model, index int)
	// wheel pages the sheet's rows. direction < 0 moves toward the start.
	wheel(m *Model, direction int)
	// restyle re-applies the shared input styles after a theme change.
	restyle(styles textinput.Styles)
	// setInputWidth keeps the filter input in step with the window.
	setInputWidth(width int)
	// discard silently drops the sheet's state when it is replaced or closed.
	// It does not move focus; the caller owns that.
	discard(m *Model)
}

// setOverlay makes next the open filter sheet. A sheet that was open is
// discarded first, so exactly one value is ever held.
func (m *Model) setOverlay(next filterOverlay) {
	previous := m.overlay
	m.overlay = next
	if previous != nil {
		previous.discard(m)
	}
}

// clearOverlay closes the open filter sheet without moving focus. Callers
// refocus the composer themselves when the user is returning to it.
func (m *Model) clearOverlay() {
	previous := m.overlay
	m.overlay = nil
	if previous != nil {
		previous.discard(m)
	}
}

// overlaysVisible reports whether a filter sheet owns the keys.
func (m *Model) overlaysVisible() bool { return m != nil && m.overlay != nil }

// handleOverlayKey routes one key to the open filter sheet.
func (m *Model) handleOverlayKey(key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	if m.overlay == nil {
		return m, nil
	}
	return m.overlay.handleKey(m, key, msg)
}

// overlayInputUpdate folds a non-key message (a paste, for one) into the open
// sheet's filter input.
func (m *Model) overlayInputUpdate(msg tea.Msg) tea.Cmd {
	if m.overlay == nil {
		return nil
	}
	cmd := m.overlay.updateInput(msg)
	if m.searchSheet() != nil {
		// A paste edits the search field, so it restarts the search like a key.
		cmd = tea.Batch(cmd, m.searchQueryChanged())
	}
	return cmd
}

// Typed accessors: each returns the open sheet when it is that sheet, else nil.

func (m *Model) jumpSheet() *jumpState {
	if m == nil {
		return nil
	}
	sheet, _ := m.overlay.(*jumpState)
	return sheet
}

func (m *Model) nickSheet() *nickJumpState {
	if m == nil {
		return nil
	}
	sheet, _ := m.overlay.(*nickJumpState)
	return sheet
}

func (m *Model) linkSheet() *linkState {
	if m == nil {
		return nil
	}
	sheet, _ := m.overlay.(*linkState)
	return sheet
}

func (m *Model) inboxSheet() *inboxState {
	if m == nil {
		return nil
	}
	sheet, _ := m.overlay.(*inboxState)
	return sheet
}

func (m *Model) searchSheet() *searchState {
	if m == nil {
		return nil
	}
	sheet, _ := m.overlay.(*searchState)
	return sheet
}

func (m *Model) listSheet() *channelListState {
	if m == nil {
		return nil
	}
	sheet, _ := m.overlay.(*channelListState)
	return sheet
}

func (m *Model) fileSheet() *filePickState {
	if m == nil {
		return nil
	}
	sheet, _ := m.overlay.(*filePickState)
	return sheet
}

// filterInputUpdate folds msg into one sheet's filter input.
func filterInputUpdate(input *textinput.Model, msg tea.Msg) tea.Cmd {
	var cmd tea.Cmd
	*input, cmd = input.Update(msg)
	return cmd
}

// Interface implementations. The sheets' own behavior lives in their files;
// these adapters bind each sheet to the one method set above.

func (s *jumpState) card(m *Model, width int) string { return m.jumpCard(width) }
func (s *jumpState) handleKey(m *Model, key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	return m.handleJumpKey(key, msg)
}
func (s *jumpState) updateInput(msg tea.Msg) tea.Cmd { return filterInputUpdate(&s.input, msg) }
func (s *jumpState) dismiss(m *Model)                { m.closeJump() }
func (s *jumpState) pointerDismissable() bool        { return true }
func (s *jumpState) overlayLines(m *Model, inner int) []overlayLine {
	return m.jumpOverlayLines(inner)
}
func (s *jumpState) activateRow(m *Model, index int) {
	s.selected = index
	m.activateJump()
}
func (s *jumpState) wheel(m *Model, direction int) {
	wheelList(&s.scroll, len(m.jumpEntries()), m.overlayRowBudget(1), direction)
}
func (s *jumpState) restyle(styles textinput.Styles) { s.input.SetStyles(styles) }
func (s *jumpState) setInputWidth(width int)         { s.input.SetWidth(width) }
func (s *jumpState) discard(*Model)                  { s.input.Blur() }

func (s *nickJumpState) card(m *Model, width int) string { return m.nickCard(width) }
func (s *nickJumpState) handleKey(m *Model, key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	return m.handleNickKey(key, msg)
}
func (s *nickJumpState) updateInput(msg tea.Msg) tea.Cmd { return filterInputUpdate(&s.input, msg) }
func (s *nickJumpState) dismiss(m *Model)                { m.closeNickJump() }
func (s *nickJumpState) pointerDismissable() bool        { return true }
func (s *nickJumpState) overlayLines(m *Model, inner int) []overlayLine {
	return m.nickOverlayLines(inner)
}
func (s *nickJumpState) activateRow(m *Model, index int) {
	s.selected = index
	m.activateNick()
}
func (s *nickJumpState) wheel(m *Model, direction int) {
	wheelList(&s.scroll, len(m.nickEntries()), m.overlayRowBudget(1), direction)
}
func (s *nickJumpState) restyle(styles textinput.Styles) { s.input.SetStyles(styles) }
func (s *nickJumpState) setInputWidth(width int)         { s.input.SetWidth(width) }
func (s *nickJumpState) discard(*Model)                  { s.input.Blur() }

func (s *linkState) card(m *Model, width int) string { return m.linkCard(width) }
func (s *linkState) handleKey(m *Model, key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	return m.handleLinkKey(key, msg)
}
func (s *linkState) updateInput(msg tea.Msg) tea.Cmd { return filterInputUpdate(&s.input, msg) }
func (s *linkState) dismiss(m *Model)                { m.closeLink() }
func (s *linkState) pointerDismissable() bool        { return true }
func (s *linkState) overlayLines(m *Model, inner int) []overlayLine {
	return m.linkOverlayLines(inner)
}
func (s *linkState) activateRow(m *Model, index int) {
	s.selected = index
	m.activateLink()
}
func (s *linkState) wheel(m *Model, direction int) {
	wheelList(&s.scroll, len(m.linkMatches()), m.overlayRowBudget(1), direction)
}
func (s *linkState) restyle(styles textinput.Styles) { s.input.SetStyles(styles) }
func (s *linkState) setInputWidth(width int)         { s.input.SetWidth(width) }
func (s *linkState) discard(*Model)                  { s.input.Blur() }

func (s *inboxState) card(m *Model, width int) string { return m.inboxCard(width) }
func (s *inboxState) handleKey(m *Model, key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	return m.handleInboxKey(key, msg)
}

// The inbox has no filter input yet, so paste, restyle, and width are no-ops.
func (s *inboxState) updateInput(tea.Msg) tea.Cmd { return nil }
func (s *inboxState) dismiss(m *Model)            { m.closeInbox() }
func (s *inboxState) pointerDismissable() bool    { return true }
func (s *inboxState) overlayLines(m *Model, inner int) []overlayLine {
	return m.inboxOverlayLines(inner)
}
func (s *inboxState) activateRow(m *Model, index int) {
	s.selected = index
	m.activateInboxSelection()
	m.closeInbox()
}
func (s *inboxState) wheel(m *Model, direction int) {
	wheelList(&s.scroll, len(m.inboxEntries()), m.overlayRowBudget(0), direction)
}
func (s *inboxState) restyle(textinput.Styles) {}
func (s *inboxState) setInputWidth(int)        {}
func (s *inboxState) discard(*Model)           {}

func (s *searchState) card(m *Model, width int) string { return m.searchCard(width) }
func (s *searchState) handleKey(m *Model, key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	return m.handleSearchKey(key, msg)
}

func (s *searchState) updateInput(msg tea.Msg) tea.Cmd { return filterInputUpdate(&s.input, msg) }
func (s *searchState) dismiss(m *Model)                { m.closeSearch() }
func (s *searchState) pointerDismissable() bool        { return true }
func (s *searchState) overlayLines(m *Model, inner int) []overlayLine {
	return m.searchOverlayLines(inner)
}
func (s *searchState) activateRow(m *Model, index int) {
	s.selected = index
	m.activateSearch()
}
func (s *searchState) wheel(m *Model, direction int) {
	wheelList(&s.scroll, len(s.results.Hits), m.overlayRowBudget(searchChromeLines), direction)
}
func (s *searchState) restyle(styles textinput.Styles) { s.input.SetStyles(styles) }
func (s *searchState) setInputWidth(width int)         { s.input.SetWidth(width) }

// discard cancels the running scan when the sheet is closed or replaced.
func (s *searchState) discard(*Model) {
	s.stopSearch()
	s.input.Blur()
}

func (s *channelListState) card(m *Model, width int) string { return m.channelListCard(width) }
func (s *channelListState) handleKey(m *Model, key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	return m.handleChannelListKey(key, msg)
}
func (s *channelListState) updateInput(msg tea.Msg) tea.Cmd {
	return filterInputUpdate(&s.input, msg)
}
func (s *channelListState) dismiss(m *Model)         { m.closeChannelList() }
func (s *channelListState) pointerDismissable() bool { return true }
func (s *channelListState) overlayLines(m *Model, inner int) []overlayLine {
	return m.listOverlayLines(inner)
}
func (s *channelListState) activateRow(m *Model, index int) {
	s.selected = index
	m.activateChannelList()
}
func (s *channelListState) wheel(m *Model, direction int) {
	rows := m.channelListRows()
	if len(rows) > channelListRowCap {
		rows = rows[:channelListRowCap]
	}
	visible := m.overlayRowBudget(1) - channelListTableChrome
	if visible < 1 {
		visible = 1
	}
	wheelList(&s.scroll, len(rows), visible, direction)
}
func (s *channelListState) restyle(styles textinput.Styles) { s.input.SetStyles(styles) }
func (s *channelListState) setInputWidth(width int)         { s.input.SetWidth(width) }

// discard is the one place a replaced or Escaped list drops the controller's
// list request. syncChannelList clears the sheet without it, because there the
// controller has already closed the list.
func (s *channelListState) discard(m *Model) {
	s.input.Blur()
	if m.ctrl != nil {
		m.ctrl.DismissChannelList()
	}
}

func (s *filePickState) card(m *Model, width int) string { return m.fileCard(width) }
func (s *filePickState) handleKey(m *Model, key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	return m.handleFilePickKey(key, msg)
}
func (s *filePickState) updateInput(msg tea.Msg) tea.Cmd { return filterInputUpdate(&s.input, msg) }
func (s *filePickState) dismiss(m *Model)                { m.closeFilePick() }

// The path prompt has no rows and keeps outside clicks, as before.
func (s *filePickState) pointerDismissable() bool               { return false }
func (s *filePickState) overlayLines(*Model, int) []overlayLine { return nil }
func (s *filePickState) activateRow(*Model, int)                {}
func (s *filePickState) wheel(*Model, int)                      {}
func (s *filePickState) restyle(styles textinput.Styles)        { s.input.SetStyles(styles) }
func (s *filePickState) setInputWidth(width int)                { s.input.SetWidth(width) }
func (s *filePickState) discard(*Model)                         { s.input.Blur() }
