package gate

import (
	"path/filepath"
	"strings"
	"testing"
)

// TestTerminalEnvStripsNoColorHints pins that the PTY driver never hands the
// child an inherited no-color hint. NO_COLOR and CLICOLOR are presence checks in
// the color libraries the shell renders through, so a hint from the developer's
// shell or a CI runner stripped every SGR and left the gate's PNG evidence mono:
// a mono screenshot cannot show the composer's surface fill, the nick colors, or
// the presence dots the screenshots exist to prove.
func TestTerminalEnvStripsNoColorHints(t *testing.T) {
	t.Setenv("NO_COLOR", "1")
	t.Setenv("CLICOLOR", "0")
	t.Setenv("CLICOLOR_FORCE", "1")

	stateDir := t.TempDir()
	values := map[string]string{}
	for _, entry := range terminalEnv(stateDir) {
		name, value, ok := strings.Cut(entry, "=")
		if !ok {
			continue
		}
		values[strings.ToUpper(name)] = value
	}

	for _, name := range []string{"NO_COLOR", "CLICOLOR"} {
		if value, ok := values[name]; ok {
			t.Fatalf("terminalEnv kept %s=%q; the shell renders mono when a no-color hint leaks", name, value)
		}
	}
	// A force hint is not a no-color hint: it stays, and the pinned terminal
	// variables make the child color-capable either way.
	if got := values["CLICOLOR_FORCE"]; got != "1" {
		t.Fatalf("CLICOLOR_FORCE = %q, want the inherited force preserved", got)
	}
	if got := values["TERM"]; got != "xterm-256color" {
		t.Fatalf("TERM = %q, want xterm-256color", got)
	}
	if got := values["COLORTERM"]; got != "truecolor" {
		t.Fatalf("COLORTERM = %q, want truecolor", got)
	}
	if got, want := values["XDG_STATE_HOME"], filepath.Join(stateDir, "app-state"); got != want {
		t.Fatalf("XDG_STATE_HOME = %q, want %q", got, want)
	}
}

// TestEnvNamesMatchesOnlyTheExactName pins the filter's edge: an entry is only
// dropped when its name matches whole, so CLICOLOR_FORCE survives a CLICOLOR
// filter and a lookalike prefix is untouched.
func TestEnvNamesMatchesOnlyTheExactName(t *testing.T) {
	cases := []struct {
		entry, name string
		want        bool
	}{
		{"NO_COLOR=1", "NO_COLOR", true},
		{"no_color=", "NO_COLOR", true},
		{"NO_COLOR", "NO_COLOR", false},
		{"NO_COLORX=1", "NO_COLOR", false},
		{"CLICOLOR_FORCE=1", "CLICOLOR", false},
		{"CLICOLOR=0", "CLICOLOR", true},
	}
	for _, c := range cases {
		if got := envNames(c.entry, c.name); got != c.want {
			t.Fatalf("envNames(%q, %q) = %v, want %v", c.entry, c.name, got, c.want)
		}
	}
}
