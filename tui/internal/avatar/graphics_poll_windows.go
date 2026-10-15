//go:build windows

package avatar

import (
	"io"
	"os"
	"time"
)

// readProbeReply waits for DA1 or the graphics timeout. Windows consoles are
// not a Kitty target; when the console file cannot take a read deadline the
// probe falls back to the half-block raster.
func readProbeReply(w io.Writer, in *os.File, timeout time.Duration) (bool, io.Reader) {
	res := readProbeDeadline(w, in, timeout)
	return res.supported, res.wrap(in)
}
