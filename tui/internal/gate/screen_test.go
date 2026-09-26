package gate

import (
	"bytes"
	"image"
	"image/color"
	"image/png"
	"testing"
)

func TestScreenOSCTitleBEL(t *testing.T) {
	s := NewScreen(20, 3)
	s.Feed([]byte("\x1b]2;hello world\x07"))
	if got := s.Title(); got != "hello world" {
		t.Fatalf("title = %q, want %q", got, "hello world")
	}
}

func TestScreenOSCTitleST(t *testing.T) {
	s := NewScreen(20, 3)
	s.Feed([]byte("\x1b]0;st title\x1b\\"))
	if got := s.Title(); got != "st title" {
		t.Fatalf("title = %q, want %q", got, "st title")
	}
}

func TestScreenOSCTitleSelector1(t *testing.T) {
	s := NewScreen(20, 3)
	s.Feed([]byte("\x1b]1;icon name\x07"))
	if got := s.Title(); got != "icon name" {
		t.Fatalf("title = %q, want %q", got, "icon name")
	}
}

func TestScreenOSCTitleSplitAcrossFeeds(t *testing.T) {
	s := NewScreen(20, 3)
	s.Feed([]byte("\x1b]2;par"))
	if got := s.Title(); got != "" {
		t.Fatalf("title after partial OSC = %q, want empty", got)
	}
	s.Feed([]byte("tial\x07"))
	if got := s.Title(); got != "partial" {
		t.Fatalf("title = %q, want %q", got, "partial")
	}
}

func TestScreenOSCIgnoresOtherSelectors(t *testing.T) {
	s := NewScreen(20, 3)
	s.Feed([]byte("\x1b]8;;https://example.com\x07x"))
	if got := s.Title(); got != "" {
		t.Fatalf("title = %q, want empty", got)
	}
	if got := s.Lines()[0]; got != "x" {
		t.Fatalf("line = %q, want %q", got, "x")
	}
}

func TestScreenCursorPosition(t *testing.T) {
	s := NewScreen(10, 3)
	s.Feed([]byte("\x1b[3;5Hhi"))
	lines := s.Lines()
	if lines[2] != "    hi" {
		t.Fatalf("row 3 = %q, want %q", lines[2], "    hi")
	}
}

func TestScreenCRLF(t *testing.T) {
	s := NewScreen(8, 3)
	s.Feed([]byte("a\r\nb"))
	lines := s.Lines()
	if lines[0] != "a" || lines[1] != "b" {
		t.Fatalf("lines = %q, want [a b]", lines)
	}
}

func TestScreenBackspaceAndTab(t *testing.T) {
	s := NewScreen(16, 1)
	s.Feed([]byte("ab\bX"))
	if got := s.Lines()[0]; got != "aX" {
		t.Fatalf("backspace line = %q, want %q", got, "aX")
	}

	tab := NewScreen(16, 1)
	tab.Feed([]byte("a\tb"))
	if got := tab.Lines()[0]; got != "a       b" {
		t.Fatalf("tab line = %q, want %q", got, "a       b")
	}
}

func TestScreenEraseInLine(t *testing.T) {
	s := NewScreen(10, 1)
	s.Feed([]byte("abcdef\rxy\x1b[K"))
	if got := s.Lines()[0]; got != "xy" {
		t.Fatalf("EL0 line = %q, want %q", got, "xy")
	}

	s.Feed([]byte("\x1b[2K"))
	if got := s.Lines()[0]; got != "" {
		t.Fatalf("EL2 line = %q, want empty", got)
	}

	left := NewScreen(10, 1)
	left.Feed([]byte("abcdef\r\x1b[3C\x1b[1K"))
	if got := left.Lines()[0]; got != "    ef" {
		t.Fatalf("EL1 line = %q, want %q", got, "    ef")
	}
}

func TestScreenEraseInDisplay(t *testing.T) {
	s := NewScreen(4, 3)
	s.Feed([]byte("aa\r\nbb\r\ncc"))
	s.Feed([]byte("\x1b[2;1H\x1b[J"))
	lines := s.Lines()
	if lines[0] != "aa" || lines[1] != "" || lines[2] != "" {
		t.Fatalf("lines = %q, want [aa  ]", lines)
	}

	s.Feed([]byte("\x1b[2J"))
	for i, line := range s.Lines() {
		if line != "" {
			t.Fatalf("row %d = %q after ED2, want empty", i, line)
		}
	}
}

func TestScreenInsertAndDeleteLines(t *testing.T) {
	ins := NewScreen(4, 3)
	ins.Feed([]byte("aa\r\nbb\r\ncc"))
	ins.Feed([]byte("\x1b[1;1H\x1b[L"))
	got := ins.Lines()
	want := []string{"", "aa", "bb"}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("insert lines = %q, want %q", got, want)
		}
	}

	del := NewScreen(4, 3)
	del.Feed([]byte("aa\r\nbb\r\ncc"))
	del.Feed([]byte("\x1b[1;1H\x1b[M"))
	got = del.Lines()
	want = []string{"bb", "cc", ""}
	for i := range want {
		if got[i] != want[i] {
			t.Fatalf("delete lines = %q, want %q", got, want)
		}
	}
}

func TestScreenInsertDeleteEraseChars(t *testing.T) {
	ins := NewScreen(6, 1)
	ins.Feed([]byte("abc\x1b[1;2H\x1b[2@"))
	if got := ins.Lines()[0]; got != "a  bc" {
		t.Fatalf("insert chars = %q, want %q", got, "a  bc")
	}

	del := NewScreen(6, 1)
	del.Feed([]byte("abcd\x1b[1;2H\x1b[2P"))
	if got := del.Lines()[0]; got != "ad" {
		t.Fatalf("delete chars = %q, want %q", got, "ad")
	}

	er := NewScreen(6, 1)
	er.Feed([]byte("abcdef\x1b[1;2H\x1b[2X"))
	if got := er.Lines()[0]; got != "a  def" {
		t.Fatalf("erase chars = %q, want %q", got, "a  def")
	}
}

func TestScreenSaveRestoreCursor(t *testing.T) {
	s := NewScreen(10, 2)
	s.Feed([]byte("ab\x1b7cd\x1b8ef"))
	if got := s.Lines()[0]; got != "abef" {
		t.Fatalf("ESC 7/8 line = %q, want %q", got, "abef")
	}

	csi := NewScreen(10, 2)
	csi.Feed([]byte("ab\x1b[scd\x1b[uef"))
	if got := csi.Lines()[0]; got != "abef" {
		t.Fatalf("CSI s/u line = %q, want %q", got, "abef")
	}
}

func TestScreenResize(t *testing.T) {
	s := NewScreen(4, 2)
	s.Feed([]byte("abcd\r\nefgh"))
	s.Resize(6, 3)
	lines := s.Lines()
	if len(lines) != 3 {
		t.Fatalf("height after resize = %d, want 3", len(lines))
	}
	if lines[0] != "abcd" || lines[1] != "efgh" || lines[2] != "" {
		t.Fatalf("lines after grow = %q", lines)
	}
	s.Resize(2, 2)
	lines = s.Lines()
	if lines[0] != "ab" || lines[1] != "ef" {
		t.Fatalf("lines after shrink = %q, want [ab ef]", lines)
	}
}

func TestScreenUTF8SplitAcrossFeeds(t *testing.T) {
	s := NewScreen(8, 1)
	text := []byte("héllo")
	s.Feed(text[:2]) // "h" plus the first byte of é
	if got := s.Lines()[0]; got != "h" {
		t.Fatalf("line after partial rune = %q, want %q", got, "h")
	}
	s.Feed(text[2:])
	if got := s.Lines()[0]; got != "héllo" {
		t.Fatalf("line = %q, want %q", got, "héllo")
	}
}

func TestScreenWideRune(t *testing.T) {
	s := NewScreen(8, 1)
	s.Feed([]byte("中x"))
	if got := s.Lines()[0]; got != "中x" {
		t.Fatalf("line = %q, want %q", got, "中x")
	}
}

func TestScreenAltScreenEntryClearsGrid(t *testing.T) {
	s := NewScreen(6, 2)
	s.Feed([]byte("hello"))
	if got := s.Lines()[0]; got != "hello" {
		t.Fatalf("line before alt screen = %q, want %q", got, "hello")
	}
	s.Feed([]byte("\x1b[?1049h"))
	for i, line := range s.Lines() {
		if line != "" {
			t.Fatalf("row %d = %q after ?1049h, want empty", i, line)
		}
	}
}

func TestScreenUnknownCSIIgnored(t *testing.T) {
	s := NewScreen(8, 1)
	s.Feed([]byte("a\x1b[?25l\x1b[>0qb"))
	if got := s.Lines()[0]; got != "ab" {
		t.Fatalf("line = %q, want %q", got, "ab")
	}
}

func TestScreenTextAndLines(t *testing.T) {
	s := NewScreen(4, 2)
	s.Feed([]byte("hi\r\nyo"))
	if got := s.Text(); got != "hi\nyo" {
		t.Fatalf("Text() = %q, want %q", got, "hi\nyo")
	}
}

func TestScreenPNGDeterministic(t *testing.T) {
	s := NewScreen(4, 2)
	s.Feed([]byte("hi"))
	first, err := s.PNG()
	if err != nil {
		t.Fatalf("PNG: %v", err)
	}
	second, err := s.PNG()
	if err != nil {
		t.Fatalf("PNG: %v", err)
	}
	if !bytes.Equal(first, second) {
		t.Fatalf("PNG output is not deterministic")
	}
	img, err := png.Decode(bytes.NewReader(first))
	if err != nil {
		t.Fatalf("decode PNG: %v", err)
	}
	if img.Bounds().Dx() != 4*pngCellWidth || img.Bounds().Dy() != 2*pngCellHeight {
		t.Fatalf("PNG bounds = %v, want %dx%d", img.Bounds(), 4*pngCellWidth, 2*pngCellHeight)
	}
	colors := map[uint32]bool{}
	for y := img.Bounds().Min.Y; y < img.Bounds().Max.Y; y++ {
		for x := img.Bounds().Min.X; x < img.Bounds().Max.X; x++ {
			r, g, b, _ := img.At(x, y).RGBA()
			colors[r<<16|g<<8|b] = true
		}
	}
	if len(colors) < 2 {
		t.Fatalf("PNG has %d colors, want at least 2 (background plus glyphs)", len(colors))
	}
}

func TestScreenPNGRejectsEmptyGrid(t *testing.T) {
	if _, err := NewScreen(0, 0).PNG(); err == nil {
		t.Fatalf("PNG on an empty grid should error")
	}
}

// decodePNG renders s and decodes the result, failing the test on any error.
func decodePNG(t *testing.T, s *Screen) image.Image {
	t.Helper()
	data, err := s.PNG()
	if err != nil {
		t.Fatalf("PNG: %v", err)
	}
	img, err := png.Decode(bytes.NewReader(data))
	if err != nil {
		t.Fatalf("decode PNG: %v", err)
	}
	return img
}

// pixelAt returns the 8-bit RGBA value at a pixel.
func pixelAt(img image.Image, x, y int) color.RGBA {
	r, g, b, a := img.At(x, y).RGBA()
	return color.RGBA{R: uint8(r >> 8), G: uint8(g >> 8), B: uint8(b >> 8), A: uint8(a >> 8)}
}

// cellColorsIn collects the distinct colors in one cell rectangle.
func cellColorsIn(img image.Image, cellX, cellY int) map[color.RGBA]bool {
	set := map[color.RGBA]bool{}
	for y := cellY; y < cellY+pngCellHeight; y++ {
		for x := cellX; x < cellX+pngCellWidth; x++ {
			set[pixelAt(img, x, y)] = true
		}
	}
	return set
}

func TestScreenPNGGlyphStrokes(t *testing.T) {
	s := NewScreen(8, 1)
	s.Feed([]byte("Omairc"))
	img := decodePNG(t, s)

	if got, want := img.Bounds().Dx(), 8*pngCellWidth; got != want {
		t.Fatalf("PNG width = %d, want %d", got, want)
	}
	if got, want := img.Bounds().Dy(), 1*pngCellHeight; got != want {
		t.Fatalf("PNG height = %d, want %d", got, want)
	}

	// The first cell holds 'O'. Its strokes must paint foreground pixels over
	// the cell background, so the cell is neither all background nor all
	// foreground: both colors have to appear.
	colors := cellColorsIn(img, 0, 0)
	if !colors[pngDefaultBg] {
		t.Fatalf("'O' cell has no background pixels: %v", colors)
	}
	if !colors[pngDefaultFg] {
		t.Fatalf("'O' cell has no glyph strokes: %v", colors)
	}
	if len(colors) < 2 {
		t.Fatalf("'O' cell is a single flat color, want strokes over background: %v", colors)
	}
}

func TestScreenPNGSpaceCellIsBackground(t *testing.T) {
	s := NewScreen(3, 1)
	s.Feed([]byte("a b"))
	img := decodePNG(t, s)

	// Column 1 is the space: pure cell background, no ink.
	colors := cellColorsIn(img, pngCellWidth, 0)
	if len(colors) != 1 || !colors[pngDefaultBg] {
		t.Fatalf("space cell colors = %v, want only background %v", colors, pngDefaultBg)
	}
}

func TestScreenPNGUnknownRuneFallback(t *testing.T) {
	s := NewScreen(2, 1)
	s.Feed([]byte("\u2603")) // snowman: has no glyph in the font
	img := decodePNG(t, s)

	// The unknown rune draws a solid box, so the cell still shows ink over its
	// background instead of silently disappearing.
	colors := cellColorsIn(img, 0, 0)
	if !colors[pngDefaultBg] || !colors[pngDefaultFg] {
		t.Fatalf("fallback cell colors = %v, want background plus a solid box", colors)
	}
}

func TestScreenPNGReverseSwapsColors(t *testing.T) {
	s := NewScreen(1, 1)
	s.Feed([]byte("\x1b[7mO"))
	img := decodePNG(t, s)

	// With reverse video the cell background becomes the default foreground,
	// so the cell corner (background) is the foreground color and vice versa.
	if got := pixelAt(img, 0, 0); got != pngDefaultFg {
		t.Fatalf("reverse cell corner = %v, want reversed background %v", got, pngDefaultFg)
	}
	if got := pixelAt(img, 0, pngCellHeight-1); got != pngDefaultFg {
		t.Fatalf("reverse cell bottom corner = %v, want reversed background %v", got, pngDefaultFg)
	}
}
