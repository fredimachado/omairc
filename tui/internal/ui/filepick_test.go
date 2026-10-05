package ui

import (
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	tea "charm.land/bubbletea/v2"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

func TestInsertFileLinkAtCaret(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("hello world")
	m.composer.SetCursor(5)
	m.insertFileLink("https://example.org/a.png")
	if got := m.composer.Value(); got != "hello https://example.org/a.png world" {
		t.Fatalf("draft = %q", got)
	}
	m.insertFileLink("javascript:alert(1)")
	if got := m.composer.Value(); got != "hello https://example.org/a.png world" {
		t.Fatalf("draft after a refused link = %q", got)
	}
}

func TestFilePickStaysClosedWithoutAHost(t *testing.T) {
	m := seededModel(t)
	m.openFilePick()
	if m.filePickVisible() {
		t.Fatal("file pick opened without a file host")
	}
	m = press(t, m, tea.KeyPressMsg{Code: 'u', Mod: tea.ModCtrl | tea.ModShift})
	if m.filePickVisible() {
		t.Fatal("Ctrl+Shift+U opened file pick without a file host")
	}
}

func TestFilePickOpensWhenTheServerOffersAHost(t *testing.T) {
	m := fileHostModel(t, "https://irc.example/upload", true)
	m = press(t, m, tea.KeyPressMsg{Code: 'u', Mod: tea.ModCtrl | tea.ModShift})
	if !m.filePickVisible() {
		t.Fatal("Ctrl+Shift+U did not open file pick")
	}
	if !strings.Contains(m.fileCard(m.width), "File") {
		t.Fatal("file card missing its title")
	}
	m = press(t, m, tea.KeyPressMsg{Code: tea.KeyEscape})
	if m.filePickVisible() {
		t.Fatal("Escape left the file pick open")
	}
	m.beginOrAdvanceFind()
	m.openFilePick()
	if m.filePickVisible() {
		t.Fatal("file pick opened during find")
	}
}

func TestPasteOfTextStaysText(t *testing.T) {
	m := fileHostModel(t, "https://irc.example/upload", true)
	updated, _ := m.Update(tea.PasteMsg{Content: "hello there"})
	m = updated.(*Model)
	if got := m.composer.Value(); got != "hello there" {
		t.Fatalf("draft = %q", got)
	}
	if len(m.fileQueue) != 0 || m.ctrl.FileUploadBusy() {
		t.Fatal("a text paste started an upload")
	}
}

func TestPasteOfAFileInsertsTheLink(t *testing.T) {
	var hits int
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		hits++
		_, _ = io.Copy(io.Discard, r.Body)
		w.Header().Set("Location", "/files/note.txt")
		w.WriteHeader(http.StatusCreated)
	}))
	defer server.Close()

	m := fileHostModel(t, server.URL+"/upload", false)
	path := filepath.Join(t.TempDir(), "note.txt")
	if err := os.WriteFile(path, []byte("hello"), 0o644); err != nil {
		t.Fatal(err)
	}
	done := make(chan FileLinkMsg, 1)
	m.ctrl.OnFileLink = func(url, message string) {
		done <- FileLinkMsg{URL: url, Message: message}
	}
	updated, _ := m.Update(tea.PasteMsg{Content: path})
	m = updated.(*Model)
	if strings.Contains(m.composer.Value(), path) {
		t.Fatalf("the path was pasted into the draft: %q", m.composer.Value())
	}
	select {
	case msg := <-done:
		updated, _ = m.Update(msg)
		m = updated.(*Model)
	case <-time.After(5 * time.Second):
		t.Fatal("upload did not finish")
	}
	want := server.URL + "/files/note.txt"
	if got := m.composer.Value(); got != want {
		t.Fatalf("draft = %q, want %q", got, want)
	}
	if hits != 1 {
		t.Fatalf("uploads = %d", hits)
	}
	if !m.tryUploadPastedText("not a file\n" + path) {
		// A mixed paste stays text, so this must return false.
	} else {
		t.Fatal("a mixed paste was uploaded")
	}
}

func TestLocalFileLinesRequireEveryLine(t *testing.T) {
	path := filepath.Join(t.TempDir(), "note.txt")
	if err := os.WriteFile(path, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if got := localFileLines(path); len(got) != 1 || got[0] != path {
		t.Fatalf("lines = %#v", got)
	}
	if got := localFileLines("file://" + path); len(got) != 1 || got[0] != path {
		t.Fatalf("file url lines = %#v", got)
	}
	if localFileLines("hello") != nil {
		t.Fatal("text was treated as a file")
	}
	if localFileLines("note.txt") != nil {
		t.Fatal("a relative path was treated as a pasted file")
	}
	if localFileLines(path+"\nhello") != nil {
		t.Fatal("a mixed paste was treated as files")
	}
}

func fileHostModel(t *testing.T, endpoint string, encrypted bool) *Model {
	t.Helper()
	ctrl := controller.New()
	config := session.DefaultSessionConfig("net", "net", "irc.example", "alice")
	config.TLSEnabled = encrypted
	config.ReconnectEnabled = false
	if _, err := ctrl.AddSession(config, session.NewLoopbackTransport(), nil); err != nil {
		t.Fatal(err)
	}
	features := irc.NewServerFeatures()
	features.ApplyToken("CHANTYPES=#")
	features.ApplyToken("soju.im/FILEHOST=" + endpoint)
	ctrl.Reducer().SetServerFeatures("net", features)
	ctrl.SelectConversation("net", "#room")
	m := New(ctrl, nil)
	updated, _ := m.Update(tea.WindowSizeMsg{Width: 118, Height: 30})
	return updated.(*Model)
}
