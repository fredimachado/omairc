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

// newConnectSheetState builds the sheet's one live text field through the
// shared newTextInput factory, so the focused field renders with the same
// theme-driven input styles as the composer and the filter overlays. It starts
// on the fixed fallback palette because New builds the sheet before a theme is
// wired; syncSheetField re-applies the live palette whenever focus moves.
func newConnectSheetState() connectSheetState {
	return connectSheetState{input: newTextInput(defaultStyles(), "", "")}
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

// isConnectApplyKey reports whether a key press is the window-level Apply
// chord. OmaircWindow.qml binds Ctrl+Return and Ctrl+Enter separately; the
// keystroke string can read ctrl+enter or ctrl+return, and the raw message
// must be checked when a terminal strips the ctrl+ prefix from Enter.
//
// Windows Terminal and ConPTY have no distinct VT encoding for Ctrl+Enter:
// they send LF (0x0a), which ultraviolet decodes as ctrl+j
// (microsoft/terminal#6912). Treat that alias while the Connect sheet is
// open so Apply works from any field on Windows.
func isConnectApplyKey(key string, msg tea.KeyPressMsg) bool {
	switch key {
	case "ctrl+enter", "ctrl+return", "ctrl+j", "ctrl+m":
		return true
	}
	if msg.Mod&tea.ModCtrl == 0 {
		return false
	}
	switch msg.Code {
	case tea.KeyEnter, tea.KeyKpEnter, 'j', 'J', 'm', 'M':
		return true
	}
	return false
}

// applyConnect is Ctrl+Enter: apply the selected network from any tab. It
// dismisses the sheet only when a session was accepted, mirrors the stored
// roster into the sidebar order, and opens that network's Status transcript.
func (m *Model) applyConnect() {
	if !m.conn.Apply() {
		return
	}
	networkID := m.conn.SelectedNetworkID()
	m.syncSidebarNetworkOrder()
	m.closeConnect()
	m.switchSelection(func() {
		m.ctrl.OpenStatus(networkID)
	})
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
	// The field keeps the live palette across a theme change without rebuilding
	// the sheet state, because applyStyles cannot reach into the sheet.
	m.sheet.input.SetStyles(m.styles.Input)
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

// connectCard renders the Connect sheet as one card block through the shared
// overlay frame in model.go.
func (m *Model) connectCard(width int) string {
	return m.overlayCardBlock(width, m.connectCardBody)
}

// connectCardContent is the sheet's rows plus the metadata the viewport needs:
// how many leading rows are the pinned header, how many trailing rows are the
// pinned footer, and the row the focused stop landed on (-1 when none did).
type connectCardContent struct {
	lines      []string
	headerRows int
	footerRows int
	focusRow   int
}

// connectCardBody is the sheet's content, before the shared frame. It windows
// the rows through windowCardRows so a short terminal scrolls the fields between
// the pinned title/tabs and the pinned validation/action rows, keeping the
// focused control reachable instead of clipping the action row off the bottom.
func (m *Model) connectCardBody(inner int) []string {
	content := m.connectCardContent(inner)
	return windowCardRows(content.lines, content.headerRows, content.footerRows,
		content.focusRow, m.overlayCardRowBudget())
}

// connectCardContent builds the sheet's rows and records where the focused stop
// renders, so connectCardBody can scroll it into view. inner is the content
// width inside the border; every line is truncated to inner cells, because the
// shared frame in model.go owns the border and the outer width.
func (m *Model) connectCardContent(inner int) connectCardContent {
	content := connectCardContent{focusRow: -1}
	add := func(line string) {
		content.lines = append(content.lines, truncateLine(line, inner))
	}
	markFocus := func(stop connectStop) {
		if m.stopFocused(stop) {
			content.focusRow = len(content.lines)
		}
	}

	add(m.connectTitleLine())
	add(m.connectTabLine())
	content.headerRows = len(content.lines)

	add(m.sectionHeader("NETWORKS", inner))
	for index, row := range m.conn.Networks() {
		stop := connectStop{kind: stopNetwork, index: index}
		markFocus(stop)
		marker := "  "
		markerStyle := m.styles.SheetField
		if row.Selected {
			marker = "▸ "
			markerStyle = m.styles.SheetRowFocused
		}
		style := m.styles.SheetRow
		if m.stopFocused(stop) {
			style = m.styles.SheetRowFocused.Background(m.styles.Colors.SurfaceRaised)
		}
		line := markerStyle.Render(marker) + style.Render(row.DisplayName)
		if !row.Stored {
			line += " " + m.styles.BadgeMuted.Render("new")
		}
		add(line)
	}
	if m.conn.CanAdd() {
		markFocus(connectStop{kind: stopAddNetwork})
		style := m.styles.SheetField
		if m.stopFocused(connectStop{kind: stopAddNetwork}) {
			style = m.styles.SheetRowFocused.Background(m.styles.Colors.SurfaceRaised)
		}
		add(style.Render("  + Add network"))
	}
	add("")

	if m.sheet.tab == connectTabConnection {
		for index := 0; index < int(connectFieldCount); index++ {
			markFocus(connectStop{kind: stopField, index: index})
			add(m.connectFieldLine(connectField(index), inner))
		}
	} else {
		for index, line := range m.preferencesLines() {
			markFocus(connectStop{kind: stopField, index: index})
			add(line)
		}
	}

	// The persistence and credential sentences render beside the validation
	// problem, muted, on both tabs. IrcConnection carries the same two
	// sentences to the Qt sheet; an empty value renders nothing.
	if status := m.conn.PersistenceStatus(); status != "" {
		add(m.styles.MutedLine.Render(truncateLine(status, inner)))
	}
	if status := m.conn.CredentialStatus(); status != "" {
		add(m.styles.MutedLine.Render(truncateLine(status, inner)))
	}

	// The validation problem is the first pinned footer row, so a scrolled
	// sheet never hides why Apply is unavailable. It is present on both tabs.
	if problem := m.conn.Problem(); problem != "" {
		add(m.styles.StatusErr.Render(truncateLine("✗ "+problem, inner)))
	} else {
		add("")
	}
	content.footerRows = 1

	if m.sheet.tab == connectTabConnection {
		if m.footerFocused() {
			content.focusRow = len(content.lines)
		}
		add(truncateLine(m.connectFooterLine(), inner))
		content.footerRows = 2
	}
	return content
}

// footerFocused reports whether any footer action holds the sheet focus, so the
// viewport can treat the action row as the focused one.
func (m *Model) footerFocused() bool {
	for index := range m.footerActions() {
		if m.stopFocused(connectStop{kind: stopFooter, index: index}) {
			return true
		}
	}
	return false
}

// connectTitleLine is the "Connect" heading.
func (m *Model) connectTitleLine() string {
	return m.styles.SheetTitle.Render("Connect")
}

// connectTabLine renders the two sheet tabs as one segmented rail: the active
// tab is accented and underlined, the inactive one stays dim, and both sit on
// the raised surface so the pair reads as a tab strip.
func (m *Model) connectTabLine() string {
	return m.connectTab("Connection", m.sheet.tab == connectTabConnection) + " " +
		m.connectTab("Preferences", m.sheet.tab == connectTabPreferences)
}

// connectTab renders one tab label.
func (m *Model) connectTab(label string, active bool) string {
	style := m.styles.SheetTab
	if active {
		style = m.styles.SheetTabActive
	}
	return style.Background(m.styles.Colors.SurfaceRaised).Render(" " + label + " ")
}

// connectChip renders a toggle as an on/off pill on the raised surface, so the
// state reads as a switch instead of a bracketed checkbox.
func (m *Model) connectChip(on bool) string {
	style := m.styles.SheetButtonMuted
	label := "off"
	if on {
		style = m.styles.SheetToggleOn
		label = "on"
	}
	return style.Background(m.styles.Colors.SurfaceRaised).Render(" " + label + " ")
}

// connectFieldLine renders one Connection-tab field: a label column, then
// either a toggle chip or the bracketed value box.
func (m *Model) connectFieldLine(field connectField, inner int) string {
	stop := connectStop{kind: stopField, index: int(field)}
	focused := m.stopFocused(stop)
	labelStyle := m.styles.SheetField
	if focused {
		labelStyle = m.styles.SheetFieldActive.Foreground(m.styles.Colors.Accent)
	}
	label := labelStyle.Render(fmt.Sprintf("%-21s", connectFieldLabels[field]))
	switch field {
	case fieldTLS:
		return truncateLine(label+" "+m.connectChip(m.conn.TLSEnabled()), inner)
	case fieldConnectOnStartup:
		return truncateLine(label+" "+m.connectChip(m.conn.ConnectOnStartup()), inner)
	}
	boxStyle := m.styles.SheetField
	if focused {
		boxStyle = m.styles.SheetFieldActive.Foreground(m.styles.Colors.Accent)
	}
	box := boxStyle.Render("[" + m.fieldDisplayValue(stop) + "]")
	return truncateLine(label+" "+box, inner)
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
			style = m.styles.SheetFieldActive.Foreground(m.styles.Colors.Accent)
		}
		lines = append(lines, m.connectChip(on[prefField(index)])+" "+style.Render(prefFieldLabels[prefField(index)]))
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

// connectFooterLine renders the footer buttons as raised-surface pills. Apply
// is muted while the draft has a problem, and the focused button is accented
// (or underlined while muted) so the keyboard walk stays visible.
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
		parts = append(parts, style.Background(m.styles.Colors.SurfaceRaised).Render(" "+label+" "))
	}
	return strings.Join(parts, " ")
}
