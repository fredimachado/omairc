// Command omairc-tui is the terminal client entry point.
//
// Phase 4 ships the CLI flags plus the Bubble Tea shell: --version, --help, and
// --demo-server, which seeds the two-network demo world through internal/demo
// and then runs the interactive shell over it. A no-argument invocation still
// exits non-zero rather than pretending to work, because the Connect sheet (and
// with it the live IRC path) lands in Phase 5. See tui/AGENTS.md.
package main

import (
	"fmt"
	"io"
	"os"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/ui"
	"github.com/fredimachado/omairc/tui/internal/version"
)

const usage = `usage: omairc-tui [--version] [--help] [--demo-server]

Omairc terminal client.

Flags:
  --version      print the omairc-tui version and exit
  --help         print this help and exit
  --demo-server  seed the in-process demo world and run the shell

The Connect sheet lands in Phase 5; --demo-server runs the interactive shell
over the seeded demo world.`

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
	if demoServer {
		c := controller.New()
		server := demo.New()
		if !server.Attach(c, true) {
			fmt.Fprintf(stderr, "omairc-tui: demo server failed: %s\n", server.LastError())
			return 1
		}
		// Wire the wake-up callbacks after NewProgram: p.Send needs the
		// program, and demo.Attach fired the callbacks (still nil) while
		// seeding. The shell reads the rebuilt snapshots directly on its
		// first View, so nothing is missed.
		p := tea.NewProgram(ui.New(c))
		c.OnSelectionChanged = func() { p.Send(ui.NotifyMsg{}) }
		c.OnStatusChanged = func() { p.Send(ui.NotifyMsg{}) }
		c.OnCapabilitiesChanged = func() { p.Send(ui.NotifyMsg{}) }
		if _, err := p.Run(); err != nil {
			fmt.Fprintf(stderr, "omairc-tui: %v\n", err)
			return 1
		}
		return 0
	}

	fmt.Fprintln(stderr,
		"omairc-tui: no Connect sheet yet (Phase 5); run --demo-server for the seeded demo")
	return 1
}
