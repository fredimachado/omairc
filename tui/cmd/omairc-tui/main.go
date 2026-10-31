// Command omairc-tui is the terminal client entry point.
// It ships the CLI flags plus the Bubble Tea shell: --version, --help, and
// --demo-server, which seeds the two-network demo world through internal/demo
// and then runs the interactive shell over it. A plain launch wires the disk
// stores, shows the Connect sheet over an empty controller, applies a real
// session, loads the stored profiles and preferences, and connects the
// connectOnStartup networks once the program starts. See tui/AGENTS.md.
package main

import (
	"fmt"
	"io"
	"os"
	"os/signal"
	"sync"
	"syscall"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/avatar"
	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/crashlog"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/notify"
	"github.com/fredimachado/omairc/tui/internal/session"
	"github.com/fredimachado/omairc/tui/internal/storage"
	"github.com/fredimachado/omairc/tui/internal/theme"
	"github.com/fredimachado/omairc/tui/internal/ui"
	"github.com/fredimachado/omairc/tui/internal/version"
)

const usage = `usage: omairc-tui [--version] [--help] [--demo-server] [--mouse]

Omairc terminal client.

Flags:
  --version      print the omairc-tui version and exit
  --help         print this help and exit
  --demo-server  seed the in-process demo world and run the shell
  --mouse        wheel-scroll and click rows (or set OMAIRC_TUI_MOUSE=1)

Mouse reporting is off unless --mouse or OMAIRC_TUI_MOUSE is set, so the
terminal keeps its own text selection. Accepted environment values are
1, true, yes, and on. This is not a /pref toggle.

Without --demo-server the shell loads the stored profiles and preferences,
applies a live IRC session once a network profile is complete, and connects
the connectOnStartup networks after launch.`

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}

func run(args []string, stdout, stderr io.Writer) int {
	helpRequested := false
	versionRequested := false
	demoServer := false
	mouseFlag := false

	for _, arg := range args {
		switch arg {
		case "--help", "-h":
			helpRequested = true
		case "--version":
			versionRequested = true
		case "--demo-server":
			demoServer = true
		case "--mouse":
			mouseFlag = true
		default:
			fmt.Fprintf(stderr, "omairc-tui: unknown argument %q\n", arg)
			fmt.Fprintln(stderr, usage)
			return 2
		}
	}

	// Help wins over version so `--help` can never print the version line.
	if helpRequested {
		fmt.Fprintln(stdout, usage)
		return 0
	}
	if versionRequested {
		fmt.Fprintf(stdout, "omairc-tui %s\n", version.Value)
		return 0
	}

	// Keep a crash record on disk. Bubble Tea recovers panics and prints them
	// to stderr, and the runtime prints unrecovered panics on its own, so a
	// crash leaves no trace once the terminal scrolls or closes. --help and
	// --version return above, so neither creates a log.
	if restore, err := crashlog.Install(); err != nil {
		fmt.Fprintf(stderr, "omairc-tui: crash log disabled: %v\n", err)
	} else {
		defer restore()
	}

	c := controller.New()
	// tea.Quit (Ctrl+Q) and p.Quit (SIGHUP) return from Run without calling
	// Session.Stop. Refresh STS expiry on the way out so a long-lived session
	// does not leave an already-expired policy on disk for the next start.
	// Once so the explicit post-Run call and this defer for early returns share
	// one Shutdown; the post-Run path must run Shutdown before close(wake).
	var shutOnce sync.Once
	shutdownSessions := func() {
		shutOnce.Do(func() { c.Shutdown() })
	}
	defer shutdownSessions()
	var conn *connection.Connection
	if demoServer {
		// The demo stays ephemeral: it must never read or write the user's
		// config or transcripts.
		server := demo.New()
		if !server.Attach(c, true) {
			fmt.Fprintf(stderr, "omairc-tui: demo server failed: %s\n", server.LastError())
			return 1
		}
	} else {
		// Disk stores. New starts ephemeral so unit tests and the
		// demo never touch the config; the shell opts in and wires the stores
		// before the first session is applied.
		profiles := storage.NewProfileStore()
		credentials := storage.NewCredentialStore()
		openDirects := storage.NewOpenDirectStore()
		playbackTimes := storage.NewPlaybackTimeStore()
		transcripts := storage.NewConversationLog("")

		c.SetEphemeral(false)
		c.SetOpenDirectStore(openDirects)
		c.SetPlaybackTimeStore(playbackTimes)
		c.SetConversationLog(transcripts)
		// Share the desktop client's `sts` INI beside omairc.conf so a
		// learned policy survives a TUI restart (and a desktop restart).
		c.AttachSTSDisk(storage.STSPath())
		c.LoadStoredPreferences()

		conn = connection.New(c, func() session.Transport { return session.NewNetTransport() })
		conn.SetProfileStore(profiles)
		conn.SetCredentialStore(credentials)
		// The session moves a network's standing avatar URL or autojoin list
		// the controller forwards it here so the profile store stays current.
		c.OnAvatarURLChanged = func(networkID, url string) {
			if conn != nil {
				conn.PersistAvatarURL(networkID, url)
			}
		}
		c.OnAutojoinChanged = func(networkID string, channels []string, keys map[string]string) {
			if conn != nil {
				conn.PersistAutojoin(networkID, channels, keys)
			}
		}
	}

	// Controller and connection callbacks fire both from the Bubble Tea
	// goroutine (inside Update) and from session goroutines. p.Send blocks on
	// the program's message channel, so calling it synchronously from a
	// callback would deadlock the event loop the moment a navigation chord
	// rebuilt a snapshot. Coalesce the wake-ups onto one dispatcher instead.
	// ui.New reads c.ColorPalette() for the fixed nick hue set.
	model := ui.New(c, conn)
	model.SetMouseEnabled(ui.MouseOptIn(mouseFlag, os.Getenv("OMAIRC_TUI_MOUSE")))

	// The live Omarchy theme watcher. SetThemeWatcher adopts the current
	// palette immediately; the shell then arms one blocking Changes read as a
	// tea.Cmd on the first WindowSizeMsg, and the Bubble Tea runtime delivers
	// each ThemeChangedMsg on the Update goroutine. The watcher therefore never
	// calls p.Send itself, so it stays off the synchronous-callback path that
	// would block the event loop.
	watcher := theme.NewWatcher(nil, 0)
	defer watcher.Close()
	model.SetThemeWatcher(watcher)

	// Peer avatars need 24-bit color for the half-block raster. Kitty graphics
	// is a separate probe: a positive a=q reply, never a TERM guess. WezTerm
	// answers that probe and stays on the half-block raster, because it does
	// not draw Unicode placeholders. The store exists when either path is
	// available. A row with no cached image stays on the nick identicon.
	// input is non-nil when keys typed during the probe must be replayed, or
	// a Kitty reply that arrives after the probe must be discarded.
	graphics, input := avatar.ProbeGraphics(os.Stdin, os.Stdout, avatar.ProbeTimeout)
	if c, ok := input.(io.Closer); ok {
		defer func() { _ = c.Close() }()
	}
	var opts []tea.ProgramOption
	if input != nil {
		opts = append(opts, tea.WithInput(input))
	}
	// External SIGTSTP on the normal path (no probe replay) must release and
	// restore through Bubble Tea. The avatar holdTerminal watcher already
	// covers tea.WithInput(replay); do not register a second handler there.
	var stops *stopWatcher
	if input == nil {
		stopDone := make(chan struct{})
		defer close(stopDone)
		stops = newStopWatcher(stopDone)
		opts = append(opts, tea.WithFilter(suspendFilter(stops)))
	}
	p := tea.NewProgram(model, opts...)
	if stops != nil {
		stops.bind(p)
		go stops.run()
	}

	// SIGHUP's default action kills the process before deferred cleanup runs.
	// Quit the program instead so Run returns and the delete escapes are written.
	hup := make(chan os.Signal, 1)
	signal.Notify(hup, syscall.SIGHUP)
	defer signal.Stop(hup)
	stopHup := make(chan struct{})
	defer close(stopHup)
	go func() {
		select {
		case <-hup:
			p.Quit()
		case <-stopHup:
		}
	}()

	var avatars *avatar.Store
	if avatar.TruecolorSupported() || graphics {
		avatars = avatar.New(avatar.TruecolorSupported(), func(string) {
			p.Send(ui.AvatarReadyMsg{})
		})
		if graphics {
			avatars.EnableGraphics()
		}
		model.SetAvatarSource(avatars)
	}
	defer func() {
		if avatars != nil {
			avatars.Close()
		}
	}()
	defer func() {
		_ = avatar.WriteDeleteEscapes(os.Stdout, avatars)
	}()

	// The desktop notifier is platform-specific (internal/notify). Its activation
	// callback fires from the D-Bus goroutine, so p.Send is the normal path.
	desktop := notify.New(func(a notify.Activation) {
		p.Send(ui.NotificationActivatedMsg(a))
	})
	model.SetNotifier(desktop)
	defer desktop.Close()

	wake := make(chan struct{}, 1)
	var wakeMu sync.Mutex
	wakeOpen := true
	go func() {
		for range wake {
			p.Send(ui.NotifyMsg{})
		}
	}()
	// notify must not panic if wake is already closed. Shutdown can fire
	// OnStatusChanged while stopping sessions; that must stay safe even if
	// closeWake raced ahead.
	notify := func() {
		wakeMu.Lock()
		defer wakeMu.Unlock()
		if !wakeOpen {
			return
		}
		select {
		case wake <- struct{}{}:
		default:
		}
	}
	closeWake := func() {
		wakeMu.Lock()
		defer wakeMu.Unlock()
		if !wakeOpen {
			return
		}
		wakeOpen = false
		close(wake)
	}
	if conn != nil {
		conn.OnDraftChanged = notify
		conn.OnNetworksChanged = func() {
			c.RefreshStoredProfileIDs()
			notify()
		}
		conn.OnSetupRequiredChanged = notify
		conn.OnPersistenceChanged = notify
		conn.OnCredentialChanged = notify
	}
	c.OnSelectionChanged = notify
	c.OnStatusChanged = notify
	c.OnCapabilitiesChanged = notify
	c.OnViewChanged = notify
	// Only session goroutines raise these, so a direct p.Send is safe here (unlike
	// the navigation callbacks, which coalesce onto `wake`).
	c.OnMentionArrived = func(author, body, networkID, target, msgid string) {
		p.Send(ui.MentionArrivalMsg{
			Author: author, Body: body, NetworkID: networkID, Target: target, MsgID: msgid,
		})
	}
	c.OnFileLink = func(url, message string) {
		p.Send(ui.FileLinkMsg{URL: url, Message: message})
	}
	c.OnMonitorArrived = func(networkID, display, body string, _ bool) {
		p.Send(ui.MonitorArrivalMsg{NetworkID: networkID, Author: display, Body: body})
	}
	c.OnInboxChanged = notify
	if !demoServer {
		// ActivateStartup runs from the Update goroutine (the StartupMsg
		// handler), so controller mutation stays single-threaded; notify
		// coalesces the wake-up that repaints the connected networks.
		model.SetOnStartup(func() {
			conn.ActivateStartup()
			notify()
		})
	}
	var runErr error
	if _, err := p.Run(); err != nil {
		// Report through os.Stderr, which crashlog.Install replaced with the
		// tee: a recovered Bubble Tea panic arrives here as ErrProgramPanic
		// and must reach the log, not just the parameter's pre-install file.
		fmt.Fprintf(os.Stderr, "omairc-tui: %v\n", err)
		runErr = err
	}
	// Quit order: refresh STS (Stop → StateChanged → notify) while wake is
	// still open, then close wake. Closing first panics every Ctrl+Q/SIGHUP
	// with a live session and aborts StopAll after the first network.
	shutdownSessions()
	closeWake()
	if runErr != nil {
		return 1
	}
	return 0
}
