package ui

import (
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"
	"charm.land/lipgloss/v2"
	"github.com/charmbracelet/colorprofile"

	"github.com/fredimachado/omairc/tui/internal/avatar"
	"github.com/fredimachado/omairc/tui/internal/controller"
)

// recordingGraphics is an AvatarSource that also speaks avatarGraphics, so the
// shell tests can see placeholder cells and the escapes that must stay out of
// the view.
type recordingGraphics struct {
	fakeAvatarSource
	cells   string
	escapes string
	asked   int
	drains  int
	noted   int
	cols    int
	rows    int
}

func (r *recordingGraphics) Placeholder(string, int, int) (string, bool) {
	r.asked++
	if r.cells == "" {
		return "", false
	}
	return r.cells, true
}

func (r *recordingGraphics) DrainEscapes(cols, rows int) string {
	r.drains++
	r.cols = cols
	r.rows = rows
	out := r.escapes
	r.escapes = ""
	return out
}

func (r *recordingGraphics) NoteDrawn() { r.noted++ }

func TestAvatarsOffDrawsNoPlaceholder(t *testing.T) {
	row := controller.ConversationSnapshot{
		Conversation: "mira",
		Direct:       true,
		Avatar:       "https://cdn.example/mira.png",
	}
	m := seededModel(t)
	m.ctrl.SetShowAvatars(false)
	source := &recordingGraphics{cells: string(avatar.Placeholder(1, 2, 1))}
	m.SetAvatarSource(source)

	got := m.conversationRow(row)
	if strings.Contains(got, "\U0010EEEE") || strings.Contains(got, "\x1b_G") {
		t.Fatalf("avatars off drew a placeholder or an escape: %q", got)
	}
	if !strings.Contains(got, "M") {
		t.Fatalf("avatars off = %q, want the identicon initial", got)
	}
	if source.asked != 0 {
		t.Fatalf("placeholder consulted %d times with avatars off", source.asked)
	}

	updated, cmd := m.Update(AvatarReadyMsg{})
	if cmd != nil {
		t.Fatalf("avatars off AvatarReadyMsg command = %T, want nil", cmd)
	}
	if updated.(*Model) != m {
		t.Fatal("AvatarReadyMsg replaced the model")
	}
	if source.drains != 0 {
		t.Fatal("avatars off drained graphics escapes")
	}
}

func TestPlaceholderStaysInTheGlyphBoxAndOutOfTheViewEscape(t *testing.T) {
	row := controller.ConversationSnapshot{
		Conversation: "mira",
		Direct:       true,
		Avatar:       "https://cdn.example/mira.png",
	}
	cells := avatar.Placeholder(42, avatarGlyphCols, 1)
	if lipgloss.Width(cells) != avatarGlyphCols {
		t.Fatalf("placeholder width %d, want %d cells", lipgloss.Width(cells), avatarGlyphCols)
	}
	if strings.Contains(cells, "\x1b_G") || strings.Contains(cells, "\n") {
		t.Fatalf("placeholder cells = %q", cells)
	}

	half := seededModel(t)
	halfSource := &fakeAvatarSource{block: "\x1b[38;2;1;2;3m▀▀\x1b[0m"}
	half.SetAvatarSource(halfSource)
	halfRow := half.conversationRow(row)

	m := seededModel(t)
	source := &recordingGraphics{cells: cells}
	m.SetAvatarSource(source)
	got := m.conversationRow(row)
	if !strings.Contains(got, "\U0010EEEE") {
		t.Fatalf("avatars on = %q, want placeholder cells", got)
	}
	if strings.Contains(got, "\x1b_G") {
		t.Fatalf("view contains a graphics APC: %q", got)
	}
	if lipgloss.Width(got) != lipgloss.Width(halfRow) {
		t.Fatalf("placeholder row width %d, half-block row width %d", lipgloss.Width(got), lipgloss.Width(halfRow))
	}
	if source.asked != 1 {
		t.Fatalf("placeholder asked %d times, want 1", source.asked)
	}

	// A profile that cannot keep a 24-bit id must stay on the half-block
	// raster. Otherwise the renderer drops the color and the private-use
	// glyph is a blank hole.
	limited := seededModel(t)
	limited.colorProfile = colorprofile.ASCII
	limited.SetAvatarSource(&recordingGraphics{cells: cells})
	got = limited.conversationRow(row)
	if strings.Contains(got, "\U0010EEEE") {
		t.Fatal("limited color drew a placeholder")
	}
}

func TestAvatarReadySendsEscapesOutsideTheView(t *testing.T) {
	m := seededModel(t)
	payload := "\x1b_Ga=t,f=100,i=1,q=2,U=1,m=0;AAAA\x1b\\"
	source := &recordingGraphics{escapes: payload}
	m.SetAvatarSource(source)

	_, cmd := m.Update(AvatarReadyMsg{})
	if cmd == nil {
		t.Fatal("AvatarReadyMsg produced no command")
	}
	raw, ok := cmd().(tea.RawMsg)
	if !ok || raw.Msg != payload {
		t.Fatalf("AvatarReadyMsg command = %#v, want tea.Raw of the transmit", cmd())
	}
	if source.drains != 1 || source.cols != avatarGlyphCols || source.rows != 1 {
		t.Fatalf("drain = %d times, cols=%d rows=%d", source.drains, source.cols, source.rows)
	}
	if strings.Contains(m.View().Content, "\x1b_G") {
		t.Fatal("view contains a graphics APC after AvatarReadyMsg")
	}

	updated, next := m.Update(raw)
	if next == nil {
		t.Fatal("RawMsg produced no follow-up")
	}
	if updated.(*Model) != m {
		t.Fatal("RawMsg replaced the model")
	}
	if source.noted != 0 {
		t.Fatal("NoteDrawn ran in the same turn as the transmit")
	}
	drawn := next()
	if _, ok := drawn.(avatarGraphicsDrawnMsg); !ok {
		t.Fatalf("RawMsg follow-up = %T, want avatarGraphicsDrawnMsg", drawn)
	}
	updated, after := m.Update(drawn)
	if after != nil {
		t.Fatalf("drawn follow-up command = %T, want nil", after)
	}
	if updated.(*Model) != m {
		t.Fatal("drawn follow-up replaced the model")
	}
	if source.noted != 1 {
		t.Fatalf("NoteDrawn called %d times, want 1", source.noted)
	}
}

func TestQuitLeavesDeletionUntilTheProgramReturns(t *testing.T) {
	m := seededModel(t)
	_, cmd := m.Update(ctrlKey('q'))
	if _, ok := cmd().(tea.QuitMsg); !ok {
		t.Fatalf("quit with no images = %T, want tea.QuitMsg", cmd())
	}

	m = seededModel(t)
	source := &recordingGraphics{escapes: "\x1b_Ga=t,f=100,i=1,q=2,U=1,m=0;AAAA\x1b\\"}
	m.SetAvatarSource(source)
	_, cmd = m.Update(ctrlKey('q'))
	if cmd == nil {
		t.Fatal("quit command is nil")
	}
	if _, ok := cmd().(tea.QuitMsg); !ok {
		t.Fatalf("quit with a live avatar = %T, want tea.QuitMsg", cmd())
	}
}

func TestServerTextDoesNotBecomeGraphicsEscapes(t *testing.T) {
	m := seededModel(t)
	msg := controller.MessageSnapshot{
		Author: "anna",
		Kind:   "message",
		Body:   "see a=t,i=1 U+10EEEE \U0010EEEE q=2 from the server",
		Time:   time.Date(2026, 10, 15, 12, 0, 0, 0, time.UTC),
	}
	row := m.messageRow(0, msg)
	if strings.Contains(row, "\x1b_G") || strings.Contains(row, "\x1b_G") {
		t.Fatalf("server text grew a graphics APC: %q", row)
	}
	if !strings.Contains(row, "a=t,i=1") || !strings.Contains(row, "from the server") {
		t.Fatalf("server text was not shown as text: %q", row)
	}
}
