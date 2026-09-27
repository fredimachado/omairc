// Command omairc-tui is the terminal client entry point.
//
// It ships the CLI flags plus the Bubble Tea shell: --version, --help, and
// --demo-server, which seeds the two-network demo world through internal/demo
// and then runs the interactive shell over it. A plain launch shows the
// Connect sheet over an empty controller and applies a real session; the sheet
// is in-memory only until the phase-11 profile store lands. See tui/AGENTS.md.
package main

import (
	"fmt"
	"io"
	"os"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/session"
	"github.com/fredimachado/omairc/tui/internal/ui"
	"github.com/fredimachado/omairc/tui/internal/version"
)

const usage = `usage: omairc-tui [--version] [--help] [--demo-server]

Omairc terminal client.

Flags:
  --version      print the omairc-tui version and exit
  --help         print this help and exit
  --demo-server  seed the in-process demo world and run the shell

Without --demo-server the shell starts on the Connect sheet and applies a
live IRC session once a network profile is complete.`

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
		server := demo.New()
		if !server.Attach(c, true) {
			fmt.Fprintf(stderr, "omairc-tui: demo server failed: %s\n", server.LastError())
			return 1
		}
	} else {
		conn = connection.New(c, func() session.Transport { return session.NewNetTransport() })
	}

	// Controller and connection callbacks fire both from the Bubble Tea
	// goroutine (inside Update) and from session goroutines. p.Send blocks on
	// the program's message channel, so calling it synchronously from a
	// callback would deadlock the event loop the moment a navigation chord
	// rebuilt a snapshot. Coalesce the wake-ups onto one dispatcher instead.
	p := tea.NewProgram(ui.New(c, conn))
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
	}
	c.OnSelectionChanged = notify
	c.OnStatusChanged = notify
	c.OnCapabilitiesChanged = notify
	if _, err := p.Run(); err != nil {
		fmt.Fprintf(stderr, "omairc-tui: %v\n", err)
		return 1
	}
	close(wake)
	return 0
}
