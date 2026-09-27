// Package avatar is the terminal peer-avatar seam. It fetches, decodes,
// caches, and half-block-rasterizes avatars for the TUI shell, mirroring the
// safety model of src/irc/ircavatarstore.cpp (bounded body, bounded decode,
// DNS pinning, fail closed, a small insertion-ordered cache).
//
// The package is presentation-neutral: Block returns a raw SGR string and the
// shell decides where to place it. It never starts a fetch from Block; Ensure
// is the only path that touches the network.
package avatar

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"image"
	"image/color"
	"net/url"
	"os"
	"strings"
	"sync"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// maximumCacheEntries bounds the decoded-image cache. It mirrors
// kMaximumCacheEntries in ircavatarstore.cpp.
const maximumCacheEntries = 64

// TruecolorSupported reports whether the environment advertises 24-bit color:
// COLORTERM contains "truecolor" or "24bit", or TERM contains "truecolor" or
// "direct", case-insensitive.
func TruecolorSupported() bool {
	colorterm := strings.ToLower(os.Getenv("COLORTERM"))
	if strings.Contains(colorterm, "truecolor") || strings.Contains(colorterm, "24bit") {
		return true
	}
	term := strings.ToLower(os.Getenv("TERM"))
	return strings.Contains(term, "truecolor") || strings.Contains(term, "direct")
}

// Store caches decoded avatars and schedules their fetches. It is safe for
// concurrent use. onReady, which may be nil, is called from a background
// goroutine with the original raw URL after a fetched image is cached; it must
// be safe to call concurrently and is never called after Close returns.
type Store struct {
	truecolor bool
	onReady   func(rawURL string)

	mu      sync.Mutex
	images  map[string]image.Image
	order   []string
	pending map[string]bool
	cancels map[string]context.CancelFunc
	closed  bool

	// callbacks counts onReady calls that are in flight. Close waits for it so
	// a late callback cannot outlive Close.
	callbacks sync.WaitGroup
}

// New returns a store. truecolor selects the half-block raster path; when it
// is false Block always reports false.
func New(truecolor bool, onReady func(rawURL string)) *Store {
	return &Store{
		truecolor: truecolor,
		onReady:   onReady,
		images:    make(map[string]image.Image),
		pending:   make(map[string]bool),
		cancels:   make(map[string]context.CancelFunc),
	}
}

// Ensure schedules a fetch for rawURL at layoutPixels if the resolved URL is
// safe and neither cached nor already in flight. It never blocks and never
// panics.
func (s *Store) Ensure(rawURL string, layoutPixels int) {
	trimmed := strings.TrimSpace(rawURL)
	fetchSize := irc.AvatarFetchPixelSize(layoutPixels)
	if fetchSize <= 0 {
		return
	}
	resolved, ok := irc.ResolvedAvatarURL(trimmed, fetchSize)
	if !ok || !irc.AvatarURLIsSafe(resolved) {
		return
	}
	key := cacheKey(resolved)

	s.mu.Lock()
	if s.closed {
		s.mu.Unlock()
		return
	}
	if _, cached := s.images[key]; cached {
		s.mu.Unlock()
		return
	}
	if s.pending[key] {
		s.mu.Unlock()
		return
	}
	ctx, cancel := context.WithCancel(context.Background())
	s.pending[key] = true
	s.cancels[key] = cancel
	s.mu.Unlock()

	go s.fetch(ctx, rawURL, resolved, key, cancel)
}

// Block renders the cached image for rawURL as exactly cols half-block cells
// (each cell is an upper-half fg plus a lower-half bg, using the "▀" rune), or
// reports false when truecolor is off, the URL is empty/unsafe/unresolved, cols
// is not positive, or the image is not cached. It is a pure lookup: it never
// starts a fetch.
func (s *Store) Block(rawURL string, layoutPixels, cols int) (string, bool) {
	if !s.truecolor || cols <= 0 {
		return "", false
	}
	fetchSize := irc.AvatarFetchPixelSize(layoutPixels)
	if fetchSize <= 0 {
		return "", false
	}
	resolved, ok := irc.ResolvedAvatarURL(strings.TrimSpace(rawURL), fetchSize)
	if !ok || !irc.AvatarURLIsSafe(resolved) {
		return "", false
	}
	img, cached := s.lookup(cacheKey(resolved))
	if !cached {
		return "", false
	}
	return renderHalfBlocks(img, cols), true
}

// Close stops in-flight fetches and prevents late callbacks. It is idempotent
// and waits for any in-flight onReady call to finish before returning, so
// onReady is never invoked after Close.
func (s *Store) Close() {
	s.mu.Lock()
	if s.closed {
		s.mu.Unlock()
		return
	}
	s.closed = true
	cancels := make([]context.CancelFunc, 0, len(s.cancels))
	for _, cancel := range s.cancels {
		cancels = append(cancels, cancel)
	}
	s.cancels = nil
	s.pending = nil
	s.mu.Unlock()

	for _, cancel := range cancels {
		cancel()
	}
	s.callbacks.Wait()
}

// fetch downloads and caches one avatar, then reports it through onReady. It
// always releases its pending slot.
func (s *Store) fetch(ctx context.Context, rawURL string, resolved *url.URL,
	key string, cancel context.CancelFunc) {
	defer cancel()

	img, ok := s.download(ctx, resolved)
	if !ok {
		s.forget(key)
		return
	}

	s.mu.Lock()
	if s.closed {
		s.mu.Unlock()
		return
	}
	s.rememberLocked(key, img)
	delete(s.pending, key)
	delete(s.cancels, key)
	s.callbacks.Add(1)
	onReady := s.onReady
	s.mu.Unlock()

	if onReady != nil {
		onReady(rawURL)
	}
	s.callbacks.Done()
}

// forget clears the pending slot for a failed fetch.
func (s *Store) forget(key string) {
	s.mu.Lock()
	delete(s.pending, key)
	delete(s.cancels, key)
	s.mu.Unlock()
}

// cacheKey is the hex SHA-256 of the resolved URL string. It mirrors
// IrcAvatarStore::keyForUrl.
func cacheKey(resolved *url.URL) string {
	sum := sha256.Sum256([]byte(resolved.String()))
	return hex.EncodeToString(sum[:])
}

// remember inserts img under key and moves key to the newest position,
// evicting the oldest entries past the cap. It is the in-package test seam.
func (s *Store) remember(key string, img image.Image) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.rememberLocked(key, img)
}

func (s *Store) rememberLocked(key string, img image.Image) {
	s.order = removeKey(s.order, key)
	s.order = append(s.order, key)
	s.images[key] = img
	for len(s.order) > maximumCacheEntries {
		oldest := s.order[0]
		s.order = s.order[1:]
		delete(s.images, oldest)
	}
}

// removeKey returns keys without the first occurrence of key.
func removeKey(keys []string, key string) []string {
	for i, existing := range keys {
		if existing == key {
			return append(keys[:i], keys[i+1:]...)
		}
	}
	return keys
}

// lookup returns the cached image for key, or false when it is absent.
func (s *Store) lookup(key string) (image.Image, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	img, ok := s.images[key]
	return img, ok
}

// renderHalfBlocks samples img into cols columns and two rows (top half and
// bottom half), averaging each source region over black, then emits one
// "\x1b[38;2;R;G;Bm\x1b[48;2;R;G;Bm▀" per column and a single trailing reset.
func renderHalfBlocks(img image.Image, cols int) string {
	if img == nil || cols <= 0 {
		return ""
	}
	bounds := img.Bounds()
	width := bounds.Dx()
	height := bounds.Dy()
	if width <= 0 || height <= 0 {
		return ""
	}
	middle := bounds.Min.Y + height/2
	if middle <= bounds.Min.Y {
		middle = bounds.Min.Y + 1
	}
	if middle > bounds.Max.Y {
		middle = bounds.Max.Y
	}

	var builder strings.Builder
	for x := 0; x < cols; x++ {
		minX := bounds.Min.X + x*width/cols
		maxX := bounds.Min.X + (x+1)*width/cols
		if maxX <= minX {
			maxX = minX + 1
		}
		if minX >= bounds.Max.X {
			minX = bounds.Max.X - 1
		}
		if maxX > bounds.Max.X {
			maxX = bounds.Max.X
		}
		top := averageRegion(img, minX, maxX, bounds.Min.Y, middle)
		bottom := averageRegion(img, minX, maxX, middle, bounds.Max.Y)
		fmt.Fprintf(&builder,
			"\x1b[38;2;%d;%d;%dm\x1b[48;2;%d;%d;%dm▀",
			top.r, top.g, top.b, bottom.r, bottom.g, bottom.b)
	}
	builder.WriteString("\x1b[0m")
	return builder.String()
}

// rgb is a color composited over black, ready for a truecolor SGR sequence.
type rgb struct {
	r, g, b int
}

// averageRegion averages the pixels in [minX,maxX) x [minY,maxY), compositing
// each pixel's alpha over black. An empty region falls back to one sample.
func averageRegion(img image.Image, minX, maxX, minY, maxY int) rgb {
	var totalR, totalG, totalB, count int
	for y := minY; y < maxY; y++ {
		for x := minX; x < maxX; x++ {
			c := color.NRGBAModel.Convert(img.At(x, y)).(color.NRGBA)
			alpha := int(c.A)
			totalR += int(c.R) * alpha / 255
			totalG += int(c.G) * alpha / 255
			totalB += int(c.B) * alpha / 255
			count++
		}
	}
	if count == 0 {
		c := color.NRGBAModel.Convert(img.At(minX, minY)).(color.NRGBA)
		alpha := int(c.A)
		return rgb{
			r: int(c.R) * alpha / 255,
			g: int(c.G) * alpha / 255,
			b: int(c.B) * alpha / 255,
		}
	}
	return rgb{r: totalR / count, g: totalG / count, b: totalB / count}
}
