// Command omairc-tui is the terminal client entry point.
//
// Phase 3 ships the CLI shell plus the in-process demo seed: --version,
// --help, and --demo-server, which seeds the two-network demo world through
// internal/demo and exits 0 after printing a deterministic summary of the
// seeded conversations. The Bubble Tea shell is still unimplemented, so a
// no-argument invocation exits non-zero rather than pretending to work; the
// IRC core lands in later phases. See tui/AGENTS.md.
package main

import (
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/version"
)

const usage = `usage: omairc-tui [--version] [--help] [--demo-server]

Omairc terminal client.

Flags:
  --version      print the omairc-tui version and exit
  --help         print this help and exit
  --demo-server  seed the in-process demo world and exit

The Bubble Tea shell is not implemented in this build; --demo-server seeds the
IRC core and exits.`

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
		writeDemoSummary(stdout, c)
		return 0
	}

	fmt.Fprintln(stderr,
		"omairc-tui: the TUI is not implemented yet; run --help for usage")
	return 1
}

// writeDemoSummary prints a deterministic one-line-per-conversation summary of
// the seeded demo world. The first line names every network the seed attached,
// in the order the conversations first mention them; each following line names
// one sidebar conversation with its unread and mention state.
func writeDemoSummary(w io.Writer, c *controller.Controller) {
	conversations := c.Conversations()

	networks := make([]string, 0, 2)
	seen := make(map[string]bool)
	for _, row := range conversations {
		if row.NetworkID == "" || seen[row.NetworkID] {
			continue
		}
		seen[row.NetworkID] = true
		networks = append(networks, row.NetworkID)
	}

	fmt.Fprintf(w, "demo server seeded: %s\n", strings.Join(networks, ", "))
	for _, row := range conversations {
		fmt.Fprintf(w, "%s %s unread=%d mention=%t\n",
			row.NetworkID, row.Conversation, row.Unread, row.Mention)
	}
}
