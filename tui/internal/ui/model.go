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
	membersHidden        bool
	transcriptScroll     int
	transcriptFollowEnd  bool
	transcriptCursor     int
	find                 findState
	composerHistory      []string
	composerHistoryIndex int
	composerHistoryDraft string
	memberFocus          bool
	memberIndex          int
	shortcutsOpen        bool
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
	if m.connectVisible() {
		m.focus = focusConnect
		m.composer.Blur()
		m.syncSheetField()
	} else {
		_ = m.composer.Focus()
	}
	m.ensureAvatars()
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
	m.composer.SetStyles(styles.ComposerInput)
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
		// restart the spinner if it stopped.
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
		// The terminal regained focus. Mirror win.active and consume the
		// selected conversation's unread through the controller.
		m.windowActive = true
		if m.ctrl != nil {
			m.ctrl.SetWindowActive(true)
		}
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
	key := msg.String()
	if key == "ctrl+q" {
		return m, tea.Quit
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
	// on top of it; Ctrl+C still reaches the copy path; every other chord is
	// owned by the sheet.
	if m.connectVisible() {
		if key == "ctrl+/" {
			m.openShortcuts()
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
	if m.connectVisible() || m.overlaysVisible() || m.shortcutsOpen {
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
	width := m.composerWidth() - composerPrefixWidth
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

// render composes the columns, the composer, any open overlay, and the status
// footer. It never indexes a slice unguarded, so a tiny or empty terminal
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

	parts := []string{body}
	if slash := m.slashLines(); len(slash) > 0 {
		parts = append(parts, strings.Join(m.alignToComposer(slash), "\n"))
	}
	parts = append(parts, m.composerView())
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

// transcriptColumn renders the transcript, or a centered calm state when the
// selected conversation has nothing to show yet. The transcript itself stays
// unframed, so transcriptWidth() remains the exact content width.
func (m *Model) transcriptColumn(height int) string {
	lines, _ := m.transcriptLines()
	if len(lines) == 0 {
		return m.emptyTranscript(height)
	}
	return m.transcriptView(height)
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
// composer line, the status footer when it fits, and any slash-completion rows
// above the composer.
func (m *Model) bodyHeight() int {
	height := m.height - 1 - len(m.slashLines())
	if m.footerVisible() {
		height -= footerHeight
	}
	if height < 1 {
		height = 1
	}
	return height
}

// composerRow is the composer line's zero-based row in the rendered frame. The
// composer sits below the body and any slash-completion rows and above the
// footer, so View derives the cursor's Y from the same layout render() builds.
// The tiny-terminal path renders only the composer, so its row is 0.
func (m *Model) composerRow() int {
	if m == nil || m.width < minWidth || m.height < minHeight {
		return 0
	}
	return m.bodyHeight() + len(m.slashLines())
}

// overlayCard returns the topmost overlay card as one rendered block, if any.
// Every overlay renders through its own <name>Card method, which wraps its
// content with the shared overlayCardBlock frame, so model.go owns the single
// framing path. The shortcuts sheet can sit on top of Connect, so it is
// checked first.
func (m *Model) overlayCard() (string, bool) {
	switch {
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

// compositeOverlay dims the body and composites the overlay card over it, with
// a solid surface shadow one cell down and two right. The card is centered on
// the body and the offsets clamp at the top-left on a tiny terminal. The
// result is re-fitted to the body box so the card and shadow can never push the
// composer or the footer off the grid.
func (m *Model) compositeOverlay(body, card string) string {
	width := lipgloss.Width(body)
	height := lipgloss.Height(body)
	dimmed := m.styles.Dimmed.Render(body)

	cardWidth := lipgloss.Width(card)
	cardHeight := lipgloss.Height(card)
	x := (width - cardWidth) / 2
	y := (height - cardHeight) / 2
	if x < 0 {
		x = 0
	}
	if y < 0 {
		y = 0
	}

	shadow := m.shadowBlock(cardWidth, cardHeight)
	rendered := lipgloss.NewCompositor(
		lipgloss.NewLayer(dimmed),
		lipgloss.NewLayer(shadow).X(x+2).Y(y+1).Z(1),
		lipgloss.NewLayer(card).X(x).Y(y).Z(2),
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

// composerInset is the blank cell margin the composer field leaves inside the
// transcript column on each side, so it sits centered under the transcript it
// belongs to. It mirrors ConversationColumn.qml's composerShell margins.
const composerInset = 2

// composerColumnLeft is the left edge of the transcript column: past the
// sidebar when that rail is visible, else the window edge.
func (m *Model) composerColumnLeft() int {
	if m.serverListVisible {
		return sidebarWidth(m.width)
	}
	return 0
}

// composerLeft is the composer field's left cell: the transcript column plus its
// inset. composerView pads to it and composerCursor moves the real cursor by it.
func (m *Model) composerLeft() int {
	return m.composerColumnLeft() + composerInset
}

// composerWidth is the composer field's total visible width: the transcript
// column minus both insets, clamped so a narrow window still yields one cell.
func (m *Model) composerWidth() int {
	width := m.transcriptWidth() - 2*composerInset
	if width < 1 {
		width = 1
	}
	return width
}

// alignToComposer shifts inline composer chrome (the slash menu) so it opens
// over the field it belongs to instead of the window's left edge. The tiny
// full-width fallback leaves the lines at the left edge.
func (m *Model) alignToComposer(lines []string) []string {
	if len(lines) == 0 || m.width < minWidth || m.height < minHeight {
		return lines
	}
	indent := strings.Repeat(" ", m.composerLeft())
	shifted := make([]string, len(lines))
	for index, line := range lines {
		shifted[index] = indent + line
	}
	return shifted
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
