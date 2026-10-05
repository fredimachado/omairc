package controller

import (
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

func TestBeginFileUploadInsertsNothingUntilTheLinkArrives(t *testing.T) {
	var auth string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		auth = r.Header.Get("Authorization")
		_, _ = io.Copy(io.Discard, r.Body)
		w.Header().Set("Location", "/upload/note.txt")
		w.WriteHeader(http.StatusCreated)
	}))
	defer server.Close()

	c, clock := newController(t)
	config := baseConfig("net", "alice")
	config.Host = "127.0.0.1"
	config.TLSEnabled = false
	config.Password = "s3cret-token"
	addSession(t, c, clock, config)

	features := irc.NewServerFeatures()
	features.ApplyToken("CHANTYPES=#")
	features.ApplyToken("soju.im/FILEHOST=" + server.URL + "/upload")
	c.Reducer().SetServerFeatures("net", features)
	c.SelectConversation("net", "#room")

	if got := c.FileHost(); got != server.URL+"/upload" {
		t.Fatalf("file host = %q", got)
	}
	path := filepath.Join(t.TempDir(), "note.txt")
	if err := os.WriteFile(path, []byte("hello"), 0o644); err != nil {
		t.Fatal(err)
	}

	type result struct {
		url     string
		message string
	}
	done := make(chan result, 1)
	c.OnFileLink = func(url, message string) {
		done <- result{url, message}
	}
	if !c.BeginFileUpload(path) {
		t.Fatal("BeginFileUpload returned false")
	}
	if c.BeginFileUpload(path) {
		t.Fatal("a second upload started while the first was busy")
	}
	select {
	case got := <-done:
		c.FinishFileUpload()
		if got.message != "" {
			t.Fatalf("message = %q", got.message)
		}
		if got.url != server.URL+"/upload/note.txt" {
			t.Fatalf("url = %q", got.url)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("upload did not finish")
	}
	if auth != basicAlice() {
		t.Fatalf("authorization = %q", auth)
	}
	for _, line := range c.ConsoleText("net") {
		if strings.Contains(line, "s3cret-token") {
			t.Fatalf("status leaked the password: %q", line)
		}
	}
	if c.FileUploadBusy() {
		t.Fatal("upload still busy after finish")
	}
}

func TestFileUploadFailureStaysOnStatus(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		http.Error(w, "s3cret-token", http.StatusForbidden)
	}))
	defer server.Close()

	c, clock := newController(t)
	config := baseConfig("net", "alice")
	config.Host = "127.0.0.1"
	config.TLSEnabled = false
	config.Password = "s3cret-token"
	addSession(t, c, clock, config)
	features := irc.NewServerFeatures()
	features.ApplyToken("soju.im/FILEHOST=" + server.URL + "/upload")
	c.Reducer().SetServerFeatures("net", features)
	c.SelectConversation("net", "bob")

	path := filepath.Join(t.TempDir(), "note.txt")
	if err := os.WriteFile(path, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	type result struct {
		url     string
		message string
	}
	done := make(chan result, 1)
	c.OnFileLink = func(url, message string) {
		done <- result{url, message}
	}
	if !c.BeginFileUpload(path) {
		t.Fatal("BeginFileUpload returned false")
	}
	select {
	case got := <-done:
		c.FinishFileUpload()
		c.NoteFileUploadFailure(got.message)
		if got.url != "" {
			t.Fatalf("url = %q, want empty", got.url)
		}
		if got.message != "Could not upload the file." {
			t.Fatalf("message = %q", got.message)
		}
		if strings.Contains(got.message, "s3cret-token") || strings.Contains(got.message, server.URL) {
			t.Fatalf("failure leaked a secret or the endpoint: %q", got.message)
		}
	case <-time.After(5 * time.Second):
		t.Fatal("upload did not finish")
	}
	text := strings.Join(c.ConsoleText("net"), "\n")
	if !strings.Contains(text, "Could not upload the file.") {
		t.Fatalf("status = %q", text)
	}
	if strings.Contains(text, "s3cret-token") {
		t.Fatalf("status leaked the password: %q", text)
	}
}

func TestFileHostHiddenWithoutAnOffer(t *testing.T) {
	c, clock := newController(t)
	config := baseConfig("net", "alice")
	addSession(t, c, clock, config)
	c.SelectConversation("net", "#room")
	if c.FileHost() != "" {
		t.Fatalf("file host = %q, want empty", c.FileHost())
	}
	if c.BeginFileUpload("note.txt") {
		t.Fatal("upload started without a file host")
	}
}

func basicAlice() string {
	return irc.BasicAuthorization("alice", "s3cret-token")
}
