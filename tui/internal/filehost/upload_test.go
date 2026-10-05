package filehost

import (
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestUploadPostsFileAndResolvesLocation(t *testing.T) {
	var gotAuth, gotType, gotDisposition, gotBody string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		body, _ := io.ReadAll(r.Body)
		gotAuth = r.Header.Get("Authorization")
		gotType = r.Header.Get("Content-Type")
		gotDisposition = r.Header.Get("Content-Disposition")
		gotBody = string(body)
		w.Header().Set("Location", "/upload/hoh5eFThae4e.txt")
		w.WriteHeader(http.StatusCreated)
	}))
	defer server.Close()

	path := filepath.Join(t.TempDir(), "note.txt")
	if err := os.WriteFile(path, []byte("hello file"), 0o644); err != nil {
		t.Fatal(err)
	}
	link, message := Upload(Request{
		Endpoint:        server.URL + "/upload",
		Path:            path,
		User:            "seunghye",
		Secret:          "no",
		ServerHost:      "127.0.0.1",
		ServerEncrypted: false,
	})
	if message != "" {
		t.Fatalf("message = %q", message)
	}
	want := server.URL + "/upload/hoh5eFThae4e.txt"
	if link != want {
		t.Fatalf("link = %q, want %q", link, want)
	}
	if gotAuth != "Basic c2V1bmdoeWU6bm8=" {
		t.Fatalf("authorization = %q", gotAuth)
	}
	if gotType != "text/plain" {
		t.Fatalf("content-type = %q", gotType)
	}
	if gotDisposition != `attachment; filename="note.txt"` {
		t.Fatalf("content-disposition = %q", gotDisposition)
	}
	if gotBody != "hello file" {
		t.Fatalf("body = %q", gotBody)
	}
}

func TestUploadOmitsAuthForADifferentHost(t *testing.T) {
	var gotAuth string
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotAuth = r.Header.Get("Authorization")
		w.Header().Set("Location", "/ok.txt")
		w.WriteHeader(http.StatusCreated)
	}))
	defer server.Close()

	path := filepath.Join(t.TempDir(), "note.txt")
	if err := os.WriteFile(path, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	if _, message := Upload(Request{
		Endpoint:        server.URL + "/upload",
		Path:            path,
		User:            "alice",
		Secret:          "s3cret-token",
		ServerHost:      "irc.example",
		ServerEncrypted: false,
	}); message != "" {
		t.Fatalf("message = %q", message)
	}
	if gotAuth != "" {
		t.Fatalf("authorization = %q, want none", gotAuth)
	}
}

func TestUploadDoesNotFollowRedirectsOrLeakTheSecret(t *testing.T) {
	var hits int
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		hits++
		w.Header().Set("Location", serverLocation(r))
		http.Error(w, "s3cret-token", http.StatusFound)
	}))
	defer server.Close()

	path := filepath.Join(t.TempDir(), "note.txt")
	if err := os.WriteFile(path, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	link, message := Upload(Request{
		Endpoint:        server.URL + "/upload",
		Path:            path,
		User:            "alice",
		Secret:          "s3cret-token",
		ServerHost:      "127.0.0.1",
		ServerEncrypted: false,
	})
	if link != "" || message != couldNotUpload {
		t.Fatalf("link = %q message = %q", link, message)
	}
	if strings.Contains(message, "s3cret-token") || strings.Contains(message, server.URL) {
		t.Fatalf("failure leaked a secret or the endpoint: %q", message)
	}
	if hits != 1 {
		t.Fatalf("requests = %d, want 1", hits)
	}
}

func serverLocation(r *http.Request) string {
	return "http://" + r.Host + "/stolen"
}

func TestUploadRefusesCleartextWhenTheConnectionIsEncrypted(t *testing.T) {
	var hits int
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		hits++
		w.WriteHeader(http.StatusCreated)
	}))
	defer server.Close()
	path := filepath.Join(t.TempDir(), "note.txt")
	if err := os.WriteFile(path, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	link, message := Upload(Request{
		Endpoint:        server.URL + "/upload",
		Path:            path,
		ServerEncrypted: true,
	})
	if link != "" || message != couldNotUpload {
		t.Fatalf("link = %q message = %q", link, message)
	}
	if hits != 0 {
		t.Fatalf("requests = %d, want 0", hits)
	}
}

func TestUploadRejectsEmptyDirectoryAndHugeFiles(t *testing.T) {
	dir := t.TempDir()
	empty := filepath.Join(dir, "empty.txt")
	if err := os.WriteFile(empty, nil, 0o644); err != nil {
		t.Fatal(err)
	}
	if _, message := Upload(Request{Endpoint: "https://irc.example/upload", Path: empty}); message != fileEmpty {
		t.Fatalf("empty message = %q", message)
	}
	if _, message := Upload(Request{Endpoint: "https://irc.example/upload", Path: dir}); message != notAFile {
		t.Fatalf("directory message = %q", message)
	}
	if _, message := Upload(Request{Endpoint: "https://irc.example/upload", Path: filepath.Join(dir, "missing")}); message != notAFile {
		t.Fatalf("missing message = %q", message)
	}
	huge := filepath.Join(dir, "huge.bin")
	file, err := os.Create(huge)
	if err != nil {
		t.Fatal(err)
	}
	if err := file.Truncate(maximumBytes + 1); err != nil {
		t.Fatal(err)
	}
	file.Close()
	if _, message := Upload(Request{Endpoint: "https://irc.example/upload", Path: huge}); message != fileTooLarge {
		t.Fatalf("huge message = %q", message)
	}
}

func TestUploadAcceptsARelativePath(t *testing.T) {
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Location", "https://cdn.example/note.txt")
		w.WriteHeader(http.StatusCreated)
	}))
	defer server.Close()

	dir := t.TempDir()
	abs := filepath.Join(dir, "note.txt")
	if err := os.WriteFile(abs, []byte("x"), 0o644); err != nil {
		t.Fatal(err)
	}
	wd, err := os.Getwd()
	if err != nil {
		t.Fatal(err)
	}
	rel, err := filepath.Rel(wd, abs)
	if err != nil {
		t.Fatal(err)
	}
	if filepath.IsAbs(rel) {
		t.Skip("temp dir is not under the working directory")
	}
	link, message := Upload(Request{
		Endpoint:        server.URL + "/upload",
		Path:            rel,
		ServerEncrypted: false,
	})
	if message != "" || link != "https://cdn.example/note.txt" {
		t.Fatalf("link = %q message = %q", link, message)
	}
}

func TestSafeFileNameStripsHeaderBytes(t *testing.T) {
	if got := safeFileName("quote\"me.txt"); got != "quote_me.txt" {
		t.Fatalf("safe name = %q", got)
	}
	if got := safeFileName("semi;colon.txt"); got != "semi_colon.txt" {
		t.Fatalf("safe name = %q", got)
	}
}
