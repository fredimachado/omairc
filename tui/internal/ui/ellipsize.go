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
	// A newline inside an OSC stays inside the sequence for ansi.StringWidth,
	// but lipgloss.Width splits on it and counts the rest of the payload as
	// cells. Drop those newlines before measuring so a URL tail cannot
	// consume the budget or skip the ellipsis.
	s = stripNewlinesInOSC(s)
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
			// A raw newline in an OSC payload is what makes lipgloss count
			// the rest of the URL as cells. Keep the sequence, drop the break.
			if s[i] == '\n' && pstate == parser.OscStringState {
				i++
				pstate = state
				continue
			}
			buf.WriteByte(s[i])
			i++
		}

		pstate = state
	}

	return buf.String(), ellipsisIdx
}

// stripNewlinesInOSC removes raw newlines that sit inside an OSC payload.
// Terminating BEL and ST bytes stay. Newlines outside an OSC stay, so a real
// line break still pads on the ellipsis line.
func stripNewlinesInOSC(s string) string {
	if strings.IndexByte(s, '\n') < 0 {
		return s
	}
	var buf strings.Builder
	buf.Grow(len(s))
	pstate := parser.GroundState
	dropped := false
	for i := 0; i < len(s); i++ {
		state, _ := parser.Table.Transition(pstate, s[i])
		if s[i] == '\n' && pstate == parser.OscStringState {
			dropped = true
			pstate = state
			continue
		}
		buf.WriteByte(s[i])
		pstate = state
	}
	if !dropped {
		return s
	}
	return buf.String()
}

func repairBareEllipsis(out string, ellipsisIdx int) (string, int) {
	if ellipsisIdx < 0 || ellipsisIdx+len(ellipsisRune) > len(out) {
		return out, ellipsisIdx
	}
	if out[ellipsisIdx:ellipsisIdx+len(ellipsisRune)] != ellipsisRune {
		return out, ellipsisIdx
	}
	prefix := out[:ellipsisIdx]
	if activeSGRReplay(prefix) != "" {
		return out, ellipsisIdx
	}
	// The cut can fall on a wide grapheme that follows a reset. Replay the
	// kept cells after that reset is removed; cellPrefix still had room for
	// the reset and would report an empty style. An empty open/reset span
	// has no visible cells and stays bare. A zero-width tail after the reset
	// (OSC, joiner, variation selector) stays in place so it is not glued
	// back onto the previous cell; the style is reopened in front of the ellipsis.
	trimmed := trimTrailingSGRReset(prefix)
	if ansi.StringWidth(trimmed) == 0 {
		return out, ellipsisIdx
	}
	style := activeSGRReplay(trimmed)
	suffix := out[ellipsisIdx+len(ellipsisRune):]
	if style != "" {
		out = trimmed + style + ellipsisRune + suffix
		ellipsisIdx = len(trimmed) + len(style)
		return out, ellipsisIdx
	}
	style = activeStyleBeforeTrail(prefix)
	if style == "" {
		return out, ellipsisIdx
	}
	out = prefix + style + ellipsisRune + suffix
	ellipsisIdx = len(prefix) + len(style)
	return out, ellipsisIdx
}

// activeStyleBeforeTrail is the SGR active on the last visible cells of
// prefix. Trailing resets and other zero-width tails are not part of that style.
func activeStyleBeforeTrail(prefix string) string {
	kept, _ := splitTrailingZeroWidth(prefix)
	kept = trimTrailingSGRReset(kept)
	if ansi.StringWidth(kept) == 0 {
		return ""
	}
	return activeSGRReplay(kept)
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

func trimTrailingSGRReset(s string) string {
	for {
		seq, ok := trailingSGREscape(s)
		if !ok || !sgrSequenceResets(seq) {
			return s
		}
		s = s[:len(s)-len(seq)]
	}
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
