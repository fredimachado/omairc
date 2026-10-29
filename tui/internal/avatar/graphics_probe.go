package avatar

import (
	"bytes"
	"io"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/charmbracelet/x/term"
)

// deviceAttributesQuery is DA1 (ESC [ c). Terminals answer it, so the probe
// can stop when it arrives instead of holding the timeout open. The graphics
// query is sent first; a DA1 with no matching OK means graphics are off.
const deviceAttributesQuery = "\x1b[c"

// probeResult is what one graphics probe learned. replay is keystrokes typed
// while the probe was reading. dropLate is set only when the wait ends on the
// timeout without a DA1, and arms a filter for one late graphics reply, DA1
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
// response budget. A graphics OK does not stop the wait: DA1 is the sentinel
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
	return bytes.HasPrefix(b, []byte("\x1b_G")) || bytes.HasPrefix(b, []byte("\x1bP>|"))
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
// r must honor SetReadDeadline. Real terminals reject that deadline
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
	// Replay lives on a pipe, not in a userspace buffer. cancelreader waits
	// until Fd is readable before it calls Read, so the bytes have to be in
	// the kernel buffer or the first key waits for another keystroke.
	pr, pw, err := os.Pipe()
	if err != nil {
		return nil
	}
	late := &lateFilter{}
	if r.dropLate {
		late.dropAPC = !r.sawAPC && !bytes.HasPrefix(r.partial, []byte("\x1b_G"))
		late.dropDA1 = true
		late.dropXT = !r.sawXT && !bytes.HasPrefix(r.partial, []byte("\x1bP>|"))
	}
	if len(r.partial) > 0 {
		late.seed(r.partial)
	}
	// The replay is in the kernel buffer before wrap returns, so a cancel
	// reader that waits for the fd to be readable does not block.
	if err := writeAll(pw, r.replay); err != nil {
		_ = pr.Close()
		_ = pw.Close()
		return nil
	}
	wakeR, wakeW, err := os.Pipe()
	if err != nil {
		_ = pr.Close()
		_ = pw.Close()
		return nil
	}
	held := &bufferedProbeInput{f: pr, src: in, wake: wakeW}
	held.copyWG.Add(1)
	go func() {
		defer held.copyWG.Done()
		copyProbeInput(in, pw, wakeR, late)
	}()
	return held
}

func writeAll(w io.Writer, p []byte) error {
	for len(p) > 0 {
		n, err := w.Write(p)
		if n > 0 {
			p = p[n:]
		}
		if err != nil {
			return err
		}
		if n == 0 {
			return io.ErrShortWrite
		}
	}
	return nil
}

func filterCopyChunk(dst *os.File, late *lateFilter, chunk []byte) error {
	if len(chunk) == 0 {
		return nil
	}
	return writeAll(dst, late.push(chunk))
}

// bufferedProbeInput is the pipe end that carries replayed keys and filtered
// input. Name and Fd refer to that pipe, so a cancel reader sees the replay
// as readable. The pipe is not a terminal: Bubble Tea leaves newline mapping
// on, and holdTerminal keeps ONLCR on the real tty so a newline still returns
// to column zero. Write goes to the real terminal. Close restores it and does
// not close it.
type bufferedProbeInput struct {
	f        *os.File
	src      *os.File
	wake     *os.File
	copyWG   sync.WaitGroup
	once     sync.Once
	mu       sync.Mutex
	fd       uintptr
	orig     *term.State
	done     chan struct{}
	watching chan struct{}
	stopped  bool
}

func (b *bufferedProbeInput) Read(p []byte) (int, error) { return b.f.Read(p) }

func (b *bufferedProbeInput) Write(p []byte) (int, error) { return b.src.Write(p) }

func (b *bufferedProbeInput) Fd() uintptr { return b.f.Fd() }

func (b *bufferedProbeInput) Name() string { return b.f.Name() }

func (b *bufferedProbeInput) Close() error {
	b.once.Do(func() {
		b.finishTerminal()
		if b.wake != nil {
			_ = b.wake.Close()
			b.wake = nil
		}
		b.copyWG.Wait()
		_ = b.f.Close()
	})
	return nil
}

func (b *bufferedProbeInput) finishTerminal() {
	b.mu.Lock()
	b.stopped = true
	orig, fd := b.orig, b.fd
	done := b.done
	b.mu.Unlock()
	if orig != nil {
		_ = term.Restore(fd, orig)
	}
	if done != nil {
		close(done)
	}
}

const (
	lateGround = iota
	lateAPC
	lateDCS
	lateCSI
)

// lateHoldCap is how many bytes of an unfinished reply the filter will keep.
// A longer run is input, not a probe response.
const lateHoldCap = 64

// lateFilter drops one graphics APC, one DA1 (`ESC [ ?` … `c`), and one
// XTVERSION that arrive after a probe timeout. A lone ESC is returned in the
// read that contained it. If that ESC was the first byte of a reply, the next
// read's `[?...c`, `P>|...`, or `_G...` continuation is dropped. A bare `_`
// or `P` after Esc is ordinary input: holding on those alone would later
// re-send the already-delivered Esc when the unfinished hold flushed.
type lateFilter struct {
	dropAPC        bool
	dropDA1        bool
	dropXT         bool
	state          int
	buf            []byte
	sawEsc         bool
	heldBracket    bool
	heldUnderscore bool
	heldP          bool
	heldPGT        bool
}

func (f *lateFilter) armed() bool {
	return f.dropAPC || f.dropDA1 || f.dropXT || f.state != lateGround ||
		f.heldBracket || f.heldUnderscore || f.heldP || f.heldPGT
}

func (f *lateFilter) seed(partial []byte) {
	f.buf = append([]byte(nil), partial...)
	switch {
	case bytes.HasPrefix(partial, []byte("\x1b_G")):
		f.state = lateAPC
		f.dropAPC = false
	case bytes.HasPrefix(partial, []byte("\x1bP>|")):
		f.state = lateDCS
		f.dropXT = false
	default:
		f.buf = nil
		f.state = lateGround
	}
}

func (f *lateFilter) push(in []byte) []byte {
	if len(in) == 0 {
		return in
	}
	if f.state == lateGround && !f.dropAPC && !f.dropDA1 && !f.dropXT && !f.sawEsc &&
		!f.heldBracket && !f.heldUnderscore && !f.heldP && !f.heldPGT {
		return in
	}
	out := make([]byte, 0, len(in))
	if f.state == lateGround && f.sawEsc {
		f.sawEsc = false
		in = f.takeEscContinuation(in)
	}
	if f.state == lateGround && f.heldBracket && len(in) > 0 {
		in = f.takeHeldBracket(in, &out)
	}
	if f.state == lateGround && f.heldUnderscore && len(in) > 0 {
		in = f.takeHeldUnderscore(in, &out)
	}
	if f.state == lateGround && f.heldP && len(in) > 0 {
		in = f.takeHeldP(in, &out)
	}
	if f.state == lateGround && f.heldPGT && len(in) > 0 {
		in = f.takeHeldPGT(in, &out)
	}
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
			case next == '_' && f.dropAPC && i+2 < len(in) && in[i+2] == 'G':
				f.buf = append(f.buf[:0], 0x1b, '_', 'G')
				f.state = lateAPC
				i += 2
			case next == '_' && f.dropAPC && i+2 >= len(in):
				out = append(out, 0x1b)
				f.heldUnderscore = true
				i++
			case next == 'P' && f.dropXT && i+3 < len(in) && in[i+2] == '>' && in[i+3] == '|':
				f.buf = append(f.buf[:0], 0x1b, 'P', '>', '|')
				f.state = lateDCS
				i += 3
			case next == 'P' && f.dropXT && i+2 >= len(in):
				out = append(out, 0x1b)
				f.heldP = true
				i++
			case next == 'P' && f.dropXT && i+2 < len(in) && in[i+2] == '>' && i+3 >= len(in):
				out = append(out, 0x1b)
				f.heldPGT = true
				i += 2
			case next == '[' && f.dropDA1 && i+2 < len(in) && in[i+2] == '?':
				f.buf = append(f.buf[:0], 0x1b, '[', '?')
				f.state = lateCSI
				i += 2
			case next == '[' && f.dropDA1 && i+2 >= len(in):
				// ESC [ at the end of the read. Deliver ESC now and remember
				// the bracket so a later `?...c` is still a DA1.
				out = append(out, 0x1b)
				f.heldBracket = true
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
			f.flushHeld(&out)
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
				continue
			}
			f.flushHeld(&out)
		}
	}
	if f.heldBracket || f.heldUnderscore || f.heldP || f.heldPGT {
		f.sawEsc = false
	} else if f.state == lateGround && len(out) > 0 {
		f.sawEsc = out[len(out)-1] == 0x1b
	} else if len(out) > 0 {
		f.sawEsc = false
	}
	return out
}

// takeEscContinuation drops the rest of a reply whose ESC was delivered at
// the end of the previous read. `[?...c`, `P>|...`, and `_G...` match.
// Anything else is returned for normal delivery. The held buf omits the
// already-delivered ESC so a later flush cannot re-send it.
func (f *lateFilter) takeEscContinuation(in []byte) []byte {
	if len(in) == 0 {
		return in
	}
	switch in[0] {
	case '_':
		if !f.dropAPC {
			return in
		}
		if len(in) == 1 {
			f.heldUnderscore = true
			return nil
		}
		if in[1] == 'G' {
			f.buf = append(f.buf[:0], '_', 'G')
			f.state = lateAPC
			return in[2:]
		}
		return in
	case 'P':
		if !f.dropXT {
			return in
		}
		if len(in) == 1 {
			f.heldP = true
			return nil
		}
		if in[1] != '>' {
			return in
		}
		if len(in) == 2 {
			f.heldPGT = true
			return nil
		}
		if in[2] == '|' {
			f.buf = append(f.buf[:0], 'P', '>', '|')
			f.state = lateDCS
			return in[3:]
		}
		return in
	case '[':
		if !f.dropDA1 {
			return in
		}
		if len(in) == 1 {
			f.heldBracket = true
			return nil
		}
		if in[1] == '?' {
			f.buf = append(f.buf[:0], 0x1b, '[', '?')
			f.state = lateCSI
			return in[2:]
		}
		return in
	default:
		return in
	}
}

// takeHeldBracket finishes a `[` that followed a delivered ESC. `?` continues
// a DA1. Any other byte emits the bracket and is parsed with the rest.
func (f *lateFilter) takeHeldBracket(in []byte, out *[]byte) []byte {
	f.heldBracket = false
	if len(in) > 0 && in[0] == '?' && f.dropDA1 {
		f.buf = append(f.buf[:0], 0x1b, '[', '?')
		f.state = lateCSI
		return in[1:]
	}
	*out = append(*out, '[')
	return in
}

// takeHeldUnderscore finishes a `_` that followed a delivered ESC. `G` starts
// a graphics APC hold without re-adding Esc. Anything else emits the
// underscore as input.
func (f *lateFilter) takeHeldUnderscore(in []byte, out *[]byte) []byte {
	f.heldUnderscore = false
	if len(in) > 0 && in[0] == 'G' && f.dropAPC {
		f.buf = append(f.buf[:0], '_', 'G')
		f.state = lateAPC
		return in[1:]
	}
	*out = append(*out, '_')
	return in
}

// takeHeldP finishes a `P` that followed a delivered ESC. `>` waits for `|`;
// anything else emits P as input.
func (f *lateFilter) takeHeldP(in []byte, out *[]byte) []byte {
	f.heldP = false
	if len(in) > 0 && in[0] == '>' && f.dropXT {
		if len(in) == 1 {
			f.heldPGT = true
			return nil
		}
		if in[1] == '|' {
			f.buf = append(f.buf[:0], 'P', '>', '|')
			f.state = lateDCS
			return in[2:]
		}
		*out = append(*out, 'P')
		return in
	}
	*out = append(*out, 'P')
	return in
}

// takeHeldPGT finishes a `P>` that followed a delivered ESC. `|` starts an
// XTVERSION hold without re-adding Esc.
func (f *lateFilter) takeHeldPGT(in []byte, out *[]byte) []byte {
	f.heldPGT = false
	if len(in) > 0 && in[0] == '|' && f.dropXT {
		f.buf = append(f.buf[:0], 'P', '>', '|')
		f.state = lateDCS
		return in[1:]
	}
	*out = append(*out, 'P', '>')
	return in
}

// flushHeld emits an unfinished reply once it is too long to be one, and
// returns to ground so the following bytes are input.
func (f *lateFilter) flushHeld(out *[]byte) {
	if len(f.buf) <= lateHoldCap {
		return
	}
	*out = append(*out, f.buf...)
	f.buf = f.buf[:0]
	f.state = lateGround
}
