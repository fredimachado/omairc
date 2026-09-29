package ui

import (
	"strings"

	"charm.land/bubbles/v2/help"
	"charm.land/bubbles/v2/spinner"
	"charm.land/bubbles/v2/textinput"
	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/notify"
	"github.com/fredimachado/omairc/tui/internal/theme"
)

// Layout constants. The column widths are proportional with clamps, so a
// narrow window shrinks the transcript before it drops a column.
const (
	defaultWidth  = 80
	defaultHeight = 24
	minWidth      = 24
	minHeight     = 5

	sidebarMinWidth = 16
	sidebarMaxWidth = 30
	membersWidth    = 22
	membersMinTotal = 72
)

// NotifyMsg tells the shell a controller callback fired and it should
// re-render. The main program sends it with p.Send(ui.NotifyMsg{}); Update
// treats it as a no-op that leaves the snapshots already rebuilt.
type NotifyMsg struct{}

// StartupMsg is delivered once after the program starts, so the shell can
// reconcile stored profiles that connect on startup.
type StartupMsg struct{}

// MentionArrivalMsg carries one reducer mention arrival to the shell. It is
// the session-goroutine-to-Bubble-Tea hop that mirrors the QML
// onMentionArrived handler: the loop handles it on the shell goroutine, where
// the notifier and the composer live.
type MentionArrivalMsg struct{ Author, Body, NetworkID, Target, MsgID string }

// MonitorArrivalMsg carries one MONITOR presence change to the shell. Target
// and MsgID stay empty, mirroring the QML onMonitorArrived handler.
type MonitorArrivalMsg struct{ NetworkID, Author, Body string }

// NotificationActivatedMsg is a notification activation (body click or Open),
// mirroring Backend's notificationActivated signal.
type NotificationActivatedMsg = notify.Activation

// AvatarReadyMsg tells the shell a background avatar fetch cached a new image
// and it should re-render.
type AvatarReadyMsg struct{}

// ThemeChangedMsg carries a new palette read from the theme watcher. It takes
// the same program hop as AvatarReadyMsg: the watcher read is a tea.Cmd whose
// message the runtime delivers to Update, which rebuilds the styles and
// re-arms the read for the next change.
type ThemeChangedMsg struct{ Colors theme.Colors }

// AvatarSource is the nil-able avatar seam. The shell asks it to schedule a
// peer image fetch and, once cached, to rasterize it into a half-block glyph.
// The real implementation is internal/avatar; the interface keeps the network
// path out of the view package.
type AvatarSource interface {
	Ensure(rawURL string, layoutPixels int)
	Block(rawURL string, layoutPixels, cols int) (string, bool)
}

// Model is the Bubble Tea shell state over one controller and the Connect
// sheet's connection model. conn is nil in the seeded demo, which has no
// profile to edit.
type Model struct {
	ctrl    *controller.Controller
	conn    *connection.Connection
	avatars AvatarSource
	styles  Styles

	// onStartup runs once when StartupMsg arrives. It fires from the Update
	// goroutine so controller mutation stays single-threaded. Nil-able.
	onStartup func()

	width  int
	height int
	focus  focusArea

	composer textinput.Model

	// Footer chrome. help renders the contextual chord hint from footerKeyMap;
	// spinner animates while the focused network is disconnected. spinning
	// latches the single tick chain so repeated starts cannot multiply frames.
	help     help.Model
	spinner  spinner.Model
	spinning bool

	// Theme wiring. themeWatcher streams live palette changes; themeArmed
	// records whether the read loop has been started (once, on the first
	// WindowSizeMsg, because Init is reserved for StartupMsg).
	themeWatcher *theme.Watcher
	themeArmed   bool

	// connectOpen reopens the sheet after a profile exists. On first run the
	// sheet is visible because the connection still requires setup.
	connectOpen bool
	sheet       connectSheetState

	jump jumpState

	// Phase 6 keyboard state. Every field is a mirror of an OmaircWindow.qml
	// property; the controller owns the network collapse/reorder facts.
	sidebarNetworkFocusID string
	serverListVisible     bool
	// membersHidden is the window-level Ctrl+Shift+M state. It survives
	// channel switches and is moot on a direct message; membersVisible() also
	// requires the column to fit and the target to be a channel.
	membersHidden       bool
	transcriptScroll    int
	transcriptFollowEnd bool
	transcriptCursor    int
	// firstUnseenRow is the row that first arrived while the reader was scrolled
	// up, or -1 when nothing is waiting below. transcriptCount tracks the row
	// count between notifications so a growth can be detected. They mirror
	// TranscriptList's firstUnseenIndex and trackedCount.
	firstUnseenRow       int
	transcriptCount      int
	find                 findState
	nickComplete         nickCompleteSession
	composerHistory      []string
	composerHistoryIndex int
	composerHistoryDraft string
	memberFocus          bool
	memberIndex          int
	shortcutsOpen        bool
	aboutOpen            bool
	nick                 nickJumpState
	link                 linkState
	inbox                inboxState
	slash                slashSession
	list                 channelListState

	// Phase 9 desktop-notification state. notifier is the nil-able desktop
	// seam (mirroring backend.notifyDesktop); windowActive mirrors win.active;
	// lastNotification mirrors OmaircWindow.qml's lastNotification;
	// suppressDesktopNotification is the test latch mirroring the QML
	// property of the same name.
	notifier                    notify.Notifier
	windowActive                bool
	lastNotification            *notificationRecord
	suppressDesktopNotification bool

	// drafts keeps unsent composer text per conversation id, or per Status
	// surface ("status\n<networkID>").
	drafts   map[string]string
	draftKey string

	// statusReturnID is the conversation Escape returns to after Status.
	statusReturnID string

	// Phase 10 URL policy state. lastOpenedURL mirrors OmaircWindow.qml's
	// lastOpenedUrl (the most recent URL openAllowedURL allowed);
	// suppressExternalOpen mirrors suppressExternalUrlOpen, the latch that
	// keeps tests from launching a real OS handler while still exercising the
	// allowlist.
	lastOpenedURL        string
	suppressExternalOpen bool
}

// focusArea names where keyboard input goes. The Connect sheet and the
// overlays are modal and own the keys while they are open.
type focusArea int

const (
	focusComposer focusArea = iota
	focusConnect
	focusJump
)

// New returns a shell over ctrl with fixed width defaults and the composer
// focused. A nil controller renders the empty state. conn may be nil (the
// seeded demo), in which case the Connect sheet is unavailable.
func New(ctrl *controller.Controller, conn *connection.Connection) *Model {
	styles := defaultStyles()
	composer := newComposerInput(styles)
	m := &Model{
		ctrl:                 ctrl,
		conn:                 conn,
		width:                defaultWidth,
		height:               defaultHeight,
		focus:                focusComposer,
		composer:             composer,
		help:                 help.New(),
		spinner:              spinner.New(spinner.WithSpinner(spinner.MiniDot)),
		sheet:                newConnectSheetState(),
		jump:                 newJumpState(),
		nick:                 newNickJumpState(),
		link:                 newLinkState(),
		inbox:                newInboxState(),
		list:                 newChannelListState(),
		serverListVisible:    true,
		transcriptFollowEnd:  true,
		transcriptCursor:     -1,
		firstUnseenRow:       -1,
		composerHistoryIndex: -1,
		drafts:               make(map[string]string),
		// The terminal starts focused until a Blur arrives, mirroring
		// OmaircWindow.qml's win.active default.
		windowActive: true,
	}
	// applyStyles also wires the help and spinner chrome, and defaults to the
	// fixed fallback palette so New keeps its signature with no theme wired.
	m.applyStyles(styles)
	m.resize()
	m.draftKey = m.composerDraftKey()
	m.transcriptCount = m.transcriptRowTotal()
	if m.connectVisible() {
		m.focus = focusConnect
		m.composer.Blur()
		m.syncSheetField()
	} else {
		_ = m.composer.Focus()
	}
	m.ensureAvatars()
	m.syncSidebarNetworkOrder()
	return m
}

// Init starts the shell and delivers StartupMsg once, so the shell can
// reconcile stored profiles that connect on startup.
func (m *Model) Init() tea.Cmd {
	return func() tea.Msg { return StartupMsg{} }
}

// SetOnStartup installs the startup callback invoked by Update when StartupMsg
// arrives. It runs on the Update goroutine. A nil callback is a no-op.
func (m *Model) SetOnStartup(fn func()) { m.onStartup = fn }

// SetNotifier installs the desktop notifier. A nil notifier is a no-op.
func (m *Model) SetNotifier(n notify.Notifier) { m.notifier = n }

// SetAvatarSource installs the avatar store. A nil source leaves every direct
// row on its identicon.
func (m *Model) SetAvatarSource(s AvatarSource) { m.avatars = s }

// SetTheme rebuilds every style from colors. It is the manual counterpart to a
// ThemeChangedMsg and the entry point for an initial palette.
func (m *Model) SetTheme(colors theme.Colors) {
	if m == nil {
		return
	}
	m.applyStyles(buildStyles(colors))
}

// SetThemeWatcher installs the live theme watcher and immediately adopts its
// current palette. The read loop is armed on the first WindowSizeMsg (see
// startBackgroundWork), so the watcher only needs to be set before the program
// runs; its changes then arrive as ThemeChangedMsg.
func (m *Model) SetThemeWatcher(w *theme.Watcher) {
	if m == nil {
		return
	}
	m.themeWatcher = w
	if w != nil {
		m.applyStyles(buildStyles(w.Current()))
	}
}

// applyStyles swaps in a rebuilt palette and re-derives the chrome that lives
// outside Styles: the composer's input styles, the help hint styles, and the
// spinner's frame color. The composer keeps its value and focus.
func (m *Model) applyStyles(styles Styles) {
	m.styles = styles
	m.composer.SetStyles(styles.Input)
	m.help.Styles = helpStyles(styles)
	m.spinner.Style = styles.StatusWarn
	m.restyleOverlayInputs()
}

// restyleOverlayInputs re-applies the shared input style set to the overlay
// filters and the Connect sheet. Those text inputs are built when their sheet
// opens (or, for Connect, when a field takes focus), so without this a live
// ThemeChangedMsg would restyle only the composer and leave an already-open
// sheet on the previous palette. SetStyles keeps each input's value, width,
// and focus, so the call is behavior-neutral while the sheet is closed.
func (m *Model) restyleOverlayInputs() {
	input := m.styles.Input
	m.jump.input.SetStyles(input)
	m.nick.input.SetStyles(input)
	m.link.input.SetStyles(input)
	m.list.input.SetStyles(input)
	m.sheet.input.SetStyles(input)
}

// nextThemeChange arms one read from the watcher's channel. The read blocks off
// the Update goroutine; its ThemeChangedMsg triggers the next arm.
func (m *Model) nextThemeChange() tea.Cmd {
	watcher := m.themeWatcher
	if watcher == nil {
		return nil
	}
	return func() tea.Msg {
		change, ok := <-watcher.Changes()
		if !ok {
			return nil
		}
		return ThemeChangedMsg{Colors: change.Colors}
	}
}

// disconnected reports whether the focused network is not registered yet.
func (m *Model) disconnected() bool {
	return m.ctrl == nil || m.ctrl.ConnectionStatus() != "Connected"
}

// spinnerRunning reports whether the footer's spinner should be animating: the
// focused network is disconnected and the footer is on screen.
func (m *Model) spinnerRunning() bool {
	return m.disconnected() && m.footerVisible()
}

// startBackgroundWork arms the spinner and the theme read exactly once each,
// from the first WindowSizeMsg and from status changes that may have left the
// network disconnected. It returns nil when there is nothing to start.
func (m *Model) startBackgroundWork() tea.Cmd {
	var cmds []tea.Cmd
	if m.spinnerRunning() && !m.spinning {
		m.spinning = true
		// Tick is a method value: a func() tea.Msg, i.e. a tea.Cmd.
		cmds = append(cmds, m.spinner.Tick)
	}
	if m.themeWatcher != nil && !m.themeArmed {
		m.themeArmed = true
		cmds = append(cmds, m.nextThemeChange())
	}
	if len(cmds) == 0 {
		return nil
	}
	return tea.Batch(cmds...)
}

// ensureAvatars schedules a fetch for every direct-message peer with an avatar
// URL. It is a no-op without a controller, a source, or the /pref avatars
// toggle, and it never blocks.
func (m *Model) ensureAvatars() {
	if m.ctrl == nil || m.avatars == nil || !m.ctrl.PrefAvatarsEnabled() {
		return
	}
	for _, row := range m.ctrl.Conversations() {
		if row.Direct && row.Avatar != "" {
			m.avatars.Ensure(row.Avatar, avatarGlyphPixelSize)
		}
	}
}

// Update folds one Bubble Tea message. Quit chords leave the program; the
// Connect sheet and the overlays are modal; otherwise the navigation chords
// run before the rest reaches the composer.
func (m *Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.resize()
		// The program always sends a size message on start; it is the first
		// test-safe place to arm the spinner and the theme read, because Init
		// stays reserved for StartupMsg.
		return m, m.startBackgroundWork()
	case StartupMsg:
		// Runs on the Update goroutine, so ActivateStartup mutating the
		// controller and the session manager stays single-threaded.
		if m.onStartup != nil {
			m.onStartup()
		}
		return m, nil
	case NotifyMsg:
		m.ensureAvatars()
		// A status change may have left the focused network disconnected, so
		// restart the spinner if it stopped. A chat or membership publish may
		// have grown the transcript while the reader was scrolled up, which
		// arms the jump-to-first-new marker.
		m.noteTranscriptGrowth()
		return m, m.startBackgroundWork()
	case ThemeChangedMsg:
		// A live theme swap. Rebuild every style from the new palette, then
		// re-arm the read for the next change.
		m.applyStyles(buildStyles(msg.Colors))
		return m, m.nextThemeChange()
	case spinner.TickMsg:
		if !m.spinnerRunning() {
			m.spinning = false
			return m, nil
		}
		var cmd tea.Cmd
		m.spinner, cmd = m.spinner.Update(msg)
		return m, cmd
	case AvatarReadyMsg:
		// A background avatar fetch cached an image; the next render reads it.
		return m, nil
	case tea.FocusMsg:
		// The terminal regained focus. Mirror win.active, consume the
		// selected conversation's unread through the controller, then land on
		// the "New messages" mark when one is planted.
		m.windowActive = true
		if m.ctrl != nil {
			m.ctrl.SetWindowActive(true)
		}
		m.pinTranscriptOnFocusReturn()
		return m, nil
	case tea.BlurMsg:
		// The terminal lost focus; arrivals now earn a desktop notification.
		m.windowActive = false
		if m.ctrl != nil {
			m.ctrl.SetWindowActive(false)
		}
		return m, nil
	case MentionArrivalMsg:
		m.notifyMentionIfUnfocused(m.windowActive, msg.Author, msg.Body,
			msg.NetworkID, msg.Target, msg.MsgID)
		return m, nil
	case MonitorArrivalMsg:
		m.notifyMentionIfUnfocused(m.windowActive, msg.Author, msg.Body,
			msg.NetworkID, "", "")
		return m, nil
	case NotificationActivatedMsg:
		m.activateNotifiedConversation(msg.NetworkID, msg.Target, msg.MsgID)
		return m, nil
	case tea.KeyPressMsg:
		return m.handleKey(msg)
	}
	if m.connectVisible() {
		var cmd tea.Cmd
		m.sheet.input, cmd = m.sheet.input.Update(msg)
		return m, cmd
	}
	if m.overlaysVisible() {
		return m, m.overlayInputUpdate(msg)
	}
	var cmd tea.Cmd
	m.composer, cmd = m.composer.Update(msg)
	m.syncSlash()
	m.saveDraft()
	return m, cmd
}

// handleKey routes one key press to the open modal, overlay, find session, or
// the chord table. Ctrl+Q is the only quit chord; Ctrl+C copies.
func (m *Model) handleKey(msg tea.KeyPressMsg) (tea.Model, tea.Cmd) {
	// Keystroke, not String: kitty and ConPTY can populate Text on ctrl+printable
	// chords, and String returns "." for ctrl+. instead of "ctrl+.".
	key := normalizeChordKey(msg.Keystroke())
	if key == "ctrl+q" {
		return m, tea.Quit
	}
	// About is an informational modal that can sit on top of the Connect
	// sheet. While it is open every other chord is blocked; Escape, Enter, or
	// Space dismiss it (see handleAboutKey).
	if m.aboutOpen {
		return m.handleAboutKey(key)
	}
	// The shortcuts sheet sits on top of everything; while it is open only the
	// toggle and Escape close it.
	if m.shortcutsOpen {
		if key == "ctrl+/" || key == "esc" || key == "escape" {
			m.closeShortcuts()
		}
		return m, nil
	}
	// Connect is a window-level modal. Ctrl+/ still opens the shortcuts sheet
	// on top of it, Ctrl+Shift+/ opens About, and Ctrl+C still reaches the copy
	// path; every other chord is owned by the sheet.
	if m.connectVisible() {
		// Apply is window-level, like OmaircWindow.qml's Ctrl+Enter shortcut,
		// not a per-control handler. Check it before the sheet walk consumes
		// Enter or before Windows VT ctrl+j (Ctrl+Enter) reaches a text field.
		if isConnectApplyKey(key, msg) {
			m.focus = focusConnect
			m.applyConnect()
			return m, nil
		}
		if key == "ctrl+/" {
			m.openShortcuts()
			return m, nil
		}
		if key == "ctrl+shift+/" {
			m.openAbout()
			return m, nil
		}
		if key == "ctrl+c" {
			return m, m.copySelection()
		}
		m.focus = focusConnect
		return m.handleConnectKey(key, msg)
	}
	// Overlays own the keys while they are open.
	if m.overlaysVisible() {
		return m.handleOverlayKey(key, msg)
	}
	m.focus = focusComposer
	if m.find.active {
		return m.handleFindKey(key, msg)
	}
	// Any key but Tab ends a nick-completion session, mirroring the QML's
	// resetNickComplete before its non-Tab branches. Tab alone starts or
	// advances the session.
	if key != "tab" {
		m.resetNickComplete()
	}
	key = nickJumpChordKey(key)
	if handled, cmd := m.dispatchChord(key, msg); handled {
		return m, cmd
	}
	var cmd tea.Cmd
	m.composer, cmd = m.composer.Update(msg)
	m.syncSlash()
	m.resetHistoryBrowse()
	m.saveDraft()
	return m, cmd
}

// sendComposer submits the composer text and clears it, along with its stored
// draft, only when the controller accepted the submission. A refused command
// (for example `/close` on a channel) stays in the composer. It mirrors
// OmaircWindow.qml's sendMessage.
func (m *Model) sendComposer() {
	value := m.composer.Value()
	if strings.TrimSpace(value) != "" && m.ctrl != nil {
		sent := false
		if m.ctrl.ConsoleOpen() {
			sent = m.ctrl.ConsoleSubmit(value)
		} else {
			sent = m.ctrl.SendMessage(value)
		}
		if !sent {
			m.syncChannelList()
			return
		}
		m.rememberSentLine(value)
	}
	m.composer.Reset()
	m.clearCurrentDraft()
	m.resetHistoryBrowse()
	m.slash.reset()
	m.syncChannelList()
}

// refocusComposer restores composer focus after a chord when no modal is open.
func (m *Model) refocusComposer() {
	if m.connectVisible() || m.overlaysVisible() || m.shortcutsOpen || m.aboutOpen {
		return
	}
	_ = m.composer.Focus()
}

// View renders the shell into the declarative View: the alternate screen and
// the Qt-equivalent window title.
func (m *Model) View() tea.View {
	v := tea.NewView(m.render())
	v.AltScreen = true
	v.WindowTitle = Title(m.ctrl, m.conn)
	// Focus reporting drives FocusMsg/BlurMsg, which the shell needs to decide
	// whether an arrival earns a desktop notification (win.active in the QML).
	v.ReportFocus = true
	// ConPTY collapses ctrl+shift+letter to ctrl+letter and Ctrl+Enter to
	// ctrl+j. Request escape-coded keys so shift chords (nick jump, server
	// list, inbox, …) and Connect Apply stay distinct on Windows Terminal.
	v.KeyboardEnhancements.ReportAllKeysAsEscapeCodes = true
	// Paint the terminal from the live palette so the surface behind the
	// columns matches the theme instead of the terminal default.
	v.BackgroundColor = m.styles.Colors.Background
	v.ForegroundColor = m.styles.Colors.Foreground
	// Place the composer's real terminal cursor at its frame row. The row is
	// the shell's, because the footer sits below the composer; composerCursor
	// returns nil while the composer is blurred (an overlay or modal owns the
	// keys), so the cursor never floats over a sheet.
	v.Cursor = m.composerCursor(m.composerRow())
	return v
}

// resize keeps the composer and every overlay input in step with the window.
func (m *Model) resize() {
	width := m.composerInteriorWidth() - composerPrefixWidth
	if width < 1 {
		width = 1
	}
	m.composer.SetWidth(width)
	m.sheet.input.SetWidth(m.overlayInputWidth())
	m.jump.input.SetWidth(m.overlayInputWidth())
	m.nick.input.SetWidth(m.overlayInputWidth())
	m.link.input.SetWidth(m.overlayInputWidth())
	m.list.input.SetWidth(m.overlayInputWidth())
}

// render composes the columns, any open overlay, and the status footer. The
// composer is not a window-level part here: transcriptColumn carries it at the
// bottom of the middle column, so the side columns run the full body height
// beside it. It never indexes a slice unguarded, so a tiny or empty terminal
// renders a short line instead of panicking.
func (m *Model) render() string {
	if m == nil || m.ctrl == nil {
		return "Omairc"
	}
	if m.width < minWidth || m.height < minHeight {
		return m.composerView()
	}
	bodyHeight := m.bodyHeight()

	columns := make([]string, 0, 3)
	if m.serverListVisible {
		columns = append(columns, m.framedColumn(
			m.sidebarView, sidebarWidth(m.width), bodyHeight, m.sidebarNetworkFocusID != ""))
	}
	columns = append(columns, m.transcriptColumn(bodyHeight))
	if m.membersVisible() {
		columns = append(columns, m.framedColumn(
			m.membersView, membersWidth, bodyHeight, m.memberFocus))
	}
	body := lipgloss.JoinHorizontal(lipgloss.Top, columns...)

	if card, ok := m.overlayCard(); ok {
		body = m.compositeOverlay(body, card)
	}
	// The slash menu floats over the body's last rows instead of spending rows
	// from the column budget, so the sidebar, transcript, and member columns
	// keep their height while the completion list is open.
	body = m.compositeSlashMenu(body, bodyHeight)

	parts := []string{body}
	if m.footerVisible() {
		parts = append(parts, m.footerView())
	}
	return lipgloss.JoinVertical(lipgloss.Left, parts...)
}

// framedColumn wraps one side column in a rounded card. outerWidth and
// outerHeight are the invariant outer sizes the layout arithmetic assumes
// (sidebarWidth() / membersWidth and bodyHeight); the border consumes its
// frame, so the leaf renderer receives the remainder and produces inner lines
// only. That keeps the transcript, which stays unframed, on the same grid.
func (m *Model) framedColumn(render func(width, height int) string, outerWidth, outerHeight int, focused bool) string {
	style := m.styles.Panel
	if focused {
		style = m.styles.PanelFocused
	}
	frameX, frameY := style.GetFrameSize()
	innerWidth := outerWidth - frameX
	innerHeight := outerHeight - frameY
	if innerWidth < 1 || innerHeight < 1 {
		// No room for a border: fall back to the bare column so the grid stays
		// aligned on a tiny terminal.
		return render(outerWidth, outerHeight)
	}
	// Panel Width/Height set the total block size, border included, so the leaf
	// gets the remainder as its content box.
	return style.Width(outerWidth).Height(outerHeight).Render(render(innerWidth, innerHeight))
}

// transcriptColumn renders the middle column: the transcript, or a centered
// calm state when the selected conversation has nothing to show yet, with the
// composer pinned under it. The composer is part of this column rather than a
// window-level bar, so the side columns run the full body height beside it and
// the field spans only the transcript width. The column stays unframed, so
// transcriptWidth() remains the exact content width. height is the full body
// height; the transcript gets everything but the composer block.
func (m *Model) transcriptColumn(height int) string {
	content := height - composerFieldHeight
	if content < 1 {
		content = 1
	}
	var transcript string
	if lines, _ := m.transcriptLines(); len(lines) == 0 {
		transcript = m.emptyTranscript(content)
	} else {
		transcript = m.transcriptView(content)
	}
	return lipgloss.JoinVertical(lipgloss.Left, transcript, m.composerView())
}

// emptyTranscript centers a short muted caption through lipgloss.Place, so an
// empty conversation reads as calm space rather than a blank column.
func (m *Model) emptyTranscript(height int) string {
	caption := "No messages yet"
	if m.ctrl == nil || m.ctrl.SelectedTarget() == "" {
		caption = "Pick a conversation"
	}
	block := m.styles.Empty.Render(caption)
	return lipgloss.Place(m.transcriptWidth(), height, lipgloss.Center, lipgloss.Center, block)
}

// bodyHeight is the row budget of the three columns: the window minus the
// status footer when it fits. The composer lives inside the middle column (see
// transcriptColumn), so it is part of this budget instead of a separate
// window-level bar. The slash-completion menu is deliberately not subtracted:
// it floats over the transcript just above the composer (see
// compositeSlashMenu), so opening it never resizes the columns.
func (m *Model) bodyHeight() int {
	height := m.height
	if m.footerVisible() {
		height -= footerHeight
	}
	if height < 1 {
		height = 1
	}
	return height
}

// transcriptHeight is the transcript's own row budget: the body minus the
// composer block pinned under it. It spans the pinned header plus the scrolling
// rows, so transcriptRowsHeight() subtracts the header before the scroll, find,
// and copy arithmetic measures against it.
func (m *Model) transcriptHeight() int {
	height := m.bodyHeight() - composerFieldHeight
	if height < 1 {
		height = 1
	}
	return height
}

// transcriptRowsHeight is the scrolling row budget: the transcript budget minus
// its pinned header. A column whose header alone fills it can make this zero, so
// each caller clamps as it did before.
func (m *Model) transcriptRowsHeight() int {
	return m.transcriptHeight() - len(m.transcriptHeader())
}

// composerRow is the composer input line's zero-based row in the rendered frame
// — the text row inside the composer block, not the block's top row. The
// composer is the last block of the middle column, directly above the footer;
// the slash menu floats over the transcript above it rather than between them,
// so it adds no rows. The tiny-terminal path renders only the bare single-row
// composer, so its row is 0.
func (m *Model) composerRow() int {
	if m == nil || m.width < minWidth || m.height < minHeight {
		return 0
	}
	return m.transcriptHeight() + composerFrameRows
}

// overlayCard returns the topmost overlay card as one rendered block, if any.
// Every overlay renders through its own <name>Card method, which wraps its
// content with the shared overlayCardBlock frame, so model.go owns the single
// framing path. About and the shortcuts sheet can sit on top of Connect, so
// they are checked first.
func (m *Model) overlayCard() (string, bool) {
	switch {
	case m.aboutOpen:
		return m.aboutCard(m.width), true
	case m.shortcutsOpen:
		return m.shortcutsCard(m.width), true
	case m.connectVisible():
		return m.connectCard(m.width), true
	case m.nick.open:
		return m.nickCard(m.width), true
	case m.link.open:
		return m.linkCard(m.width), true
	case m.inbox.open:
		return m.inboxCard(m.width), true
	case m.list.open:
		return m.channelListCard(m.width), true
	case m.jump.open:
		return m.jumpCard(m.width), true
	}
	return "", false
}

// overlaysVisible reports whether any of the filter overlays owns the keys.
func (m *Model) overlaysVisible() bool {
	return m != nil && (m.jump.open || m.nick.open || m.link.open || m.inbox.open || m.list.open)
}

// overlayInputUpdate folds a non-key message (a paste, for one) into the open
// overlay's filter input.
func (m *Model) overlayInputUpdate(msg tea.Msg) tea.Cmd {
	var cmd tea.Cmd
	switch {
	case m.jump.open:
		m.jump.input, cmd = m.jump.input.Update(msg)
	case m.nick.open:
		m.nick.input, cmd = m.nick.input.Update(msg)
	case m.link.open:
		m.link.input, cmd = m.link.input.Update(msg)
	case m.inbox.open:
		// The inbox has no filter input yet.
	case m.list.open:
		m.list.input, cmd = m.list.input.Update(msg)
	}
	return cmd
}

// closeAllOverlays dismisses every filter overlay. Opening one overlay closes
// the rest, so only one ever owns the keys.
func (m *Model) closeAllOverlays() {
	if m.jump.open {
		m.jump.open = false
		m.jump.input.Blur()
	}
	if m.nick.open {
		m.nick.open = false
		m.nick.input.Blur()
	}
	if m.link.open {
		m.link.open = false
		m.link.input.Blur()
	}
	if m.inbox.open {
		m.inbox.open = false
	}
	if m.list.open {
		m.list.open = false
		m.list.input.Blur()
		if m.ctrl != nil {
			m.ctrl.DismissChannelList()
		}
	}
}

// overlayCardInset is the blank margin the shared overlay frame leaves on each
// side of the window, so a card border never sits on the terminal's edge cell.
// The compositor centers the narrower card over the dimmed body.
const overlayCardInset = 2

// overlayCardTopInset is the fixed row the overlay card's top border occupies,
// the vertical counterpart to overlayCardInset. The card is anchored to this row
// rather than centered on its own height, so a content-length change — a
// narrowing jump filter, a shorter Connect tab, a taller network roster — cannot
// bounce the card up and down while the user works in it. Horizontal centering
// stays safe because every card shares one fixed outer width, so x never moves.
const overlayCardTopInset = 1

// overlayCardBlock is the single framing path for every overlay card: it
// clamps the card width, lays the overlay's content lines into the shared
// panel style, and returns the one block the compositor composites with the
// shadow. build receives the content width inside the border, so an overlay
// body never re-derives the frame inset and a later restyle stays inside the
// overlay's own file without editing this wrapper.
func (m *Model) overlayCardBlock(width int, build func(inner int) []string) string {
	frameX, _ := m.styles.Panel.GetFrameSize()
	inner := width - 2*overlayCardInset - frameX
	if inner < 8 {
		inner = 8
	}
	return m.styles.Panel.Width(inner + frameX).Render(strings.Join(build(inner), "\n"))
}

// overlayCardRowBudget is the number of content rows an overlay card may render:
// the body box minus the fixed top inset and the card's own border rows. A sheet
// windows its content to this through windowCardRows, so a card can never
// overhang the grid and clip its own action row off the bottom.
func (m *Model) overlayCardRowBudget() int {
	_, frameY := m.styles.Panel.GetFrameSize()
	budget := m.bodyHeight() - overlayCardTopInset - frameY
	if budget < 1 {
		budget = 1
	}
	return budget
}

// windowCardRows fits rows into budget with a fixed header and footer, scrolling
// the middle just enough to keep focusRow visible. The first headerRows rows and
// the last footerRows rows stay pinned, so a sheet's title, tabs, validation
// line, and action row stay on screen while a short terminal windows the fields
// between them; focusRow is an index into rows, and a focus inside either pin
// needs no scrolling. Rows that already fit come back unchanged, so a
// normal-size window renders exactly what the sheet built. It is pure so the
// arithmetic is testable without a terminal.
func windowCardRows(rows []string, headerRows, footerRows, focusRow, budget int) []string {
	if budget < 1 || len(rows) <= budget {
		return rows
	}
	headerRows = clampInt(headerRows, 0, len(rows))
	footerRows = clampInt(footerRows, 0, len(rows)-headerRows)
	if budget <= headerRows+footerRows {
		// Too short to hold both pins plus one middle row: keep the header, then
		// fill the remainder from the tail so the action row survives.
		out := make([]string, 0, budget)
		out = append(out, rows[:min(headerRows, budget)]...)
		if len(out) < budget {
			out = append(out, rows[len(rows)-(budget-len(out)):]...)
		}
		return out
	}
	tail := len(rows) - footerRows
	middle := rows[headerRows:tail]
	middleBudget := budget - headerRows - footerRows
	offset := 0
	switch {
	case focusRow >= tail:
		// The focus is pinned in the footer; show the end of the middle so the
		// fields beside the action row stay in view.
		offset = len(middle) - middleBudget
	case focusRow >= headerRows:
		if focus := focusRow - headerRows; focus >= middleBudget {
			offset = focus - middleBudget + 1
		}
	}
	offset = clampInt(offset, 0, len(middle)-middleBudget)
	out := make([]string, 0, budget)
	out = append(out, rows[:headerRows]...)
	out = append(out, middle[offset:offset+middleBudget]...)
	out = append(out, rows[tail:]...)
	return out
}

// clampInt clamps value to [low, high]. A high below low yields low.
func clampInt(value, low, high int) int {
	if value < low {
		return low
	}
	if value > high {
		return high
	}
	return value
}

// compositeOverlay dims the body and composites the overlay card over it, with
// a solid surface shadow one cell down and two right. The card is centered
// horizontally and anchored to a fixed top row (overlayCardTopInset) so its own
// height never moves it; the offsets clamp at the top-left on a tiny terminal.
// The result is re-fitted to the body box so the card and shadow can never push
// the composer or the footer off the grid.
func (m *Model) compositeOverlay(body, card string) string {
	width := lipgloss.Width(body)
	height := lipgloss.Height(body)
	dimmed := m.styles.Dimmed.Render(body)

	cardWidth := lipgloss.Width(card)
	cardHeight := lipgloss.Height(card)
	x := (width - cardWidth) / 2
	if x < 0 {
		x = 0
	}
	// Anchor the top edge; do not derive it from cardHeight. Centering on the
	// card's own height made a shorter list drag the whole card down the body.
	y := overlayCardTopInset

	shadow := m.shadowBlock(cardWidth, cardHeight)
	rendered := lipgloss.NewCompositor(
		lipgloss.NewLayer(dimmed),
		lipgloss.NewLayer(shadow).X(x+2).Y(y+1).Z(1),
		lipgloss.NewLayer(card).X(x).Y(y).Z(2),
	).Render()
	return fitBlock(rendered, width, height)
}

// compositeSlashMenu floats the open slash-completion menu over the transcript,
// just above the composer block it belongs to, instead of taking rows from the
// column budget. The composer is the middle column's last block, so the menu's
// bottom edge sits composerFieldHeight rows above the body's bottom and never
// covers the field. The menu is placed at slashMenuLeft with an X offset rather
// than as a pre-indented layer: an indented layer is drawn from column zero, so
// its leading spaces would overwrite the body and erase the sidebar beneath it.
// height is the body row count the caller rendered.
func (m *Model) compositeSlashMenu(body string, height int) string {
	menu := m.slashLines()
	if len(menu) == 0 {
		return body
	}
	width := lipgloss.Width(body)
	y := height - composerFieldHeight - len(menu)
	if y < 0 {
		y = 0
	}
	rendered := lipgloss.NewCompositor(
		lipgloss.NewLayer(body),
		lipgloss.NewLayer(strings.Join(menu, "\n")).X(m.slashMenuLeft()).Y(y).Z(1),
	).Render()
	return fitBlock(rendered, width, height)
}

// shadowBlock is the card-sized solid surface block drawn under the overlay to
// lift it off the dimmed body.
func (m *Model) shadowBlock(width, height int) string {
	if width < 1 {
		width = 1
	}
	if height < 1 {
		height = 1
	}
	blank := strings.Repeat(" ", width)
	rows := make([]string, height)
	for index := range rows {
		rows[index] = blank
	}
	style := lipgloss.NewStyle().Background(m.styles.Colors.SurfaceRaised)
	return style.Render(strings.Join(rows, "\n"))
}

// fitBlock clamps a composited block to exactly width cells by height lines,
// padding with spaces. It keeps the body box invariant when a card or its
// shadow overhangs the edge.
func fitBlock(content string, width, height int) string {
	if width < 1 {
		width = 1
	}
	if height < 1 {
		height = 1
	}
	lines := strings.Split(content, "\n")
	if len(lines) > height {
		lines = lines[:height]
	}
	out := make([]string, 0, height)
	for _, line := range lines {
		out = append(out, truncateLine(line, width))
	}
	blank := strings.Repeat(" ", width)
	for len(out) < height {
		out = append(out, blank)
	}
	return strings.Join(out, "\n")
}

// membersVisible reports whether the member column fits and applies. It is
// shown only for channels, matching the Qt panel, and Ctrl+Shift+M can hide it
// for good. The toggle is window-level, so it survives channel switches and is
// moot on a direct message.
func (m *Model) membersVisible() bool {
	if m.ctrl == nil || !m.ctrl.IsChannel() {
		return false
	}
	return m.width >= membersMinTotal && !m.membersHidden
}

// sidebarWidth clamps the sidebar's share of the window.
func sidebarWidth(width int) int {
	value := width / 4
	if value < sidebarMinWidth {
		value = sidebarMinWidth
	}
	if value > sidebarMaxWidth {
		value = sidebarMaxWidth
	}
	return value
}

// transcriptWidth is whatever the visible columns leave behind.
func (m *Model) transcriptWidth() int {
	width := m.width
	if m.serverListVisible {
		width -= sidebarWidth(m.width)
	}
	if m.membersVisible() {
		width -= membersWidth
	}
	if width < 1 {
		width = 1
	}
	return width
}

// composerInset is the blank cell margin the composer box leaves inside the
// transcript column on each side. The composer belongs to the middle column, so
// its left edge and width follow the column rather than the window: toggling a
// side rail carries the field with the transcript instead of leaving it behind.
// One cell keeps the box close to the column edges while still separating its
// border from the side rails.
const composerInset = 1

// transcriptLeft is the left edge of the transcript column: past the sidebar
// when that rail is visible, else the window edge. The floating slash menu and
// the composer cursor offset read it.
func (m *Model) transcriptLeft() int {
	if m.serverListVisible {
		return sidebarWidth(m.width)
	}
	return 0
}

// composerLeft is the composer field's absolute left cell in the frame: the
// transcript column's left edge plus its own inset. composerCursor moves the
// real cursor by it. composerView indents by composerInset instead, because it
// renders inside the column rather than at the frame edge.
func (m *Model) composerLeft() int {
	if m.width < minWidth || m.height < minHeight {
		return composerInset
	}
	return m.transcriptLeft() + composerInset
}

// composerWidth is the composer field's total visible width: the transcript
// column minus the inset on each side, clamped so a narrow column still yields
// one cell. It never reaches into the sidebar or the member column. The
// tiny-terminal path renders a bare full-width field, so it keeps the old
// window-based width.
func (m *Model) composerWidth() int {
	width := m.width - 2*composerInset
	if m.width >= minWidth && m.height >= minHeight {
		width = m.transcriptWidth() - 2*composerInset
	}
	if width < 1 {
		width = 1
	}
	return width
}

// slashMenuLeft is the floating slash menu's left cell: the composer field's
// left edge, so the menu opens over the transcript column and never covers the
// sidebar or blanks it while it is open.
func (m *Model) slashMenuLeft() int {
	if m.width < minWidth || m.height < minHeight {
		return 0
	}
	return m.composerLeft()
}

// fitLines clamps lines to limit, keeping the tail (the newest transcript
// rows) or the head (the top of a list), then pads with blanks so every column
// is exactly limit lines tall.
func fitLines(lines []string, limit int, tail bool) []string {
	if limit < 0 {
		limit = 0
	}
	if len(lines) > limit {
		if tail {
			lines = lines[len(lines)-limit:]
		} else {
			lines = lines[:limit]
		}
	}
	out := make([]string, 0, limit)
	out = append(out, lines...)
	for len(out) < limit {
		out = append(out, "")
	}
	return out
}

// renderColumn truncates each line to width, joins them, and pads every line
// to width so columns align. Truncating before the join keeps lipgloss from
// wrapping a long row into the next terminal line.
func renderColumn(style lipgloss.Style, width int, lines []string) string {
	trimmed := make([]string, len(lines))
	for index, line := range lines {
		trimmed[index] = truncateLine(line, width)
	}
	return style.Width(width).Render(strings.Join(trimmed, "\n"))
}

// truncateLine cuts one rendered line to width display cells without adding a
// style, so ANSI sequences survive.
func truncateLine(line string, width int) string {
	if width <= 0 {
		return ""
	}
	return lipgloss.NewStyle().MaxWidth(width).Inline(true).Render(line)
}

// composerPrefixWidth is the visible width of the composer's prompt glyph.
const composerPrefixWidth = 2
