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
	out, ellipsisIdx = repairBareEllipsis(line, out, width, ellipsisIdx)
	for lipgloss.Width(out) < width {
		prev := lipgloss.Width(out)
		out, ellipsisIdx = padBeforeEllipsis(out, line, width, ellipsisIdx)
		if lipgloss.Width(out) <= prev {
			break
		}
	}
	return out
}

func truncateWithEllipsis(s string, width int, tail string) (string, int) {
	if ansi.StringWidth(s) <= width {
		return s, -1
	}

	tw := ansi.StringWidth(tail)
	contentWidth := width - tw
	if contentWidth < 0 {
		return "", -1
	}

	var cluster string
	var buf strings.Builder
	curWidth := 0
	ignoring := false
	pstate := parser.GroundState
	ellipsisIdx := -1
	i := 0

	for i < len(s) {
		state, action := parser.Table.Transition(pstate, s[i])
		if state == parser.Utf8State {
			var w int
			cluster, w = ansi.FirstGraphemeCluster(s[i:], ansi.GraphemeWidth)
			i += len(cluster)
			curWidth += w

			if ignoring {
				continue
			}

			if curWidth > contentWidth && !ignoring {
				ignoring = true
				ellipsisIdx = buf.Len()
				buf.WriteString(tail)
			}

			if curWidth > contentWidth {
				continue
			}

			buf.WriteString(cluster)
			pstate = parser.GroundState
			continue
		}

		switch action {
		case parser.PrintAction:
			if curWidth >= contentWidth && !ignoring {
				ignoring = true
				ellipsisIdx = buf.Len()
				buf.WriteString(tail)
			}

			if ignoring {
				i++
				continue
			}

			curWidth++
			fallthrough
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

		if curWidth > contentWidth && !ignoring {
			ignoring = true
			ellipsisIdx = buf.Len()
			buf.WriteString(tail)
		}
	}

	return buf.String(), ellipsisIdx
}

func repairBareEllipsis(line, out string, width int, ellipsisIdx int) (string, int) {
	if ellipsisIdx < 0 || ellipsisIdx+len(ellipsisRune) > len(out) {
		return out, ellipsisIdx
	}
	if out[ellipsisIdx:ellipsisIdx+len(ellipsisRune)] != ellipsisRune {
		return out, ellipsisIdx
	}
	prefix := out[:ellipsisIdx]
	if !hasResetBareEllipsisAt(prefix) {
		return out, ellipsisIdx
	}
	style := activeSGRReplay(cellPrefix(line, width-1))
	if style == "" {
		return out, ellipsisIdx
	}
	prefix = trimTrailingSGRReset(prefix)
	suffix := out[ellipsisIdx+len(ellipsisRune):]
	out = prefix + style + ellipsisRune + suffix
	ellipsisIdx = len(prefix) + len(style)
	return out, ellipsisIdx
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
