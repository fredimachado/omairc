package ui

// Composer drafts across a restart. A restart is a new Model on a fresh
// controller whose draft store reads the file the first one wrote. The demo is
// ephemeral by design, so each test points the controller at a temp
// drafts.json by hand. The Rust equivalent is client/src/ui/tests/drafts.rs.

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

func draftsPath(t *testing.T) string {
	t.Helper()
	return filepath.Join(t.TempDir(), storage.DraftsFile)
}

func draftsFile(t *testing.T, path string) string {
	t.Helper()
	data, _ := os.ReadFile(path)
	return string(data)
}

// persistentModel is the seeded demo Model with its drafts on disk at path.
// attach runs before the Model is built, as the real launch wires the store
// before the first frame.
func persistentModel(t *testing.T, path string) *Model {
	t.Helper()
	ctrl := controller.New()
	if !demo.New().Attach(ctrl, true) {
		t.Fatal("demo Attach failed")
	}
	ctrl.SetDraftStore(storage.NewDraftStoreAt(path))
	m := New(ctrl, nil)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	return updated.(*Model)
}

func goTo(m *Model, target string) {
	m.switchSelection(func() { m.ctrl.SelectConversation("omarchy", target) })
}

func TestDraftLeftInAChannelIsBackAfterARestartWithTheCaretAtItsEnd(t *testing.T) {
	path := draftsPath(t)
	m := persistentModel(t, path)
	goTo(m, "#omarchy")
	m.composer.SetValue("half typed")
	goTo(m, "#desktop")
	if !strings.Contains(draftsFile(t, path), "half typed") {
		t.Fatalf("file = %s", draftsFile(t, path))
	}

	again := persistentModel(t, path)
	goTo(again, "#desktop")
	goTo(again, "#omarchy")
	if got := again.composer.Value(); got != "half typed" {
		t.Fatalf("composer after restart = %q", got)
	}
	if got := again.composer.Position(); got != len("half typed") {
		t.Fatalf("caret = %d, want the end", got)
	}
	if !strings.Contains(draftsFile(t, path), "half typed") {
		t.Fatal("leaving a restored draft untouched must not drop it")
	}
}

func TestLaunchThatOpensOnTheConversationShowsItsDraft(t *testing.T) {
	path := draftsPath(t)
	m := persistentModel(t, path)
	opened := m.ctrl.SelectedConversationID()
	m.composer.SetValue("carried over")
	goTo(m, "#desktop")

	again := persistentModel(t, path)
	if got := again.ctrl.SelectedConversationID(); got != opened {
		t.Fatalf("the demo opens on %q, want %q", got, opened)
	}
	if got := again.composer.Value(); got != "carried over" {
		t.Fatalf("composer at launch = %q", got)
	}
}

func TestSecretLeftUnsentIsNeverWrittenAndAnEarlierDraftGoes(t *testing.T) {
	path := draftsPath(t)
	m := persistentModel(t, path)
	goTo(m, "#omarchy")
	m.composer.SetValue("half typed")
	goTo(m, "#desktop")
	goTo(m, "#omarchy")
	m.composer.SetValue("/msg NickServ IDENTIFY pw")
	goTo(m, "#desktop")
	got := draftsFile(t, path)
	if strings.Contains(got, "pw") || strings.Contains(got, "half typed") {
		t.Fatalf("file = %s", got)
	}
}

func TestFindQueryDoesNotOverwriteTheStoredDraft(t *testing.T) {
	path := draftsPath(t)
	m := persistentModel(t, path)
	goTo(m, "#omarchy")
	m.composer.SetValue("half typed")
	m = press(t, m, ctrlKey('f'))
	if !m.find.active {
		t.Fatal("Ctrl+F must enter find")
	}
	m.composer.SetValue("a query")
	m.persistCurrentDraft()
	got := draftsFile(t, path)
	if !strings.Contains(got, "half typed") || strings.Contains(got, "a query") {
		t.Fatalf("file = %s", got)
	}
	goTo(m, "#desktop")
	got = draftsFile(t, path)
	if !strings.Contains(got, "half typed") || strings.Contains(got, "a query") {
		t.Fatalf("file after switch = %s", got)
	}
}

func TestHistoryWalkDoesNotOverwriteTheStoredDraft(t *testing.T) {
	path := draftsPath(t)
	m := persistentModel(t, path)
	goTo(m, "#omarchy")
	m.rememberSentLine("a sent line")
	m.composer.SetValue("half typed")
	m.recallHistory(-1)
	if got := m.composer.Value(); got != "a sent line" {
		t.Fatalf("the walk shows %q", got)
	}
	m.persistCurrentDraft()
	got := draftsFile(t, path)
	if !strings.Contains(got, "half typed") || strings.Contains(got, "a sent line") {
		t.Fatalf("file = %s", got)
	}
	goTo(m, "#desktop")
	got = draftsFile(t, path)
	if !strings.Contains(got, "half typed") || strings.Contains(got, "a sent line") {
		t.Fatalf("file after switching away mid-walk = %s", got)
	}
}

func TestBlurAndQuitKeepTheDraft(t *testing.T) {
	path := draftsPath(t)
	m := persistentModel(t, path)
	goTo(m, "#omarchy")
	m.composer.SetValue("typed then blurred")
	if got := draftsFile(t, path); got != "" {
		t.Fatalf("typing alone wrote %q", got)
	}
	m.Update(tea.BlurMsg{})
	if !strings.Contains(draftsFile(t, path), "typed then blurred") {
		t.Fatalf("blur left file = %s", draftsFile(t, path))
	}

	other := draftsPath(t)
	q := persistentModel(t, other)
	goTo(q, "#omarchy")
	q.composer.SetValue("typed then quit")
	_, quit := q.Update(ctrlKey('q'))
	if quit == nil {
		t.Fatal("Ctrl+Q must quit")
	}
	if !strings.Contains(draftsFile(t, other), "typed then quit") {
		t.Fatalf("Ctrl+Q left file = %s", draftsFile(t, other))
	}

	last := draftsPath(t)
	f := persistentModel(t, last)
	goTo(f, "#omarchy")
	f.composer.SetValue("typed then flushed")
	f.FlushDraft()
	if !strings.Contains(draftsFile(t, last), "typed then flushed") {
		t.Fatalf("FlushDraft left file = %s", draftsFile(t, last))
	}
}

func TestSendingTheLineRemovesTheStoredDraft(t *testing.T) {
	path := draftsPath(t)
	m := persistentModel(t, path)
	goTo(m, "#omarchy")
	m.composer.SetValue("half typed")
	goTo(m, "#desktop")
	goTo(m, "#omarchy")
	if got := m.composer.Value(); got != "half typed" {
		t.Fatalf("composer = %q", got)
	}
	m.sendComposer()
	if strings.Contains(draftsFile(t, path), "half typed") {
		t.Fatalf("file = %s", draftsFile(t, path))
	}
}

func TestStatusDraftNeverReachesTheFile(t *testing.T) {
	path := draftsPath(t)
	m := persistentModel(t, path)
	m.switchSelection(func() { m.ctrl.OpenStatus("omarchy") })
	if !strings.HasPrefix(m.draftKey, "status\n") {
		t.Fatalf("draft key = %q", m.draftKey)
	}
	m.composer.SetValue("half a command")
	m.persistCurrentDraft()
	goTo(m, "#omarchy")
	if got := draftsFile(t, path); got != "" {
		t.Fatalf("file = %s", got)
	}
}
