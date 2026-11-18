package ui

// The Ctrl+Shift+F history search sheet over the seeded demo: open it with the
// chord, type, walk and Enter, and the right conversation shows with the line
// in view. The Rust twin is `client/src/ui/tests/search.rs`.

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/connection"
	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

// searchDemoModel is the seeded demo with its server, so a test can inject
// lines into conversations.
func searchDemoModel(t *testing.T) (*Model, *demo.DemoServer) {
	t.Helper()
	ctrl := controller.New()
	ctrl.SetClock(session.NewFakeClock(time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)))
	d := demo.New()
	if !d.Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	m := New(ctrl, nil)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	return updated.(*Model), d
}

// typeSearch types text into the open sheet, then runs the pause's command so
// the search starts, as the loop would after 150 ms.
func typeSearch(t *testing.T, m *Model, text string) *Model {
	t.Helper()
	var cmds []tea.Cmd
	for _, r := range text {
		updated, cmd := m.Update(tea.KeyPressMsg{Code: r, Text: string(r)})
		m = updated.(*Model)
		cmds = append(cmds, cmd)
	}
	return settleSearch(t, m, cmds)
}

// settleSearch runs the pending commands the way the loop does: the debounce
// message first, then every batch the worker sends until it is done.
func settleSearch(t *testing.T, m *Model, cmds []tea.Cmd) *Model {
	t.Helper()
	queue := append([]tea.Cmd(nil), cmds...)
	for steps := 0; len(queue) > 0 && steps < 200; steps++ {
		cmd := queue[0]
		queue = queue[1:]
		if cmd == nil {
			continue
		}
		msg := cmd()
		switch msg := msg.(type) {
		case searchDebounceMsg:
			updated, next := m.Update(msg)
			m = updated.(*Model)
			queue = append(queue, next)
		case searchBatchMsg:
			updated, next := m.Update(msg)
			m = updated.(*Model)
			queue = append(queue, next)
		case tea.BatchMsg:
			queue = append(queue, msg...)
		}
	}
	return m
}

func injectOmarchy(d *demo.DemoServer, lines ...string) {
	d.InjectOmarchy([]byte(strings.Join(lines, "\r\n") + "\r\n"))
}

func TestSearchSheetOpenTypeDownEnterLandsOnTheSecondHit(t *testing.T) {
	m, d := searchDemoModel(t)
	var fillers []string
	for n := 0; n < 60; n++ {
		fillers = append(fillers, fmt.Sprintf(":kai!u@h PRIVMSG #help :filler %d", n))
	}
	injectOmarchy(d, "@msgid=help-zebra :kai!u@h PRIVMSG #help :a zebra in the help room")
	injectOmarchy(d, fillers...)
	injectOmarchy(d, "@msgid=omarchy-zebra :dax!u@h PRIVMSG #omarchy :Zebra crossing at the omarchy desk")
	var more []string
	for n := 0; n < 60; n++ {
		more = append(more, fmt.Sprintf(":dax!u@h PRIVMSG #omarchy :filler %d", n))
	}
	injectOmarchy(d, more...)
	m.ctrl.SelectConversation("omarchy", "#omarchy")

	m = press(t, m, ctrlShiftKey('f'))
	if !m.searchVisible() {
		t.Fatal("Ctrl+Shift+F must open the search sheet")
	}
	m = typeSearch(t, m, "zebra")
	hits := m.searchSheet().results.Hits
	if len(hits) != 2 {
		t.Fatalf("want one zebra per conversation, got %d: %+v", len(hits), hits)
	}
	second := hits[1]

	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyDown})
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})

	if m.searchVisible() {
		t.Fatal("Enter must close the sheet")
	}
	if got := m.ctrl.SelectedConversationID(); got != irc.ConversationID(second.Conversation) {
		t.Fatalf("selected %q, want the second hit's conversation %q", got, irc.ConversationID(second.Conversation))
	}
	if !m.composer.Focused() {
		t.Fatal("focus must return to the composer")
	}
	if m.followingEnd() {
		t.Fatal("the reader must be on the line, not following the end")
	}
	view := m.View().Content
	if !strings.Contains(view, "zebra") && !strings.Contains(view, "Zebra") {
		t.Fatalf("the target line is not on screen:\n%s", view)
	}
	if m.searchNote != "" {
		t.Fatalf("an on-screen jump leaves no note, got %q", m.searchNote)
	}
}

func TestSearchSheetEscapeAndTheChordClose(t *testing.T) {
	m, _ := searchDemoModel(t)
	m = press(t, m, ctrlShiftKey('f'))
	m = typeSearch(t, m, "zeb")
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.searchVisible() || !m.composer.Focused() {
		t.Fatal("Escape must close the sheet and return focus to the composer")
	}
	m = press(t, m, ctrlShiftKey('f'))
	m = press(t, m, ctrlShiftKey('f'))
	if m.searchVisible() {
		t.Fatal("the chord must close the sheet it opened")
	}
}

func TestSearchSheetIsBlockedByConnect(t *testing.T) {
	ctrl := controller.New()
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	m := resizeModel(t, New(ctrl, connection.New(ctrl, nil)), 118, 30)
	m = press(t, m, ctrlKey(','))
	if !m.connectVisible() {
		t.Fatal("Connect must be visible")
	}
	m = press(t, m, ctrlShiftKey('f'))
	if m.searchVisible() || !m.connectVisible() {
		t.Fatal("Connect holds the keyboard")
	}
}

func TestSearchSheetBlankFieldShowsOnlyTheHint(t *testing.T) {
	m, _ := searchDemoModel(t)
	m = press(t, m, ctrlShiftKey('f'))
	m = typeSearch(t, m, "   ")
	if len(m.searchSheet().results.Hits) != 0 {
		t.Fatal("a blank query lists nothing")
	}
	if view := m.View().Content; !strings.Contains(view, "Type to search every conversation") {
		t.Fatalf("missing the hint:\n%s", view)
	}
}

func TestSearchSheetMatchesTheAuthorAndThePlainText(t *testing.T) {
	m, d := searchDemoModel(t)
	injectOmarchy(d, "@msgid=bold-1 :kai!u@h PRIVMSG #help :\x02bold\x02 words")
	m = press(t, m, ctrlShiftKey('f'))
	m = typeSearch(t, m, "bold words")
	if hits := m.searchSheet().results.Hits; len(hits) != 1 || hits[0].Text != "bold words" {
		t.Fatalf("got %+v", hits)
	}
}

func TestSearchSheetStaleBatchesNeverPaint(t *testing.T) {
	m, d := searchDemoModel(t)
	injectOmarchy(d, "@msgid=z1 :kai!u@h PRIVMSG #help :a zebra here")
	m = press(t, m, ctrlShiftKey('f'))
	m = typeSearch(t, m, "zebra")
	if len(m.searchSheet().results.Hits) != 1 {
		t.Fatalf("got %+v", m.searchSheet().results.Hits)
	}
	staleGen := m.searchSheet().generation
	m = typeSearch(t, m, "q")
	updated, _ := m.Update(searchBatchMsg{gen: staleGen, hits: []irc.SearchHit{{Text: "stale"}}})
	m = updated.(*Model)
	for _, hit := range m.searchSheet().results.Hits {
		if hit.Text == "stale" {
			t.Fatal("a batch for an older generation painted")
		}
	}
	updated, _ = m.Update(searchDebounceMsg{gen: staleGen})
	if got := updated.(*Model).searchSheet().results.Hits; len(got) != 0 {
		t.Fatalf("a stale debounce ran a search: %+v", got)
	}
}

func TestSearchSheetOlderThanLoadedOpensTheConversationWithANote(t *testing.T) {
	root := t.TempDir()
	m, _ := searchDemoModel(t)
	m.ctrl.SetConversationLog(storage.NewConversationLog(root))
	log := storage.NewConversationLog(root)
	if !log.Append("omarchy", "#help", irc.CaseMapping{}, irc.TranscriptLine{
		Timestamp: time.Unix(1_000_000_000, 0).UTC(), Author: "old-timer", Kind: "message",
		Body: "the walrus was here first", MsgID: "old-1",
	}) {
		t.Fatal("append failed")
	}
	if _, err := os.Stat(filepath.Join(root, "omarchy")); err != nil {
		t.Fatal(err)
	}
	m.ctrl.SelectConversation("omarchy", "#omarchy")

	m = press(t, m, ctrlShiftKey('f'))
	m = typeSearch(t, m, "walrus")
	if len(m.searchSheet().results.Hits) != 1 || m.searchSheet().results.Hits[0].Conversation.NormalizedTarget != "#help" {
		t.Fatalf("the disk did not answer: %+v", m.searchSheet().results.Hits)
	}
	if m.searchSheet().scanning {
		t.Fatal("the scan must be finished")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEnter})
	if m.ctrl.SelectedTarget() != "#help" {
		t.Fatalf("selected %q, want #help", m.ctrl.SelectedTarget())
	}
	if m.searchNote != irc.OlderThanLoadedNote {
		t.Fatalf("note = %q", m.searchNote)
	}
	if view := m.View().Content; !strings.Contains(view, irc.OlderThanLoadedNote) {
		t.Fatalf("the note is not shown:\n%s", view)
	}
	m.ctrl.SelectConversation("omarchy", "#omarchy")
	m.switchSelection(func() { m.ctrl.SelectConversation("omarchy", "#omarchy") })
	if m.searchNote != "" {
		t.Fatal("leaving the conversation must clear the note")
	}
}

func TestShortcutsSheetListsTheSearchChord(t *testing.T) {
	m := seededModel(t)
	m = press(t, m, ctrlKey('/'))
	if view := m.View().Content; !strings.Contains(view, "Ctrl+Shift+F") || !strings.Contains(view, "search history") {
		t.Fatalf("the shortcuts sheet does not list the search chord:\n%s", view)
	}
}
