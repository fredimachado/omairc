package ui

import (
	"regexp"
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// ansiPattern strips the SGR sequences lipgloss emits, so a test can assert the
// visible text of a rendered field.
var ansiPattern = regexp.MustCompile("\x1b\\[[0-9;]*m")

// TestNewComposerInputAppliesSharedStyles pins the composer contract: the input
// owns the prompt and placeholder from the shared text-input styles and uses the
// real terminal cursor, so composerCursor can hand it to View.
func TestNewComposerInputAppliesSharedStyles(t *testing.T) {
	input := newComposerInput(defaultStyles())
	if input.Prompt != composerPrompt {
		t.Fatalf("Prompt = %q, want %q", input.Prompt, composerPrompt)
	}
	if input.Placeholder != composerPlaceholder {
		t.Fatalf("Placeholder = %q, want %q", input.Placeholder, composerPlaceholder)
	}
	if input.VirtualCursor() {
		t.Fatal("composer must use the real terminal cursor")
	}

	input.SetWidth(20)
	_ = input.Focus()
	view := ansiPattern.ReplaceAllString(input.View(), "")
	if !strings.Contains(view, composerPrompt) {
		t.Fatalf("View missing prompt %q:\n%s", composerPrompt, view)
	}
	if !strings.Contains(view, composerPlaceholder) {
		t.Fatalf("View missing placeholder %q:\n%s", composerPlaceholder, view)
	}
}

// TestComposerCursorFollowsFocusAndRow pins that the composer exposes its
// terminal cursor at the frame row supplied by model.go's View, offsets it past
// the prompt, and hides it while the composer is blurred.
func TestComposerCursorFollowsFocusAndRow(t *testing.T) {
	m := New(controller.New(), nil)
	m.composer = newComposerInput(m.styles)
	_ = m.composer.Focus()

	cursor := m.composerCursor(5)
	if cursor == nil {
		t.Fatal("focused composer cursor = nil, want a cursor")
	}
	if cursor.Position.Y != 5 {
		t.Fatalf("cursor row = %d, want 5", cursor.Position.Y)
	}

	m.composer.SetValue("hello")
	m.composer.CursorEnd()
	cursor = m.composerCursor(5)
	if cursor == nil {
		t.Fatal("cursor after typing = nil, want a cursor")
	}
	// composerPrompt is two cells wide, so the caret sits two cells past the text.
	if want := composerPrefixWidth + len("hello"); cursor.Position.X != want {
		t.Fatalf("cursor column = %d, want %d", cursor.Position.X, want)
	}

	m.composer.Blur()
	if got := m.composerCursor(5); got != nil {
		t.Fatalf("blurred composer cursor = %v, want nil", got)
	}
}
