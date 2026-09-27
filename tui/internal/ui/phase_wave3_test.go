package ui

import (
	"strings"
	"testing"

	"charm.land/lipgloss/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// This file covers the Wave 3 cross-workstream wiring that no single
// workstream could exercise: the composer's real cursor row, the single
// overlay-card framing path, and the member panel's one inner rule.

// TestComposerCursorRowMatchesFrame pins that View places the composer's real
// cursor on the composer row: above the footer, below the body and any
// slash-completion rows, and at row 0 on the tiny-terminal composer-only path.
func TestComposerCursorRowMatchesFrame(t *testing.T) {
	m := seededModel(t)
	if !m.footerVisible() {
		t.Fatal("footer must be visible at the seeded size")
	}
	cursor := m.View().Cursor
	if cursor == nil {
		t.Fatal("focused composer must expose a terminal cursor")
	}
	if want := m.composerRow(); cursor.Position.Y != want {
		t.Fatalf("cursor row = %d, want composerRow() %d", cursor.Position.Y, want)
	}
	// The composer's text row sits composerFieldPad rows above the block's
	// bottom padding row, which is itself just above the footer.
	if want := m.height - 1 - footerHeight - composerFieldPad; cursor.Position.Y != want {
		t.Fatalf("cursor row = %d, want %d with the footer", cursor.Position.Y, want)
	}

	// A modal overlay blurs the composer, so the cursor disappears.
	m.openJump()
	if m.View().Cursor != nil {
		t.Fatal("an open overlay must hide the composer cursor")
	}

	// With slash-completion rows crowding out the footer, the composer block is
	// the last thing on the grid instead of sitting above the footer.
	short := resizeModel(t, seededModel(t), defaultWidth, minHeight)
	short.slash.probe = controller.SlashProbe{Open: true, Needle: "/", Hits: []controller.SlashHit{
		{Label: "/a", Usage: "one"},
		{Label: "/b", Usage: "two"},
		{Label: "/c", Usage: "three"},
	}}
	if short.footerVisible() {
		t.Fatal("footer must drop when the slash rows fill the window")
	}
	if want := short.height - 1 - composerFieldPad; short.View().Cursor == nil ||
		short.View().Cursor.Position.Y != want {
		t.Fatalf("no-footer cursor = %v, want row %d", short.View().Cursor, want)
	}
	// The menu is capped so the frame still fits the window exactly.
	if got, want := len(strings.Split(short.render(), "\n")), short.height; got != want {
		t.Fatalf("cramped frame = %d rows, want the %d-row window", got, want)
	}

	// The tiny-terminal path renders only the composer, so its row is 0.
	tiny := resizeModel(t, seededModel(t), minWidth-1, minHeight-1)
	if got := tiny.View().Cursor; got == nil || got.Position.Y != 0 {
		t.Fatalf("tiny-terminal cursor = %v, want row 0", got)
	}
}

// TestOverlayCardInterface pins the Wave 3 overlay contract: every overlay has
// one <name>Card(width) block method that frames through the shared
// overlayCardBlock, so every card shares one outer width and one panel border,
// and overlayCard() returns that same block.
func TestOverlayCardInterface(t *testing.T) {
	m := seededModel(t)
	wantWidth := m.width - 2*overlayCardInset
	cases := []struct {
		name string
		card string
	}{
		{"shortcuts", m.shortcutsCard(m.width)},
		{"jump", m.jumpCard(m.width)},
		{"nick", m.nickCard(m.width)},
		{"link", m.linkCard(m.width)},
		{"inbox", m.inboxCard(m.width)},
	}
	for _, c := range cases {
		plain := strings.TrimRight(ansiPattern.ReplaceAllString(c.card, ""), "\n")
		if got := lipgloss.Width(c.card); got != wantWidth {
			t.Fatalf("%s card width = %d, want the shared outer width %d", c.name, got, wantWidth)
		}
		if !strings.HasPrefix(plain, "╭") || !strings.HasSuffix(plain, "╯") {
			t.Fatalf("%s card must be the shared panel frame:\n%s", c.name, c.card)
		}
	}

	// overlayCard() delegates to the open overlay's card method.
	m.openJump()
	card, ok := m.overlayCard()
	if !ok || card != m.jumpCard(m.width) {
		t.Fatalf("overlayCard() must delegate to jumpCard; ok=%v", ok)
	}
}

// TestOverlayCardTopIsAnchoredAgainstContent pins the zero-jitter anchor: the
// card's top border sits on the fixed overlayCardTopInset row and does not move
// when the card's own height changes. Centering the card on its own height let a
// narrowing jump filter shrink the card and drag it down the body a row at a
// time while the user typed.
func TestOverlayCardTopIsAnchoredAgainstContent(t *testing.T) {
	m := seededModel(t)
	m.openJump()

	topRow := func() int {
		t.Helper()
		for index, line := range strings.Split(m.render(), "\n") {
			plain := []rune(ansiPattern.ReplaceAllString(line, ""))
			if len(plain) > overlayCardInset && plain[overlayCardInset] == '╭' {
				return index
			}
		}
		t.Fatal("no overlay card top border in the render")
		return -1
	}

	wideRow, wideHeight := topRow(), lipgloss.Height(m.jumpCard(m.width))
	m.jump.input.SetValue("#desktop")
	narrowRow, narrowHeight := topRow(), lipgloss.Height(m.jumpCard(m.width))

	if narrowHeight >= wideHeight {
		t.Fatalf("the filter did not shrink the card (%d -> %d lines); the proof needs a height change",
			wideHeight, narrowHeight)
	}
	if wideRow != narrowRow {
		t.Fatalf("card top row moved %d -> %d as its height changed %d -> %d lines",
			wideRow, narrowRow, wideHeight, narrowHeight)
	}
	if want := overlayCardTopInset; wideRow != want {
		t.Fatalf("card top row = %d, want the anchored row %d", wideRow, want)
	}
}

// TestMemberPanelHasSingleInnerRule reconciles L2's heading rule with F2's
// frame: the panel border owns the outer rule and the heading is the one inner
// separator, drawn at the frame's content width, so the column shows neither a
// double rule nor a double inset.
func TestMemberPanelHasSingleInnerRule(t *testing.T) {
	m := seededModel(t)
	framed := m.framedColumn(m.membersView, membersWidth, m.bodyHeight(), false)
	if got := lipgloss.Width(framed); got != membersWidth {
		t.Fatalf("framed member panel width = %d, want the invariant outer width %d", got, membersWidth)
	}
	rules := 0
	for _, line := range strings.Split(ansiPattern.ReplaceAllString(framed, ""), "\n") {
		inner := strings.Trim(line, "│")
		if inner == "" || strings.Trim(inner, "─") != "" {
			continue
		}
		rules++
		if got := lipgloss.Width(inner); got != membersWidth-2 {
			t.Fatalf("member heading rule width = %d, want the inner width %d", got, membersWidth-2)
		}
	}
	if rules != 1 {
		t.Fatalf("framed member panel has %d inner rules, want exactly 1:\n%s", rules, framed)
	}
}
