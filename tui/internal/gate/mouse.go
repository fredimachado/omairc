package gate

import (
	"fmt"
	"strings"

	"github.com/mattn/go-runewidth"
)

// SGR mouse button codes (DEC 1006). Coordinates in the sequence are 1-based
// the x and y arguments here are 0-based cells, matching Bubble Tea.
const (
	sgrButtonLeft      = 0
	sgrButtonWheelUp   = 64
	sgrButtonWheelDown = 65
)

// sgrMouse is one SGR mouse report. release uses the trailing 'm'; a press or
// a wheel notch uses 'M'. Bubble Tea's cell-motion mode asks the terminal for
// this encoding.
func sgrMouse(button, x, y int, release bool) []byte {
	final := byte('M')
	if release {
		final = 'm'
	}
	if x < 0 {
		x = 0
	}
	if y < 0 {
		y = 0
	}
	return fmt.Appendf(nil, "\x1b[<%d;%d;%d%c", button, x+1, y+1, final)
}

// clickSequence is a left press followed by a release at the same cell.
func clickSequence(x, y int) []byte {
	return append(sgrMouse(sgrButtonLeft, x, y, false), sgrMouse(sgrButtonLeft, x, y, true)...)
}

// wheelSequence is one wheel notch. up is wheel-up (toward earlier lines).
func wheelSequence(x, y int, up bool) []byte {
	button := sgrButtonWheelDown
	if up {
		button = sgrButtonWheelUp
	}
	return sgrMouse(button, x, y, false)
}

// sidebarAimWidth matches ui.sidebarWidth so a named sidebar click stays in
// the sidebar column. The driven terminal's column count is cols.
func sidebarAimWidth(cols int) int {
	value := cols / 4
	if value < 16 {
		value = 16
	}
	if value > 30 {
		value = 30
	}
	return value
}

// membersAimWidth is the preferred member column width (ui.membersWidth).
const membersAimWidth = 22

// aimedColumns is the sidebar and member column spans for a channel whose
// sidebar and member list are wanted. It copies ui.columnLayout's constants
// (sidebar 16..30, transcript minimum 45, members preferred 22 and minimum 12)
// so a named click lands in the column the shell drew. This package stays free
// of internal/ui.
func aimedColumns(cols int) (sidebarX1, membersX0, membersX1 int) {
	if cols < 1 {
		return 0, 0, 0
	}
	sidebar := cols / 4
	if sidebar < 16 {
		sidebar = 16
	}
	if sidebar > 30 {
		sidebar = 30
	}
	members := 22
	transcript := cols - sidebar - members
	if transcript < 45 {
		transcript = 45
		if transcript > cols {
			transcript = cols
		}
		remaining := cols - transcript
		if sidebar > remaining {
			sidebar = remaining
		}
		remaining -= sidebar
		if remaining >= 12 {
			members = 22
			if members > remaining {
				members = remaining
			}
			if members < 12 {
				members = 0
			}
		} else {
			members = 0
		}
		transcript = cols - sidebar - members
		if transcript < 1 {
			transcript = 1
		}
	}
	membersX0 = sidebar + transcript
	if members < 1 {
		return sidebar, membersX0, membersX0
	}
	return sidebar, membersX0, membersX0 + members
}

// findInColumns returns the cell of the first name match whose column is in
// [x0, x1). rightmost picks the last such match instead, which is how a member
// nick is told apart from the same nick in the transcript. The column is a
// cell index, so a wide rune before the name does not shift the click.
func findInColumns(lines []string, name string, x0, x1 int, rightmost bool) (x, y int, ok bool) {
	if name == "" || x1 <= x0 {
		return 0, 0, false
	}
	found := false
	for row, line := range lines {
		start := 0
		for {
			rel := strings.Index(line[start:], name)
			if rel < 0 {
				break
			}
			at := start + rel
			cell := runewidth.StringWidth(line[:at])
			if cell >= x0 && cell < x1 {
				x, y, found = cell, row, true
				if !rightmost {
					return x, y, true
				}
			}
			start = at + len(name)
			if start >= len(line) {
				break
			}
		}
	}
	return x, y, found
}
