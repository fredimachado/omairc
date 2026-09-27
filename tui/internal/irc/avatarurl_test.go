package irc

import (
	"crypto/sha256"
	"encoding/hex"
	"net/netip"
	"net/url"
	"testing"
)

// resolvedAvatarIsSafe mirrors the Qt tests that compose
// ircResolvedAvatarUrl and ircAvatarUrlIsSafe in one call.
func resolvedAvatarIsSafe(raw string) bool {
	resolved, ok := ResolvedAvatarURL(raw, 32)
	return ok && AvatarURLIsSafe(resolved)
}

func TestAvatarSubstitutesSizePlaceholder(t *testing.T) {
	sized, ok := ResolvedAvatarURL("https://example.com/avatars/{size}/mira.png", 34)
	requireTrue(t, "size 34 resolved", ok)
	requireString(t, "size 34 substituted", sized.String(),
		"https://example.com/avatars/34/mira.png")
	requireTrue(t, "size 34 safe", AvatarURLIsSafe(sized))

	floor, ok := ResolvedAvatarURL("https://example.com/avatars/{size}/mira.png", 8)
	requireTrue(t, "size 8 resolved", ok)
	requireString(t, "size 8 floored", floor.String(),
		"https://example.com/avatars/16/mira.png")
	requireTrue(t, "size 8 safe", AvatarURLIsSafe(floor))
}

func TestAvatarAcceptsHTTPS(t *testing.T) {
	requireTrue(t, "https", resolvedAvatarIsSafe("https://cdn.example.com/a.png"))
	requireTrue(t, "uppercase scheme", resolvedAvatarIsSafe("HTTPS://cdn.example.com/a.png"))
	requireTrue(t, "explicit 443", resolvedAvatarIsSafe("https://example.com:443/a.png"))
}

func TestAvatarRejectsHTTP(t *testing.T) {
	requireFalse(t, "http", resolvedAvatarIsSafe("http://example.com/a.png"))
}

func TestAvatarRejectsLocalhost(t *testing.T) {
	requireFalse(t, "localhost", resolvedAvatarIsSafe("https://localhost/a.png"))
	requireFalse(t, "LOCALHOST", resolvedAvatarIsSafe("https://LOCALHOST/a.png"))
	requireFalse(t, "trailing dot localhost",
		resolvedAvatarIsSafe("https://localhost./a.png"))
	requireTrue(t, "trailing dot example.com is safe",
		resolvedAvatarIsSafe("https://example.com./a.png"))
}

func TestAvatarRejectsLoopback(t *testing.T) {
	requireFalse(t, "ipv4 loopback", resolvedAvatarIsSafe("https://127.0.0.1/a.png"))
	requireFalse(t, "ipv6 loopback", resolvedAvatarIsSafe("https://[::1]/a.png"))
	requireFalse(t, "ipv4-mapped loopback url",
		resolvedAvatarIsSafe("https://[::ffff:127.0.0.1]/a.png"))
	requireTrue(t, "ipv4-mapped loopback unsafe",
		HostAddressIsUnsafe(netip.MustParseAddr("::ffff:127.0.0.1")))
}

func TestAvatarRejectsPrivate(t *testing.T) {
	requireFalse(t, "10.x", resolvedAvatarIsSafe("https://10.0.0.4/a.png"))
	requireFalse(t, "192.168.x", resolvedAvatarIsSafe("https://192.168.1.10/a.png"))
	requireFalse(t, "carrier-grade nat", resolvedAvatarIsSafe("https://100.64.0.1/a.png"))
	requireTrue(t, "100.64 unsafe",
		HostAddressIsUnsafe(netip.MustParseAddr("100.64.0.1")))
}

func TestAvatarRejectsZeroNetwork(t *testing.T) {
	requireFalse(t, "0.0.0.0", resolvedAvatarIsSafe("https://0.0.0.0/a.png"))
	requireFalse(t, "0.1.2.3", resolvedAvatarIsSafe("https://0.1.2.3/a.png"))
	requireTrue(t, "0.0.0.0 unsafe",
		HostAddressIsUnsafe(netip.MustParseAddr("0.0.0.0")))
	requireTrue(t, "0.1.2.3 unsafe",
		HostAddressIsUnsafe(netip.MustParseAddr("0.1.2.3")))
}

func TestAvatarRejectsLinkLocal(t *testing.T) {
	requireFalse(t, "link-local url", resolvedAvatarIsSafe("https://169.254.12.34/a.png"))
	requireTrue(t, "link-local unsafe",
		HostAddressIsUnsafe(netip.MustParseAddr("169.254.12.34")))
}

func TestAvatarRejectsNonHTTPSchemes(t *testing.T) {
	requireFalse(t, "file", resolvedAvatarIsSafe("file:///tmp/a.png"))
	requireFalse(t, "data", resolvedAvatarIsSafe("data:image/png;base64,aaaa"))
}

func TestAvatarRejectsEmpty(t *testing.T) {
	if _, ok := ResolvedAvatarURL("", 32); ok {
		t.Fatal("empty input must not resolve")
	}
	requireFalse(t, "nil url", AvatarURLIsSafe(nil))
	requireFalse(t, "zero url", AvatarURLIsSafe(&url.URL{}))
	requireFalse(t, "whitespace", resolvedAvatarIsSafe("   "))
}

func TestAvatarRejectsUserinfoAndLocalSuffix(t *testing.T) {
	requireFalse(t, "user:pass@", resolvedAvatarIsSafe("https://user:pass@example.com/a.png"))
	requireFalse(t, "empty userinfo", resolvedAvatarIsSafe("https://@example.com/a.png"))
	requireFalse(t, ".local suffix", resolvedAvatarIsSafe("https://printer.local/a.png"))
}

func TestAvatarRejectsNonStandardPort(t *testing.T) {
	requireFalse(t, "8443", resolvedAvatarIsSafe("https://example.com:8443/a.png"))
	requireTrue(t, "no port", resolvedAvatarIsSafe("https://example.com/a.png"))
}

func TestAvatarQuantizesFetchPixelSize(t *testing.T) {
	cases := []struct {
		layout int
		want   int
	}{
		{0, 0}, {-1, 0}, {22, 32}, {32, 32}, {34, 32}, {48, 32}, {49, 64}, {64, 64},
	}
	for _, testCase := range cases {
		requireInt(t, "fetch pixel size", AvatarFetchPixelSize(testCase.layout), testCase.want)
	}
}

func TestAvatarSelectsSafeAddressAndPinsURL(t *testing.T) {
	if address := SelectSafeAvatarAddress(nil); address.IsValid() {
		t.Fatalf("empty list must select the zero address, got %v", address)
	}
	if address := SelectSafeAvatarAddress([]netip.Addr{netip.MustParseAddr("127.0.0.1")}); address.IsValid() {
		t.Fatalf("only-unsafe list must select the zero address, got %v", address)
	}
	selected := SelectSafeAvatarAddress([]netip.Addr{
		netip.MustParseAddr("10.0.0.1"),
		netip.MustParseAddr("93.184.216.34"),
	})
	requireString(t, "first safe address", selected.String(), "93.184.216.34")

	hostURL, ok := ResolvedAvatarURL("https://cdn.example/a.png", 32)
	requireTrue(t, "host url resolved", ok)
	pinned, ok := AvatarURLPinnedToAddress(hostURL, netip.MustParseAddr("93.184.216.34"))
	requireTrue(t, "pin ok", ok)
	requireString(t, "pinned url", pinned.String(), "https://93.184.216.34/a.png")
	requireTrue(t, "pinned url safe", AvatarURLIsSafe(pinned))

	if _, ok := AvatarURLPinnedToAddress(hostURL, netip.Addr{}); ok {
		t.Fatal("invalid address must not pin")
	}
	if _, ok := AvatarURLPinnedToAddress(nil, netip.MustParseAddr("93.184.216.34")); ok {
		t.Fatal("nil url must not pin")
	}
}

func TestAvatarMetadataAcceptsHTTPSURL(t *testing.T) {
	const raw = "https://cdn.example.com/a.png"
	requireString(t, "round trip", AvatarMetadataValue(raw), raw)
	requireString(t, "trimmed", AvatarMetadataValue("  "+raw+"  "), raw)
}

func TestAvatarMetadataBuildsGravatar(t *testing.T) {
	sum := sha256.Sum256([]byte("me@example.com"))
	expected := "https://www.gravatar.com/avatar/" + hex.EncodeToString(sum[:]) + "?s={size}&d=404"
	requireString(t, "email", AvatarMetadataValue("Me@Example.COM"), expected)
	requireString(t, "mailto", AvatarMetadataValue("mailto:Me@Example.COM"), expected)
}

func TestAvatarMetadataRejectsUnsafeInput(t *testing.T) {
	inputs := []string{
		"",
		"   ",
		"http://example.com/a.png",
		"https://localhost/a.png",
		"not-an-email",
		"missing-domain@host",
	}
	for _, input := range inputs {
		requireString(t, "reject "+input, AvatarMetadataValue(input), "")
	}
}
