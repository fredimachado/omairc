package ui

import (
	"fmt"
	"image/color"
	"strings"
	"testing"

	"charm.land/lipgloss/v2"
)

func frameModel() *Model {
	return &Model{styles: defaultStyles()}
}

func framePlain(rendered string) string {
	return ansiPattern.ReplaceAllString(rendered, "")
}

func TestRoundedFrameTitleAndLabelFit(t *testing.T) {
	m := frameModel()
	body := strings.Repeat("x", 21)
	got := m.roundedFrame(body, 23, 3, "Online 12", "3 of 12", false, frameLabelBottom)
	plain := framePlain(got)
	lines := strings.Split(plain, "\n")
	if len(lines) != 3 {
		t.Fatalf("lines = %d, want 3:\n%s", len(lines), plain)
	}
	if lines[0] != "╭─ Online 12 ─────────╮" {
		t.Fatalf("top = %q", lines[0])
	}
	if lines[1] != "│"+body+"│" {
		t.Fatalf("body = %q", lines[1])
	}
	if lines[2] != "╰──────────── 3 of 12 ╯" {
		t.Fatalf("bottom = %q", lines[2])
	}
	assertFrameSize(t, got, 23, 3)
}

func TestRoundedFrameLabelOnTopEdge(t *testing.T) {
	m := frameModel()
	got := m.roundedFrame("field", 23, 3, "", "4 of 48", false, frameLabelTop)
	lines := strings.Split(framePlain(got), "\n")
	if len(lines) != 3 {
		t.Fatalf("lines = %d, want 3", len(lines))
	}
	if lines[0] != "╭──────────── 4 of 48 ╮" {
		t.Fatalf("top = %q", lines[0])
	}
	if lines[2] != "╰─────────────────────╯" {
		t.Fatalf("bottom = %q", lines[2])
	}
	if !strings.Contains(lines[1], "field") {
		t.Fatalf("body = %q, want the field", lines[1])
	}
	assertFrameSize(t, got, 23, 3)
}

func TestRoundedFrameEllipsizesLongTitleAndLabel(t *testing.T) {
	m := frameModel()
	// Interior of 8 cells cannot hold "─ Online 12 ─" whole.
	got := m.roundedFrame("ab", 10, 4, "Online 12", "3 of 12", false, frameLabelBottom)
	plain := framePlain(got)
	if strings.Contains(plain, "Online 12") {
		t.Fatalf("title was not ellipsized:\n%s", plain)
	}
	if strings.Contains(plain, "3 of 12") {
		t.Fatalf("label was not ellipsized:\n%s", plain)
	}
	if !strings.Contains(plain, "…") {
		t.Fatalf("ellipsis missing:\n%s", plain)
	}
	assertFrameSize(t, got, 10, 4)
	assertFrameClosed(t, plain, 10, 4)
}

func TestRoundedFrameKeepsSizeWhenBodyOverflows(t *testing.T) {
	m := frameModel()
	var body strings.Builder
	for i := 0; i < 6; i++ {
		if i > 0 {
			body.WriteByte('\n')
		}
		body.WriteString(strings.Repeat("y", 40))
	}
	got := m.roundedFrame(body.String(), 12, 4, "Title", "", false, frameLabelBottom)
	assertFrameSize(t, got, 12, 4)
	lines := strings.Split(framePlain(got), "\n")
	// Two content rows between the edges; the rest of the body is dropped.
	if strings.Count(lines[1], "y") != 10 || strings.Count(lines[2], "y") != 10 {
		t.Fatalf("content rows were not clipped to the interior:\n%s", framePlain(got))
	}
	if strings.Contains(lines[0], "y") || strings.Contains(lines[3], "y") {
		t.Fatalf("clipped body leaked onto an edge:\n%s", framePlain(got))
	}
}

func TestRoundedFramePadsShortBody(t *testing.T) {
	m := frameModel()
	got := m.roundedFrame("only", 8, 5, "", "", false, frameLabelBottom)
	assertFrameSize(t, got, 8, 5)
	lines := strings.Split(framePlain(got), "\n")
	if !strings.Contains(lines[1], "only") {
		t.Fatalf("short body = %q", lines[1])
	}
	for _, line := range lines[2:4] {
		if strings.Trim(line, "│ ") != "" {
			t.Fatalf("padding row = %q, want a blank interior", line)
		}
	}
}

func TestRoundedFrameFocusedTitleUsesBorderColor(t *testing.T) {
	m := frameModel()
	focused := m.roundedFrame("x", 24, 3, "Online 12", "1 of 12", true, frameLabelBottom)
	plain := m.roundedFrame("x", 24, 3, "Online 12", "1 of 12", false, frameLabelBottom)

	focusInk := m.frameTitleStyle(true).Render("Online 12")
	plainInk := m.frameTitleStyle(false).Render("Online 12")
	if !strings.Contains(focused, focusInk) {
		t.Fatalf("focused title is not the focused border colour:\n%s", focused)
	}
	if strings.Contains(focused, plainInk) {
		t.Fatalf("focused title still uses the unfocused title ink:\n%s", focused)
	}
	if !strings.Contains(plain, plainInk) {
		t.Fatalf("unfocused title is not the panel title ink:\n%s", plain)
	}
	if strings.Contains(plain, focusInk) {
		t.Fatalf("unfocused title uses the focused border colour:\n%s", plain)
	}

	focusBorder := foregroundParams(m.styles.Colors.BorderFocus)
	if !strings.Contains(focused, focusBorder) {
		t.Fatalf("focused frame missing border colour %s", focusBorder)
	}
	if strings.Contains(plain, focusBorder) {
		t.Fatalf("unfocused frame uses the focused border colour")
	}
}

func TestRoundedFrameTinyAndZeroSizes(t *testing.T) {
	m := frameModel()
	if got := m.roundedFrame("x", 0, 4, "Online 12", "1 of 2", true, frameLabelBottom); got != "" {
		t.Fatalf("zero width = %q, want empty", got)
	}
	if got := m.roundedFrame("x", 6, 0, "Online 12", "1 of 2", false, frameLabelBottom); got != "" {
		t.Fatalf("zero height = %q, want empty", got)
	}
	if got := m.roundedFrame("x", -3, -1, "Online", "1 of 2", false, frameLabelBottom); got != "" {
		t.Fatalf("negative size = %q, want empty", got)
	}

	one := m.roundedFrame("too wide", 1, 3, "Online 12", "9 of 9", true, frameLabelBottom)
	assertFrameSize(t, one, 1, 3)
	if framePlain(one) != "╭\n│\n╰" {
		t.Fatalf("width 1 = %q", framePlain(one))
	}

	two := m.roundedFrame("body", 2, 2, "Online 12", "3 of 12", false, frameLabelBottom)
	assertFrameSize(t, two, 2, 2)
	if framePlain(two) != "╭╮\n╰╯" {
		t.Fatalf("width 2 = %q", framePlain(two))
	}
	assertFrameClosed(t, framePlain(two), 2, 2)

	// A single row keeps the corners and ellipsizes onto that one edge.
	row := m.roundedFrame("body", 6, 1, "Online 12", "", false, frameLabelBottom)
	assertFrameSize(t, row, 6, 1)
	plain := framePlain(row)
	if !strings.HasPrefix(plain, "╭") || !strings.HasSuffix(plain, "╮") {
		t.Fatalf("single row = %q", plain)
	}
	if strings.Contains(plain, "Online 12") {
		t.Fatalf("single row kept the full title: %q", plain)
	}
}

func TestRoundedFrameEmptyLabelsAreSolidRules(t *testing.T) {
	m := frameModel()
	got := m.roundedFrame("hi", 8, 3, "", "", false, frameLabelBottom)
	lines := strings.Split(framePlain(got), "\n")
	if lines[0] != "╭──────╮" || lines[2] != "╰──────╯" {
		t.Fatalf("solid edges =\n%s", framePlain(got))
	}
	assertFrameClosed(t, framePlain(got), 8, 3)
}

func assertFrameSize(t *testing.T, rendered string, width, height int) {
	t.Helper()
	if got := lipgloss.Height(rendered); got != height {
		t.Fatalf("height = %d, want %d:\n%s", got, height, framePlain(rendered))
	}
	if got := lipgloss.Width(rendered); got != width {
		t.Fatalf("width = %d, want %d:\n%s", got, width, framePlain(rendered))
	}
	for index, line := range strings.Split(rendered, "\n") {
		if got := lipgloss.Width(line); got != width {
			t.Fatalf("line %d width = %d, want %d: %q", index, got, width, framePlain(line))
		}
	}
}

func assertFrameClosed(t *testing.T, plain string, width, height int) {
	t.Helper()
	if width < 2 || height < 2 {
		return
	}
	lines := strings.Split(plain, "\n")
	if !strings.HasPrefix(lines[0], "╭") || !strings.HasSuffix(lines[0], "╮") {
		t.Fatalf("top edge is not closed: %q", lines[0])
	}
	last := lines[len(lines)-1]
	if !strings.HasPrefix(last, "╰") || !strings.HasSuffix(last, "╯") {
		t.Fatalf("bottom edge is not closed: %q", last)
	}
	for index, line := range lines[1 : len(lines)-1] {
		if !strings.HasPrefix(line, "│") || !strings.HasSuffix(line, "│") {
			t.Fatalf("row %d is not closed: %q", index+1, line)
		}
	}
}

// foregroundParams is the 24-bit SGR fragment lipgloss emits for a foreground.
func foregroundParams(c color.Color) string {
	rgb := color.RGBAModel.Convert(c).(color.RGBA)
	return fmt.Sprintf("38;2;%d;%d;%d", rgb.R, rgb.G, rgb.B)
}
