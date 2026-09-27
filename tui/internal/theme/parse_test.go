package theme

import (
	"errors"
	"io"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func mustParse(t *testing.T, input string) Spec {
	t.Helper()
	spec, err := ParseColorsTOML(strings.NewReader(input))
	if err != nil {
		t.Fatalf("ParseColorsTOML(%q): %v", input, err)
	}
	return spec
}

func TestParseColorsTOMLReadsKeysAndStripsQuotes(t *testing.T) {
	spec := mustParse(t, "background = \"#101010\"\n"+
		"foreground = '#eeeeee'\n"+
		"accent = \"#5584aa\"\n"+
		"selection = \"#186a9a\"\n")

	assertHex(t, "Background", spec.Background, "#101010")
	assertHex(t, "Foreground", spec.Foreground, "#eeeeee")
	assertHex(t, "Accent", spec.Accent, "#5584aa")
	assertHex(t, "Selection", spec.Selection, "#186a9a")
	for _, present := range []bool{spec.HasBackground, spec.HasForeground, spec.HasAccent, spec.HasSelection} {
		if !present {
			t.Fatalf("expected every key to be present: %+v", spec)
		}
	}
}

func TestParseColorsTOMLSkipsBlankLinesAndComments(t *testing.T) {
	spec := mustParse(t, "# a comment\n\n   \nbackground = \"#101010\"\n# another\n")
	assertHex(t, "Background", spec.Background, "#101010")
}

func TestParseColorsTOMLTrimsWhitespaceAndKeepsTheLastValue(t *testing.T) {
	spec := mustParse(t, "   background   =   \"#111111\"   \nbackground = \"#222222\"\n")
	assertHex(t, "Background", spec.Background, "#222222")
}

func TestParseColorsTOMLModeWinsOverBackground(t *testing.T) {
	spec := mustParse(t, "mode = \"light\"\nbackground = \"#101010\"\n")
	if spec.Mode != ModeLight {
		t.Fatalf("Mode = %q, want %q", spec.Mode, ModeLight)
	}
	if !spec.HasMode {
		t.Fatal("expected HasMode")
	}
}

func TestParseColorsTOMLInfersModeFromBackground(t *testing.T) {
	dark := mustParse(t, "background = \"#101010\"\n")
	if dark.Mode != ModeDark {
		t.Fatalf("dark Mode = %q, want %q", dark.Mode, ModeDark)
	}
	light := mustParse(t, "background = \"#f0f0f0\"\n")
	if light.Mode != ModeLight {
		t.Fatalf("light Mode = %q, want %q", light.Mode, ModeLight)
	}
	if dark.HasMode || light.HasMode {
		t.Fatal("inferred mode must not report as explicit")
	}
}

func TestParseColorsTOMLUnknownModeFallsBackToLuminance(t *testing.T) {
	spec := mustParse(t, "mode = \"solarized\"\nbackground = \"#f0f0f0\"\n")
	if spec.Mode != ModeLight {
		t.Fatalf("Mode = %q, want inferred %q", spec.Mode, ModeLight)
	}
	if spec.HasMode {
		t.Fatal("an unknown mode must not count as present")
	}
}

func TestParseColorsTOMLMissingInputDefaultsToDark(t *testing.T) {
	for name, reader := range map[string]io.Reader{
		"nil":   nil,
		"empty": strings.NewReader(""),
	} {
		spec, err := ParseColorsTOML(reader)
		if err != nil {
			t.Fatalf("%s: ParseColorsTOML: %v", name, err)
		}
		if spec.Mode != ModeDark {
			t.Fatalf("%s: Mode = %q, want %q", name, spec.Mode, ModeDark)
		}
		if spec.HasBackground || spec.HasForeground || spec.HasAccent || spec.HasSelection || spec.HasMode {
			t.Fatalf("%s: expected no presence flags: %+v", name, spec)
		}
	}
}

func TestParseColorsTOMLIgnoresMalformedValues(t *testing.T) {
	spec := mustParse(t, "background = \"nope\"\naccent = \"#12345\"\nselection = \"#zzzzzz\"\n")
	if spec.HasBackground || spec.HasAccent || spec.HasSelection {
		t.Fatalf("expected malformed values to be ignored: %+v", spec)
	}
}

func TestParseColorsTOMLSkipsLinesWithoutEquals(t *testing.T) {
	spec := mustParse(t, "background = \"#101010\"\nnot a setting\n")
	assertHex(t, "Background", spec.Background, "#101010")
}

func TestParseColorsTOMLPropagatesReadError(t *testing.T) {
	if _, err := ParseColorsTOML(failingReader{}); err == nil {
		t.Fatal("expected the reader error to propagate")
	}
}

type failingReader struct{}

func (failingReader) Read([]byte) (int, error) { return 0, errors.New("boom") }

func TestLoadColorsMissingFileFallsBack(t *testing.T) {
	colors, found := loadColors(filepath.Join(t.TempDir(), "missing.toml"))
	if found {
		t.Fatal("expected found=false for a missing file")
	}
	if got, want := Hex(colors.Background), Hex(Fallback().Background); got != want {
		t.Fatalf("Background = %s, want fallback %s", got, want)
	}
}

func TestLoadColorsParsesFile(t *testing.T) {
	path := filepath.Join(t.TempDir(), "colors.toml")
	if err := os.WriteFile(path, []byte("mode = \"light\"\nbackground = \"#ffffff\"\n"), 0o644); err != nil {
		t.Fatal(err)
	}
	colors, found := loadColors(path)
	if !found {
		t.Fatal("expected found=true for a readable file")
	}
	assertHex(t, "Background", colors.Background, "#ffffff")
}
