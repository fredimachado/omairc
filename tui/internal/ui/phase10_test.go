package ui

// This file is the Phase 10 shell suite: transcript emphasis rendering, the
// sidebar identicon/half-block avatar glyph, the AvatarReadyMsg/avatar seam,
// and the Ctrl+Shift+O link sheet over the seeded and demo models. It reuses
// seededModel (phase5_test.go), press/ctrlShiftKey (phase6_test.go), and
// phase9DemoModel (phase9_inbox_test.go).

import (
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
)

// fakeAvatarSource is an AvatarSource with a configurable block. Ensure records
// every scheduled URL; Block reports the configured block when set.
type fakeAvatarSource struct {
	block   string
	ensured []string
}

func (f *fakeAvatarSource) Ensure(rawURL string, _ int) {
	f.ensured = append(f.ensured, rawURL)
}

func (f *fakeAvatarSource) Block(string, int, int) (string, bool) {
	if f.block == "" {
		return "", false
	}
	return f.block, true
}

// TestPhase10AvatarGlyphShowsHalfBlockOrIdenticon proves the direct-message row
// uses a cached half-block glyph when the source has one and the nick
// identicon otherwise, while channel rows carry no glyph at all.
func TestPhase10AvatarGlyphShowsHalfBlockOrIdenticon(t *testing.T) {
	row := controller.ConversationSnapshot{
		Conversation: "mira",
		Direct:       true,
		Avatar:       "https://cdn.example/mira.png",
	}

	m := seededModel(t)
	source := &fakeAvatarSource{block: "\x1b[38;2;1;1;1m▀\x1b[0m"}
	m.SetAvatarSource(source)
	if got := m.conversationRow(row); !strings.Contains(got, "▀") {
		t.Fatalf("conversationRow with a cached avatar = %q, want the half-block glyph", got)
	}

	plain := seededModel(t)
	got := plain.conversationRow(row)
	if !strings.Contains(got, "M") {
		t.Fatalf("conversationRow without a source = %q, want the identicon initial M", got)
	}
	if strings.Contains(got, "▀") {
		t.Fatalf("conversationRow without a source = %q, must not carry a half-block", got)
	}

	channel := controller.ConversationSnapshot{Conversation: "#omarchy", Direct: false}
	if glyph := plain.avatarGlyph(channel); glyph != "" {
		t.Fatalf("channel avatarGlyph = %q, want empty", glyph)
	}
}

// TestPhase10EnsureAvatarsSchedulesDirectRows proves the /pref-gated fetch
// scheduling walks the direct rows and ignores channels.
func TestPhase10EnsureAvatarsSchedulesDirectRows(t *testing.T) {
	m := seededModel(t)
	source := &fakeAvatarSource{}
	m.SetAvatarSource(source)

	// A NotifyMsg is the conversation/peer metadata change that reschedules.
	updated, _ := m.Update(NotifyMsg{})
	m = updated.(*Model)
	if len(source.ensured) == 0 {
		t.Fatalf("ensureAvatars scheduled no direct-message avatars")
	}
	for _, raw := range source.ensured {
		if raw == "" {
			t.Fatalf("ensureAvatars scheduled an empty URL: %v", source.ensured)
		}
	}

	// AvatarReadyMsg is a no-op that leaves the model usable.
	updated, cmd := m.Update(AvatarReadyMsg{})
	if updated != m || cmd != nil {
		t.Fatalf("AvatarReadyMsg = (%T, %v), want the same model and no command", updated, cmd)
	}
}

// TestPhase10IdenticonDeterminism proves the hashing, color, initials, and fill
// helpers are deterministic and self-consistent with the palette.
func TestPhase10IdenticonDeterminism(t *testing.T) {
	for _, nick := range []string{"mira", "anna", "dax", "Ω", ""} {
		index := nickPaletteIndex(nick)
		if index < 0 || index >= len(nickPalette) {
			t.Fatalf("nickPaletteIndex(%q) = %d, want [0,%d)", nick, index, len(nickPalette))
		}
		if again := nickPaletteIndex(nick); again != index {
			t.Fatalf("nickPaletteIndex(%q) is not deterministic: %d then %d", nick, index, again)
		}
		if got := nickColor(nick); got != nickPalette[index] {
			t.Fatalf("nickColor(%q) = %q, want palette[%d] = %q", nick, got, index, nickPalette[index])
		}
	}

	if got := initials("mira"); got != "M" {
		t.Fatalf("initials(mira) = %q, want M", got)
	}
	if got := initials(""); got != "?" {
		t.Fatalf("initials(\"\") = %q, want ?", got)
	}
	if got, want := avatarFill("mira"), nickColor("mira"); got == want {
		t.Fatalf("avatarFill(mira) = %q, want it mixed away from nickColor", got)
	}
}

// TestPhase10TranscriptEmphasis proves a bold body renders the styled text with
// no mIRC control bytes, and that the plain haystack matches what is shown.
func TestPhase10TranscriptEmphasis(t *testing.T) {
	m := seededModel(t)
	msg := controller.MessageSnapshot{
		Author: "anna",
		Kind:   "message",
		Body:   "\x02bold\x02 plain",
		Time:   time.Date(2026, 9, 27, 12, 0, 0, 0, time.UTC),
	}

	row := m.messageRow(0, msg)
	for _, want := range []string{"bold", " plain"} {
		if !strings.Contains(row, want) {
			t.Fatalf("messageRow = %q, want it to contain %q", row, want)
		}
	}
	for _, code := range []string{"\x02", "\x03", "\x1d", "\x1f"} {
		if strings.Contains(row, code) {
			t.Fatalf("messageRow = %q, must not contain the control byte %q", row, code)
		}
	}

	if got := m.transcriptLine(msg); got != "12:00 anna bold plain" {
		t.Fatalf("transcriptLine = %q, want %q", got, "12:00 anna bold plain")
	}
	if !m.ctrl.HasIrcEmphasis(msg.Body) {
		t.Fatalf("HasIrcEmphasis(%q) = false, want true", msg.Body)
	}
	if m.ctrl.HasIrcEmphasis("plain") {
		t.Fatal("HasIrcEmphasis(plain) = true, want false")
	}
}

// TestPhase10LinkSheetFilterWrapAndOpen drives the link sheet over a seeded
// model: the filter narrows, Up/Down wrap, Enter opens under the suppression
// latch, and Escape leaves the composer draft alone.
func TestPhase10LinkSheetFilterWrapAndOpen(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("keep me")
	m.saveDraft()
	draft := m.composer.Value()

	if !m.ctrl.SendMessage("see https://a.example/x and https://b.example/y") {
		t.Fatal("SendMessage refused the URL message")
	}

	m = press(t, m, ctrlShiftKey('o'))
	if !m.linkVisible() {
		t.Fatal("Ctrl+Shift+O must open the link sheet")
	}
	if got := len(m.linkMatches()); got != 2 {
		t.Fatalf("linkMatches = %d, want 2", got)
	}

	m.link.input.SetValue("b.example")
	matches := m.linkMatches()
	if len(matches) != 1 || matches[0].value != "https://b.example/y" {
		t.Fatalf("filtered linkMatches = %+v, want only the b URL", matches)
	}
	m.link.input.SetValue("")
	if got := len(m.linkMatches()); got != 2 {
		t.Fatalf("cleared-filter linkMatches = %d, want 2", got)
	}

	m.link.selected = 0
	m.moveLink(-1)
	if m.link.selected != 1 {
		t.Fatalf("moveLink(-1) from 0 = %d, want 1 (wrap)", m.link.selected)
	}
	m.moveLink(1)
	if m.link.selected != 0 {
		t.Fatalf("moveLink(1) from 1 = %d, want 0 (wrap)", m.link.selected)
	}

	m.link.selected = 0
	m.suppressExternalOpen = true
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if m.linkVisible() {
		t.Fatal("Enter must close the link sheet")
	}
	if m.lastOpenedURL != "https://b.example/y" {
		t.Fatalf("lastOpenedURL = %q, want the b URL", m.lastOpenedURL)
	}

	m = press(t, m, ctrlShiftKey('o'))
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.linkVisible() {
		t.Fatal("Escape must close the link sheet")
	}
	if got := m.composer.Value(); got != draft {
		t.Fatalf("composer draft = %q, want unchanged %q", got, draft)
	}
}

// TestPhase10LinkSheetInviteJoins proves an INVITE status line becomes an
// invite row and that activating it sends a JOIN.
func TestPhase10LinkSheetInviteJoins(t *testing.T) {
	m, d := phase9DemoModel(t)
	d.InjectOmarchy([]byte(":alice!u@h INVITE fred :#invited\r\n"))

	m = press(t, m, tea.KeyPressMsg{Code: '`', Mod: tea.ModCtrl})
	if !m.ctrl.ConsoleOpen() {
		t.Fatal("Ctrl+` must open the Status console")
	}

	m = press(t, m, ctrlShiftKey('o'))
	if !m.linkVisible() {
		t.Fatal("Ctrl+Shift+O must open the link sheet on Status")
	}
	matches := m.linkMatches()
	index := -1
	for i, match := range matches {
		if match.kind == linkKindInvite && match.value == "#invited" {
			index = i
		}
	}
	if index < 0 {
		t.Fatalf("linkMatches = %+v, want an invite row for #invited", matches)
	}

	m.link.selected = index
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if m.linkVisible() {
		t.Fatal("Enter must close the link sheet")
	}
	if !writtenOmarchyFramesContain(d, "JOIN #invited") {
		t.Fatalf("no JOIN #invited frame: %v", d.OmarchyTransport().WrittenFrames())
	}
}
