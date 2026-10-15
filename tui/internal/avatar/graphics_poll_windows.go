//go:build windows

package avatar

import (
	"io"
	"os"
	"time"
)

// readProbeReply waits for a graphics OK. Windows consoles are not a Kitty
// target; when the console file cannot take a read deadline the probe falls
// back to the half-block raster.
func readProbeReply(w io.Writer, in *os.File, timeout time.Duration) bool {
	return ReadGraphicsSupport(w, in, timeout)
}
