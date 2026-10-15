package avatar

import (
	"bytes"
	"encoding/base64"
	"image"
	"image/color"
	"io"
	"os"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/charmbracelet/x/term"
	"github.com/fredimachado/omairc/tui/internal/irc"
)

func TestTransmitChunksQuietly(t *testing.T) {
	small := []byte("hi")
	got := TransmitPNG(7, small)
	want := "\x1b_Ga=t,f=100,i=7,q=2,U=1,m=0;" + base64.StdEncoding.EncodeToString(small) + "\x1b\\"
	if got != want {
		t.Fatalf("TransmitPNG small = %q, want %q", got, want)
	}
	if strings.Count(got, "m=0") != 1 || strings.Contains(got, "m=1") {
		t.Fatalf("single chunk must be m=0 only: %q", got)
	}

	// 3075 raw bytes encode to 4100 base64 characters: 4096 + a 4-byte tail.
	two := bytes.Repeat([]byte{0xAB}, 3075)
	parts := parseAPCs(t, TransmitPNG(3, two))
	if len(parts) != 2 {
		t.Fatalf("two-chunk transmit produced %d APCs, want 2", len(parts))
	}
	assertChunk(t, parts[0], "a=t,f=100,i=3,q=2,U=1,m=1", 4096, true)
	assertChunk(t, parts[1], "m=0,q=2", 4, false)
	if decoded := decodeChunks(t, parts); !bytes.Equal(decoded, two) {
		t.Fatalf("decoded chunks = %d bytes, want %d", len(decoded), len(two))
	}

	// Three chunks: two full 4096-byte pieces and a short tail. The middle
	// piece carries m=1 and q=2 and no transmit keys.
	three := bytes.Repeat([]byte{0xCD}, 6147)
	parts = parseAPCs(t, TransmitPNG(9, three))
	if len(parts) != 3 {
		t.Fatalf("three-chunk transmit produced %d APCs, want 3", len(parts))
	}
	assertChunk(t, parts[0], "a=t,f=100,i=9,q=2,U=1,m=1", 4096, true)
	assertChunk(t, parts[1], "m=1,q=2", 4096, false)
	assertChunk(t, parts[2], "m=0,q=2", 4, false)
	if strings.Contains(parts[1].control, "a=") || strings.Contains(parts[1].control, "U=") {
		t.Fatalf("continuation chunk carries transmit keys: %q", parts[1].control)
	}
	for _, part := range parts {
		if !strings.Contains(part.control, "q=2") {
			t.Fatalf("chunk missing q=2: %q", part.control)
		}
		if len(part.payload)%4 != 0 {
			t.Fatalf("chunk payload length %d is not a multiple of 4", len(part.payload))
		}
	}
}

func TestTransmitDoesNotEmbedRawServerText(t *testing.T) {
	server := []byte("PRIVMSG #omarchy :\x1b_Ga=t,i=1;pwned")
	out := TransmitPNG(4, server)
	if strings.Contains(out, "PRIVMSG") || strings.Contains(out, "pwned") {
		t.Fatalf("server text was copied into a graphics escape: %q", out)
	}
	if strings.Contains(out, "\x1b_Ga=t,i=1;pwned") {
		t.Fatalf("server APC was copied verbatim: %q", out)
	}
	parts := parseAPCs(t, out)
	if decoded := decodeChunks(t, parts); !bytes.Equal(decoded, server) {
		t.Fatal("payload did not round-trip the bytes it was given")
	}
}

func TestPlaceholderEncoding(t *testing.T) {
	cell := func(id uint32, row, col int) string {
		text := Placeholder(id, col+1, row+1)
		if text == "" || strings.Contains(text, "\x1b_G") || strings.Contains(text, "\n") && row == 0 {
			t.Fatalf("Placeholder(%d) at %d,%d = %q", id, row, col, text)
		}
		// The requested cell is the last grapheme before the reset.
		body := strings.TrimSuffix(text, "\x1b[0m")
		lines := strings.Split(body, "\n")
		if row >= len(lines) {
			t.Fatalf("row %d missing from %q", row, text)
		}
		return lines[row]
	}

	// id 42 is the protocol's example. Truecolor stores it as RGB 0,0,42.
	// U+0305 is position 0 and U+030D is position 1.
	top := cell(42, 0, 1)
	if !strings.HasPrefix(top, "\x1b[38;2;0;0;42m") {
		t.Fatalf("id 42 color = %q, want 38;2;0;0;42", top)
	}
	if !strings.Contains(top, string([]rune{placeholderRune, 0x0305, 0x0305})) {
		t.Fatalf("cell 0,0 missing: %q", top)
	}
	if !strings.Contains(top, string([]rune{placeholderRune, 0x0305, 0x030D})) {
		t.Fatalf("cell 0,1 missing: %q", top)
	}

	bottom := cell(42, 1, 1)
	lines := strings.Split(strings.TrimSuffix(Placeholder(42, 2, 2), "\x1b[0m"), "\n")
	if len(lines) != 2 {
		t.Fatalf("2x2 placeholder lines = %d, want 2", len(lines))
	}
	if !strings.Contains(lines[1], string([]rune{placeholderRune, 0x030D, 0x0305})) {
		t.Fatalf("cell 1,0 missing: %q", lines[1])
	}
	if !strings.Contains(lines[1], string([]rune{placeholderRune, 0x030D, 0x030D})) {
		t.Fatalf("cell 1,1 missing: %q", lines[1])
	}
	_ = bottom

	wide := Placeholder(0x010203, 1, 1)
	if !strings.Contains(wide, "\x1b[38;2;1;2;3m") {
		t.Fatalf("id 0x010203 color = %q", wide)
	}
	if !strings.Contains(wide, string([]rune{placeholderRune, 0x0305, 0x0305})) {
		t.Fatalf("single cell diacritics = %q", wide)
	}
	// Index 2 is U+030E, the protocol's third sample diacritic.
	third := Placeholder(1, 3, 1)
	if !strings.Contains(third, string([]rune{placeholderRune, 0x0305, 0x030E})) {
		t.Fatalf("column 2 diacritic missing: %q", third)
	}

	if Placeholder(0, 2, 1) != "" || Placeholder(1, 0, 1) != "" || VirtualPlacement(0, 2, 1) != "" {
		t.Fatal("zero id or zero cols produced an escape or a cell")
	}
	if got, want := VirtualPlacement(5, 2, 1), "\x1b_Ga=p,U=1,i=5,c=2,r=1,q=2\x1b\\"; got != want {
		t.Fatalf("VirtualPlacement = %q, want %q", got, want)
	}
	if got, want := DeleteImage(5), "\x1b_Ga=d,d=I,i=5,q=2\x1b\\"; got != want {
		t.Fatalf("DeleteImage = %q, want %q", got, want)
	}
	if !strings.Contains(QueryEscape(), "a=q") || strings.Contains(QueryEscape(), "OK") {
		t.Fatalf("query = %q", QueryEscape())
	}
}

func TestReadGraphicsSupportReplyAndTimeout(t *testing.T) {
	t.Setenv("TERM", "xterm-kitty")
	t.Setenv("COLORTERM", "truecolor")

	okTerm := &scriptTerminal{reply: []byte("\x1b_Gi=4294967295;OK\x1b\\")}
	if !ReadGraphicsSupport(okTerm, okTerm, 50*time.Millisecond) {
		t.Fatal("OK reply did not enable graphics")
	}
	if !strings.Contains(okTerm.written.String(), "a=q") {
		t.Fatalf("probe = %q, want a=q", okTerm.written.String())
	}

	// A Kitty-looking TERM with no reply stays on the fallback.
	silent := &scriptTerminal{}
	started := time.Now()
	if ReadGraphicsSupport(silent, silent, 40*time.Millisecond) {
		t.Fatal("timeout enabled graphics")
	}
	if time.Since(started) > 500*time.Millisecond {
		t.Fatalf("timeout waited %s", time.Since(started))
	}

	negative := &scriptTerminal{reply: []byte("\x1b_Gi=1;EINVAL: no\x1b\\")}
	if ReadGraphicsSupport(negative, negative, 50*time.Millisecond) {
		t.Fatal("EINVAL reply enabled graphics")
	}
	if replyOK([]byte("\x1b_Gi=1;NOTOK\x1b\\")) {
		t.Fatal("NOTOK counted as OK")
	}
}

func TestProbeGraphicsNonTerminal(t *testing.T) {
	if term.IsTerminal(os.Stdin.Fd()) {
		t.Skip("stdin is a terminal")
	}
	if ProbeGraphics(os.Stdin, os.Stdout, time.Second) {
		t.Fatal("non-terminal probe returned true")
	}
	if ProbeGraphics(nil, nil, time.Second) {
		t.Fatal("nil probe returned true")
	}
}

func TestGraphicsCacheTransmitDeleteAndPlaceholder(t *testing.T) {
	const raw = "https://cdn.example/mira.png"
	resolved, ok := irc.ResolvedAvatarURL(raw, irc.AvatarFetchPixelSize(22))
	if !ok || !irc.AvatarURLIsSafe(resolved) {
		t.Fatalf("fixture URL is not safe: %v %v", resolved, ok)
	}
	key := cacheKey(resolved)

	store := New(true, nil)
	defer store.Close()
	img := image.NewRGBA(image.Rect(0, 0, 2, 2))
	img.Set(0, 0, color.RGBA{R: 255, A: 255})
	store.remember(key, img)
	if _, ok := store.Placeholder(raw, 22, 2); ok {
		t.Fatal("placeholder before graphics were enabled")
	}
	if store.DrainEscapes(2, 1) != "" {
		t.Fatal("graphics disabled still produced escapes")
	}

	store.EnableGraphics()
	escapes := store.DrainEscapes(2, 1)
	if !strings.Contains(escapes, "a=t,f=100,i=1,q=2,U=1,m=0") {
		t.Fatalf("transmit missing: %q", escapes)
	}
	if !strings.Contains(escapes, "a=p,U=1,i=1,c=2,r=1,q=2") {
		t.Fatalf("placement missing: %q", escapes)
	}
	if strings.Contains(escapes, raw) {
		t.Fatal("avatar URL was copied into an escape")
	}
	if _, ok := store.Placeholder(raw, 22, 2); ok {
		t.Fatal("placeholder before the transmit was written")
	}
	store.NoteDrawn()
	cells, ok := store.Placeholder(raw, 22, 2)
	if !ok || !strings.Contains(cells, string(placeholderRune)) || strings.Contains(cells, "\x1b_G") {
		t.Fatalf("placeholder after NoteDrawn = %q %v", cells, ok)
	}
	// Half-block still renders for a terminal that is not using the placeholder.
	if block, ok := store.Block(raw, 22, 2); !ok || !strings.Contains(block, "▀") {
		t.Fatalf("half-block fallback = %q %v", block, ok)
	}

	deleted := store.DeleteEscapes()
	if deleted != DeleteImage(1) {
		t.Fatalf("DeleteEscapes = %q, want %q", deleted, DeleteImage(1))
	}
	if _, ok := store.Placeholder(raw, 22, 2); ok {
		t.Fatal("placeholder survived delete")
	}
	if again := store.DeleteEscapes(); again != "" {
		t.Fatalf("second DeleteEscapes = %q, want empty", again)
	}
}

func TestGraphicsEvictionDeletesTheOldestImage(t *testing.T) {
	store := New(true, nil)
	defer store.Close()
	store.EnableGraphics()
	img := image.NewRGBA(image.Rect(0, 0, 1, 1))
	img.Set(0, 0, color.RGBA{G: 255, A: 255})
	for i := 0; i < maximumCacheEntries; i++ {
		store.remember(string(rune('a'+i)), img)
	}
	if store.DrainEscapes(2, 1) == "" {
		t.Fatal("filling the cache produced no transmits")
	}
	store.NoteDrawn()
	// One more image evicts the oldest id (1) and transmits a new one.
	store.remember("overflow", img)
	escapes := store.DrainEscapes(2, 1)
	if !strings.Contains(escapes, DeleteImage(1)) {
		t.Fatalf("eviction did not delete id 1: %q", escapes)
	}
	if !strings.Contains(escapes, "a=t") {
		t.Fatalf("eviction did not transmit the new image: %q", escapes)
	}
}

// graphicsMaxPNGBytes is the ceiling for one avatar transmit. A 32 px RGBA
// PNG is a few kilobytes; a 1024 px encode of the same pixels is not.
const graphicsMaxPNGBytes = 16 * 1024

func TestGraphicsTransmitCapsLargeAvatars(t *testing.T) {
	cases := []struct {
		name       string
		width      int
		height     int
		wantWidth  int
		wantHeight int
	}{
		{name: "decoded budget", width: 1024, height: 1024, wantWidth: graphicsMaxEdge, wantHeight: graphicsMaxEdge},
		{name: "wide", width: 200, height: 10, wantWidth: graphicsMaxEdge, wantHeight: 1},
		{name: "already small", width: 2, height: 2, wantWidth: 2, wantHeight: 2},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			pngBytes := transmittedPNG(t, noisyNRGBA(tc.width, tc.height))
			if len(pngBytes) == 0 || len(pngBytes) > graphicsMaxPNGBytes {
				t.Fatalf("transmitted PNG is %d bytes, want 1..%d", len(pngBytes), graphicsMaxPNGBytes)
			}
			cfg, _, err := image.DecodeConfig(bytes.NewReader(pngBytes))
			if err != nil {
				t.Fatalf("decode transmitted png: %v", err)
			}
			if cfg.Width != tc.wantWidth || cfg.Height != tc.wantHeight {
				t.Fatalf("transmitted size = %dx%d, want %dx%d", cfg.Width, cfg.Height, tc.wantWidth, tc.wantHeight)
			}
		})
	}
}

func TestGraphicsEncodeStaysOutsideTheStoreLock(t *testing.T) {
	const raw = "https://cdn.example/mira.png"
	cases := []struct {
		name string
		run  func(store *Store, img image.Image)
	}{
		{
			name: "remember",
			run: func(store *Store, img image.Image) {
				store.EnableGraphics()
				store.remember("k", img)
			},
		},
		{
			name: "enable",
			run: func(store *Store, img image.Image) {
				store.remember("k", img)
				store.EnableGraphics()
			},
		},
	}
	if _, ok := irc.ResolvedAvatarURL(raw, irc.AvatarFetchPixelSize(22)); !ok {
		t.Fatal("fixture URL is not safe")
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			store := New(true, nil)
			t.Cleanup(store.Close)
			assertEncodeOutsideLock(t, store, func(img image.Image) {
				tc.run(store, img)
			})
		})
	}
}

func noisyNRGBA(width, height int) *image.NRGBA {
	img := image.NewNRGBA(image.Rect(0, 0, width, height))
	for i := 0; i < len(img.Pix); i += 4 {
		img.Pix[i] = uint8(i)
		img.Pix[i+1] = uint8(i >> 8)
		img.Pix[i+2] = uint8(i >> 16)
		img.Pix[i+3] = 255
	}
	return img
}

func transmittedPNG(t *testing.T, img image.Image) []byte {
	t.Helper()
	store := New(true, nil)
	t.Cleanup(store.Close)
	store.EnableGraphics()
	store.remember("big", img)
	parts := parseAPCs(t, store.DrainEscapes(2, 1))
	var encoded strings.Builder
	for _, part := range parts {
		if strings.Contains(part.control, "a=t") || strings.HasPrefix(part.control, "m=") {
			encoded.WriteString(part.payload)
		}
	}
	decoded, err := base64.StdEncoding.DecodeString(encoded.String())
	if err != nil {
		t.Fatalf("decode transmit payload: %v", err)
	}
	return decoded
}

// lockProbeImage reports whether At runs while the store lock is held. PNG
// encoding reads pixels through At, so a stall here is a stall inside the
// encode.
type lockProbeImage struct {
	image.Image
	store   *Store
	tb      testing.TB
	once    sync.Once
	started chan struct{}
	release <-chan struct{}
}

func (p *lockProbeImage) At(x, y int) color.Color {
	if !p.store.mu.TryLock() {
		p.tb.Errorf("graphics PNG encode ran while the store lock was held")
	} else {
		p.store.mu.Unlock()
	}
	p.once.Do(func() { close(p.started) })
	<-p.release
	return p.Image.At(x, y)
}

func assertEncodeOutsideLock(t *testing.T, store *Store, encode func(image.Image)) {
	t.Helper()
	started := make(chan struct{})
	releaseCh := make(chan struct{})
	var releaseOnce sync.Once
	release := func() { releaseOnce.Do(func() { close(releaseCh) }) }
	probe := &lockProbeImage{
		Image:   noisyNRGBA(8, 8),
		store:   store,
		tb:      t,
		started: started,
		release: releaseCh,
	}

	done := make(chan struct{})
	go func() {
		defer close(done)
		encode(probe)
	}()
	defer func() {
		release()
		<-done
	}()

	select {
	case <-started:
	case <-time.After(2 * time.Second):
		t.Fatal("graphics encode did not start")
	}

	const raw = "https://cdn.example/mira.png"
	rendered := make(chan struct{})
	go func() {
		defer close(rendered)
		store.lookup("k")
		store.Block(raw, 22, 2)
		store.Placeholder(raw, 22, 2)
	}()
	select {
	case <-rendered:
	case <-time.After(2 * time.Second):
		t.Fatal("Block, Placeholder, or lookup blocked during PNG encode")
	}
}

type apcPart struct {
	control string
	payload string
}

func parseAPCs(t *testing.T, s string) []apcPart {
	t.Helper()
	var parts []apcPart
	for {
		i := strings.Index(s, "\x1b_G")
		if i < 0 {
			if strings.TrimSpace(s) != "" {
				t.Fatalf("trailing bytes outside an APC: %q", s)
			}
			return parts
		}
		if i != 0 {
			t.Fatalf("bytes before an APC: %q", s[:i])
		}
		s = s[3:]
		end := strings.Index(s, "\x1b\\")
		if end < 0 {
			t.Fatalf("APC missing ST: %q", s)
		}
		body := s[:end]
		s = s[end+2:]
		control, payload, _ := strings.Cut(body, ";")
		parts = append(parts, apcPart{control: control, payload: payload})
	}
}

func assertChunk(t *testing.T, part apcPart, control string, payloadLen int, first bool) {
	t.Helper()
	if part.control != control {
		t.Fatalf("control = %q, want %q", part.control, control)
	}
	if len(part.payload) != payloadLen {
		t.Fatalf("payload length = %d, want %d", len(part.payload), payloadLen)
	}
	if len(part.payload) > graphicsChunkSize {
		t.Fatalf("payload %d exceeds %d", len(part.payload), graphicsChunkSize)
	}
	if first && !strings.Contains(part.control, "U=1") {
		t.Fatalf("first chunk missing U=1: %q", part.control)
	}
}

func decodeChunks(t *testing.T, parts []apcPart) []byte {
	t.Helper()
	var encoded strings.Builder
	for _, part := range parts {
		encoded.WriteString(part.payload)
	}
	decoded, err := base64.StdEncoding.DecodeString(encoded.String())
	if err != nil {
		t.Fatalf("decode payload: %v", err)
	}
	return decoded
}

// scriptTerminal is a fake terminal: Write records the probe, Read returns a
// scripted reply or blocks until the deadline.
type scriptTerminal struct {
	mu       sync.Mutex
	reply    []byte
	deadline time.Time
	written  bytes.Buffer
}

func (s *scriptTerminal) Write(p []byte) (int, error) {
	return s.written.Write(p)
}

func (s *scriptTerminal) SetReadDeadline(deadline time.Time) error {
	s.mu.Lock()
	s.deadline = deadline
	s.mu.Unlock()
	return nil
}

func (s *scriptTerminal) Read(p []byte) (int, error) {
	s.mu.Lock()
	if len(s.reply) > 0 {
		n := copy(p, s.reply)
		s.reply = s.reply[n:]
		s.mu.Unlock()
		return n, nil
	}
	deadline := s.deadline
	s.mu.Unlock()
	if deadline.IsZero() {
		return 0, io.EOF
	}
	if wait := time.Until(deadline); wait > 0 {
		time.Sleep(wait)
	}
	return 0, os.ErrDeadlineExceeded
}
