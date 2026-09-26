// Package gate drives the compiled omairc-tui binary through a PTY.
//
// It owns three pieces:
//
//   - Screen, a small VT100/xterm emulator that reconstructs a width x height
//     cell grid from the bytes the child writes, plus the OSC window title.
//   - KeySequence, the chord-name to terminal-byte table the recipes use.
//   - Process and the daemon/client in driver.go, which keep one live child
//     across separate CLI invocations.
//
// The package deliberately stays free of internal/irc and internal/ui so the
// gate never reaches into the product's model. It only speaks bytes and cells.
package gate

import (
	"bytes"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"strings"
	"unicode/utf8"

	"github.com/mattn/go-runewidth"
)

// Attrs is the subset of SGR state the gate tracks per cell.
//
// Text correctness is the contract; the attributes only exist so PNG()
// can render colored blocks instead of a wall of uniform cells. Fg and Bg
// are ANSI color indices, or -1 for the terminal default.
type Attrs struct {
	Bold      bool
	Faint     bool
	Underline bool
	Reverse   bool
	Fg        int
	Bg        int
}

func defaultAttrs() Attrs { return Attrs{Fg: -1, Bg: -1} }

// Cell is one screen position. Cont marks the second half of a double-width
// rune so rendering does not draw a stray placeholder glyph.
type Cell struct {
	Rune  rune
	Attrs Attrs
	Wide  bool
	Cont  bool
}

// Screen is a minimal VT emulator. It understands the escape sequences a
// full-screen TUIs emits: cursor movement, absolute positioning, erase,
// line/character insert and delete, SGR, private modes (alt screen clears),
// and OSC window titles. Unknown CSI/ESC sequences are consumed and ignored,
// which is the safe default for a driver that only needs text and title.
//
// Screen is not safe for concurrent use; the daemon serializes access.
type Screen struct {
	width, height int
	cells         []Cell // width*height, row-major

	curX, curY     int
	savedX, savedY int

	// wrapPending implements the VT "deferred wrap": printing in the last
	// column parks the cursor there and only the next printable rune wraps.
	// Without it a full-width line would scroll the screen on its last rune.
	wrapPending bool

	title string

	// pending holds bytes that form an incomplete UTF-8 rune or escape
	// sequence so Feed can be called with arbitrary chunk boundaries.
	pending []byte

	pen Attrs // current SGR attributes

	lastRune rune // last printed rune, for CSI b (repeat)
	hasLast  bool
}

// NewScreen returns a blank width x height screen with a hidden cursor at home.
func NewScreen(width, height int) *Screen {
	if width < 0 {
		width = 0
	}
	if height < 0 {
		height = 0
	}
	s := &Screen{width: width, height: height, pen: defaultAttrs(), savedX: -1, savedY: -1}
	s.cells = make([]Cell, width*height)
	s.resetCells()
	return s
}

// Width and Height report the current grid size.
func (s *Screen) Width() int  { return s.width }
func (s *Screen) Height() int { return s.height }

// Title returns the last OSC 0/1/2 payload seen, or "" when the child has not
// set a window title yet.
func (s *Screen) Title() string { return s.title }

// resetCells blanks every cell with default attributes.
func (s *Screen) resetCells() {
	for i := range s.cells {
		s.cells[i] = Cell{Attrs: defaultAttrs()}
	}
}

// blank is the cell used by erase operations: a space that keeps the current
// background color, as real terminals do.
func (s *Screen) blank() Cell {
	return Cell{Rune: ' ', Attrs: Attrs{Fg: -1, Bg: s.pen.Bg}}
}

// Resize changes the grid size in place, preserving the top-left overlap that
// still fits. The cursor is clamped into the new bounds.
func (s *Screen) Resize(width, height int) {
	if width < 0 {
		width = 0
	}
	if height < 0 {
		height = 0
	}
	next := make([]Cell, width*height)
	for i := range next {
		next[i] = Cell{Attrs: defaultAttrs()}
	}
	for y := 0; y < height && y < s.height; y++ {
		for x := 0; x < width && x < s.width; x++ {
			next[y*width+x] = s.cells[y*s.width+x]
		}
	}
	s.width, s.height = width, height
	s.cells = next
	s.clampCursor()
	s.wrapPending = false
}

func (s *Screen) clampCursor() {
	if s.curX < 0 {
		s.curX = 0
	}
	if s.curY < 0 {
		s.curY = 0
	}
	if s.width == 0 {
		s.curX = 0
	} else if s.curX > s.width-1 {
		s.curX = s.width - 1
	}
	if s.height == 0 {
		s.curY = 0
	} else if s.curY > s.height-1 {
		s.curY = s.height - 1
	}
}

// Feed consumes a chunk of child output. It is incremental and UTF-8 safe:
// a rune or escape sequence split across calls stays in pending until the
// rest arrives.
func (s *Screen) Feed(p []byte) {
	if len(p) > 0 {
		s.pending = append(s.pending, p...)
	}
	i := 0
	for i < len(s.pending) {
		n, ok := s.step(s.pending[i:])
		if !ok {
			break
		}
		if n <= 0 {
			// Defensive: never spin on a parser that consumed nothing.
			n = 1
		}
		i += n
	}
	if i > 0 {
		copy(s.pending, s.pending[i:])
		s.pending = s.pending[:len(s.pending)-i]
	}
}

// step parses one token. ok=false means "need more bytes"; the caller keeps
// the whole token buffered.
func (s *Screen) step(b []byte) (int, bool) {
	if len(b) == 0 {
		return 0, false
	}
	switch {
	case b[0] == 0x1b:
		return s.parseEsc(b)
	case b[0] == 0x00:
		return 1, true
	case b[0] < 0x20 || b[0] == 0x7f:
		s.applyControl(b[0])
		return 1, true
	default:
		if !utf8.FullRune(b) {
			return 0, false
		}
		r, size := utf8.DecodeRune(b)
		if r == utf8.RuneError && size <= 1 {
			// Invalid byte: drop it so one bad byte cannot wedge the feed.
			return 1, true
		}
		s.putRune(r)
		return size, true
	}
}

// applyControl handles C0 control bytes other than ESC.
func (s *Screen) applyControl(ch byte) {
	s.wrapPending = false
	switch ch {
	case 0x07: // BEL: used as an OSC terminator, never a cell
	case 0x08: // BS
		if s.curX > 0 {
			s.curX--
		}
	case 0x09: // HT: next multiple-of-8 tab stop
		next := (s.curX/8 + 1) * 8
		if next >= s.width {
			next = s.width - 1
		}
		if next < 0 {
			next = 0
		}
		s.curX = next
	case 0x0a, 0x0b, 0x0c: // LF, VT, FF
		s.lineFeed()
	case 0x0d: // CR
		s.curX = 0
	}
}

// parseEsc handles a sequence that starts with ESC. It returns the number of
// bytes consumed and whether the token was complete.
func (s *Screen) parseEsc(b []byte) (int, bool) {
	if len(b) < 2 {
		return 0, false
	}
	switch b[1] {
	case '[':
		n, ok := s.parseCSI(b[2:])
		if !ok {
			return 0, false
		}
		return 2 + n, true
	case ']':
		payload, n, ok := parseOSC(b[2:])
		if !ok {
			return 0, false
		}
		s.applyOSC(payload)
		return 2 + n, true
	case 'P', 'X', '^', '_':
		// DCS / SOS / PM / APC string: skip through ST.
		n, ok := parseStringUntilST(b[2:])
		if !ok {
			return 0, false
		}
		return 2 + n, true
	case '7':
		s.savedX, s.savedY = s.curX, s.curY
		return 2, true
	case '8':
		s.restoreCursor()
		return 2, true
	case 'D': // IND, index down
		s.lineFeed()
		return 2, true
	case 'M': // RI, reverse index
		s.reverseIndex()
		return 2, true
	case 'E': // NEL, next line
		s.curX = 0
		s.lineFeed()
		return 2, true
	case '(', ')', '#':
		// Character-set designators take one more byte (ESC ( B, ESC # 8).
		if len(b) < 3 {
			return 0, false
		}
		return 3, true
	case 'c': // RIS: full reset
		s.resetCells()
		s.curX, s.curY = 0, 0
		s.pen = defaultAttrs()
		s.wrapPending = false
		return 2, true
	case '=', '>', '\\', 'N', 'O':
		// Keypad modes and ST itself are two bytes and carry no state.
		return 2, true
	default:
		return 2, true
	}
}

// parseCSI scans one CSI sequence after "ESC [". ok=false means incomplete.
// A control byte inside the sequence aborts it: the bytes before the control
// are consumed, the control is left for the next step.
func (s *Screen) parseCSI(b []byte) (int, bool) {
	for i := 0; i < len(b); i++ {
		ch := b[i]
		switch {
		case ch >= 0x40 && ch <= 0x7e:
			s.dispatchCSI(ch, b[:i])
			return i + 1, true
		case ch >= 0x20 && ch <= 0x3f:
			// Parameter or intermediate byte: keep scanning.
		default:
			return i, true
		}
	}
	return 0, false
}

func (s *Screen) dispatchCSI(final byte, raw []byte) {
	private := byte(0)
	if len(raw) > 0 && (raw[0] == '?' || raw[0] == '<' || raw[0] == '=' || raw[0] == '>') {
		private = raw[0]
		raw = raw[1:]
	}
	p := parseCSIParams(raw)

	switch final {
	case 'A':
		s.moveCursor(0, -csiNum(p, 0, 1))
	case 'B', 'e':
		s.moveCursor(0, csiNum(p, 0, 1))
	case 'C', 'a':
		s.moveCursor(csiNum(p, 0, 1), 0)
	case 'D':
		s.moveCursor(-csiNum(p, 0, 1), 0)
	case 'E':
		s.curX = 0
		s.moveCursor(0, csiNum(p, 0, 1))
	case 'F':
		s.curX = 0
		s.moveCursor(0, -csiNum(p, 0, 1))
	case 'G', '`':
		s.wrapPending = false
		s.curX = csiNum(p, 0, 1) - 1
		s.clampCursor()
	case 'H', 'f':
		s.wrapPending = false
		s.curY = csiNum(p, 0, 1) - 1
		s.curX = csiNum(p, 1, 1) - 1
		s.clampCursor()
	case 'd':
		s.wrapPending = false
		s.curY = csiNum(p, 0, 1) - 1
		s.clampCursor()
	case 'm':
		s.applySGR(p)
	case 'J':
		s.eraseInDisplay(csiMode(p, 0))
	case 'K':
		s.eraseInLine(csiMode(p, 0))
	case 'L':
		s.insertLines(csiNum(p, 0, 1))
	case 'M':
		s.deleteLines(csiNum(p, 0, 1))
	case 'P':
		s.deleteChars(csiNum(p, 0, 1))
	case '@':
		s.insertChars(csiNum(p, 0, 1))
	case 'X':
		s.eraseChars(csiNum(p, 0, 1))
	case 'S':
		s.scrollUp(csiNum(p, 0, 1))
	case 'T':
		s.scrollDown(csiNum(p, 0, 1))
	case 'Z':
		n := csiNum(p, 0, 1)
		s.curX -= 8 * n
		s.clampCursor()
	case 'I':
		n := csiNum(p, 0, 1)
		for i := 0; i < n; i++ {
			s.applyControl(0x09)
		}
	case 'b':
		n := csiNum(p, 0, 1)
		if s.hasLast {
			for i := 0; i < n; i++ {
				s.putRune(s.lastRune)
			}
		}
	case 's':
		s.savedX, s.savedY = s.curX, s.curY
	case 'u':
		s.restoreCursor()
	case 'h', 'l':
		s.applyMode(private, p, final == 'h')
	default:
		// Unknown final byte: consume and ignore.
	}
}

// applyMode handles private DEC modes. Only the alt-screen modes are acted on
// (entering one clears the grid); cursor visibility, mouse, bracketed paste,
// and the rest are ignored because they carry no cell state.
func (s *Screen) applyMode(private byte, p []int, set bool) {
	if private != '?' || !set {
		return
	}
	for _, mode := range p {
		switch mode {
		case 47, 1047, 1049:
			s.clearGrid()
		}
	}
}

// clearGrid blanks the screen and homes the cursor, as entering the alternate
// screen does.
func (s *Screen) clearGrid() {
	s.resetCells()
	s.curX, s.curY = 0, 0
	s.pen = defaultAttrs()
	s.wrapPending = false
}

// applyOSC records the window title for selectors 0 (icon + title), 1 (icon),
// and 2 (title). Everything else (hyperlinks and friends) is ignored.
func (s *Screen) applyOSC(payload []byte) {
	semi := bytes.IndexByte(payload, ';')
	if semi < 0 {
		return
	}
	selector := string(payload[:semi])
	if selector != "0" && selector != "1" && selector != "2" {
		return
	}
	s.title = string(payload[semi+1:])
}

// parseOSC scans an OSC body after "ESC ]" until BEL or ST.
func parseOSC(b []byte) (payload []byte, n int, ok bool) {
	for i := 0; i < len(b); i++ {
		switch b[i] {
		case 0x07:
			return b[:i], i + 1, true
		case 0x1b:
			if i+1 >= len(b) {
				return nil, 0, false
			}
			if b[i+1] == '\\' {
				return b[:i], i + 2, true
			}
			// A lone ESC aborts the string; let parseEsc handle it next.
			return b[:i], i, true
		}
	}
	return nil, 0, false
}

// parseStringUntilST skips a DCS/APC/PM/SOS body until ST (ESC \).
func parseStringUntilST(b []byte) (int, bool) {
	for i := 0; i < len(b); i++ {
		if b[i] != 0x1b {
			continue
		}
		if i+1 >= len(b) {
			return 0, false
		}
		if b[i+1] == '\\' {
			return i + 2, true
		}
	}
	return 0, false
}

func parseCSIParams(raw []byte) []int {
	if len(raw) == 0 {
		return nil
	}
	parts := strings.Split(string(raw), ";")
	out := make([]int, 0, len(parts))
	for _, part := range parts {
		n := 0
		valid := part != ""
		for i := 0; i < len(part); i++ {
			if part[i] < '0' || part[i] > '9' {
				valid = false
				break
			}
			n = n*10 + int(part[i]-'0')
		}
		if !valid {
			n = 0
		}
		out = append(out, n)
	}
	return out
}

// csiNum returns parameter i, treating a missing or zero value as def.
func csiNum(p []int, i, def int) int {
	if i >= len(p) || p[i] == 0 {
		return def
	}
	return p[i]
}

// csiMode returns parameter i with 0 as the default (erase commands).
func csiMode(p []int, i int) int {
	if i >= len(p) {
		return 0
	}
	return p[i]
}

func (s *Screen) moveCursor(dx, dy int) {
	s.wrapPending = false
	s.curX += dx
	s.curY += dy
	s.clampCursor()
}

func (s *Screen) putRune(r rune) {
	s.lastRune, s.hasLast = r, true
	w := runewidth.RuneWidth(r)
	if w <= 0 {
		// Combining mark: no cell of its own, and we do not join it to the
		// previous cell. Text correctness for the recipes never needs it.
		return
	}
	if s.width == 0 || s.height == 0 {
		return
	}
	// Deferred wrap: only a printable rune triggers the wrap that the last
	// column asked for.
	if s.wrapPending {
		s.wrapPending = false
		s.curX = 0
		s.lineFeed()
	}
	if w == 2 && s.curX >= s.width-1 {
		// No room for a double-width rune at the last column: wrap first.
		s.curX = 0
		s.lineFeed()
	}
	idx := s.curY*s.width + s.curX
	s.cells[idx] = Cell{Rune: r, Attrs: s.pen, Wide: w == 2}
	if w == 2 && s.curX+1 < s.width {
		s.cells[idx+1] = Cell{Attrs: s.pen, Cont: true}
	}
	if s.curX+w >= s.width {
		s.curX = s.width - 1
		s.wrapPending = true
	} else {
		s.curX += w
	}
}

func (s *Screen) lineFeed() {
	if s.height == 0 {
		return
	}
	s.curY++
	if s.curY >= s.height {
		s.scrollUp(1)
		s.curY = s.height - 1
	}
}

func (s *Screen) reverseIndex() {
	if s.height == 0 {
		return
	}
	s.curY--
	if s.curY < 0 {
		s.scrollDown(1)
		s.curY = 0
	}
}

func (s *Screen) restoreCursor() {
	s.wrapPending = false
	if s.savedX >= 0 {
		s.curX = s.savedX
	}
	if s.savedY >= 0 {
		s.curY = s.savedY
	}
	s.clampCursor()
}

func (s *Screen) row(y int) []Cell {
	return s.cells[y*s.width : (y+1)*s.width]
}

func (s *Screen) blankRow(y int) {
	row := s.row(y)
	for i := range row {
		row[i] = s.blank()
	}
}

func (s *Screen) scrollUp(n int) {
	if n <= 0 || s.width == 0 || s.height == 0 {
		return
	}
	if n > s.height {
		n = s.height
	}
	copy(s.cells, s.cells[n*s.width:])
	for i := (s.height - n) * s.width; i < len(s.cells); i++ {
		s.cells[i] = s.blank()
	}
}

func (s *Screen) scrollDown(n int) {
	if n <= 0 || s.width == 0 || s.height == 0 {
		return
	}
	if n > s.height {
		n = s.height
	}
	copy(s.cells[n*s.width:], s.cells[:(s.height-n)*s.width])
	for i := 0; i < n*s.width; i++ {
		s.cells[i] = s.blank()
	}
}

func (s *Screen) insertLines(n int) {
	if n <= 0 || s.width == 0 || s.height == 0 || s.curY >= s.height {
		return
	}
	if n > s.height-s.curY {
		n = s.height - s.curY
	}
	for y := s.height - 1; y >= s.curY+n; y-- {
		copy(s.row(y), s.row(y-n))
	}
	for y := s.curY; y < s.curY+n; y++ {
		s.blankRow(y)
	}
}

func (s *Screen) deleteLines(n int) {
	if n <= 0 || s.width == 0 || s.height == 0 || s.curY >= s.height {
		return
	}
	if n > s.height-s.curY {
		n = s.height - s.curY
	}
	for y := s.curY; y+n < s.height; y++ {
		copy(s.row(y), s.row(y+n))
	}
	for y := s.height - n; y < s.height; y++ {
		s.blankRow(y)
	}
}

func (s *Screen) insertChars(n int) {
	if n <= 0 || s.width == 0 || s.height == 0 || s.curY >= s.height {
		return
	}
	row := s.row(s.curY)
	if n > s.width-s.curX {
		n = s.width - s.curX
	}
	copy(row[s.curX+n:], row[s.curX:])
	for i := s.curX; i < s.curX+n; i++ {
		row[i] = s.blank()
	}
}

func (s *Screen) deleteChars(n int) {
	if n <= 0 || s.width == 0 || s.height == 0 || s.curY >= s.height {
		return
	}
	row := s.row(s.curY)
	if n > s.width-s.curX {
		n = s.width - s.curX
	}
	copy(row[s.curX:], row[s.curX+n:])
	for i := s.width - n; i < s.width; i++ {
		row[i] = s.blank()
	}
}

func (s *Screen) eraseChars(n int) {
	if n <= 0 || s.width == 0 || s.height == 0 || s.curY >= s.height {
		return
	}
	row := s.row(s.curY)
	end := s.curX + n
	if end > s.width {
		end = s.width
	}
	for i := s.curX; i < end; i++ {
		row[i] = s.blank()
	}
}

func (s *Screen) eraseInLine(mode int) {
	if s.width == 0 || s.height == 0 || s.curY >= s.height {
		return
	}
	row := s.row(s.curY)
	from, to := 0, s.width
	switch mode {
	case 0:
		from = s.curX
	case 1:
		to = s.curX + 1 // inclusive of the cursor per VT510
	case 2:
		// whole line
	default:
		return
	}
	if from < 0 {
		from = 0
	}
	if to > s.width {
		to = s.width
	}
	for i := from; i < to; i++ {
		row[i] = s.blank()
	}
}

func (s *Screen) eraseInDisplay(mode int) {
	if s.width == 0 || s.height == 0 {
		return
	}
	switch mode {
	case 0:
		start := s.curY*s.width + s.curX
		for i := start; i < len(s.cells); i++ {
			s.cells[i] = s.blank()
		}
	case 1:
		end := s.curY*s.width + s.curX
		if end >= len(s.cells) {
			end = len(s.cells) - 1
		}
		for i := 0; i <= end; i++ {
			s.cells[i] = s.blank()
		}
	case 2, 3:
		for i := range s.cells {
			s.cells[i] = s.blank()
		}
	}
}

func (s *Screen) applySGR(p []int) {
	if len(p) == 0 {
		s.pen = defaultAttrs()
		return
	}
	for i := 0; i < len(p); i++ {
		switch n := p[i]; {
		case n == 0:
			s.pen = defaultAttrs()
		case n == 1:
			s.pen.Bold = true
		case n == 2:
			s.pen.Faint = true
		case n == 4:
			s.pen.Underline = true
		case n == 7:
			s.pen.Reverse = true
		case n == 22:
			s.pen.Bold, s.pen.Faint = false, false
		case n == 24:
			s.pen.Underline = false
		case n == 27:
			s.pen.Reverse = false
		case n >= 30 && n <= 37:
			s.pen.Fg = n - 30
		case n == 39:
			s.pen.Fg = -1
		case n >= 40 && n <= 47:
			s.pen.Bg = n - 40
		case n == 49:
			s.pen.Bg = -1
		case n >= 90 && n <= 97:
			s.pen.Fg = n - 90 + 8
		case n >= 100 && n <= 107:
			s.pen.Bg = n - 100 + 8
		case n == 38 || n == 48:
			if i+1 < len(p) && p[i+1] == 5 && i+2 < len(p) {
				if n == 38 {
					s.pen.Fg = p[i+2]
				} else {
					s.pen.Bg = p[i+2]
				}
				i += 2
			} else if i+1 < len(p) && p[i+1] == 2 && i+4 < len(p) {
				idx := rgbTo256(p[i+2], p[i+3], p[i+4])
				if n == 38 {
					s.pen.Fg = idx
				} else {
					s.pen.Bg = idx
				}
				i += 4
			}
		}
	}
}

// Lines returns the grid top to bottom with each row's trailing blanks
// trimmed. Continuation cells of a double-width rune contribute nothing.
func (s *Screen) Lines() []string {
	lines := make([]string, 0, s.height)
	for y := 0; y < s.height; y++ {
		var b strings.Builder
		for x := 0; x < s.width; x++ {
			c := s.cells[y*s.width+x]
			if c.Cont {
				continue
			}
			r := c.Rune
			if r == 0 {
				r = ' '
			}
			b.WriteRune(r)
		}
		lines = append(lines, strings.TrimRight(b.String(), " "))
	}
	return lines
}

// Text joins Lines with newlines. It is the "screenshot as text" used by the
// smoke test and the daemon's text/status responses.
func (s *Screen) Text() string { return strings.Join(s.Lines(), "\n") }

// PNG rendering: a classic 5x7 bitmap font, scaled up so a human can read the
// frame. Each grid cell becomes pngCellWidth x pngCellHeight pixels; the glyph
// is painted at pngGlyphScale so a normal cell shows strokes, not a solid
// block. It stays pure image/png with no font dependency.
const (
	pngGlyphW     = 5
	pngGlyphH     = 7
	pngGlyphScale = 2

	pngCellWidth  = pngGlyphW*pngGlyphScale + 2 // 12
	pngCellHeight = pngGlyphH*pngGlyphScale + 2 // 16
)

// pngDefaultBg / pngDefaultFg are the terminal default background and
// foreground used when a cell carries no explicit SGR color.
var (
	pngDefaultBg = color.RGBA{R: 0x18, G: 0x18, B: 0x1c, A: 0xff}
	pngDefaultFg = color.RGBA{R: 0xe4, G: 0xe4, B: 0xe4, A: 0xff}
)

// ascii5x7 is a classic 5x7 ASCII bitmap font covering 0x20..0x7e. Each glyph
// has five columns; each byte is one column, least significant bit first, with
// bit 0 the top row and bit 6 the bottom row.
var ascii5x7 = [95][5]byte{
	{0x00, 0x00, 0x00, 0x00, 0x00}, // 0x20 space
	{0x00, 0x00, 0x5f, 0x00, 0x00}, // !
	{0x00, 0x07, 0x00, 0x07, 0x00}, // "
	{0x14, 0x7f, 0x14, 0x7f, 0x14}, // #
	{0x24, 0x2a, 0x7f, 0x2a, 0x12}, // $
	{0x23, 0x13, 0x08, 0x64, 0x62}, // %
	{0x36, 0x49, 0x55, 0x22, 0x50}, // &
	{0x00, 0x05, 0x03, 0x00, 0x00}, // '
	{0x00, 0x1c, 0x22, 0x41, 0x00}, // (
	{0x00, 0x41, 0x22, 0x1c, 0x00}, // )
	{0x14, 0x08, 0x3e, 0x08, 0x14}, // *
	{0x08, 0x08, 0x3e, 0x08, 0x08}, // +
	{0x00, 0x50, 0x30, 0x00, 0x00}, // ,
	{0x08, 0x08, 0x08, 0x08, 0x08}, // -
	{0x00, 0x60, 0x60, 0x00, 0x00}, // .
	{0x20, 0x10, 0x08, 0x04, 0x02}, // /
	{0x3e, 0x51, 0x49, 0x45, 0x3e}, // 0
	{0x00, 0x42, 0x7f, 0x40, 0x00}, // 1
	{0x42, 0x61, 0x51, 0x49, 0x46}, // 2
	{0x21, 0x41, 0x45, 0x4b, 0x31}, // 3
	{0x18, 0x14, 0x12, 0x7f, 0x10}, // 4
	{0x27, 0x45, 0x45, 0x45, 0x39}, // 5
	{0x3c, 0x4a, 0x49, 0x49, 0x30}, // 6
	{0x01, 0x71, 0x09, 0x05, 0x03}, // 7
	{0x36, 0x49, 0x49, 0x49, 0x36}, // 8
	{0x06, 0x49, 0x49, 0x29, 0x1e}, // 9
	{0x00, 0x36, 0x36, 0x00, 0x00}, // :
	{0x00, 0x56, 0x36, 0x00, 0x00}, // ;
	{0x08, 0x14, 0x22, 0x41, 0x00}, // <
	{0x14, 0x14, 0x14, 0x14, 0x14}, // =
	{0x00, 0x41, 0x22, 0x14, 0x08}, // >
	{0x02, 0x01, 0x51, 0x09, 0x06}, // ?
	{0x32, 0x49, 0x79, 0x41, 0x3e}, // @
	{0x7e, 0x11, 0x11, 0x11, 0x7e}, // A
	{0x7f, 0x49, 0x49, 0x49, 0x36}, // B
	{0x3e, 0x41, 0x41, 0x41, 0x22}, // C
	{0x7f, 0x41, 0x41, 0x22, 0x1c}, // D
	{0x7f, 0x49, 0x49, 0x49, 0x41}, // E
	{0x7f, 0x09, 0x09, 0x01, 0x01}, // F
	{0x3e, 0x41, 0x41, 0x51, 0x32}, // G
	{0x7f, 0x08, 0x08, 0x08, 0x7f}, // H
	{0x00, 0x41, 0x7f, 0x41, 0x00}, // I
	{0x20, 0x40, 0x41, 0x3f, 0x01}, // J
	{0x7f, 0x08, 0x14, 0x22, 0x41}, // K
	{0x7f, 0x40, 0x40, 0x40, 0x40}, // L
	{0x7f, 0x02, 0x04, 0x02, 0x7f}, // M
	{0x7f, 0x04, 0x08, 0x10, 0x7f}, // N
	{0x3e, 0x41, 0x41, 0x41, 0x3e}, // O
	{0x7f, 0x09, 0x09, 0x09, 0x06}, // P
	{0x3e, 0x41, 0x51, 0x21, 0x5e}, // Q
	{0x7f, 0x09, 0x19, 0x29, 0x46}, // R
	{0x46, 0x49, 0x49, 0x49, 0x31}, // S
	{0x01, 0x01, 0x7f, 0x01, 0x01}, // T
	{0x3f, 0x40, 0x40, 0x40, 0x3f}, // U
	{0x1f, 0x20, 0x40, 0x20, 0x1f}, // V
	{0x7f, 0x20, 0x18, 0x20, 0x7f}, // W
	{0x63, 0x14, 0x08, 0x14, 0x63}, // X
	{0x03, 0x04, 0x78, 0x04, 0x03}, // Y
	{0x61, 0x51, 0x49, 0x45, 0x43}, // Z
	{0x00, 0x00, 0x7f, 0x41, 0x41}, // [
	{0x02, 0x04, 0x08, 0x10, 0x20}, // backslash
	{0x41, 0x41, 0x7f, 0x00, 0x00}, // ]
	{0x04, 0x02, 0x01, 0x02, 0x04}, // ^
	{0x40, 0x40, 0x40, 0x40, 0x40}, // _
	{0x00, 0x01, 0x02, 0x04, 0x00}, // `
	{0x20, 0x54, 0x54, 0x54, 0x78}, // a
	{0x7f, 0x48, 0x44, 0x44, 0x38}, // b
	{0x38, 0x44, 0x44, 0x44, 0x20}, // c
	{0x38, 0x44, 0x44, 0x48, 0x7f}, // d
	{0x38, 0x54, 0x54, 0x54, 0x18}, // e
	{0x08, 0x7e, 0x09, 0x01, 0x02}, // f
	{0x08, 0x14, 0x54, 0x54, 0x3c}, // g
	{0x7f, 0x08, 0x04, 0x04, 0x78}, // h
	{0x00, 0x44, 0x7d, 0x40, 0x00}, // i
	{0x20, 0x40, 0x44, 0x3d, 0x00}, // j
	{0x00, 0x7f, 0x10, 0x28, 0x44}, // k
	{0x00, 0x41, 0x7f, 0x40, 0x00}, // l
	{0x7c, 0x04, 0x18, 0x04, 0x78}, // m
	{0x7c, 0x08, 0x04, 0x04, 0x78}, // n
	{0x38, 0x44, 0x44, 0x44, 0x38}, // o
	{0x7c, 0x14, 0x14, 0x14, 0x08}, // p
	{0x08, 0x14, 0x14, 0x18, 0x7c}, // q
	{0x7c, 0x08, 0x04, 0x04, 0x08}, // r
	{0x48, 0x54, 0x54, 0x54, 0x20}, // s
	{0x04, 0x3f, 0x44, 0x40, 0x20}, // t
	{0x3c, 0x40, 0x40, 0x20, 0x7c}, // u
	{0x1c, 0x20, 0x40, 0x20, 0x1c}, // v
	{0x3c, 0x40, 0x30, 0x40, 0x3c}, // w
	{0x44, 0x28, 0x10, 0x28, 0x44}, // x
	{0x0c, 0x50, 0x50, 0x50, 0x3c}, // y
	{0x44, 0x64, 0x54, 0x4c, 0x44}, // z
	{0x00, 0x08, 0x36, 0x41, 0x00}, // {
	{0x00, 0x00, 0x7f, 0x00, 0x00}, // |
	{0x00, 0x41, 0x36, 0x08, 0x00}, // }
	{0x08, 0x04, 0x04, 0x08, 0x08}, // ~
}

// extraGlyphs covers the handful of non-ASCII runes the TUI draws: the title
// separator, composer prompt, sidebar marks, and member status dash. They use
// the same 5x7 column layout as ascii5x7.
var extraGlyphs = map[rune][5]byte{
	'\u00b7': {0x00, 0x00, 0x08, 0x00, 0x00}, // · middle dot
	'\u2014': {0x08, 0x08, 0x08, 0x08, 0x08}, // — em dash
	'\u2022': {0x00, 0x0c, 0x0c, 0x00, 0x00}, // • bullet
	'\u2026': {0x40, 0x00, 0x40, 0x00, 0x40}, // … ellipsis
	'\u203a': {0x00, 0x08, 0x14, 0x22, 0x00}, // › single right angle quote
	'\u25cf': {0x3e, 0x7f, 0x7f, 0x7f, 0x3e}, // ● black circle
	'\u25cb': {0x3e, 0x41, 0x41, 0x41, 0x3e}, // ○ white circle
}

// glyphFor returns the bitmap for r. ok is false when r has no glyph, which
// PNG renders as a small solid box so the rune does not vanish.
func glyphFor(r rune) (g [5]byte, ok bool) {
	if r >= 0x20 && r <= 0x7e {
		return ascii5x7[r-0x20], true
	}
	g, ok = extraGlyphs[r]
	return g, ok
}

// PNG renders the grid so a human (or a test) can read the frame. Each cell is
// painted with its background color and, for a printable rune, its 5x7 glyph
// in the foreground color. Space and the continuation half of a wide rune stay
// blank; a rune outside the font draws a small solid box. The output is a
// deterministic image/png.
func (s *Screen) PNG() ([]byte, error) {
	if s.width <= 0 || s.height <= 0 {
		return nil, fmt.Errorf("cannot render a %dx%d screen", s.width, s.height)
	}
	img := image.NewRGBA(image.Rect(0, 0, s.width*pngCellWidth, s.height*pngCellHeight))
	// First pass: paint every cell background, continuation cells included, so
	// a wide glyph drawn in the second pass is not clipped by its neighbor.
	for y := 0; y < s.height; y++ {
		for x := 0; x < s.width; x++ {
			c := s.cells[y*s.width+x]
			cellBg, _ := cellColors(c, pngDefaultBg)
			fillRect(img, x*pngCellWidth, y*pngCellHeight, pngCellWidth, pngCellHeight, cellBg)
		}
	}
	// Second pass: draw glyphs.
	for y := 0; y < s.height; y++ {
		for x := 0; x < s.width; x++ {
			c := s.cells[y*s.width+x]
			if c.Cont {
				continue // second half of a wide rune: background only
			}
			r := c.Rune
			if r == 0 || r == ' ' || r == '\u00a0' {
				continue
			}
			_, cellFg := cellColors(c, pngDefaultBg)
			cellX := x * pngCellWidth
			cellY := y * pngCellHeight
			span := 1
			if c.Wide {
				span = 2
			}
			g, ok := glyphFor(r)
			if !ok {
				drawFallbackBox(img, cellX, cellY, span, cellFg)
				continue
			}
			drawGlyph(img, g, cellX, cellY, span, cellFg)
		}
	}
	var buf bytes.Buffer
	if err := png.Encode(&buf, img); err != nil {
		return nil, err
	}
	return buf.Bytes(), nil
}

// drawGlyph paints g scaled by pngGlyphScale, centered in the cell (or across
// span cells for a double-width rune), in the foreground color.
func drawGlyph(img *image.RGBA, g [5]byte, cellX, cellY, span int, fg color.RGBA) {
	originX := cellX + (span*pngCellWidth-pngGlyphW*pngGlyphScale)/2
	originY := cellY + (pngCellHeight-pngGlyphH*pngGlyphScale)/2
	for col := 0; col < pngGlyphW; col++ {
		bits := g[col]
		for row := 0; row < pngGlyphH; row++ {
			if bits&(1<<uint(row)) == 0 {
				continue
			}
			fillRect(img, originX+col*pngGlyphScale, originY+row*pngGlyphScale, pngGlyphScale, pngGlyphScale, fg)
		}
	}
}

// drawFallbackBox is the stand-in for a rune outside the font: a solid box
// inset from the cell edges so the missing glyph is visible, not blank.
func drawFallbackBox(img *image.RGBA, cellX, cellY, span int, fg color.RGBA) {
	w := span*pngCellWidth - 2*pngGlyphScale
	h := pngCellHeight - 2*pngGlyphScale
	fillRect(img, cellX+pngGlyphScale, cellY+pngGlyphScale, w, h, fg)
}

func cellColors(c Cell, fallbackBg color.RGBA) (bg, fg color.RGBA) {
	bg = fallbackBg
	fg = pngDefaultFg
	if c.Attrs.Bg >= 0 {
		bg = ansiColor(c.Attrs.Bg)
	}
	if c.Attrs.Fg >= 0 {
		fg = ansiColor(c.Attrs.Fg)
	}
	if c.Attrs.Reverse {
		bg, fg = fg, bg
	}
	return bg, fg
}

func fillRect(img *image.RGBA, x, y, w, h int, c color.RGBA) {
	if w <= 0 || h <= 0 {
		return
	}
	b := img.Bounds()
	for yy := y; yy < y+h; yy++ {
		if yy < b.Min.Y || yy >= b.Max.Y {
			continue
		}
		for xx := x; xx < x+w; xx++ {
			if xx < b.Min.X || xx >= b.Max.X {
				continue
			}
			img.SetRGBA(xx, yy, c)
		}
	}
}

// ansiColor maps an xterm 256-color index to RGB.
func ansiColor(idx int) color.RGBA {
	if idx < 0 || idx > 255 {
		return color.RGBA{R: 0xe4, G: 0xe4, B: 0xe4, A: 0xff}
	}
	if idx < 16 {
		return ansi16[idx]
	}
	if idx < 232 {
		idx -= 16
		level := [6]uint8{0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff}
		return color.RGBA{
			R: level[idx/36],
			G: level[(idx/6)%6],
			B: level[idx%6],
			A: 0xff,
		}
	}
	v := uint8(8 + (idx-232)*10)
	return color.RGBA{R: v, G: v, B: v, A: 0xff}
}

var ansi16 = [16]color.RGBA{
	{R: 0x1c, G: 0x1c, B: 0x1c, A: 0xff},
	{R: 0xcc, G: 0x3e, B: 0x3e, A: 0xff},
	{R: 0x4e, G: 0x9a, B: 0x4e, A: 0xff},
	{R: 0xcc, G: 0xac, B: 0x3e, A: 0xff},
	{R: 0x4e, G: 0x7e, B: 0xcc, A: 0xff},
	{R: 0xb0, G: 0x4e, B: 0xcc, A: 0xff},
	{R: 0x3e, G: 0xa9, B: 0xb0, A: 0xff},
	{R: 0xcc, G: 0xcc, B: 0xcc, A: 0xff},
	{R: 0x66, G: 0x66, B: 0x66, A: 0xff},
	{R: 0xff, G: 0x5f, B: 0x5f, A: 0xff},
	{R: 0x5f, G: 0xdf, B: 0x5f, A: 0xff},
	{R: 0xff, G: 0xdf, B: 0x5f, A: 0xff},
	{R: 0x5f, G: 0x9f, B: 0xff, A: 0xff},
	{R: 0xdf, G: 0x5f, B: 0xff, A: 0xff},
	{R: 0x5f, G: 0xdf, B: 0xdf, A: 0xff},
	{R: 0xff, G: 0xff, B: 0xff, A: 0xff},
}

func rgbTo256(r, g, b int) int {
	q := func(v int) int {
		if v < 0 {
			v = 0
		}
		if v > 255 {
			v = 255
		}
		return v * 5 / 255
	}
	return 16 + 36*q(r) + 6*q(g) + q(b)
}
