package ui

import (
	"fmt"
	"strconv"
	"strings"

	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
)

// This file is the Connect sheet: the first-run overlay, the Connection and
// Preferences tabs, and the keyboard focus walk. It mirrors
// src/qml/ConnectionSheet.qml as a file-based component and reads the
// internal/connection model, never internal/irc.

// connectTab names the two sheet tabs.
type connectTab int

const (
	connectTabConnection connectTab = iota
	connectTabPreferences
)

// connectField indexes the Connection tab's form stops.
type connectField int

const (
	fieldName connectField = iota
	fieldHost
	fieldPort
	fieldTLS
	fieldNick
	fieldUsername
	fieldRealname
	fieldAutojoin
	fieldConnectOnStartup
	fieldServerPassword
	fieldNickServPassword
	connectFieldCount
)

var connectFieldLabels = [connectFieldCount]string{
	fieldName:             "Name",
	fieldHost:             "Host",
	fieldPort:             "Port",
	fieldTLS:              "TLS",
	fieldNick:             "Nick",
	fieldUsername:         "Username",
	fieldRealname:         "Real name",
	fieldAutojoin:         "Autojoin",
	fieldConnectOnStartup: "Connect automatically",
	fieldServerPassword:   "Server password",
	fieldNickServPassword: "NickServ password",
}

// prefField indexes the Preferences tab's toggles.
type prefField int

const (
	prefReopen prefField = iota
	prefAvatars
	prefUnread
	prefFieldCount
)

var prefFieldLabels = [prefFieldCount]string{
	prefReopen:  "Reopen direct messages on startup",
	prefAvatars: "Show peer avatars",
	prefUnread:  "Open conversations at unread",
}

// connectStopKind names what a focus stop points at.
type connectStopKind int

const (
	stopNetwork connectStopKind = iota
	stopAddNetwork
	stopField
	stopFooter
)

// connectStop is one stop in the sheet's ordered focus walk: a network rail
// row, the Add control, a form field, or a footer button.
type connectStop struct {
	kind  connectStopKind
	index int
}

// footerAction indexes the Connection tab's footer buttons.
type footerAction int

const (
	footerRemove footerAction = iota
	footerDiscard
	footerDisconnect
	footerApply
)

var footerLabels = map[footerAction]string{
	footerRemove:     "Remove",
	footerDiscard:    "Discard",
	footerDisconnect: "Disconnect",
	footerApply:      "Apply",
}

// connectSheetState is the sheet's focus, tab, and live text field.
type connectSheetState struct {
	tab         connectTab
	focus       int
	input       textinput.Model
	armedRemove bool
}

func newConnectSheetState() connectSheetState {
	input := textinput.New()
	input.Prompt = ""
	return connectSheetState{input: input}
}

// connectVisible reports whether the sheet is on top: first run cannot dismiss
// it, and Ctrl+, reopens it after a profile exists.
func (m *Model) connectVisible() bool {
	if m == nil || m.conn == nil {
		return false
	}
	return m.conn.SetupRequired() || m.connectOpen
}

// openConnect reopens the sheet for the focused network.
func (m *Model) openConnect() {
	if m.conn == nil {
		return
	}
	if networkID := m.ctrl.FocusedNetworkID(); networkID != "" {
		m.conn.Select(networkID)
	}
	m.connectOpen = true
	m.sheet.tab = connectTabConnection
	m.sheet.focus = 0
	m.sheet.armedRemove = false
	m.composer.Blur()
	m.syncSheetField()
}

// closeConnect dismisses the sheet and returns focus to the composer.
func (m *Model) closeConnect() {
	m.connectOpen = false
	m.sheet.armedRemove = false
	m.sheet.input.Blur()
	_ = m.composer.Focus()
}

// connectStops builds the ordered focus walk for the current tab: the network
// rail, Add network, the form fields, then the footer buttons.
func (m *Model) connectStops() []connectStop {
	if m.conn == nil {
		return nil
	}
	var stops []connectStop
	for index := range m.conn.Networks() {
		stops = append(stops, connectStop{kind: stopNetwork, index: index})
	}
	if m.conn.CanAdd() {
		stops = append(stops, connectStop{kind: stopAddNetwork})
	}
	switch m.sheet.tab {
	case connectTabConnection:
		for index := 0; index < int(connectFieldCount); index++ {
			stops = append(stops, connectStop{kind: stopField, index: index})
		}
		for index := range m.footerActions() {
			stops = append(stops, connectStop{kind: stopFooter, index: index})
		}
	case connectTabPreferences:
		for index := 0; index < int(prefFieldCount); index++ {
			stops = append(stops, connectStop{kind: stopField, index: index})
		}
	}
	return stops
}

// footerActions returns the footer buttons the draft currently offers.
func (m *Model) footerActions() []footerAction {
	actions := make([]footerAction, 0, 4)
	if m.conn.CanRemove() {
		actions = append(actions, footerRemove)
	}
	actions = append(actions, footerDiscard)
	if m.conn.CanDisconnect() {
		actions = append(actions, footerDisconnect)
	}
	actions = append(actions, footerApply)
	return actions
}

// clampSheetFocus keeps the focus index inside the current stop list as the
// list shrinks or grows.
func (m *Model) clampSheetFocus() {
	stops := m.connectStops()
	if len(stops) == 0 {
		m.sheet.focus = 0
		return
	}
	if m.sheet.focus < 0 {
		m.sheet.focus = 0
	}
	if m.sheet.focus >= len(stops) {
		m.sheet.focus = len(stops) - 1
	}
}

func (m *Model) focusedStop() (connectStop, bool) {
	stops := m.connectStops()
	if len(stops) == 0 {
		return connectStop{}, false
	}
	index := m.sheet.focus
	if index < 0 || index >= len(stops) {
		index = 0
	}
	return stops[index], true
}

func (m *Model) stopFocused(stop connectStop) bool {
	focused, ok := m.focusedStop()
	return ok && focused == stop
}

// stepSheet commits the live field, moves the focus by delta (wrapping), and
// loads the next field.
func (m *Model) stepSheet(delta int) {
	stops := m.connectStops()
	if len(stops) == 0 {
		return
	}
	m.commitSheetField()
	index := m.sheet.focus + delta
	index = ((index % len(stops)) + len(stops)) % len(stops)
	m.sheet.focus = index
	m.syncSheetField()
}

// stepSheetNetwork walks the network rail from anywhere in the sheet
// (Alt+Left / Alt+Right) and puts focus on the newly selected row.
func (m *Model) stepSheetNetwork(delta int) {
	rows := m.conn.Networks()
	if len(rows) == 0 {
		return
	}
	current := 0
	for index, row := range rows {
		if row.Selected {
			current = index
			break
		}
	}
	if delta != 0 {
		current = ((current+delta)%len(rows) + len(rows)) % len(rows)
	}
	m.conn.Select(rows[current].NetworkID)
	stops := m.connectStops()
	for index, stop := range stops {
		if stop.kind == stopNetwork && stop.index == current {
			m.sheet.focus = index
			break
		}
	}
	m.syncSheetField()
}

// toggleConnectTab switches between Connection and Preferences.
func (m *Model) toggleConnectTab() {
	if m.sheet.tab == connectTabConnection {
		m.sheet.tab = connectTabPreferences
	} else {
		m.sheet.tab = connectTabConnection
	}
	m.sheet.focus = 0
	m.syncSheetField()
}

// activateConnectStop is Enter: rail rows select, Add network adds, footer
// buttons activate, and fields walk to the next stop.
func (m *Model) activateConnectStop() {
	stop, ok := m.focusedStop()
	if !ok {
		return
	}
	switch stop.kind {
	case stopNetwork:
		rows := m.conn.Networks()
		if stop.index >= 0 && stop.index < len(rows) {
			m.conn.Select(rows[stop.index].NetworkID)
		}
		m.stepSheet(1)
	case stopAddNetwork:
		if m.conn.Add() {
			m.sheet.focus = 0
		}
		m.syncSheetField()
	case stopField:
		m.stepSheet(1)
	case stopFooter:
		m.activateFooter(stop.index)
	}
}

// activateFooter runs one footer button.
func (m *Model) activateFooter(index int) {
	actions := m.footerActions()
	if index < 0 || index >= len(actions) {
		return
	}
	switch actions[index] {
	case footerRemove:
		m.conn.RemoveSelected()
	case footerDiscard:
		m.conn.Discard()
	case footerDisconnect:
		m.conn.DisconnectSelected()
	case footerApply:
		m.applyConnect()
		return
	}
	m.clampSheetFocus()
	m.syncSheetField()
}

// applyConnect is Ctrl+Enter: apply the selected network from any tab. It
// dismisses the sheet only when a session was accepted.
func (m *Model) applyConnect() {
	if m.conn.Apply() {
		m.closeConnect()
	}
}

// armRemoveSelected is Ctrl+Shift+Delete: the first press arms, the second
// removes, matching the sheet's two-step remove.
func (m *Model) armRemoveSelected() {
	if !m.conn.CanRemove() {
		return
	}
	if !m.sheet.armedRemove {
		m.sheet.armedRemove = true
		return
	}
	m.sheet.armedRemove = false
	m.conn.RemoveSelected()
	m.clampSheetFocus()
	m.syncSheetField()
}

// stopIsText reports whether a field takes typed text, and whether it is a
// secret that renders masked.
func (m *Model) stopIsText(stop connectStop) (isText, isSecret bool) {
	if stop.kind != stopField || m.sheet.tab != connectTabConnection {
		return false, false
	}
	switch connectField(stop.index) {
	case fieldName, fieldHost, fieldPort, fieldNick, fieldUsername, fieldRealname, fieldAutojoin:
		return true, false
	case fieldServerPassword, fieldNickServPassword:
		return true, true
	}
	return false, false
}

// stopIsToggle reports whether a field flips on Space.
func (m *Model) stopIsToggle(stop connectStop) bool {
	if stop.kind != stopField {
		return false
	}
	if m.sheet.tab == connectTabPreferences {
		return true
	}
	switch connectField(stop.index) {
	case fieldTLS, fieldConnectOnStartup:
		return true
	}
	return false
}

// stopText is the committed value a text field shows when it is not focused.
// A secret is never read back, so it returns "".
func (m *Model) stopText(stop connectStop) string {
	switch connectField(stop.index) {
	case fieldName:
		return m.conn.Name()
	case fieldHost:
		return m.conn.Host()
	case fieldPort:
		return strconv.Itoa(m.conn.Port())
	case fieldNick:
		return m.conn.Nick()
	case fieldUsername:
		return m.conn.Username()
	case fieldRealname:
		return m.conn.Realname()
	case fieldAutojoin:
		return m.conn.Autojoin()
	}
	return ""
}

// syncSheetField loads the focused text field into the live input (or blurs it
// for toggles and buttons).
func (m *Model) syncSheetField() {
	stop, ok := m.focusedStop()
	if !ok {
		m.sheet.input.Blur()
		return
	}
	isText, isSecret := m.stopIsText(stop)
	if !isText {
		m.sheet.input.Blur()
		return
	}
	m.sheet.input.SetWidth(m.overlayInputWidth())
	m.sheet.input.Placeholder = ""
	m.sheet.input.EchoMode = textinput.EchoNormal
	if isSecret {
		m.sheet.input.EchoMode = textinput.EchoPassword
	}
	m.sheet.input.SetValue(m.stopText(stop))
	_ = m.sheet.input.Focus()
	m.sheet.input.CursorEnd()
}

// commitSheetField writes the live input back into the draft.
func (m *Model) commitSheetField() {
	stop, ok := m.focusedStop()
	if !ok {
		return
	}
	isText, _ := m.stopIsText(stop)
	if !isText {
		return
	}
	value := m.sheet.input.Value()
	switch connectField(stop.index) {
	case fieldName:
		m.conn.SetName(value)
	case fieldHost:
		m.conn.SetHost(value)
	case fieldPort:
		port, err := strconv.Atoi(strings.TrimSpace(value))
		if err != nil {
			port = 0
		}
		m.conn.SetPort(port)
	case fieldNick:
		m.conn.SetNick(value)
	case fieldUsername:
		m.conn.SetUsername(value)
	case fieldRealname:
		m.conn.SetRealname(value)
	case fieldAutojoin:
		m.conn.SetAutojoin(value)
	case fieldServerPassword:
		m.conn.SetPassword(value)
	case fieldNickServPassword:
		m.conn.SetNickServPassword(value)
	}
}

// flipFocusedToggle flips the focused toggle. It reports whether it consumed
// the key.
func (m *Model) flipFocusedToggle() bool {
	stop, ok := m.focusedStop()
	if !ok || !m.stopIsToggle(stop) {
		return false
	}
	if m.sheet.tab == connectTabPreferences {
		switch prefField(stop.index) {
		case prefReopen:
			m.conn.SetReopenDirects(!m.conn.ReopenDirects())
		case prefAvatars:
			m.conn.SetShowAvatars(!m.conn.ShowAvatars())
		case prefUnread:
			m.conn.SetOpenAtUnread(!m.conn.OpenAtUnread())
		}
		return true
	}
	switch connectField(stop.index) {
	case fieldTLS:
		m.conn.SetTLSEnabled(!m.conn.TLSEnabled())
	case fieldConnectOnStartup:
		m.conn.SetConnectOnStartup(!m.conn.ConnectOnStartup())
	}
	return true
}

// handleConnectKey folds one key while the sheet is on top. The sheet is modal:
// no other chord reaches the columns behind it.
func (m *Model) handleConnectKey(key string, msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	switch key {
	case "esc", "escape":
		// First run cannot be dismissed; a stored profile lets Escape close.
		if !m.conn.SetupRequired() {
			m.closeConnect()
		}
		return m, nil
	case "tab":
		m.stepSheet(1)
		return m, nil
	case "shift+tab":
		m.stepSheet(-1)
		return m, nil
	case "enter":
		m.activateConnectStop()
		return m, nil
	case "ctrl+enter":
		m.applyConnect()
		return m, nil
	case "ctrl+tab":
		m.toggleConnectTab()
		return m, nil
	case "alt+left":
		m.stepSheetNetwork(-1)
		return m, nil
	case "alt+right":
		m.stepSheetNetwork(1)
		return m, nil
	case "ctrl+n":
		if m.conn.Add() {
			m.sheet.focus = 0
		}
		m.syncSheetField()
		return m, nil
	case "ctrl+shift+delete":
		m.armRemoveSelected()
		return m, nil
	case "space":
		if m.flipFocusedToggle() {
			return m, nil
		}
	}
	stop, ok := m.focusedStop()
	if ok {
		isText, _ := m.stopIsText(stop)
		if isText {
			var cmd tea.Cmd
			m.sheet.input, cmd = m.sheet.input.Update(msg)
			m.commitSheetField()
			return m, cmd
		}
	}
	return m, nil
}

// --- Rendering ------------------------------------------------------------

// connectCardLines renders the sheet into a bordered block exactly width cells
// wide.
func (m *Model) connectCardLines(width int) []string {
	inner := width - 4
	if inner < 8 {
		inner = 8
	}
	lines := m.connectCard(inner)
	rendered := m.styles.SheetCard.Width(inner).Render(strings.Join(lines, "\n"))
	return strings.Split(rendered, "\n")
}

// connectCard is the sheet's content, before the border.
func (m *Model) connectCard(inner int) []string {
	lines := []string{m.connectTitleLine()}
	lines = append(lines, m.styles.SheetLabel.Render("NETWORKS"))

	for index, row := range m.conn.Networks() {
		stop := connectStop{kind: stopNetwork, index: index}
		marker := "  "
		if row.Selected {
			marker = "▸ "
		}
		suffix := ""
		if !row.Stored {
			suffix = "  (new)"
		}
		style := m.styles.SheetRow
		if m.stopFocused(stop) {
			style = m.styles.SheetRowFocused
		}
		lines = append(lines, style.Render(truncateLine(marker+row.DisplayName+suffix, inner)))
	}
	if m.conn.CanAdd() {
		style := m.styles.SheetRow
		if m.stopFocused(connectStop{kind: stopAddNetwork}) {
			style = m.styles.SheetRowFocused
		}
		lines = append(lines, style.Render("  + Add network"))
	}
	lines = append(lines, "")

	if m.sheet.tab == connectTabConnection {
		for index := 0; index < int(connectFieldCount); index++ {
			lines = append(lines, m.connectFieldLine(connectField(index), inner))
		}
	} else {
		lines = append(lines, m.preferencesLines()...)
	}

	if problem := m.conn.Problem(); problem != "" {
		lines = append(lines, m.styles.SheetProblem.Render(truncateLine(problem, inner)))
	} else {
		lines = append(lines, "")
	}

	if m.sheet.tab == connectTabConnection {
		lines = append(lines, m.connectFooterLine())
	}
	return lines
}

// connectTitleLine is the "Connect" heading plus the two tab labels.
func (m *Model) connectTitleLine() string {
	connectionTab := m.styles.SheetTab.Render("Connection")
	preferencesTab := m.styles.SheetTab.Render("Preferences")
	if m.sheet.tab == connectTabConnection {
		connectionTab = m.styles.SheetTabActive.Render("Connection")
	} else {
		preferencesTab = m.styles.SheetTabActive.Render("Preferences")
	}
	return m.styles.SheetTitle.Render("Connect") + "    " + connectionTab + "  " + preferencesTab
}

// connectFieldLine renders one Connection-tab field.
func (m *Model) connectFieldLine(field connectField, inner int) string {
	stop := connectStop{kind: stopField, index: int(field)}
	label := fmt.Sprintf("%-17s ", connectFieldLabels[field])
	style := m.styles.SheetField
	if m.stopFocused(stop) {
		style = m.styles.SheetFieldActive
	}
	switch field {
	case fieldTLS:
		return style.Render(label) + toggleMark(m.conn.TLSEnabled())
	case fieldConnectOnStartup:
		return style.Render(label) + toggleMark(m.conn.ConnectOnStartup())
	}
	value := m.fieldDisplayValue(stop)
	line := label + "[" + value + "]"
	return style.Render(truncateLine(line, inner))
}

// preferencesLines renders the Preferences-tab toggles.
func (m *Model) preferencesLines() []string {
	on := [prefFieldCount]bool{
		prefReopen:  m.conn.ReopenDirects(),
		prefAvatars: m.conn.ShowAvatars(),
		prefUnread:  m.conn.OpenAtUnread(),
	}
	lines := make([]string, 0, prefFieldCount)
	for index := 0; index < int(prefFieldCount); index++ {
		stop := connectStop{kind: stopField, index: index}
		style := m.styles.SheetField
		if m.stopFocused(stop) {
			style = m.styles.SheetFieldActive
		}
		lines = append(lines, style.Render(toggleMark(on[prefField(index)])+" "+prefFieldLabels[prefField(index)]))
	}
	return lines
}

// fieldDisplayValue is the live value for a text field: the input while it is
// focused, else the committed draft. A secret renders as a mask.
func (m *Model) fieldDisplayValue(stop connectStop) string {
	_, isSecret := m.stopIsText(stop)
	focused := m.stopFocused(stop)
	if isSecret {
		set := m.secretSet(connectField(stop.index))
		if focused {
			set = m.sheet.input.Value() != ""
		}
		if set {
			return "••••••"
		}
		return ""
	}
	if focused {
		return m.sheet.input.Value()
	}
	return m.stopText(stop)
}

func (m *Model) secretSet(field connectField) bool {
	switch field {
	case fieldServerPassword:
		return m.conn.PasswordSet()
	case fieldNickServPassword:
		return m.conn.NickServSet()
	}
	return false
}

// connectFooterLine renders the footer buttons. Apply is muted while the draft
// has a problem.
func (m *Model) connectFooterLine() string {
	actions := m.footerActions()
	parts := make([]string, 0, len(actions))
	for index, action := range actions {
		label := footerLabels[action]
		style := m.styles.SheetButton
		muted := action == footerApply && m.conn.Problem() != ""
		if muted {
			style = m.styles.SheetButtonMuted
		}
		if m.stopFocused(connectStop{kind: stopFooter, index: index}) {
			if muted {
				style = m.styles.SheetButtonMuted.Underline(true)
			} else {
				style = m.styles.SheetButtonFocus
			}
		}
		parts = append(parts, style.Render("["+label+"]"))
	}
	return strings.Join(parts, " ")
}

func toggleMark(on bool) string {
	if on {
		return "[x]"
	}
	return "[ ]"
}
