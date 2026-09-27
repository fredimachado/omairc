package main

import (
	"bytes"
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/version"
)

func runCapture(t *testing.T, args ...string) (code int, stdout, stderr string) {
	t.Helper()
	var out, errOut bytes.Buffer
	code = run(args, &out, &errOut)
	return code, out.String(), errOut.String()
}

func TestVersionPrintsInjectedValue(t *testing.T) {
	code, stdout, stderr := runCapture(t, "--version")
	if code != 0 {
		t.Fatalf("--version exit = %d, want 0 (stderr: %q)", code, stderr)
	}
	want := "omairc-tui " + version.Value + "\n"
	if stdout != want {
		t.Fatalf("--version stdout = %q, want %q", stdout, want)
	}
}

func TestHelpPrintsUsageWithoutVersionLine(t *testing.T) {
	code, stdout, stderr := runCapture(t, "--help")
	if code != 0 {
		t.Fatalf("--help exit = %d, want 0 (stderr: %q)", code, stderr)
	}
	if strings.HasPrefix(stdout, "omairc-tui ") {
		t.Fatalf("--help must not print the version line, got %q", stdout)
	}
	for _, token := range []string{"usage: omairc-tui", "--version", "--help", "--demo-server"} {
		if !strings.Contains(stdout, token) {
			t.Fatalf("--help output must mention %q, got %q", token, stdout)
		}
	}
}

func TestDemoServerFailsClosed(t *testing.T) {
	code, stdout, stderr := runCapture(t, "--demo-server")
	if code == 0 {
		t.Fatalf("--demo-server must fail closed until Phase 3")
	}
	if stdout != "" {
		t.Fatalf("--demo-server stdout = %q, want empty", stdout)
	}
	if !strings.Contains(stderr, "demo-server") {
		t.Fatalf("--demo-server stderr must name the flag, got %q", stderr)
	}
}

func TestNoArgumentsFailsClosed(t *testing.T) {
	code, _, _ := runCapture(t)
	if code == 0 {
		t.Fatalf("no-argument invocation must fail while the TUI is unimplemented")
	}
}

func TestUnknownArgumentFails(t *testing.T) {
	code, _, stderr := runCapture(t, "--nope")
	if code == 0 {
		t.Fatalf("unknown argument must fail")
	}
	if !strings.Contains(stderr, "--nope") {
		t.Fatalf("unknown-argument stderr must name the argument, got %q", stderr)
	}
}
