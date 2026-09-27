package avatar

import (
	"bytes"
	"context"
	"crypto/tls"
	"image"
	"io"
	"net"
	"net/http"
	"net/netip"
	"net/url"
	"strings"
	"time"

	// Register the decoders reachable through image.Decode. WebP is
	// intentionally unsupported: src/irc/ircavatarstore.cpp decodes through
	// QImageReader, while the terminal port ships only the stdlib formats and
	// fails closed on anything else.
	_ "image/gif"
	_ "image/jpeg"
	_ "image/png"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

const (
	// maximumAvatarBytes bounds the compressed response body, mirroring
	// kMaximumAvatarBytes. io.LimitReader reads one byte past it so an
	// over-cap body is detected rather than silently truncated.
	maximumAvatarBytes = 512 * 1024
	// fetchTimeout mirrors kFetchTimeoutMs.
	fetchTimeout = 10 * time.Second
	// maximumDecodedEdge and maximumDecodedPixels bound the decoded budget
	// independently of the render size: a tiny payload can still declare a
	// huge IHDR. They mirror kMaximumDecodedEdge / kMaximumDecodedPixels.
	maximumDecodedEdge   = 1024
	maximumDecodedPixels = 1024 * 1024

	avatarAccept    = "image/*"
	avatarUserAgent = "Omairc"
)

// download performs one pinned, redirect-refusing HTTPS fetch and returns a
// bounded, decoded image. Every failure path reports false.
func (s *Store) download(ctx context.Context, resolved *url.URL) (image.Image, bool) {
	address, ok := resolvePinnedAddress(ctx, resolved.Hostname())
	if !ok {
		return nil, false
	}
	pinned, ok := irc.AvatarURLPinnedToAddress(resolved, address)
	if !ok {
		return nil, false
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodGet, pinned.String(), nil)
	if err != nil {
		return nil, false
	}
	// The request connects to the pinned address, but the Host header and the
	// TLS name still name the real host so certificate verification is intact.
	req.Host = resolved.Host
	req.Header.Set("Accept", avatarAccept)
	req.Header.Set("User-Agent", avatarUserAgent)

	client := &http.Client{
		Timeout: fetchTimeout,
		// Redirects are not followed: a 3xx is a failure, so an allowed host
		// cannot bounce the fetch somewhere else. Fail closed.
		CheckRedirect: func(*http.Request, []*http.Request) error {
			return http.ErrUseLastResponse
		},
		Transport: &http.Transport{
			// Ignore an ambient proxy; the connection is already pinned.
			Proxy:             nil,
			DisableKeepAlives: true,
			TLSClientConfig: &tls.Config{
				ServerName: resolved.Hostname(),
				MinVersion: tls.VersionTLS12,
			},
		},
	}

	resp, err := client.Do(req)
	if err != nil {
		return nil, false
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, false
	}
	contentType := strings.ToLower(strings.TrimSpace(resp.Header.Get("Content-Type")))
	if !strings.HasPrefix(contentType, "image/") {
		return nil, false
	}
	body, err := io.ReadAll(io.LimitReader(resp.Body, int64(maximumAvatarBytes)+1))
	if err != nil || len(body) == 0 || len(body) > maximumAvatarBytes {
		return nil, false
	}
	return decodeBoundedImage(body)
}

// resolvePinnedAddress returns one safe address for host. An IP literal is used
// directly when it is safe; otherwise the name is resolved exactly once so the
// later connection cannot re-resolve it (DNS-rebinding TOCTOU), mirroring
// IrcAvatarStore::applyResolvedAddresses.
func resolvePinnedAddress(ctx context.Context, host string) (netip.Addr, bool) {
	if host == "" {
		return netip.Addr{}, false
	}
	if literal, err := netip.ParseAddr(host); err == nil {
		if irc.HostAddressIsUnsafe(literal) {
			return netip.Addr{}, false
		}
		return literal, true
	}
	addresses, err := net.DefaultResolver.LookupNetIP(ctx, "ip", host)
	if err != nil {
		return netip.Addr{}, false
	}
	address := irc.SelectSafeAvatarAddress(addresses)
	if !address.IsValid() {
		return netip.Addr{}, false
	}
	return address, true
}

// decodeBoundedImage decodes body within the decoded-size budget. It asks
// image.DecodeConfig for the declared bounds first so a payload that declares a
// huge image is rejected before it is decoded, then re-checks the budget on the
// decoded bounds, mirroring readBoundedImage.
func decodeBoundedImage(body []byte) (image.Image, bool) {
	config, _, err := image.DecodeConfig(bytes.NewReader(body))
	if err != nil || !decodedConfigWithinBudget(config) {
		return nil, false
	}
	img, _, err := image.Decode(bytes.NewReader(body))
	if err != nil {
		return nil, false
	}
	bounds := img.Bounds()
	if !decodedSizeWithinBudget(bounds.Dx(), bounds.Dy()) {
		return nil, false
	}
	return img, true
}

// decodedConfigWithinBudget applies the decoded-size budget to a decoded
// header. It is the image.DecodeConfig seam used before a full decode.
func decodedConfigWithinBudget(config image.Config) bool {
	return decodedSizeWithinBudget(config.Width, config.Height)
}

// decodedSizeWithinBudget mirrors decodedSizeWithinBudget in
// ircavatarstore.cpp: positive edges within the cap and a positive pixel count
// within the pixel budget.
func decodedSizeWithinBudget(width, height int) bool {
	if width <= 0 || height <= 0 {
		return false
	}
	if width > maximumDecodedEdge || height > maximumDecodedEdge {
		return false
	}
	pixels := int64(width) * int64(height)
	if pixels <= 0 || pixels > maximumDecodedPixels {
		return false
	}
	return true
}
