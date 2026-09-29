package ui

import (
	"fmt"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/theme"
)

// pressCmd folds one key press into the shell and returns the model and the
// command the chord produced (copy emits the clipboard sequence).
func pressCmd(t *testing.T, m *Model, msg tea.KeyPressMsg) (*Model, tea.Cmd) {
	t.Helper()
	updated, cmd := m.Update(msg)
	model, ok := updated.(*Model)
	if !ok {
		t.Fatalf("Update returned %T, want *Model", updated)
	}
	return model, cmd
}

func resizeModel(t *testing.T, m *Model, width, height int) *Model {
	t.Helper()
	updated, _ := m.Update(tea.WindowSizeMsg{Width: width, Height: height})
	return updated.(*Model)
}

func ctrlKey(code rune) tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: code, Mod: tea.ModCtrl}
}

func ctrlShiftKey(code rune) tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: code, Mod: tea.ModCtrl | tea.ModShift}
}

func altKey(code rune) tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: code, Mod: tea.ModAlt}
}

func altShiftArrow(code rune) tea.KeyPressMsg {
	return tea.KeyPressMsg{Code: code, Mod: tea.ModAlt | tea.ModShift}
}

func TestNetworkWalkWraps(t *testing.T) {
	m := seededModel(t)
	// With no header focus, Alt+Left starts from the selected network
	// (omarchy) and moves to oftc, then wraps back to omarchy.
	m = press(t, m, altKey(tea.KeyLeft))
	if got := m.sidebarNetworkFocusID; got != "oftc" {
		t.Fatalf("Alt+Left focus = %q, want oftc", got)
	}
	m = press(t, m, altKey(tea.KeyRight))
	if got := m.sidebarNetworkFocusID; got != "omarchy" {
		t.Fatalf("Alt+Right focus = %q, want omarchy", got)
	}
}

func TestHeaderEnterOpensStatus(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, altKey(tea.KeyLeft)) // focus oftc
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if got := m.View().WindowTitle; got != "irc.example · oak Status" {
		t.Fatalf("Enter on oftc header title = %q, want %q", got, "irc.example · oak Status")
	}
	if m.sidebarNetworkFocusID != "" {
		t.Fatalf("header focus = %q, want cleared after Enter", m.sidebarNetworkFocusID)
	}
}

// TestWalkClearsNetworkFocusSoEnterSends pins the Qt parity of selectConversation
// clearing sidebarNetworkFocusId (src/OmaircWindow.qml). After walking a network
// header (Alt+Left) and then a conversation (Alt+Down), Enter must send in the
// composer. The old bug left the header armed, so Enter ran
// openFocusedNetworkStatus instead: the transcript jumped to Status and the
// typed line stayed in the composer as a draft until a second attempt.
func TestWalkClearsNetworkFocusSoEnterSends(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, altKey(tea.KeyLeft)) // focus a network header
	if m.sidebarNetworkFocusID == "" {
		t.Fatal("Alt+Left must focus a network header")
	}
	m = press(t, m, altKey(tea.KeyDown)) // walk to a conversation
	if m.sidebarNetworkFocusID != "" {
		t.Fatalf("walk left header focus = %q, want cleared", m.sidebarNetworkFocusID)
	}
	if m.ctrl.SelectedTarget() == "" {
		t.Fatal("walk must select a conversation")
	}
	m.composer.SetValue("hello from the walk")
	m.composer.CursorEnd()
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if got := m.composer.Value(); got != "" {
		t.Fatalf("composer after Enter = %q, want cleared", got)
	}
	if m.ctrl.ConsoleOpen() {
		t.Fatal("Enter must send, not open Status")
	}
	messages := m.ctrl.Messages()
	if len(messages) == 0 || messages[len(messages)-1].Body != "hello from the walk" {
		t.Fatalf("transcript did not receive the line: %+v", messages)
	}
}

func TestCollapseFocusedNetworkHidesRows(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, altKey(tea.KeyLeft))  // focus oftc
	m = press(t, m, altKey(tea.KeyRight)) // back to omarchy
	m = press(t, m, altShiftArrow(tea.KeyLeft))
	if !m.ctrl.IsNetworkCollapsed("omarchy") {
		t.Fatal("Alt+Shift+Left must collapse the focused network")
	}
	sidebar := m.sidebarView(sidebarWidth(m.width), m.bodyHeight())
	if strings.Contains(sidebar, "#desktop") {
		t.Fatalf("collapsed omarchy still shows its channel rows:\n%s", sidebar)
	}
	if !strings.Contains(sidebar, "irc.example · fred") {
		t.Fatalf("collapsed omarchy hid its header:\n%s", sidebar)
	}
	m = press(t, m, altShiftArrow(tea.KeyRight))
	if m.ctrl.IsNetworkCollapsed("omarchy") {
		t.Fatal("Alt+Shift+Right must expand the focused network")
	}
}

func TestCollapseAllNetworks(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyLeft, Mod: tea.ModCtrl | tea.ModAlt | tea.ModShift})
	if !m.ctrl.IsNetworkCollapsed("omarchy") || !m.ctrl.IsNetworkCollapsed("oftc") {
		t.Fatal("Ctrl+Alt+Shift+Left must collapse every network")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyRight, Mod: tea.ModCtrl | tea.ModAlt | tea.ModShift})
	if m.ctrl.IsNetworkCollapsed("omarchy") || m.ctrl.IsNetworkCollapsed("oftc") {
		t.Fatal("Ctrl+Alt+Shift+Right must expand every network")
	}
}

func TestMoveFocusedNetwork(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, altKey(tea.KeyLeft))  // focus oftc
	m = press(t, m, altKey(tea.KeyRight)) // back to omarchy
	m = press(t, m, altShiftArrow(tea.KeyDown))
	order := m.ctrl.NetworkOrder()
	if len(order) < 2 || order[0] != "oftc" || order[1] != "omarchy" {
		t.Fatalf("NetworkOrder after move = %v, want [oftc omarchy]", order)
	}
	if m.sidebarNetworkFocusID != "omarchy" {
		t.Fatalf("focus after move = %q, want omarchy", m.sidebarNetworkFocusID)
	}
}

func TestCollapsedWalkSkipsHiddenRows(t *testing.T) {
	m := seededModel(t)
	// The walk without collapse lands on #ricing (the seeded order).
	m = press(t, m, altKey(tea.KeyDown))
	if got := m.View().WindowTitle; got != "#ricing - Omairc" {
		t.Fatalf("walk title = %q, want %q", got, "#ricing - Omairc")
	}
}

func TestServerListCollapseKeepsWalking(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, ctrlShiftKey('s'))
	if m.serverListVisible {
		t.Fatal("Ctrl+Shift+S must collapse the left rail")
	}
	m = press(t, m, altKey(tea.KeyDown))
	if got := m.View().WindowTitle; got != "#ricing - Omairc" {
		t.Fatalf("walk while collapsed title = %q, want %q", got, "#ricing - Omairc")
	}
	m = press(t, m, altKey(tea.KeyLeft))
	if !m.serverListVisible {
		t.Fatal("Alt+Left must restore the left rail")
	}
}

func TestCloseDirectMessageSelectsNeighbor(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, ctrlShiftKey('k'))
	if !m.nickVisible() {
		t.Fatal("Ctrl+Shift+K must open the nick jump on a channel")
	}
	m.nick.input.SetValue("mira")
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if got := m.View().WindowTitle; got != "mira - Omairc" {
		t.Fatalf("after nick jump title = %q, want %q", got, "mira - Omairc")
	}
	m = press(t, m, ctrlKey('w'))
	if got := m.View().WindowTitle; got != "dax - Omairc" {
		t.Fatalf("after Ctrl+W title = %q, want %q", got, "dax - Omairc")
	}
}

func TestNickJumpNoOpOffChannel(t *testing.T) {
	m := seededModel(t)
	// Opening a direct message first makes the selected target a DM.
	m = press(t, m, ctrlShiftKey('k'))
	m.nick.input.SetValue("anna")
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	m = press(t, m, ctrlShiftKey('k'))
	if m.nickVisible() {
		t.Fatal("Ctrl+Shift+K must be a no-op on a direct message")
	}
}

func TestShortcutsSheetTogglesAndGates(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, ctrlKey('/'))
	if !m.shortcutsOpen {
		t.Fatal("Ctrl+/ must open the shortcuts sheet")
	}
	before := m.View().WindowTitle
	m = press(t, m, altKey(tea.KeyDown))
	if got := m.View().WindowTitle; got != before {
		t.Fatalf("Alt+Down under the sheet changed the title to %q, want %q", got, before)
	}
	if !m.shortcutsOpen {
		t.Fatal("a gated chord must not close the shortcuts sheet")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.shortcutsOpen {
		t.Fatal("Escape must close the shortcuts sheet")
	}
}

func TestEscapeClosesSheetBeforeStatus(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, ctrlKey('`'))
	if got := m.View().WindowTitle; !strings.HasSuffix(got, "Status") {
		t.Fatalf("Ctrl+` title = %q, want a Status title", got)
	}
	m = press(t, m, ctrlKey('/'))
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.shortcutsOpen {
		t.Fatal("Escape must close the sheet first")
	}
	if got := m.View().WindowTitle; !strings.HasSuffix(got, "Status") {
		t.Fatalf("Status must survive the sheet's Escape, title = %q", got)
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if got := m.View().WindowTitle; strings.HasSuffix(got, "Status") {
		t.Fatalf("second Escape title = %q, want Status closed", got)
	}
}

func TestFindAdvancesWrapsAndRestoresDraft(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("mira")
	matches := m.findRowsForQuery("mira")
	if len(matches) < 2 {
		t.Fatalf("expected at least two rows containing mira, got %d", len(matches))
	}
	m = press(t, m, ctrlKey('f'))
	if !m.find.active {
		t.Fatal("Ctrl+F must enter find")
	}
	if m.find.index != matches[0] {
		t.Fatalf("find index = %d, want the first match %d", m.find.index, matches[0])
	}
	for step := 0; step < len(matches); step++ {
		m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	}
	if m.find.index != matches[0] {
		t.Fatalf("find index after wrapping = %d, want %d", m.find.index, matches[0])
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.find.active {
		t.Fatal("Escape must leave find")
	}
	if got := m.composer.Value(); got != "mira" {
		t.Fatalf("Escape restored composer = %q, want %q", got, "mira")
	}
}

func TestFindRestoresTypedDraft(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("zzz-no-match")
	m = press(t, m, ctrlKey('f'))
	if !m.find.active {
		t.Fatal("Ctrl+F must enter find")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if got := m.composer.Value(); got != "zzz-no-match" {
		t.Fatalf("find restored composer = %q, want %q", got, "zzz-no-match")
	}
}

func TestTabCompletesNick(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("mi")
	m.composer.CursorEnd()
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})
	if got := m.composer.Value(); got != "mira: " {
		t.Fatalf("Tab completion = %q, want %q", got, "mira: ")
	}
}

// TestTabCompletesAmbiguousNickAndCycles pins the Qt contract: a prefix with
// more than one match still completes (to the alphabetically first nick) and
// repeated Tabs cycle through the candidates. The old "exactly one candidate"
// rule left Tab dead in #ricing, where "s" matches both sam and sol.
func TestTabCompletesAmbiguousNickAndCycles(t *testing.T) {
	m := seededModel(t)
	m.ctrl.SelectConversation("omarchy", "#ricing")
	if got := m.ctrl.SelectedTarget(); got != "#ricing" {
		t.Fatalf("selected target = %q, want #ricing", got)
	}
	m.composer.SetValue("s")
	m.composer.CursorEnd()

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})
	if got := m.composer.Value(); got != "sam: " {
		t.Fatalf("first Tab = %q, want %q", got, "sam: ")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})
	if got := m.composer.Value(); got != "sol: " {
		t.Fatalf("second Tab = %q, want %q", got, "sol: ")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})
	if got := m.composer.Value(); got != "sam: " {
		t.Fatalf("third Tab (wrap) = %q, want %q", got, "sam: ")
	}
}

// TestTabCompletionResetsOnOtherKeys proves a keystroke other than Tab ends the
// session, so a later Tab starts a fresh completion instead of cycling stale
// candidates.
func TestTabCompletionResetsOnOtherKeys(t *testing.T) {
	m := seededModel(t)
	m.ctrl.SelectConversation("omarchy", "#ricing")
	m.composer.SetValue("s")
	m.composer.CursorEnd()
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})
	if got := m.composer.Value(); got != "sam: " {
		t.Fatalf("first Tab = %q, want %q", got, "sam: ")
	}
	// Backspace edits the line and must end the session.
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyBackspace})
	if got := m.composer.Value(); got != "sam:" {
		t.Fatalf("after backspace = %q, want %q", got, "sam:")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyTab})
	if got := m.composer.Value(); got != "sam:" {
		t.Fatalf("Tab after edit = %q, want unchanged %q", got, "sam:")
	}
}

func TestComposerHistoryRecallsAndRestores(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("hello there")
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if got := m.composer.Value(); got != "" {
		t.Fatalf("composer after send = %q, want empty", got)
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyUp})
	if got := m.composer.Value(); got != "hello there" {
		t.Fatalf("Up recalled %q, want %q", got, "hello there")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyDown})
	if got := m.composer.Value(); got != "" {
		t.Fatalf("Down restored %q, want the empty draft", got)
	}
}

func TestMemberFocusOpensDirectMessage(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, ctrlShiftKey('p'))
	if !m.memberFocus {
		t.Fatal("Ctrl+Shift+P must focus the member list")
	}
	index := -1
	for i, member := range m.ctrl.Members() {
		if member.Nick == "dax" {
			index = i
			break
		}
	}
	if index < 0 {
		t.Fatal("dax not found in the member list")
	}
	m.memberIndex = index
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if got := m.View().WindowTitle; got != "dax - Omairc" {
		t.Fatalf("Enter on focused member title = %q, want %q", got, "dax - Omairc")
	}
}

func TestTranscriptPagingOffsets(t *testing.T) {
	m := seededModel(t)
	m = resizeModel(t, m, 118, 6)
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyHome, Mod: tea.ModCtrl})
	if m.transcriptFollowEnd {
		t.Fatal("Ctrl+Home must leave follow-the-end")
	}
	if m.transcriptScroll <= 0 {
		t.Fatalf("Ctrl+Home scroll = %d, want > 0", m.transcriptScroll)
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnd, Mod: tea.ModCtrl})
	if !m.transcriptFollowEnd {
		t.Fatal("Ctrl+End must restore follow-the-end")
	}
}

func TestCopyEmitsClipboard(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("mira")
	m = press(t, m, ctrlKey('f'))
	if m.find.index < 0 {
		t.Fatal("find must land on a mira row")
	}
	m, cmd := pressCmd(t, m, ctrlKey('c'))
	if cmd == nil {
		t.Fatal("Ctrl+C must produce a clipboard command")
	}
	if got := fmt.Sprint(cmd()); !strings.Contains(got, "mira") {
		t.Fatalf("clipboard text = %q, want it to contain mira", got)
	}
}

func TestModalRuleUnderConnect(t *testing.T) {
	ctrl := controller.New()
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	conn := connection.New(ctrl, nil)
	m := New(ctrl, conn)
	m = resizeModel(t, m, 118, 30)
	if !m.connectVisible() {
		t.Fatal("first-run Connect must be visible")
	}
	selection := ctrl.SelectedConversationID()
	m = press(t, m, altKey(tea.KeyLeft))
	if m.sidebarNetworkFocusID != "" {
		t.Fatalf("Alt+Left under Connect set focus %q, want none", m.sidebarNetworkFocusID)
	}
	m = press(t, m, ctrlShiftKey('s'))
	if !m.serverListVisible {
		t.Fatal("Ctrl+Shift+S must be gated under Connect")
	}
	if ctrl.SelectedConversationID() != selection {
		t.Fatal("a gated chord must not change the selected conversation")
	}
	// Ctrl+/ still opens the sheet on top of Connect.
	m = press(t, m, ctrlKey('/'))
	if !m.shortcutsOpen {
		t.Fatal("Ctrl+/ must open the shortcuts sheet on top of Connect")
	}
}

// findRowsForQuery is a test helper that reproduces the find haystack so a test
// can count matches without opening the overlay.
func (m *Model) findRowsForQuery(query string) []int {
	rows := m.transcriptRowTexts()
	var matches []int
	for index, row := range rows {
		if strings.Contains(strings.ToLower(row), strings.ToLower(query)) {
			matches = append(matches, index)
		}
	}
	return matches
}

func TestModalBlocksChord(t *testing.T) {
	m := seededModel(t)
	if m.modalBlocksChord("alt+down") {
		t.Fatal("no modal is open; alt+down must not be blocked")
	}
	m = press(t, m, ctrlKey('/'))
	for _, key := range []string{"alt+down", "alt+left", "ctrl+w", "ctrl+shift+s", "ctrl+f", "pgup"} {
		if !m.modalBlocksChord(key) {
			t.Fatalf("shortcuts sheet must block %q", key)
		}
	}
	for _, key := range []string{"ctrl+q", "ctrl+/", "esc"} {
		if m.modalBlocksChord(key) {
			t.Fatalf("shortcuts sheet must let %q through", key)
		}
	}
}

// TestSideColumnOuterWidthsAreInvariant pins the F2 card contract: framing the
// sidebar and member column must not change their outer sizes, because
// transcriptWidth() and the width tests are built on those constants.
func TestSideColumnOuterWidthsAreInvariant(t *testing.T) {
	m := seededModel(t)
	height := m.bodyHeight()

	sidebar := m.framedColumn(m.sidebarView, sidebarWidth(m.width), height, false)
	if got := lipgloss.Width(sidebar); got != sidebarWidth(m.width) {
		t.Fatalf("framed sidebar width = %d, want %d", got, sidebarWidth(m.width))
	}
	if got := lipgloss.Height(sidebar); got != height {
		t.Fatalf("framed sidebar height = %d, want %d", got, height)
	}

	members := m.framedColumn(m.membersView, membersWidth, height, false)
	if got := lipgloss.Width(members); got != membersWidth {
		t.Fatalf("framed members width = %d, want %d", got, membersWidth)
	}
	// The transcript stays unframed, so its content width is unchanged by the
	// surrounding cards.
	if got := lipgloss.Width(m.transcriptView(m.transcriptHeight())); got != m.transcriptWidth() {
		t.Fatalf("transcript width = %d, want %d", got, m.transcriptWidth())
	}
}

// TestFooterUsesHelpKeyMap checks the footer's chrome: it costs one row, its
// left side reports the focused network, and its right side is the contextual
// help ending with the Ctrl+/ shortcuts binding.
func TestFooterUsesHelpKeyMap(t *testing.T) {
	m := seededModel(t)
	if !m.footerVisible() {
		t.Fatal("footer must be visible at the default size")
	}
	if got, want := m.bodyHeight(), m.height-footerHeight; got != want {
		t.Fatalf("bodyHeight = %d, want %d with the footer and no slash rows", got, want)
	}
	view := m.View().Content
	if !strings.Contains(view, "Connected") {
		t.Fatalf("footer must report the connection state:\n%s", view)
	}
	if !strings.Contains(view, "Ctrl+/") {
		t.Fatalf("footer help must end with the Ctrl+/ binding:\n%s", view)
	}
}

// TestThemeChangedRebuildsStyles covers the live-theme hop: a ThemeChangedMsg
// swaps the palette, and View paints the new background and foreground.
func TestThemeChangedRebuildsStyles(t *testing.T) {
	m := seededModel(t)
	colors := theme.Derive(theme.Spec{Mode: theme.ModeLight})
	if theme.Hex(colors.Background) == theme.Hex(m.styles.Colors.Background) {
		t.Fatal("light palette unexpectedly matches the fallback background")
	}
	updated, cmd := m.Update(ThemeChangedMsg{Colors: colors})
	m = updated.(*Model)
	if cmd != nil {
		t.Fatalf("ThemeChangedMsg cmd = %v, want nil without a watcher", cmd)
	}
	if got := theme.Hex(m.styles.Colors.Background); got != theme.Hex(colors.Background) {
		t.Fatalf("background = %s, want %s", got, theme.Hex(colors.Background))
	}
	view := m.View()
	if view.BackgroundColor != colors.Background {
		t.Fatalf("View.BackgroundColor did not follow the theme")
	}
}
