package ui

import (
	"fmt"
	"math"
	"strings"

	"charm.land/lipgloss/v2"
)

// transcriptColumnMin is the transcript column width the minimum terminal size
// reserves when the server list is collapsed, so the middle column still fits
// about this many cells of transcript content.
const transcriptColumnMin = 20

// transcriptTopicHeaderRows is the pinned header block when a topic (or Status
// refusal line) is shown: one caption line and one trailing blank.
const transcriptTopicHeaderRows = 2

// transcriptScrollRowsMin is the scrolling transcript row budget the minimum
// height keeps after that header, the composer block, and the footer.
const transcriptScrollRowsMin = 3

// TerminalMinWidth returns the smallest terminal width that still renders the
// normal shell with the sidebar collapsed.
func TerminalMinWidth() int {
	return transcriptColumnMin
}

// TerminalMinHeight returns the smallest terminal height that still renders the
// normal shell. It reserves a topic header, three scrolling transcript rows,
// the composer block, and the footer so a topic still leaves three rows to
// scroll.
func TerminalMinHeight() int {
	return transcriptTopicHeaderRows + transcriptScrollRowsMin + composerFieldHeight + footerHeight
}

func (m *Model) terminalTooSmall() bool {
	if m == nil {
		return true
	}
	return m.width < TerminalMinWidth() || m.height < TerminalMinHeight()
}

func (m *Model) terminalTooSmallView() string {
	width := m.width
	height := m.height
	if width <= 0 || height <= 0 {
		return ""
	}
	lines := []string{
		m.styles.Empty.Render("Terminal too small"),
		m.styles.Empty.Render(fmt.Sprintf("%dx%d", width, height)),
		m.styles.Empty.Render(fmt.Sprintf("Minimum %dx%d", TerminalMinWidth(), TerminalMinHeight())),
	}
	block := strings.Join(lines, "\n")
	placed := lipgloss.Place(width, height, lipgloss.Center, lipgloss.Center, block)
	return fitNoticeBlock(placed, width, height)
}

// fitNoticeBlock sizes the too-small notice to exactly width cells by height
// lines. lipgloss.Place skips horizontal padding when a line is at least as
// wide as the terminal, so every row is padded or truncated here instead of
// using fitBlock's 1x1 floor on a non-positive size.
func fitNoticeBlock(content string, width, height int) string {
	if width <= 0 || height <= 0 {
		return ""
	}
	raw := strings.Split(content, "\n")
	if len(raw) > height {
		raw = raw[:height]
	}
	blank := strings.Repeat(" ", width)
	out := make([]string, 0, height)
	for _, line := range raw {
		out = append(out, padLineToWidth(line, width))
	}
	for len(out) < height {
		out = append(out, blank)
	}
	return strings.Join(out, "\n")
}

func padLineToWidth(line string, width int) string {
	if width <= 0 {
		return ""
	}
	line = truncateLine(line, width)
	got := lipgloss.Width(line)
	if got < width {
		gap := width - got
		split := int(math.Round(float64(gap) * float64(lipgloss.Center)))
		left := gap - split
		right := gap - left
		line = strings.Repeat(" ", left) + line + strings.Repeat(" ", right)
	}
	return line
}
