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
	m.insertFileLink("https://example.org/a.png", m.composerDraftKey())
	if got := m.composer.Value(); got != "hello https://example.org/a.png world" {
		t.Fatalf("draft = %q", got)
	}
	m.insertFileLink("javascript:alert(1)", m.composerDraftKey())
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
	if m.tryUploadPastedText("not a file\n" + path) {
		t.Fatal("a mixed paste was uploaded")
	}
}

func TestFileLinkStaysOnTheDraftThatQueuedIt(t *testing.T) {
	var hits int
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		hits++
		_, _ = io.Copy(io.Discard, r.Body)
		w.Header().Set("Location", "/files/note.txt")
		w.WriteHeader(http.StatusCreated)
	}))
	defer server.Close()

	m := fileHostModel(t, server.URL+"/upload", false)
	m.composer.SetValue("hello")
	original := m.composerDraftKey()
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
	m.saveDraft()
	m.ctrl.SelectConversation("net", "bob")
	m.composer.SetValue("other draft")
	select {
	case msg := <-done:
		updated, _ = m.Update(msg)
		m = updated.(*Model)
	case <-time.After(5 * time.Second):
		t.Fatal("upload did not finish")
	}
	want := server.URL + "/files/note.txt"
	if got := m.composer.Value(); got != "other draft" {
		t.Fatalf("visible draft = %q", got)
	}
	if m.drafts[original] != "hello "+want {
		t.Fatalf("queued draft = %q, want %q", m.drafts[original], "hello "+want)
	}
	if hits != 1 {
		t.Fatalf("uploads = %d", hits)
	}
}

func TestFileLinkAppendsToTheFindStash(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = io.Copy(io.Discard, r.Body)
		w.Header().Set("Location", "/files/note.txt")
		w.WriteHeader(http.StatusCreated)
	}))
	defer server.Close()

	m := fileHostModel(t, server.URL+"/upload", false)
	m.composer.SetValue("hello")
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
	m.beginOrAdvanceFind()
	query := m.composer.Value()
	select {
	case msg := <-done:
		updated, _ = m.Update(msg)
		m = updated.(*Model)
	case <-time.After(5 * time.Second):
		t.Fatal("upload did not finish")
	}
	if m.composer.Value() != query {
		t.Fatalf("query = %q, want %q", m.composer.Value(), query)
	}
	want := server.URL + "/files/note.txt"
	if !strings.Contains(m.find.draft, want) {
		t.Fatalf("find stash = %q, want %q", m.find.draft, want)
	}
}

func TestPasteOfADirectoryRecordsNotAFile(t *testing.T) {
	var hits int
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		hits++
		w.WriteHeader(http.StatusCreated)
	}))
	defer server.Close()

	m := fileHostModel(t, server.URL+"/upload", false)
	dir := t.TempDir()
	file := filepath.Join(dir, "note.txt")
	if err := os.WriteFile(file, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	done := make(chan FileLinkMsg, 1)
	m.ctrl.OnFileLink = func(url, message string) {
		done <- FileLinkMsg{URL: url, Message: message}
	}
	updated, _ := m.Update(tea.PasteMsg{Content: dir})
	m = updated.(*Model)
	if strings.Contains(m.composer.Value(), dir) {
		t.Fatalf("directory was pasted: %q", m.composer.Value())
	}
	select {
	case msg := <-done:
		updated, _ = m.Update(msg)
		m = updated.(*Model)
		if msg.Message != "That is not a file." {
			t.Fatalf("message = %q", msg.Message)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("directory paste did not finish")
	}
	text := strings.Join(m.ctrl.ConsoleText("net"), "\n")
	if !strings.Contains(text, "That is not a file.") {
		t.Fatalf("status = %q", text)
	}
	if hits != 0 {
		t.Fatalf("uploads = %d", hits)
	}
	if m.tryUploadPastedText(file + "\n" + dir) {
		select {
		case msg := <-done:
			updated, _ = m.Update(msg)
			m = updated.(*Model)
			if msg.Message != "That is not a file." {
				t.Fatalf("mixed message = %q", msg.Message)
			}
		case <-time.After(5 * time.Second):
			t.Fatal("mixed directory paste did not finish")
		}
	} else {
		t.Fatal("a file and a directory stayed text")
	}
	if hits != 0 {
		t.Fatalf("mixed paste uploaded %d files", hits)
	}
	if m.tryUploadPastedText("not a file\n" + file) {
		t.Fatal("a non-path line was consumed")
	}
}

func TestFileUploadFailureStaysOnTheNetworkThatQueuedIt(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "no", http.StatusForbidden)
	}))
	defer server.Close()

	m := fileHostModel(t, server.URL+"/upload", false)
	other := session.DefaultSessionConfig("other", "other", "irc.example", "alice")
	other.TLSEnabled = false
	other.ReconnectEnabled = false
	if _, err := m.ctrl.AddSession(other, session.NewLoopbackTransport(), nil); err != nil {
		t.Fatal(err)
	}
	path := filepath.Join(t.TempDir(), "note.txt")
	if err := os.WriteFile(path, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	done := make(chan FileLinkMsg, 1)
	m.ctrl.OnFileLink = func(url, message string) {
		done <- FileLinkMsg{URL: url, Message: message}
	}
	updated, _ := m.Update(tea.PasteMsg{Content: path})
	m = updated.(*Model)
	m.ctrl.SelectConversation("other", "bob")
	select {
	case msg := <-done:
		updated, _ = m.Update(msg)
		m = updated.(*Model)
		if msg.Message != "Could not upload the file." {
			t.Fatalf("message = %q", msg.Message)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("upload did not finish")
	}
	queued := strings.Join(m.ctrl.ConsoleText("net"), "\n")
	if !strings.Contains(queued, "Could not upload the file.") {
		t.Fatalf("queued network status = %q", queued)
	}
	elsewhere := strings.Join(m.ctrl.ConsoleText("other"), "\n")
	if strings.Contains(elsewhere, "Could not upload the file.") {
		t.Fatalf("other network status = %q", elsewhere)
	}
}

func TestBeginFileUploadFalseKeepsTheRestOfTheQueue(t *testing.T) {
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
	m.fileQueue = []queuedFile{
		{path: "", draftKey: m.composerDraftKey(), networkID: "net"},
		{path: path, draftKey: m.composerDraftKey(), networkID: "net"},
	}
	m.pumpFileUploads()
	text := strings.Join(m.ctrl.ConsoleText("net"), "\n")
	if !strings.Contains(text, "Could not upload the file.") {
		t.Fatalf("status = %q", text)
	}
	if !m.ctrl.FileUploadBusy() {
		t.Fatal("the next file was dropped")
	}
	select {
	case msg := <-done:
		updated, _ := m.Update(msg)
		m = updated.(*Model)
		if msg.URL == "" {
			t.Fatalf("second upload failed: %q", msg.Message)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("second upload did not finish")
	}
	if hits != 1 {
		t.Fatalf("uploads = %d", hits)
	}
}

func TestInsertFileLinkLeavesHistoryAndOtherDrafts(t *testing.T) {
	m := seededModel(t)
	m.composer.SetValue("recalled")
	m.composerHistoryIndex = 0
	m.composerHistoryDraft = "stashed"
	m.insertFileLink("https://example.org/a.png", m.composerDraftKey())
	if m.composer.Value() != "recalled" {
		t.Fatalf("recalled line = %q", m.composer.Value())
	}
	if m.composerHistoryDraft != "stashed https://example.org/a.png" {
		t.Fatalf("history draft = %q", m.composerHistoryDraft)
	}
	m.composerHistoryIndex = -1
	m.insertFileLink("https://example.org/b.png", "other")
	if m.composer.Value() != "recalled" {
		t.Fatalf("composer = %q", m.composer.Value())
	}
	if m.drafts["other"] != "https://example.org/b.png" {
		t.Fatalf("other draft = %q", m.drafts["other"])
	}
	m.beginOrAdvanceFind()
	query := m.composer.Value()
	m.insertFileLink("https://example.org/c.png", m.composerDraftKey())
	if m.composer.Value() != query {
		t.Fatalf("query = %q, want %q", m.composer.Value(), query)
	}
	if !strings.Contains(m.find.draft, "https://example.org/c.png") {
		t.Fatalf("find stash = %q", m.find.draft)
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
