package avatar

import (
	"bytes"
	"io"
	"os"
	"strconv"
	"strings"
	"time"
)

// deviceAttributesQuery is DA1 (ESC [ c). Terminals answer it, so the probe
// can stop when it arrives instead of holding the timeout open. The graphics
// query is sent first; a DA1 with no matching OK means graphics are off.
const deviceAttributesQuery = "\x1b[c"

// probeResult is what one graphics probe learned. replay is keystrokes typed
// while the probe was reading. dropLate is set only when the wait ends on the
// timeout without a DA1, and arms a filter for one late graphics reply, DA1,
// or XTVERSION. partial is the start of one of those sequences the probe
// already consumed and the filter must finish.
type probeResult struct {
	supported bool
	replay    []byte
	dropLate  bool
	sawAPC    bool
	sawXT     bool
	partial   []byte
}

// probeScan splits a probe byte stream into terminal responses and keys.
type probeScan struct {
	keys       []byte
	hold       []byte
	sawAPC     bool
	sawDA1     bool
	sawXT      bool
	graphicsOK bool
	wezterm    bool
}

// feed reports whether the probe has its DA1 sentinel or has grown past the
// response budget. A graphics OK does not stop the wait: DA1 is the sentinel,
// and the timeout remains the backstop when DA1 never arrives.
func (s *probeScan) feed(chunk []byte) bool {
	s.hold = append(s.hold, chunk...)
	for {
		if len(s.keys)+len(s.hold) > 4096 {
			return true
		}
		if len(s.hold) == 0 {
			return false
		}
		if s.hold[0] != 0x1b {
			i := bytes.IndexByte(s.hold, 0x1b)
			if i < 0 {
				s.keys = append(s.keys, s.hold...)
				s.hold = nil
				return false
			}
			s.keys = append(s.keys, s.hold[:i]...)
			s.hold = s.hold[i:]
			continue
		}
		if len(s.hold) == 1 {
			return false
		}
		switch s.hold[1] {
		case '_':
			n := stEnd(s.hold)
			if n == 0 {
				return false
			}
			msg := s.hold[:n]
			s.hold = s.hold[n:]
			if len(msg) >= 3 && msg[2] == 'G' {
				s.sawAPC = true
				if replyOK(msg) {
					s.graphicsOK = true
				}
				continue
			}
			s.keys = append(s.keys, msg...)
		case 'P':
			n := stEnd(s.hold)
			if n == 0 {
				return false
			}
			msg := s.hold[:n]
			s.hold = s.hold[n:]
			s.sawXT = true
			if xtversionWezTerm(msg) {
				s.wezterm = true
			}
		case '[':
			n, final := csiEnd(s.hold)
			if n == 0 {
				return false
			}
			if n < 0 {
				s.keys = append(s.keys, s.hold[0])
				s.hold = s.hold[1:]
				continue
			}
			if final == 'c' {
				s.sawDA1 = true
				s.hold = s.hold[n:]
				return true
			}
			s.keys = append(s.keys, s.hold[:n]...)
			s.hold = s.hold[n:]
		case ']':
			n := oscEnd(s.hold)
			if n == 0 {
				return false
			}
			s.keys = append(s.keys, s.hold[:n]...)
			s.hold = s.hold[n:]
		default:
			s.keys = append(s.keys, s.hold[:2]...)
			s.hold = s.hold[2:]
		}
	}
}

// finish builds the probe result. timedOut is set when the wait ended because
// DA1 did not arrive. The late filter is armed only on that path. A DA1 answer
// means the graphics reply is already in hand or is not coming, so the filter
// stays off and later Esc keys are ordinary input.
func (s *probeScan) finish(timedOut bool) probeResult {
	res := probeResult{
		supported: s.graphicsOK && !s.wezterm,
		replay:    append([]byte(nil), s.keys...),
		dropLate:  timedOut && !s.sawDA1,
		sawAPC:    s.sawAPC,
		sawXT:     s.sawXT,
	}
	if res.dropLate && isResponsePrefix(s.hold) {
		res.partial = append([]byte(nil), s.hold...)
		return res
	}
	if len(s.hold) > 0 {
		res.replay = append(res.replay, s.hold...)
	}
	return res
}

func isResponsePrefix(b []byte) bool {
	return bytes.HasPrefix(b, []byte("\x1b_")) || bytes.HasPrefix(b, []byte("\x1bP"))
}

// stEnd is the length of a DCS or APC through its ST (ESC \), or 0 while the
// terminator has not arrived. b starts with ESC.
func stEnd(b []byte) int {
	for i := 2; i+1 < len(b); i++ {
		if b[i] == 0x1b && b[i+1] == '\\' {
			return i + 2
		}
	}
	return 0
}

// csiEnd reports the length of a CSI sequence and its final byte. n == 0 means
// the sequence is incomplete. n < 0 means these bytes are not a CSI.
func csiEnd(b []byte) (n int, final byte) {
	if len(b) < 2 || b[0] != 0x1b || b[1] != '[' {
		return -1, 0
	}
	for i := 2; i < len(b); i++ {
		c := b[i]
		if c >= 0x40 && c <= 0x7e {
			return i + 1, c
		}
		if c < 0x20 || c > 0x3f {
			return -1, 0
		}
	}
	return 0, 0
}

func oscEnd(b []byte) int {
	for i := 2; i < len(b); i++ {
		if b[i] == 0x07 {
			return i + 1
		}
		if b[i] == 0x1b && i+1 == len(b) {
			return 0
		}
		if b[i] == 0x1b && b[i+1] == '\\' {
			return i + 2
		}
	}
	return 0
}

// probeID reports whether a graphics control section names the startup query.
func probeID(control string) bool {
	for _, part := range strings.Split(control, ",") {
		part = strings.TrimSpace(part)
		if !strings.HasPrefix(part, "i=") {
			continue
		}
		n, err := strconv.ParseUint(part[2:], 10, 32)
		return err == nil && uint32(n) == probeImageID
	}
	return false
}

// readProbeDeadline writes the probe and reads r until DA1 or the deadline.
// r must honor SetReadDeadline. Real terminals reject that deadline;
// ProbeGraphics polls instead.
func readProbeDeadline(w io.Writer, r interface {
	io.Reader
	SetReadDeadline(time.Time) error
}, timeout time.Duration) probeResult {
	if !placeholdersAllowed() || w == nil || r == nil || timeout <= 0 {
		return probeResult{}
	}
	deadline := time.Now().Add(timeout)
	if err := r.SetReadDeadline(deadline); err != nil {
		return probeResult{}
	}
	defer func() { _ = r.SetReadDeadline(time.Time{}) }()
	if _, err := io.WriteString(w, probeQueries()); err != nil {
		return probeResult{}
	}
	var scan probeScan
	tmp := make([]byte, 256)
	for {
		n, err := r.Read(tmp)
		if n > 0 && scan.feed(tmp[:n]) {
			return scan.finish(false)
		}
		if err != nil || !time.Now().Before(deadline) {
			return scan.finish(true)
		}
	}
}

func (r probeResult) wrap(in *os.File) io.Reader {
	if in == nil || (len(r.replay) == 0 && !r.dropLate && len(r.partial) == 0) {
		return nil
	}
	filter := &probeFilter{pending: append([]byte(nil), r.replay...)}
	if r.dropLate {
		filter.late.dropAPC = !r.sawAPC && !bytes.HasPrefix(r.partial, []byte("\x1b_"))
		filter.late.dropDA1 = true
		filter.late.dropXT = !r.sawXT && !bytes.HasPrefix(r.partial, []byte("\x1bP"))
	}
	if len(r.partial) > 0 {
		filter.late.seed(r.partial)
	}
	return &bufferedProbeInput{f: in, probeFilter: filter}
}

// probeFilter keeps keystrokes that arrived during the probe and drops one
// Kitty graphics reply that shows up after the probe stopped.
type probeFilter struct {
	pending []byte
	late    lateFilter
	tmp     [256]byte
}

func (f *probeFilter) read(file *os.File, p []byte) (int, error) {
	if len(p) == 0 {
		return 0, nil
	}
	for {
		if len(f.pending) > 0 {
			n := copy(p, f.pending)
			f.pending = f.pending[n:]
			return n, nil
		}
		n, err := file.Read(f.tmp[:])
		if n > 0 {
			f.pending = append(f.pending, f.late.push(f.tmp[:n])...)
		}
		if len(f.pending) > 0 {
			continue
		}
		if err != nil {
			return 0, err
		}
		if n == 0 {
			return 0, io.EOF
		}
	}
}

// bufferedProbeInput serves replayed keystrokes before the descriptor and
// drops a late probe reply. Name and Fd keep it a cancellable terminal file
// so Bubble Tea can interrupt a blocked read. Close leaves the real stdin open.
type bufferedProbeInput struct {
	f *os.File
	*probeFilter
}

func (b *bufferedProbeInput) Read(p []byte) (int, error) {
	return b.probeFilter.read(b.f, p)
}

func (b *bufferedProbeInput) Write(p []byte) (int, error) { return b.f.Write(p) }

func (b *bufferedProbeInput) Fd() uintptr { return b.f.Fd() }

func (b *bufferedProbeInput) Name() string { return b.f.Name() }

func (b *bufferedProbeInput) Close() error { return nil }

const (
	lateGround = iota
	lateAPC
	lateDCS
	lateCSI
)

// lateFilter drops one graphics APC, one DA1, and one XTVERSION that arrive
// after a probe timeout. A lone ESC at the end of a read is returned in that
// read, so Esc is not held until the next key.
type lateFilter struct {
	dropAPC bool
	dropDA1 bool
	dropXT  bool
	state   int
	buf     []byte
}

func (f *lateFilter) armed() bool {
	return f.dropAPC || f.dropDA1 || f.dropXT || f.state != lateGround
}

func (f *lateFilter) seed(partial []byte) {
	f.buf = append([]byte(nil), partial...)
	switch {
	case bytes.HasPrefix(partial, []byte("\x1b_")):
		f.state = lateAPC
		f.dropAPC = false
	case bytes.HasPrefix(partial, []byte("\x1bP")):
		f.state = lateDCS
		f.dropXT = false
	default:
		f.buf = nil
		f.state = lateGround
	}
}

func (f *lateFilter) push(in []byte) []byte {
	if f.state == lateGround && !f.armed() {
		return in
	}
	if f.state == lateGround && bytes.IndexByte(in, 0x1b) < 0 {
		return in
	}
	out := make([]byte, 0, len(in))
	for i := 0; i < len(in); i++ {
		b := in[i]
		switch f.state {
		case lateGround:
			if b != 0x1b || i+1 >= len(in) {
				// A trailing ESC is delivered with this read. Holding it
				// would glue it to the next key and turn Esc into Alt.
				out = append(out, b)
				continue
			}
			next := in[i+1]
			switch {
			case next == '_' && f.dropAPC:
				f.buf = append(f.buf[:0], b, next)
				f.state = lateAPC
				i++
			case next == 'P' && f.dropXT:
				f.buf = append(f.buf[:0], b, next)
				f.state = lateDCS
				i++
			case next == '[' && f.dropDA1:
				f.buf = append(f.buf[:0], b, next)
				f.state = lateCSI
				i++
			default:
				out = append(out, b)
			}
		case lateAPC, lateDCS:
			f.buf = append(f.buf, b)
			if len(f.buf) >= 2 && f.buf[len(f.buf)-2] == 0x1b && f.buf[len(f.buf)-1] == '\\' {
				if f.state == lateAPC {
					f.dropAPC = false
				} else {
					f.dropXT = false
				}
				f.buf = f.buf[:0]
				f.state = lateGround
				continue
			}
			if len(f.buf) > 8192 {
				out = append(out, f.buf...)
				f.buf = f.buf[:0]
				f.state = lateGround
				f.dropAPC = false
				f.dropXT = false
			}
		case lateCSI:
			f.buf = append(f.buf, b)
			if b >= 0x40 && b <= 0x7e {
				if b == 'c' {
					f.dropDA1 = false
				} else {
					out = append(out, f.buf...)
				}
				f.buf = f.buf[:0]
				f.state = lateGround
				continue
			}
			if b < 0x20 || b > 0x3f {
				out = append(out, f.buf...)
				f.buf = f.buf[:0]
				f.state = lateGround
			}
		}
	}
	return out
}
