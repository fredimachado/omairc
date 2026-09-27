package avatar

import (
	"bytes"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

func TestTruecolorSupported(t *testing.T) {
	cases := []struct {
		name      string
		colorterm string
		term      string
		want      bool
	}{
		{"colorterm truecolor", "truecolor", "", true},
		{"colorterm 24bit", "24bit", "", true},
		{"colorterm case-insensitive", "TrueColor", "", true},
		{"term direct", "", "xterm-direct", true},
		{"term truecolor", "", "xterm-truecolor", true},
		{"term 256color", "", "xterm-256color", false},
		{"nothing advertised", "", "", false},
		{"colorterm no match", "yes", "xterm-256color", false},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			t.Setenv("COLORTERM", tc.colorterm)
			t.Setenv("TERM", tc.term)
			if got := TruecolorSupported(); got != tc.want {
				t.Fatalf("TruecolorSupported() = %v, want %v", got, tc.want)
			}
		})
	}
}

func TestRenderHalfBlocks(t *testing.T) {
	red := color.RGBA{R: 255, A: 255}
	blue := color.RGBA{B: 255, A: 255}
	rgba := image.NewRGBA(image.Rect(0, 0, 2, 2))
	rgba.Set(0, 0, red)
	rgba.Set(1, 0, red)
	rgba.Set(0, 1, blue)
	rgba.Set(1, 1, blue)

	got := renderHalfBlocks(rgba, 1)
	if want := "\x1b[38;2;255;0;0m\x1b[48;2;0;0;255m▀\x1b[0m"; got != want {
		t.Fatalf("renderHalfBlocks(rgba, 1) = %q, want %q", got, want)
	}
	if strings.Contains(got, "\n") {
		t.Fatalf("renderHalfBlocks output contains a newline: %q", got)
	}

	const cols = 7
	multi := renderHalfBlocks(rgba, cols)
	if count := strings.Count(multi, "▀"); count != cols {
		t.Fatalf("renderHalfBlocks count = %d, want %d", count, cols)
	}
	if strings.Contains(multi, "\n") {
		t.Fatalf("multi-column output contains a newline: %q", multi)
	}

	// image.NRGBA decodes through a different color model; the sampled top and
	// bottom colors must still land in the emitted SGR fragments.
	yellow := color.NRGBA{R: 255, G: 255, A: 255}
	cyan := color.NRGBA{G: 255, B: 255, A: 255}
	nrgba := image.NewNRGBA(image.Rect(0, 0, 2, 2))
	nrgba.Set(0, 0, yellow)
	nrgba.Set(1, 0, yellow)
	nrgba.Set(0, 1, cyan)
	nrgba.Set(1, 1, cyan)

	nrgbaOut := renderHalfBlocks(nrgba, 1)
	if !strings.Contains(nrgbaOut, "\x1b[38;2;255;255;0m") {
		t.Fatalf("NRGBA top color missing: %q", nrgbaOut)
	}
	if !strings.Contains(nrgbaOut, "\x1b[48;2;0;255;255m") {
		t.Fatalf("NRGBA bottom color missing: %q", nrgbaOut)
	}
	if count := strings.Count(nrgbaOut, "▀"); count != 1 {
		t.Fatalf("NRGBA rune count = %d, want 1", count)
	}
}

func TestDecodeBudgetAsksQuestions(t *testing.T) {
	// A small valid PNG decodes.
	small := image.NewRGBA(image.Rect(0, 0, 2, 2))
	small.Set(0, 0, color.RGBA{R: 255, A: 255})
	var smallBuf bytes.Buffer
	if err := png.Encode(&smallBuf, small); err != nil {
		t.Fatalf("png.Encode(small): %v", err)
	}
	if _, ok := decodeBoundedImage(smallBuf.Bytes()); !ok {
		t.Fatal("decodeBoundedImage rejected a valid PNG")
	}

	// The header budget rejects an oversized edge and an over-budget pixel
	// count before any pixel is decoded.
	if decodedConfigWithinBudget(image.Config{Width: 2048, Height: 1}) {
		t.Fatal("header budget accepted an edge past the cap")
	}
	if decodedConfigWithinBudget(image.Config{Width: 1024, Height: 1025}) {
		t.Fatal("header budget accepted more than the pixel cap")
	}
	if decodedConfigWithinBudget(image.Config{Width: 0, Height: 32}) {
		t.Fatal("header budget accepted a zero edge")
	}
	if !decodedConfigWithinBudget(image.Config{Width: 32, Height: 32}) {
		t.Fatal("header budget rejected a small image")
	}

	// A real but oversized PNG is rejected through the DecodeConfig precheck.
	big := image.NewRGBA(image.Rect(0, 0, 2048, 1))
	var bigBuf bytes.Buffer
	if err := png.Encode(&bigBuf, big); err != nil {
		t.Fatalf("png.Encode(big): %v", err)
	}
	if _, ok := decodeBoundedImage(bigBuf.Bytes()); ok {
		t.Fatal("decodeBoundedImage accepted an oversized PNG")
	}
}

func TestEnsureRejectsUnsafeURLs(t *testing.T) {
	store := New(true, nil)
	defer store.Close()

	for _, raw := range []string{
		"",
		"http://example.com/a.png",
		"https://localhost/a.png",
		"file:///etc/passwd",
	} {
		store.Ensure(raw, 32)
		if _, ok := store.Block(raw, 32, 4); ok {
			t.Fatalf("Block(%q) returned a value for an unsafe URL", raw)
		}
		store.mu.Lock()
		pending := len(store.pending)
		cached := len(store.images)
		store.mu.Unlock()
		if pending != 0 || cached != 0 {
			t.Fatalf("store not idle after Ensure(%q): pending=%d cached=%d",
				raw, pending, cached)
		}
	}
}

func TestCacheEvictionAndBlock(t *testing.T) {
	raw := "https://example.com/a.png"
	resolved, ok := irc.ResolvedAvatarURL(raw, irc.AvatarFetchPixelSize(32))
	if !ok || !irc.AvatarURLIsSafe(resolved) {
		t.Fatalf("test URL did not resolve safely: %v", resolved)
	}
	key := cacheKey(resolved)
	pixel := image.NewRGBA(image.Rect(0, 0, 1, 1))
	pixel.Set(0, 0, color.RGBA{R: 200, G: 100, B: 50, A: 255})

	const cols = 4
	store := New(true, nil)
	defer store.Close()
	store.remember(key, pixel)
	got, ok := store.Block(raw, 32, cols)
	if !ok {
		t.Fatal("Block returned false for a cached, safe image")
	}
	if count := strings.Count(got, "▀"); count != cols {
		t.Fatalf("Block rune count = %d, want %d", count, cols)
	}
	if strings.Contains(got, "\n") {
		t.Fatalf("Block output contains a newline: %q", got)
	}

	mono := New(false, nil)
	defer mono.Close()
	mono.remember(key, pixel)
	if _, ok := mono.Block(raw, 32, cols); ok {
		t.Fatal("Block returned a value with truecolor disabled")
	}

	evicting := New(true, nil)
	defer evicting.Close()
	for i := 0; i < maximumCacheEntries+3; i++ {
		evicting.remember(fmt.Sprintf("key-%d", i), pixel)
	}
	if _, ok := evicting.lookup("key-0"); ok {
		t.Fatal("oldest entry was not evicted")
	}
	if _, ok := evicting.lookup(fmt.Sprintf("key-%d", maximumCacheEntries+2)); !ok {
		t.Fatal("newest entry is missing")
	}
	evicting.mu.Lock()
	entries := len(evicting.images)
	evicting.mu.Unlock()
	if entries != maximumCacheEntries {
		t.Fatalf("cache holds %d entries, want %d", entries, maximumCacheEntries)
	}
}
