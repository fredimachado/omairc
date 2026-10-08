package ui

import (
	"strconv"
	"strings"

	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/x/ansi"
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

	out := ansi.Truncate(line, width, ellipsisRune)
	out = repairBareEllipsis(line, out, width)
	for lipgloss.Width(out) < width {
		out = padBeforeEllipsis(out, line, width)
	}
	return out
}

func repairBareEllipsis(line, out string, width int) string {
	if !endsWithResetBareEllipsis(out) {
		return out
	}
	style := styleSequenceBeforeCell(line, width-1)
	if style == "" {
		return out
	}
	out = strings.TrimSuffix(out, ellipsisRune)
	out = trimTrailingSGRReset(out)
	return out + style + ellipsisRune
}

func padBeforeEllipsis(out, line string, width int) string {
	idx := strings.LastIndex(out, ellipsisRune)
	if idx < 0 {
		return out
	}
	prefix := out[:idx]
	style := ""
	if !prefixHasOpenStyle(prefix) {
		style = styleSequenceBeforeCell(line, width-1)
	}
	return prefix + style + " " + out[idx:]
}

func endsWithResetBareEllipsis(s string) bool {
	idx := strings.LastIndex(s, ellipsisRune)
	if idx < 0 {
		return false
	}
	before := s[:idx]
	return strings.HasSuffix(before, "\x1b[0m") || strings.HasSuffix(before, "\x1b[m")
}

func trimTrailingSGRReset(s string) string {
	for {
		switch {
		case strings.HasSuffix(s, "\x1b[0m"):
			s = strings.TrimSuffix(s, "\x1b[0m")
		case strings.HasSuffix(s, "\x1b[m"):
			s = strings.TrimSuffix(s, "\x1b[m")
		default:
			return s
		}
	}
}

func prefixHasOpenStyle(prefix string) bool {
	st := styleAfterSequences(prefix)
	return len(st) > 0
}

func styleSequenceBeforeCell(s string, budget int) string {
	if budget <= 0 {
		return ""
	}
	st := styleAfterSequences(cellPrefix(s, budget))
	seq := st.String()
	if seq == ansi.ResetStyle {
		return ""
	}
	return seq
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

func styleAfterSequences(s string) ansi.Style {
	var st ansi.Style
	for i := 0; i < len(s); {
		if s[i] != '\x1b' || i+1 >= len(s) {
			i++
			continue
		}
		if s[i+1] != '[' {
			i++
			continue
		}
		end := strings.IndexByte(s[i+2:], 'm')
		if end < 0 {
			break
		}
		end += i + 2
		st = applySGREscape(st, s[i:end+1])
		i = end + 1
	}
	return st
}

func applySGREscape(st ansi.Style, seq string) ansi.Style {
	if len(seq) < 3 || seq[0] != '\x1b' || seq[1] != '[' || seq[len(seq)-1] != 'm' {
		return st
	}
	body := seq[2 : len(seq)-1]
	if body == "" {
		return ansi.NewStyle()
	}
	parts := strings.Split(body, ";")
	for index := 0; index < len(parts); index++ {
		code, err := strconv.Atoi(parts[index])
		if err != nil {
			continue
		}
		switch code {
		case 0:
			st = ansi.NewStyle()
		case 1:
			st = st.Bold()
		case 2:
			st = st.Faint()
		case 3:
			st = st.Italic(true)
		case 4:
			st = st.Underline(true)
		case 7:
			st = st.Reverse(true)
		case 9:
			st = st.Strikethrough(true)
		case 22:
			st = st.Normal()
		case 23:
			st = st.NoItalic()
		case 24:
			st = st.NoUnderline()
		case 27:
			st = st.NoReverse()
		case 29:
			st = st.NoStrikethrough()
		case 30:
			st = st.ForegroundColor(ansi.Black)
		case 31:
			st = st.ForegroundColor(ansi.Red)
		case 32:
			st = st.ForegroundColor(ansi.Green)
		case 33:
			st = st.ForegroundColor(ansi.Yellow)
		case 34:
			st = st.ForegroundColor(ansi.Blue)
		case 35:
			st = st.ForegroundColor(ansi.Magenta)
		case 36:
			st = st.ForegroundColor(ansi.Cyan)
		case 37:
			st = st.ForegroundColor(ansi.White)
		case 38:
			if index+2 < len(parts) && parts[index+1] == "5" {
				n, err := strconv.Atoi(parts[index+2])
				if err == nil {
					st = st.ForegroundColor(ansi.ExtendedColor(n))
				}
				index += 2
			} else if index+4 < len(parts) && parts[index+1] == "2" {
				r, errR := strconv.Atoi(parts[index+2])
				g, errG := strconv.Atoi(parts[index+3])
				b, errB := strconv.Atoi(parts[index+4])
				if errR == nil && errG == nil && errB == nil {
					st = st.ForegroundColor(ansi.TrueColor(uint32(r)<<16 | uint32(g)<<8 | uint32(b)))
				}
				index += 4
			}
		case 39:
			st = st.DefaultForegroundColor()
		case 40:
			st = st.BackgroundColor(ansi.Black)
		case 41:
			st = st.BackgroundColor(ansi.Red)
		case 42:
			st = st.BackgroundColor(ansi.Green)
		case 43:
			st = st.BackgroundColor(ansi.Yellow)
		case 44:
			st = st.BackgroundColor(ansi.Blue)
		case 45:
			st = st.BackgroundColor(ansi.Magenta)
		case 46:
			st = st.BackgroundColor(ansi.Cyan)
		case 47:
			st = st.BackgroundColor(ansi.White)
		case 48:
			if index+2 < len(parts) && parts[index+1] == "5" {
				n, err := strconv.Atoi(parts[index+2])
				if err == nil {
					st = st.BackgroundColor(ansi.ExtendedColor(n))
				}
				index += 2
			} else if index+4 < len(parts) && parts[index+1] == "2" {
				r, errR := strconv.Atoi(parts[index+2])
				g, errG := strconv.Atoi(parts[index+3])
				b, errB := strconv.Atoi(parts[index+4])
				if errR == nil && errG == nil && errB == nil {
					st = st.BackgroundColor(ansi.TrueColor(uint32(r)<<16 | uint32(g)<<8 | uint32(b)))
				}
				index += 4
			}
		case 49:
			st = st.DefaultBackgroundColor()
		case 90:
			st = st.ForegroundColor(ansi.BrightBlack)
		case 91:
			st = st.ForegroundColor(ansi.BrightRed)
		case 92:
			st = st.ForegroundColor(ansi.BrightGreen)
		case 93:
			st = st.ForegroundColor(ansi.BrightYellow)
		case 94:
			st = st.ForegroundColor(ansi.BrightBlue)
		case 95:
			st = st.ForegroundColor(ansi.BrightMagenta)
		case 96:
			st = st.ForegroundColor(ansi.BrightCyan)
		case 97:
			st = st.ForegroundColor(ansi.BrightWhite)
		}
	}
	return st
}
