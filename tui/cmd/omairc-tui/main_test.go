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

func TestNoArgumentsFailsClosed(t *testing.T) {
	code, _, stderr := runCapture(t)
	if code == 0 {
		t.Fatalf("no-argument invocation must fail while the Connect sheet is unlanded")
	}
	lower := strings.ToLower(stderr)
	if !strings.Contains(stderr, "--demo-server") && !strings.Contains(lower, "phase 5") {
		t.Fatalf("no-argument stderr must point at --demo-server or Phase 5, got %q", stderr)
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
