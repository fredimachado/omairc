package gate

import (
	"encoding/json"
	"image"
	"image/color"
	"image/png"
	"net"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"
)

func TestExtractDesktopRecipe(t *testing.T) {
	md := "# Switch conversation\n\n" +
		"Some prose.\n\n" +
		"## Driving it with control-omairc\n\n" +
		"Preconditions here.\n\n" +
		"```desktop-recipe\n" +
		"launch --demo-server\n" +
		"wait-title --exact \"#omarchy · irc.example · fred - Omairc\"\n" +
		"\n" +
		"# a comment\n" +
		"screenshot --feature switch-conversation --name before-switch\n" +
		"```\n\n" +
		"- Notes below.\n\n" +
		"## Gotchas\n\n" +
		"```desktop-recipe\n" +
		"this-must-not-be-read\n" +
		"```\n"

	got, err := extractDesktopRecipe(md)
	if err != nil {
		t.Fatalf("extractDesktopRecipe: %v", err)
	}
	want := []string{
		"launch --demo-server",
		`wait-title --exact "#omarchy · irc.example · fred - Omairc"`,
		"screenshot --feature switch-conversation --name before-switch",
	}
	if len(got) != len(want) {
		t.Fatalf("commands = %q, want %q", got, want)
	}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("command %d = %q, want %q", i, got[i], want[i])
		}
	}
}

func TestExtractDesktopRecipeMissingFence(t *testing.T) {
	md := "# demo\n\n## Driving it with control-omairc\n\nno fence here\n"
	if _, err := extractDesktopRecipe(md); err == nil {
		t.Fatalf("expected an error for a missing fence")
	}
}

func TestExtractDesktopRecipeEmptyFence(t *testing.T) {
	md := "## Driving it with control-omairc\n\n```desktop-recipe\n```\n"
	got, err := extractDesktopRecipe(md)
	if err != nil {
		t.Fatalf("extractDesktopRecipe: %v", err)
	}
	if len(got) != 0 {
		t.Fatalf("commands = %q, want empty", got)
	}
}

func TestSplitRecipeLine(t *testing.T) {
	cases := []struct {
		line string
		want []string
	}{
		{"launch --demo-server", []string{"launch", "--demo-server"}},
		{`wait-title --exact "#omarchy · irc.example · fred - Omairc"`,
			[]string{"wait-title", "--exact", "#omarchy · irc.example · fred - Omairc"}},
		{`jump --query "#desktop"`, []string{"jump", "--query", "#desktop"}},
		{`key --key ctrl+q # trailing comment`, []string{"key", "--key", "ctrl+q"}},
		{"# whole-line comment", nil},
		{`type --text 'a b c'`, []string{"type", "--text", "a b c"}},
		{`send --text "plain text"`, []string{"send", "--text", "plain text"}},
	}
	for _, tc := range cases {
		got, err := splitRecipeLine(tc.line)
		if err != nil {
			t.Fatalf("splitRecipeLine(%q): %v", tc.line, err)
		}
		if len(got) != len(tc.want) {
			t.Fatalf("splitRecipeLine(%q) = %q, want %q", tc.line, got, tc.want)
		}
		for i := range tc.want {
			if got[i] != tc.want[i] {
				t.Fatalf("splitRecipeLine(%q) = %q, want %q", tc.line, got, tc.want)
			}
		}
	}
}

func TestSplitRecipeLineUnbalancedQuotes(t *testing.T) {
	if _, err := splitRecipeLine(`type --text "unterminated`); err == nil {
		t.Fatalf("expected an error for unbalanced quotes")
	}
}

func TestRejectRecipeVerb(t *testing.T) {
	for _, verb := range []string{"click-conversation", "click-member", "click", "qml-suite", "doctor-qml"} {
		if verb == "click" {
			// `click` alone is not a "click-" verb; it falls through to the
			// generic unsupported-verb error during replay.
			if err := rejectRecipeVerb(verb); err != nil {
				t.Fatalf("rejectRecipeVerb(%q) = %v, want nil", verb, err)
			}
			continue
		}
		if err := rejectRecipeVerb(verb); err == nil {
			t.Fatalf("rejectRecipeVerb(%q) should error", verb)
		}
	}
	for _, verb := range []string{"launch", "wait-title", "walk", "unread", "screenshot", "send",
		"jump", "status", "connect", "compare"} {
		if err := rejectRecipeVerb(verb); err != nil {
			t.Fatalf("rejectRecipeVerb(%q) = %v, want nil", verb, err)
		}
	}
}

func TestParseSize(t *testing.T) {
	cols, rows, err := parseSize("118x30")
	if err != nil || cols != 118 || rows != 30 {
		t.Fatalf("parseSize = %d,%d,%v; want 118,30,nil", cols, rows, err)
	}
	for _, bad := range []string{"", "118", "118x", "x30", "0x30", "118x0", "axb"} {
		if _, _, err := parseSize(bad); err == nil {
			t.Fatalf("parseSize(%q) should error", bad)
		}
	}
}

func writeTestPNG(t *testing.T, path string, img image.Image) {
	t.Helper()
	f, err := os.Create(path)
	if err != nil {
		t.Fatal(err)
	}
	if err := png.Encode(f, img); err != nil {
		_ = f.Close()
		t.Fatal(err)
	}
	if err := f.Close(); err != nil {
		t.Fatal(err)
	}
}

func TestRemapVerifyPath(t *testing.T) {
	cases := map[string]string{
		"test-artifacts/verify/keyboard/before.png":     "test-artifacts/verify-tui/keyboard/before.png",
		"test-artifacts/verify-tui/keyboard/before.png": "test-artifacts/verify-tui/keyboard/before.png",
		"other/before.png":                              "other/before.png",
	}
	for input, want := range cases {
		if got := remapVerifyPath(input); got != want {
			t.Fatalf("remapVerifyPath(%q) = %q, want %q", input, got, want)
		}
	}
}

func TestCompareImages(t *testing.T) {
	dir := t.TempDir()
	red := image.NewRGBA(image.Rect(0, 0, 2, 2))
	red.Set(0, 0, color.RGBA{R: 0xff, A: 0xff})
	blue := image.NewRGBA(image.Rect(0, 0, 2, 2))
	blue.Set(0, 0, color.RGBA{B: 0xff, A: 0xff})
	wide := image.NewRGBA(image.Rect(0, 0, 3, 2))

	redPath := filepath.Join(dir, "red.png")
	redCopyPath := filepath.Join(dir, "red-copy.png")
	bluePath := filepath.Join(dir, "blue.png")
	widePath := filepath.Join(dir, "wide.png")
	writeTestPNG(t, redPath, red)
	writeTestPNG(t, redCopyPath, red)
	writeTestPNG(t, bluePath, blue)
	writeTestPNG(t, widePath, wide)

	var out strings.Builder
	if err := cmdCompare([]string{"--before", redPath, "--after", redCopyPath}, &out); err == nil ||
		!strings.Contains(err.Error(), "images are identical") {
		t.Fatalf("identical compare error = %v", err)
	}

	out.Reset()
	if err := cmdCompare([]string{"--before", redPath, "--after", bluePath}, &out); err != nil {
		t.Fatalf("differing compare error = %v", err)
	}
	if got := out.String(); !strings.Contains(got, "images differ (2x2)") {
		t.Fatalf("differing compare output = %q", got)
	}

	if err := cmdCompare([]string{"--before", redPath, "--after", widePath}, &out); err == nil ||
		!strings.Contains(err.Error(), "image sizes differ") {
		t.Fatalf("size-mismatch compare error = %v", err)
	}

	if err := cmdCompare([]string{"--before", filepath.Join(dir, "missing.png"), "--after", redPath}, &out); err == nil {
		t.Fatalf("missing-file compare should error")
	}
}

// TestRunDryRunRejectsClickVerbs checks that a dry run prints the fence and
// exits 0, and that a real replay rejects the unsupported pixel verb before
// touching a daemon.
func TestRunDryRunAndReject(t *testing.T) {
	root := t.TempDir()
	features := root + "/.cursor/skills/verify-omairc/features"
	if err := os.MkdirAll(features, 0o755); err != nil {
		t.Fatal(err)
	}
	md := "## Driving it with control-omairc\n\n```desktop-recipe\n" +
		"launch --demo-server\n" +
		"click-conversation --name \"#desktop\"\n" +
		"```\n"
	if err := os.WriteFile(features+"/demo.md", []byte(md), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Setenv("OMAIRC_TUI_ROOT", root)
	t.Setenv("OMAIRC_TUI_STATE", t.TempDir())

	var out, errBuf strings.Builder
	if code := Run([]string{"run", "demo", "--dry-run"}, &out, &errBuf); code != 0 {
		t.Fatalf("dry run exit = %d, stderr = %q", code, errBuf.String())
	}
	if !strings.Contains(out.String(), "launch --demo-server") {
		t.Fatalf("dry run output = %q", out.String())
	}

	out.Reset()
	errBuf.Reset()
	if code := Run([]string{"run", "demo"}, &out, &errBuf); code == 0 {
		t.Fatalf("replay of a click recipe should fail")
	}
	if !strings.Contains(errBuf.String(), "click-conversation") {
		t.Fatalf("replay error = %q, want it to name click-conversation", errBuf.String())
	}
	// The failure must name the feature file so the report is actionable.
	if !strings.Contains(errBuf.String(), filepath.Join("features", "demo.md")) {
		t.Fatalf("replay error = %q, want it to name the feature file", errBuf.String())
	}
}

func TestRunMissingFeature(t *testing.T) {
	t.Setenv("OMAIRC_TUI_ROOT", t.TempDir())
	t.Setenv("OMAIRC_TUI_STATE", t.TempDir())
	var out, errBuf strings.Builder
	if code := Run([]string{"run", "nope"}, &out, &errBuf); code == 0 {
		t.Fatalf("run of a missing feature should fail")
	}
}

func TestHelpAndUnknownVerb(t *testing.T) {
	var out, errBuf strings.Builder
	if code := Run([]string{"--help"}, &out, &errBuf); code != 0 {
		t.Fatalf("--help exit = %d", code)
	}
	if !strings.Contains(out.String(), "usage: control-omairc-tui") {
		t.Fatalf("help = %q", out.String())
	}
	out.Reset()
	errBuf.Reset()
	if code := Run([]string{"nope"}, &out, &errBuf); code == 0 {
		t.Fatalf("unknown verb should exit non-zero")
	}
	if !strings.Contains(errBuf.String(), "unknown verb") {
		t.Fatalf("stderr = %q", errBuf.String())
	}
}

func TestDoctorWithoutDaemon(t *testing.T) {
	t.Setenv("OMAIRC_TUI_STATE", t.TempDir())
	var out, errBuf strings.Builder
	if code := Run([]string{"doctor"}, &out, &errBuf); code == 0 {
		t.Fatalf("doctor without a daemon should fail")
	}
	if !strings.Contains(errBuf.String(), "no running omairc-tui daemon") {
		t.Fatalf("stderr = %q", errBuf.String())
	}
}

func TestPidAlive(t *testing.T) {
	if !pidAlive(os.Getpid()) {
		t.Fatalf("pidAlive(self) should be true")
	}
	if pidAlive(0) || pidAlive(-1) {
		t.Fatalf("pidAlive of a non-positive pid should be false")
	}
	cmd := exec.Command("true")
	if err := cmd.Start(); err != nil {
		t.Skipf("cannot start a throwaway process: %v", err)
	}
	dead := cmd.Process.Pid
	_ = cmd.Wait()
	if pidAlive(dead) {
		t.Fatalf("pidAlive(%d) for a reaped process should be false", dead)
	}
}

// startFakeDaemon serves the client wire protocol on the state-dir socket so a
// test can exercise launch reuse and teardown without a real PTY child. It
// answers `status` with the requested demo flag and closes on `quit`.
func startFakeDaemon(t *testing.T, demo bool) *atomic.Bool {
	t.Helper()
	if err := os.MkdirAll(stateDir(), 0o755); err != nil {
		t.Fatal(err)
	}
	sock := socketPath()
	_ = os.Remove(sock)
	ln, err := net.Listen("unix", sock)
	if err != nil {
		t.Fatalf("listen fake daemon: %v", err)
	}
	quit := &atomic.Bool{}
	go func() {
		for {
			conn, err := ln.Accept()
			if err != nil {
				return
			}
			go func(c net.Conn) {
				defer c.Close()
				var req Request
				if err := json.NewDecoder(c).Decode(&req); err != nil {
					return
				}
				resp := Response{OK: true}
				switch req.Verb {
				case "status":
					resp.Demo = demo
					resp.Title = "fake"
				case "quit":
					quit.Store(true)
					_ = ln.Close()
					_ = os.Remove(sock)
				}
				_ = json.NewEncoder(c).Encode(resp)
			}(conn)
		}
	}()
	t.Cleanup(func() {
		_ = ln.Close()
		_ = os.Remove(sock)
	})
	return quit
}

// TestStopSpawnedDaemonQuitsAndCleansState covers the launch-teardown helper:
// it asks a running daemon to quit and removes the state files it left behind.
func TestStopSpawnedDaemonQuitsAndCleansState(t *testing.T) {
	t.Setenv("OMAIRC_TUI_STATE", t.TempDir())
	quit := startFakeDaemon(t, true)
	if err := os.WriteFile(stateFilePath(), []byte(`{"pid":0}`), 0o644); err != nil {
		t.Fatal(err)
	}
	stopSpawnedDaemon(0)
	if !quit.Load() {
		t.Fatalf("stopSpawnedDaemon should ask the daemon to quit")
	}
	if _, err := os.Stat(stateFilePath()); !os.IsNotExist(err) {
		t.Fatalf("state.json should be removed, stat err = %v", err)
	}
	if _, err := os.Stat(socketPath()); !os.IsNotExist(err) {
		t.Fatalf("socket should be removed, stat err = %v", err)
	}
}

// TestRunFailureKeepsPreexistingDaemon checks finding 2: a run that skipped
// `launch` because a healthy matching daemon already existed must not tear that
// daemon down when a later recipe step fails.
func TestRunFailureKeepsPreexistingDaemon(t *testing.T) {
	root := t.TempDir()
	features := filepath.Join(root, ".cursor", "skills", "verify-omairc", "features")
	if err := os.MkdirAll(features, 0o755); err != nil {
		t.Fatal(err)
	}
	md := "## Driving it with control-omairc\n\n```desktop-recipe\n" +
		"launch --demo-server\n" +
		"bogus-verb\n" +
		"```\n"
	if err := os.WriteFile(filepath.Join(features, "demo.md"), []byte(md), 0o644); err != nil {
		t.Fatal(err)
	}
	t.Setenv("OMAIRC_TUI_ROOT", root)
	t.Setenv("OMAIRC_TUI_STATE", t.TempDir())

	quit := startFakeDaemon(t, true)

	var out, errBuf strings.Builder
	if code := Run([]string{"run", "demo"}, &out, &errBuf); code == 0 {
		t.Fatalf("run should fail on the unsupported verb")
	}
	if !strings.Contains(errBuf.String(), "bogus-verb") {
		t.Fatalf("error = %q, want it to name the unsupported verb", errBuf.String())
	}
	if !strings.Contains(errBuf.String(), filepath.Join("features", "demo.md")) {
		t.Fatalf("error = %q, want it to name the feature file", errBuf.String())
	}
	if quit.Load() {
		t.Fatalf("a daemon the run did not launch must not be torn down")
	}
}

// TestServeExitsWhenChildDies covers finding 1a: the daemon must notice its PTY
// child exit and shut itself down, so a crashed child cannot leave a daemon and
// socket behind that wedge the next launch.
func TestServeExitsWhenChildDies(t *testing.T) {
	if _, err := os.Stat("/bin/true"); err != nil {
		t.Skip("/bin/true is unavailable")
	}
	t.Setenv("OMAIRC_TUI_STATE", t.TempDir())
	done := make(chan error, 1)
	go func() {
		done <- Serve(ServeOptions{Binary: "/bin/true", Cols: 80, Rows: 24})
	}()
	select {
	case err := <-done:
		if err != nil {
			t.Fatalf("Serve: %v", err)
		}
	case <-time.After(10 * time.Second):
		t.Fatalf("Serve did not return after its PTY child exited")
	}
	if _, err := os.Stat(socketPath()); !os.IsNotExist(err) {
		t.Fatalf("daemon socket should be removed, stat err = %v", err)
	}
	if _, err := os.Stat(stateFilePath()); !os.IsNotExist(err) {
		t.Fatalf("daemon state.json should be removed, stat err = %v", err)
	}
}
