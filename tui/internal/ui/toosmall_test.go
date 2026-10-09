package ui

import (
	"fmt"
	"math"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
)

func TestTerminalTooSmallNotice(t *testing.T) {
	minW, minH := TerminalMinWidth(), TerminalMinHeight()
	cases := []struct {
		name   string
		width  int
		height int
	}{
		{"30x8", 30, 8},
		{"under floor", minW - 1, minH - 1},
		{"18x8", 18, 8},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			m := resizeModel(t, seededModel(t), tc.width, tc.height)
			view := m.View()
			plain := ansiPattern.ReplaceAllString(view.Content, "")
			for _, want := range []string{
				"Terminal too small",
				fmtSize(tc.width, tc.height),
				fmt.Sprintf("Minimum %dx%d", minW, minH),
			} {
				if !strings.Contains(plain, want) {
					t.Fatalf("notice missing %q:\n%s", want, plain)
				}
			}
			for _, absent := range []string{"irc.example · fred", "Shortcuts"} {
				if strings.Contains(plain, absent) {
					t.Fatalf("shell chrome %q in notice frame:\n%s", absent, plain)
				}
			}
			lines := strings.Split(view.Content, "\n")
			if len(lines) != tc.height {
				t.Fatalf("frame height = %d, want %d", len(lines), tc.height)
			}
			for index, line := range lines {
				if got := lipgloss.Width(line); got != tc.width {
					t.Fatalf("row %d width = %d, want %d", index, got, tc.width)
				}
			}
			for _, substr := range []string{fmtSize(tc.width, tc.height), fmt.Sprintf("Minimum %dx%d", minW, minH)} {
				line := noticeLineContaining(lines, substr)
				if line == "" {
					t.Fatalf("missing centered line for %q", substr)
				}
				assertLineCentered(t, line, tc.width, substr)
			}
			if view.WindowTitle != Title(m.ctrl, nil) {
				t.Fatalf("WindowTitle = %q, want normal title %q", view.WindowTitle, Title(m.ctrl, nil))
			}
			if view.Cursor != nil {
				t.Fatalf("Cursor = %v, want nil on the notice", view.Cursor)
			}
		})
	}
}

func fmtSize(width, height int) string {
	return fmt.Sprintf("%dx%d", width, height)
}

func noticeLineContaining(lines []string, substr string) string {
	for _, line := range lines {
		plain := ansiPattern.ReplaceAllString(line, "")
		if strings.Contains(plain, substr) {
			return line
		}
	}
	return ""
}

func leadingDisplaySpaces(line string) int {
	plain := ansiPattern.ReplaceAllString(line, "")
	n := 0
	for _, r := range plain {
		if r != ' ' {
			break
		}
		n++
	}
	return n
}

func centerSplitLeft(gap int) int {
	split := int(math.Round(float64(gap) * 0.5))
	return gap - split
}

func assertLineCentered(t *testing.T, line string, width int, label string) {
	t.Helper()
	plain := strings.TrimSpace(ansiPattern.ReplaceAllString(line, ""))
	contentW := lipgloss.Width(plain)
	if contentW >= width {
		t.Fatalf("%q line is %d cells wide, expected shorter than %d", label, contentW, width)
	}
	gap := width - contentW
	wantLeft := centerSplitLeft(gap)
	gotLeft := leadingDisplaySpaces(line)
	if gotLeft != wantLeft {
		t.Fatalf("%q leading spaces = %d, want %d (center split for gap %d):\n%s", label, gotLeft, wantLeft, gap, plain)
	}
	if gotLeft == 0 {
		t.Fatalf("%q is flush left, want centered:\n%s", label, plain)
	}
}

func TestTerminalAtMinimumShowsShell(t *testing.T) {
	m := resizeModel(t, seededModel(t), TerminalMinWidth(), TerminalMinHeight())
	m.serverListVisible = false
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if strings.Contains(plain, "Terminal too small") {
		t.Fatalf("minimum size must render the shell:\n%s", plain)
	}
	if !strings.Contains(plain, "sold.") {
		t.Fatalf("shell missing seeded transcript at minimum:\n%s", plain)
	}
}

func TestTerminalAboveMinimumShowsShell(t *testing.T) {
	m := seededModel(t)
	plain := ansiPattern.ReplaceAllString(m.View().Content, "")
	if strings.Contains(plain, "Terminal too small") {
		t.Fatalf("default size must render the shell:\n%s", plain)
	}
}

func TestTerminalResizeBelowAndBackRestoresFrame(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("draft stays")
	m.transcriptScroll = 3
	before := m.ctrl.SelectedConversationID()
	m.openJump()
	beforeView := m.View().Content

	shrunk := resizeModel(t, m, 30, 8)
	if shrunk.composer.Value() != "draft stays" {
		t.Fatalf("draft after shrink = %q, want draft stays", shrunk.composer.Value())
	}
	if shrunk.transcriptScroll != 3 {
		t.Fatalf("scroll after shrink = %d, want 3", shrunk.transcriptScroll)
	}
	if !shrunk.jump.open {
		t.Fatal("jump overlay must stay open in state while the notice shows")
	}

	restored := resizeModel(t, shrunk, 118, 30)
	if restored.composer.Value() != "draft stays" {
		t.Fatalf("draft after restore = %q, want draft stays", restored.composer.Value())
	}
	if restored.transcriptScroll != 3 {
		t.Fatalf("scroll after restore = %d, want 3", restored.transcriptScroll)
	}
	if restored.ctrl.SelectedConversationID() != before {
		t.Fatalf("conversation after restore = %q, want %q", restored.ctrl.SelectedConversationID(), before)
	}
	if !restored.jump.open {
		t.Fatal("jump overlay must return after growing past the minimum")
	}
	if restored.View().Content != beforeView {
		t.Fatalf("restored frame differs from before shrink")
	}
}

func TestCtrlQAtTooSmallSizeQuits(t *testing.T) {
	m := resizeModel(t, seededModel(t), 30, 8)
	_, cmd := m.Update(ctrlKey('q'))
	if cmd == nil {
		t.Fatal("ctrl+q cmd = nil, want a quit command")
	}
	if _, ok := cmd().(tea.QuitMsg); !ok {
		t.Fatalf("ctrl+q message = %T, want tea.QuitMsg", cmd())
	}
}
