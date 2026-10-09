package ui

import (
	"fmt"
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
	lines := []string{
		m.styles.Empty.Render("Terminal too small"),
		m.styles.Empty.Render(fmt.Sprintf("%dx%d", width, height)),
		m.styles.Empty.Render(fmt.Sprintf("Minimum %dx%d", TerminalMinWidth(), TerminalMinHeight())),
	}
	block := strings.Join(lines, "\n")
	placed := lipgloss.Place(width, height, lipgloss.Center, lipgloss.Center, block)
	return fitBlock(placed, width, height)
}
