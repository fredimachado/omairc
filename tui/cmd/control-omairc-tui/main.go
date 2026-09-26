// Command control-omairc-tui is the PTY driver for the omairc-tui terminal
// client. All behavior lives in internal/gate; this file only wires the
// process streams and exit status.
package main

import (
	"os"

	"github.com/fredimachado/omairc/tui/internal/gate"
)

func main() {
	os.Exit(gate.Run(os.Args[1:], os.Stdout, os.Stderr))
}
