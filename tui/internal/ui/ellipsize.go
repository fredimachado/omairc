package ui

import (
	"strconv"
	"strings"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
	"github.com/charmbracelet/x/ansi/parser"
)

const ellipsisRune = "…"

// ellipsizeLine fits one rendered line to width display cells, ending with a
// single-cell ellipsis when the source is longer. ANSI sequences are preserved
// and never split; wide graphemes are never cut. truncateLine hard-cuts without
// an ellipsis and stays the choice for existing call sites.
func ellipsizeLine(line string, width int) string {
	if width <= 0 {
		return ""
	}
	if lipgloss.Width(line) <= width {
		return line
	}

	out, ellipsisIdx := truncateWithEllipsis(line, width, ellipsisRune)
	out, ellipsisIdx = repairBareEllipsis(out, ellipsisIdx)
	out, ellipsisIdx = dropExtraVisibleEllipses(out, ellipsisIdx)
	// lipgloss.Width is the widest line. A space on the ellipsis line can
	// leave that max unchanged while the line is still short of the budget,
	// so keep padding until the max matches. Cap the loop by the budget: a
	// space that never adds a cell must not hang.
	for n := 0; n < width && lipgloss.Width(out) < width; n++ {
		next, nextIdx := padBeforeEllipsis(out, line, width, ellipsisIdx)
		if next == out {
			break
		}
		out, ellipsisIdx = next, nextIdx
	}
	return out
}

func truncateWithEllipsis(s string, width int, tail string) (string, int) {
	// lipgloss.Width splits on \n before it parses escapes. A newline inside
	// CSI, DCS, OSC, SOS, PM, or APC stays inside the sequence for
	// ansi.StringWidth, then becomes visible cells on the next line. Drop
	// those newlines and measure the joined string. A ground-state newline
	// is a real line break and stays.
	s = stripNewlinesInSequences(s)
	if ansi.StringWidth(s) <= width && lipgloss.Width(s) <= width {
		return s, -1
	}

	tw := ansi.StringWidth(tail)
	contentWidth := width - tw
	if contentWidth < 0 {
		return "", -1
	}

	var buf strings.Builder
	curWidth := 0
	ignoring := false
	pstate := parser.GroundState
	ellipsisIdx := -1
	i := 0

	for i < len(s) {
		state, action := parser.Table.Transition(pstate, s[i])
		// Same join as stripNewlinesInSequences. Leave pstate unchanged so
		// the rest of the sequence is parsed as if the newline was never there.
		if s[i] == '\n' && sequenceHidesNewline(pstate) {
			i++
			continue
		}
		// PrintAction is an ASCII lead. ansi.StringWidth still measures the
		// whole grapheme (digit + variation selector, keycap, …), so consume
		// that cluster here or the line lands wider than the budget.
		if action == parser.PrintAction || state == parser.Utf8State {
			cluster, w := ansi.FirstGraphemeCluster(s[i:], ansi.GraphemeWidth)
			if len(cluster) == 0 {
				i++
				pstate = parser.GroundState
				continue
			}
			if !ignoring && curWidth+w > contentWidth {
				ignoring = true
				ellipsisIdx = buf.Len()
				buf.WriteString(tail)
			}
			if ignoring {
				i += len(cluster)
				pstate = parser.GroundState
				continue
			}
			buf.WriteString(cluster)
			curWidth += w
			i += len(cluster)
			pstate = parser.GroundState
			continue
		}

		switch action {
		case parser.ExecuteAction:
			if ignoring {
				i++
				continue
			}
			fallthrough
		default:
			buf.WriteByte(s[i])
			i++
		}

		pstate = state
	}

	return buf.String(), ellipsisIdx
}

// stripNewlinesInSequences removes raw newlines that sit inside CSI, DCS,
// OSC, SOS, PM, or APC. Terminating bytes stay. A ground-state newline stays,
// so a real line break still pads on the ellipsis line.
func stripNewlinesInSequences(s string) string {
	if strings.IndexByte(s, '\n') < 0 {
		return s
	}
	var buf strings.Builder
	buf.Grow(len(s))
	pstate := parser.GroundState
	dropped := false
	for i := 0; i < len(s); i++ {
		if s[i] == '\n' && sequenceHidesNewline(pstate) {
			dropped = true
			continue
		}
		buf.WriteByte(s[i])
		state, _ := parser.Table.Transition(pstate, s[i])
		pstate = state
	}
	if !dropped {
		return s
	}
	return buf.String()
}

// sequenceHidesNewline reports whether the parser is inside a sequence whose
// raw newline lipgloss would treat as a line break. Escape and ground are
// not included: a newline there is either not yet a sequence or a real break.
func sequenceHidesNewline(state parser.State) bool {
	switch state {
	case parser.CsiEntryState, parser.CsiParamState, parser.CsiIntermediateState,
		parser.DcsEntryState, parser.DcsParamState, parser.DcsIntermediateState, parser.DcsStringState,
		parser.OscStringState, parser.SosStringState, parser.PmStringState, parser.ApcStringState:
		return true
	default:
		return false
	}
}

func repairBareEllipsis(out string, ellipsisIdx int) (string, int) {
	if ellipsisIdx < 0 || ellipsisIdx+len(ellipsisRune) > len(out) {
		return out, ellipsisIdx
	}
	if out[ellipsisIdx:ellipsisIdx+len(ellipsisRune)] != ellipsisRune {
		return out, ellipsisIdx
	}
	prefix := out[:ellipsisIdx]
	suffix := out[ellipsisIdx+len(ellipsisRune):]
	// The marker takes the style of the last positive-width kept grapheme.
	// A trailing SGR run after that grapheme belongs to the discarded tail;
	// leaving it in front wraps the marker (and any pad space) in the tail
	// style. A prefix with no positive-width cell stays as it is, so an empty
	// open/reset span stays bare and a style that only wraps a dropped glyph
	// still colors the marker.
	head, tail := splitTrailingZeroWidth(prefix)
	if ansi.StringWidth(head) == 0 {
		return out, ellipsisIdx
	}
	// Only a trailing SGR run moves. A reset that still has a zero-width tail
	// after it (OSC, joiner, variation selector) stays, so that tail is not
	// glued back onto the previous cell. The last-cell style is reopened in
	// front of the marker either way.
	body, trailing := splitTrailingSGR(tail)
	style := activeSGRReplay(head)
	if trailing == "" && activeSGRReplay(head+body) == style {
		return out, ellipsisIdx
	}
	var b strings.Builder
	b.Grow(len(head) + len(body) + len(style) + len(ellipsisRune) + len(trailing) + len(suffix))
	b.WriteString(head)
	b.WriteString(body)
	b.WriteString(style)
	ellipsisIdx = b.Len()
	b.WriteString(ellipsisRune)
	b.WriteString(trailing)
	b.WriteString(suffix)
	return b.String(), ellipsisIdx
}

// splitTrailingSGR returns tail without its trailing SGR run, and that run.
// tail starts in ground, just after the last positive-width grapheme.
func splitTrailingSGR(tail string) (body, trailing string) {
	type span struct {
		start, end int
		sgr        bool
	}
	var spans []span
	pstate := parser.GroundState
	for i := 0; i < len(tail); {
		if n := sgrLenAt(tail, i, pstate); n > 0 {
			spans = append(spans, span{i, i + n, true})
			i += n
			pstate = parser.GroundState
			continue
		}
		start := i
		state, action := parser.Table.Transition(pstate, tail[i])
		if action == parser.PrintAction || state == parser.Utf8State {
			cluster, _ := ansi.FirstGraphemeCluster(tail[i:], ansi.GraphemeWidth)
			if len(cluster) == 0 {
				i++
				pstate = parser.GroundState
			} else {
				i += len(cluster)
				pstate = parser.GroundState
			}
		} else {
			i++
			pstate = state
		}
		if len(spans) > 0 && !spans[len(spans)-1].sgr {
			spans[len(spans)-1].end = i
			continue
		}
		spans = append(spans, span{start, i, false})
	}
	end := len(spans)
	for end > 0 && spans[end-1].sgr {
		end--
	}
	if end == len(spans) {
		return tail, ""
	}
	cut := spans[end].start
	return tail[:cut], tail[cut:]
}

// sgrLenAt returns the byte length of an SGR sequence starting at s[i], or 0
// when that position is not a CSI sequence whose final byte is 'm'.
func sgrLenAt(s string, i int, pstate parser.State) int {
	if pstate != parser.GroundState || i >= len(s) || s[i] != '\x1b' {
		return 0
	}
	if i+1 >= len(s) || s[i+1] != '[' {
		return 0
	}
	st := parser.CsiEntryState
	for j := i + 2; j < len(s); j++ {
		next, action := parser.Table.Transition(st, s[j])
		if action == parser.DispatchAction {
			if s[j] == 'm' {
				return j - i + 1
			}
			return 0
		}
		switch next {
		case parser.CsiEntryState, parser.CsiParamState, parser.CsiIntermediateState:
			st = next
		default:
			return 0
		}
	}
	return 0
}

// splitTrailingZeroWidth returns the prefix through the last positive-width
// grapheme and the zero-width tail after it.
func splitTrailingZeroWidth(s string) (head, tail string) {
	pstate := parser.GroundState
	last := 0
	for i := 0; i < len(s); {
		state, action := parser.Table.Transition(pstate, s[i])
		if action == parser.PrintAction || state == parser.Utf8State {
			cluster, w := ansi.FirstGraphemeCluster(s[i:], ansi.GraphemeWidth)
			if len(cluster) == 0 {
				break
			}
			i += len(cluster)
			if w > 0 {
				last = i
			}
			pstate = parser.GroundState
			continue
		}
		i++
		pstate = state
	}
	return s[:last], s[last:]
}

// dropExtraVisibleEllipses keeps the truncation marker and removes any other
// visible U+2026. An ellipsis inside an OSC payload is not visible and stays.
func dropExtraVisibleEllipses(s string, ellipsisIdx int) (string, int) {
	if ellipsisIdx < 0 {
		return s, ellipsisIdx
	}
	type span struct{ start, end int }
	var extra []span
	pstate := parser.GroundState
	for i := 0; i < len(s); {
		state, action := parser.Table.Transition(pstate, s[i])
		if action == parser.PrintAction || state == parser.Utf8State {
			cluster, _ := ansi.FirstGraphemeCluster(s[i:], ansi.GraphemeWidth)
			if len(cluster) == 0 {
				break
			}
			// A variation selector can make U+2026 its own wide cluster. Drop that
			// whole cluster; keeping only the base rune would split it.
			if strings.Contains(cluster, ellipsisRune) && i != ellipsisIdx {
				extra = append(extra, span{i, i + len(cluster)})
			}
			i += len(cluster)
			pstate = parser.GroundState
			continue
		}
		i++
		pstate = state
	}
	for k := len(extra) - 1; k >= 0; k-- {
		sp := extra[k]
		s = s[:sp.start] + s[sp.end:]
		if ellipsisIdx > sp.start {
			ellipsisIdx -= sp.end - sp.start
		}
	}
	return s, ellipsisIdx
}

func padBeforeEllipsis(out, line string, width int, ellipsisIdx int) (string, int) {
	if ellipsisIdx < 0 || ellipsisIdx+len(ellipsisRune) > len(out) {
		return out, ellipsisIdx
	}
	if out[ellipsisIdx:ellipsisIdx+len(ellipsisRune)] != ellipsisRune {
		return out, ellipsisIdx
	}
	prefix := out[:ellipsisIdx]
	style := ""
	if !prefixHasOpenStyle(prefix) {
		style = activeSGRReplay(cellPrefix(line, width-1))
	}
	out = prefix + style + " " + out[ellipsisIdx:]
	ellipsisIdx = len(prefix) + len(style) + 1
	return out, ellipsisIdx
}

func hasResetBareEllipsisAt(prefix string) bool {
	seq, ok := trailingSGREscape(prefix)
	return ok && sgrSequenceResets(seq)
}

func endsWithResetBareEllipsis(s string) bool {
	idx := strings.LastIndex(s, ellipsisRune)
	if idx < 0 {
		return false
	}
	return hasResetBareEllipsisAt(s[:idx])
}

func prefixHasOpenStyle(prefix string) bool {
	return activeSGRReplay(prefix) != ""
}

func cellPrefix(s string, budget int) string {
	if budget <= 0 {
		return ""
	}
	p := ansi.NewParser()
	var (
		state  byte
		used   int
		length int
	)
	for length < len(s) && used < budget {
		_, w, n, newState := ansi.GraphemeWidth.DecodeSequenceInString(s[length:], state, p)
		state = newState
		if w > 0 {
			if used+w > budget {
				break
			}
			used += w
		}
		length += n
	}
	return s[:length]
}

// activeSGRReplay returns the original SGR bytes active after parsing s,
// replaying each non-reset sequence in order instead of re-encoding attributes.
func activeSGRReplay(s string) string {
	var replay strings.Builder
	for i := 0; i < len(s); {
		if s[i] != '\x1b' || i+1 >= len(s) {
			i++
			continue
		}
		switch s[i+1] {
		case '[':
			end := strings.IndexByte(s[i+2:], 'm')
			if end < 0 {
				return replay.String()
			}
			end += i + 2
			seq := s[i : end+1]
			if sgrSequenceResets(seq) {
				replay.Reset()
			} else {
				if sgrSequenceClearsThenSets(seq) {
					replay.Reset()
				}
				replay.WriteString(seq)
			}
			i = end + 1
		case ']':
			// OSC: skip without touching SGR replay.
			j := i + 2
			for j < len(s) {
				if s[j] == '\x1b' && j+1 < len(s) && s[j+1] == '\\' {
					j += 2
					break
				}
				if s[j] == '\x07' {
					j++
					break
				}
				j++
			}
			i = j
		default:
			i++
		}
	}
	return replay.String()
}

func trailingSGREscape(s string) (string, bool) {
	if !strings.HasSuffix(s, "m") {
		return "", false
	}
	idx := strings.LastIndex(s, "\x1b[")
	if idx < 0 {
		return "", false
	}
	seq := s[idx:]
	if len(seq) < 3 || seq[len(seq)-1] != 'm' {
		return "", false
	}
	return seq, true
}

func sgrSequenceResets(seq string) bool {
	if len(seq) < 3 || seq[0] != '\x1b' || seq[1] != '[' || seq[len(seq)-1] != 'm' {
		return false
	}
	return sgrBodyResetOnly(seq[2 : len(seq)-1])
}

// sgrBodyResetOnly reports whether every SGR parameter is empty or numeric zero.
func sgrBodyResetOnly(body string) bool {
	parts := strings.Split(body, ";")
	i := 0
	for i < len(parts) {
		p := parts[i]
		if p == "" {
			i++
			continue
		}
		n, err := strconv.Atoi(p)
		if err != nil || n != 0 {
			return false
		}
		i++
	}
	return true
}

// sgrSequenceClearsThenSets is true when a top-level 0 appears before later
// attributes in the same sequence (for example 0;31 or 0;101).
func sgrSequenceClearsThenSets(seq string) bool {
	if len(seq) < 3 || seq[0] != '\x1b' || seq[1] != '[' || seq[len(seq)-1] != 'm' {
		return false
	}
	body := seq[2 : len(seq)-1]
	parts := strings.Split(body, ";")
	i := 0
	sawReset := false
	for i < len(parts) {
		p := parts[i]
		if p == "" {
			i++
			continue
		}
		n, err := strconv.Atoi(p)
		if err != nil {
			if sawReset {
				return true
			}
			i++
			continue
		}
		switch n {
		case 0:
			sawReset = true
			i++
		case 38, 48, 58:
			if sawReset {
				return true
			}
			if i+1 >= len(parts) {
				i++
				continue
			}
			mode, err := strconv.Atoi(parts[i+1])
			if err != nil {
				i++
				continue
			}
			switch mode {
			case 5:
				i += 3
				continue
			case 2:
				if i+4 >= len(parts) {
					i = len(parts)
					continue
				}
				i += 5
				continue
			default:
				i++
			}
		default:
			if sawReset {
				return true
			}
			i++
		}
	}
	return false
}
