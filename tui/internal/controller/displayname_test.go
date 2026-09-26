package controller

import (
	"testing"

	"github.com/fredimachado/omairc/tui/internal/session"
)

func TestNetworkDisplayName(t *testing.T) {
	c := New()
	configs := []session.SessionConfig{
		session.DefaultSessionConfig("omarchy", "", "irc.example", "fred"),
		session.DefaultSessionConfig("oftc", "", "irc.example", "oak"),
		session.DefaultSessionConfig("libera", "Libera", "irc.libera.chat", "fred"),
	}
	for _, config := range configs {
		transport := session.NewLoopbackTransport()
		if _, err := c.AddSession(config, transport, nil); err != nil {
			t.Fatalf("AddSession(%q): %v", config.NetworkID, err)
		}
	}

	if got := c.NetworkDisplayName("omarchy"); got != "irc.example · fred" {
		t.Fatalf("NetworkDisplayName(omarchy) = %q, want %q", got, "irc.example · fred")
	}
	if got := c.NetworkDisplayName("oftc"); got != "irc.example · oak" {
		t.Fatalf("NetworkDisplayName(oftc) = %q, want %q", got, "irc.example · oak")
	}
	if got := c.NetworkDisplayName("libera"); got != "Libera" {
		t.Fatalf("NetworkDisplayName(libera) = %q, want %q", got, "Libera")
	}
	if got := c.NetworkDisplayName("nope"); got != "" {
		t.Fatalf("NetworkDisplayName(nope) = %q, want empty", got)
	}
	if got := c.NetworkDisplayName(""); got != "" {
		t.Fatalf("NetworkDisplayName(\"\") = %q, want empty", got)
	}
}
