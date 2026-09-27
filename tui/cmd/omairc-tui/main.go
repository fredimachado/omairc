// Command omairc-tui is the terminal client entry point.
//
// Phase 0 ships only the CLI shell: --version, --help, and a --demo-server
// stub that fails closed. The Bubble Tea shell and the IRC core land in later
// phases; see tui/AGENTS.md. A silent no-op would hide a wiring gap, so an
// unimplemented path exits non-zero instead of pretending to work.
package main

import (
	"fmt"
	"io"
	"os"

	"github.com/fredimachado/omairc/tui/internal/version"
)

const usage = `usage: omairc-tui [--version] [--help] [--demo-server]

Omairc terminal client.

Flags:
  --version      print the omairc-tui version and exit
  --help         print this help and exit
  --demo-server  seed the in-process demo world (not implemented yet)

The TUI and the IRC core are not implemented in this build, so running with no
flags exits non-zero until the Bubble Tea shell lands.`

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
		fmt.Fprintln(stderr,
			"omairc-tui: --demo-server is not implemented yet; failing closed")
		return 1
	}

	fmt.Fprintln(stderr,
		"omairc-tui: the TUI is not implemented yet; run --help for usage")
	return 1
}
