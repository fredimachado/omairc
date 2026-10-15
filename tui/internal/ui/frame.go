package ui

import (
	"strings"

	"charm.land/lipgloss/v2"
)

// frameLabelEdge says which edge carries the position counter. The bottom-right
// is the member panel and the overlay cards. The transcript has no frame of its
// own, so its counter sits on the composer frame's top edge, which is the
// bottom of the transcript area.
type frameLabelEdge int

const (
	frameLabelBottom frameLabelEdge = iota
	frameLabelTop
)

const (
	edgeRule = iota
	edgeTitle
	edgeLabel
)

// edgeSegment is one run of an edge: rule glyphs, the title, or the counter.
// The runs are styled separately and their display widths sum to the interior.
type edgeSegment struct {
	text string
	kind int
}

// roundedFrame draws a one-cell rounded border of exactly width by height.
// title sits on the top edge after a leading rule ("╭─ Title ──╮"). label sits
// on the bottom-right edge, or on the top-right when edge is frameLabelTop
// ("╰── 3 of 12 ╯"). Either string may be empty, which leaves that edge a solid
// rule. A label that does not fit is ellipsized; the frame never grows or
// shrinks to make room. focused paints the border, and the title, in the
// focused border colour.
//
// A non-positive width or height is an empty frame: there is no positive area
// to close. A width or height of 1 or 2 still returns exactly that size, closed
// on the corners that fit.
func (m *Model) roundedFrame(content string, width, height int, title, label string, focused bool, edge frameLabelEdge) string {
	if m == nil || width <= 0 || height <= 0 {
		return ""
	}
	border := m.frameBorderStyle(focused)
	titleStyle := m.frameTitleStyle(focused)
	labelStyle := m.frameLabelStyle(focused)

	topRight := ""
	bottomRight := label
	if edge == frameLabelTop {
		topRight = label
		bottomRight = ""
	}
	// One row can only be the top edge. A bottom counter moves up onto it when
	// there is no title already claiming that edge.
	if height == 1 && bottomRight != "" && title == "" && topRight == "" {
		topRight = bottomRight
		bottomRight = ""
	}

	lines := make([]string, 0, height)
	lines = append(lines, renderEdge("╭", "╮", width, title, topRight, border, titleStyle, labelStyle))
	if height == 1 {
		return lines[0]
	}
	innerWidth := width - 2
	if innerWidth < 0 {
		innerWidth = 0
	}
	body := fitFrameBody(content, innerWidth, height-2)
	for _, line := range body {
		lines = append(lines, frameBodyLine(line, width, border))
	}
	lines = append(lines, renderEdge("╰", "╯", width, "", bottomRight, border, titleStyle, labelStyle))
	return strings.Join(lines, "\n")
}

// frameBorderStyle is the rule colour: the focused border while the panel has
// focus, otherwise the plain border.
func (m *Model) frameBorderStyle(focused bool) lipgloss.Style {
	ink := m.styles.Colors.Border
	if focused {
		ink = m.styles.Colors.BorderFocus
	}
	return lipgloss.NewStyle().Foreground(ink)
}

// frameTitleStyle is the title's ink. Focus paints it with the focused border
// colour so the title reads as part of the lit edge; otherwise it keeps the
// panel-title ink, which is brighter than the muted rule.
func (m *Model) frameTitleStyle(focused bool) lipgloss.Style {
	if focused {
		return m.frameBorderStyle(true).Bold(true)
	}
	return m.styles.PanelTitle
}

// frameLabelStyle keeps the counter in the rule colour so it sits in the edge
// rather than reading as another content row.
func (m *Model) frameLabelStyle(focused bool) lipgloss.Style {
	return m.frameBorderStyle(focused)
}

// renderEdge builds one horizontal edge of exactly width cells.
func renderEdge(leftCorner, rightCorner string, width int, left, right string, border, leftStyle, rightStyle lipgloss.Style) string {
	if width <= 0 {
		return ""
	}
	if width == 1 {
		return border.Render(leftCorner)
	}
	var b strings.Builder
	b.WriteString(border.Render(leftCorner))
	for _, seg := range composeInterior(width-2, left, right) {
		if seg.text == "" {
			continue
		}
		style := border
		switch seg.kind {
		case edgeTitle:
			style = leftStyle
		case edgeLabel:
			style = rightStyle
		}
		b.WriteString(style.Render(seg.text))
	}
	b.WriteString(border.Render(rightCorner))
	return b.String()
}

// edgePad is one way to decorate an edge, from the most spacious form down to
// a bare label. composeInterior keeps the first form whose pieces sum to the
// interior width.
type edgePad struct {
	beforeLeft  string
	afterLeft   string
	beforeRight string
	afterRight  string
	minRule     int
}

// edgePads prefers "─ Title ── label " and sheds the gaps when the interior is
// too short for them. The last form is the ellipsized text alone.
var edgePads = []edgePad{
	{"─ ", " ", " ", " ", 1},
	{"─ ", " ", " ", "", 0},
	{"─", "", " ", "", 0},
	{"", "", "", "", 0},
}

// composeInterior fills n cells between the corners. left is the title, right
// is the counter. Empty strings leave a solid rule. The result's display width
// is exactly n; a label that does not fit is ellipsized, and if even that
// cannot be placed the edge falls back to a solid rule of length n.
func composeInterior(n int, left, right string) []edgeSegment {
	if n <= 0 {
		return nil
	}
	if left == "" && right == "" {
		return []edgeSegment{{text: strings.Repeat("─", n), kind: edgeRule}}
	}
	for _, pad := range edgePads {
		beforeLeft, afterLeft := pad.beforeLeft, pad.afterLeft
		beforeRight, afterRight := pad.beforeRight, pad.afterRight
		if left == "" {
			beforeLeft, afterLeft = "", ""
		}
		if right == "" {
			beforeRight, afterRight = "", ""
		}
		chrome := lipgloss.Width(beforeLeft) + lipgloss.Width(afterLeft) +
			lipgloss.Width(beforeRight) + lipgloss.Width(afterRight)
		minRule := pad.minRule
		if chrome+minRule > n {
			continue
		}
		avail := n - chrome - minRule
		fittedLeft, fittedRight := splitEdgeText(left, right, avail)
		extra := avail - lipgloss.Width(fittedLeft) - lipgloss.Width(fittedRight)
		if extra < 0 {
			continue
		}
		segs := make([]edgeSegment, 0, 6)
		segs = appendEdge(segs, beforeLeft, edgeRule)
		segs = appendEdge(segs, fittedLeft, edgeTitle)
		segs = appendEdge(segs, afterLeft, edgeRule)
		segs = appendEdge(segs, strings.Repeat("─", minRule+extra), edgeRule)
		segs = appendEdge(segs, beforeRight, edgeRule)
		segs = appendEdge(segs, fittedRight, edgeLabel)
		segs = appendEdge(segs, afterRight, edgeRule)
		if edgeWidth(segs) == n {
			return segs
		}
	}
	return []edgeSegment{{text: strings.Repeat("─", n), kind: edgeRule}}
}

func appendEdge(segs []edgeSegment, text string, kind int) []edgeSegment {
	if text == "" {
		return segs
	}
	return append(segs, edgeSegment{text: text, kind: kind})
}

func edgeWidth(segs []edgeSegment) int {
	total := 0
	for _, seg := range segs {
		total += lipgloss.Width(seg.text)
	}
	return total
}

// splitEdgeText fits left and right into avail cells. The counter keeps its
// cells first, because a truncated position is more useful than a truncated
// title beside a missing counter, and the title takes whatever remains.
func splitEdgeText(left, right string, avail int) (string, string) {
	if avail <= 0 {
		return "", ""
	}
	if left == "" {
		return "", fitPlain(right, avail)
	}
	if right == "" {
		return fitPlain(left, avail), ""
	}
	rightWidth := lipgloss.Width(right)
	if rightWidth >= avail {
		return "", fitPlain(right, avail)
	}
	return fitPlain(left, avail-rightWidth), right
}

// fitPlain ellipsizes text to width cells. A string that already fits is
// returned unchanged, so a short title is not padded with spaces.
func fitPlain(text string, width int) string {
	if text == "" || width <= 0 {
		return ""
	}
	if lipgloss.Width(text) <= width {
		return text
	}
	return ellipsizeLine(text, width)
}

// fitFrameBody returns exactly height lines, each exactly width cells. Extra
// lines are dropped from the tail and short blocks are padded, so a long or
// short body cannot change the frame.
func fitFrameBody(content string, width, height int) []string {
	if height <= 0 {
		return nil
	}
	if width < 0 {
		width = 0
	}
	var lines []string
	if content != "" {
		lines = strings.Split(content, "\n")
	}
	if len(lines) > height {
		lines = lines[:height]
	}
	out := make([]string, 0, height)
	for _, line := range lines {
		out = append(out, fitFrameLine(line, width))
	}
	blank := strings.Repeat(" ", width)
	for len(out) < height {
		out = append(out, blank)
	}
	return out
}

// fitFrameLine clips or pads one content line to width cells without styling it.
func fitFrameLine(line string, width int) string {
	if width <= 0 {
		return ""
	}
	if lipgloss.Width(line) > width {
		line = truncateLine(line, width)
	}
	if pad := width - lipgloss.Width(line); pad > 0 {
		line += strings.Repeat(" ", pad)
	}
	return line
}

// frameBodyLine wraps one fitted content line in vertical rules. width is the
// outer width, including the two rules. A one-cell column is just the rule.
func frameBodyLine(line string, width int, border lipgloss.Style) string {
	if width <= 0 {
		return ""
	}
	if width == 1 {
		return border.Render("│")
	}
	return border.Render("│") + line + border.Render("│")
}
