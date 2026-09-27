// Command omairc-tui is the terminal client entry point.
//
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

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/avatar"
	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/notify"
	"github.com/fredimachado/omairc/tui/internal/session"
	"github.com/fredimachado/omairc/tui/internal/storage"
	"github.com/fredimachado/omairc/tui/internal/theme"
	"github.com/fredimachado/omairc/tui/internal/ui"
	"github.com/fredimachado/omairc/tui/internal/version"
)

const usage = `usage: omairc-tui [--version] [--help] [--demo-server]

Omairc terminal client.

Flags:
  --version      print the omairc-tui version and exit
  --help         print this help and exit
  --demo-server  seed the in-process demo world and run the shell

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

	for _, arg := range args {
		switch arg {
		case "--help", "-h":
			helpRequested = true
		case "--version":
			versionRequested = true
		case "--demo-server":
			demoServer = true
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

	c := controller.New()
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
		// Phase 11 disk stores. New() starts ephemeral so unit tests and the
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
		c.LoadStoredPreferences()

		conn = connection.New(c, func() session.Transport { return session.NewNetTransport() })
		conn.SetProfileStore(profiles)
		conn.SetCredentialStore(credentials)
		// The session moves a network's standing avatar URL or autojoin list;
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
	model := ui.New(c, conn)
	p := tea.NewProgram(model)

	// The live Omarchy theme watcher. SetThemeWatcher adopts the current
	// palette immediately; the shell then arms one blocking Changes() read as a
	// tea.Cmd on the first WindowSizeMsg, and the Bubble Tea runtime delivers
	// each ThemeChangedMsg on the Update goroutine. The watcher therefore never
	// calls p.Send itself, so it stays off the synchronous-callback path that
	// would block the event loop.
	watcher := theme.NewWatcher(nil, 0)
	defer watcher.Close()
	model.SetThemeWatcher(watcher)

	// Peer avatars need 24-bit color for the half-block raster. Without it the
	// shell falls back to the nick identicon, so no store is created.
	if avatar.TruecolorSupported() {
		avatars := avatar.New(true, func(string) {
			p.Send(ui.AvatarReadyMsg{})
		})
		model.SetAvatarSource(avatars)
		defer avatars.Close()
	}

	// The desktop notifier is platform-specific (internal/notify). Its activation
	// callback fires from the D-Bus goroutine, so p.Send is the normal path.
	desktop := notify.New(func(a notify.Activation) {
		p.Send(ui.NotificationActivatedMsg(a))
	})
	model.SetNotifier(desktop)
	defer desktop.Close()

	wake := make(chan struct{}, 1)
	go func() {
		for range wake {
			p.Send(ui.NotifyMsg{})
		}
	}()
	notify := func() {
		select {
		case wake <- struct{}{}:
		default:
		}
	}
	if conn != nil {
		conn.OnDraftChanged = notify
		conn.OnNetworksChanged = notify
		conn.OnSetupRequiredChanged = notify
		conn.OnPersistenceChanged = notify
		conn.OnCredentialChanged = notify
	}
	c.OnSelectionChanged = notify
	c.OnStatusChanged = notify
	c.OnCapabilitiesChanged = notify
	// Only session goroutines raise these, so a direct p.Send is safe here (unlike
	// the navigation callbacks, which coalesce onto `wake`).
	c.OnMentionArrived = func(author, body, networkID, target, msgid string) {
		p.Send(ui.MentionArrivalMsg{
			Author: author, Body: body, NetworkID: networkID, Target: target, MsgID: msgid,
		})
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
	if _, err := p.Run(); err != nil {
		fmt.Fprintf(stderr, "omairc-tui: %v\n", err)
		return 1
	}
	close(wake)
	return 0
}
