package version

import "testing"

func TestResolveValueLinkFlagsWin(t *testing.T) {
	got := resolveValue("1.0.4", "v9.9.9")
	if got != "1.0.4" {
		t.Fatalf("ldflags value = %q, want 1.0.4", got)
	}
}

func TestResolveValueStripsReleaseTag(t *testing.T) {
	got := resolveValue(sentinel, "v1.0.4")
	if got != "1.0.4" {
		t.Fatalf("module tag = %q, want 1.0.4", got)
	}
}

func TestResolveValueKeepsPseudoVersion(t *testing.T) {
	pseudo := "v0.0.0-20260929150405-abcdefabcdef"
	got := resolveValue(sentinel, pseudo)
	if got != pseudo {
		t.Fatalf("pseudo-version = %q, want %q", got, pseudo)
	}
}

func TestResolveValueKeepsSentinelForDevel(t *testing.T) {
	for _, raw := range []string{"", "(devel)"} {
		got := resolveValue(sentinel, raw)
		if got != sentinel {
			t.Fatalf("module version %q resolved to %q, want the sentinel", raw, got)
		}
	}
}

func TestDisplayModuleVersionStripsOneLeadingV(t *testing.T) {
	got := displayModuleVersion("v1.0.4-rc.1")
	if got != "1.0.4-rc.1" {
		t.Fatalf("prerelease = %q, want 1.0.4-rc.1", got)
	}
}
