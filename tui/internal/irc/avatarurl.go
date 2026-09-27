package irc

import (
	"crypto/sha256"
	"encoding/hex"
	"net/netip"
	"net/url"
	"strconv"
	"strings"
)

// This file ports the avatar URL policy from src/irc/ircavatarurl.cpp.
// /avatar accepts an HTTPS URL or an email address; an email becomes a Gravatar
// SHA-256 URL, and only safe HTTPS URLs are stored. The Qt code resolves and
// pins a global-unicast address so QNAM cannot re-resolve the host; the fetch
// itself is deferred with the avatar cache, so only the metadata value, the
// URL/address predicates, and the pin helper are ported here.
//
// net/url and net/netip are the URL and IP-literal parsers behind QUrl and
// QHostAddress; bin/check-conventions exempts exactly this file, so it must not
// import plain net (or anything else platform-bound).

const maximumAvatarURLLength = 2048

// strippedHost trims the trailing dots QUrl treats as an empty label so
// "localhost." matches the local-name check. It mirrors strippedHost.
func strippedHost(host string) string {
	return strings.TrimRight(host, ".")
}

// hostLooksLocal reports whether host is a name that must never be fetched.
// It mirrors hostLooksLocal.
func hostLooksLocal(host string) bool {
	lower := strings.ToLower(host)
	return lower == "localhost" ||
		strings.HasSuffix(lower, ".local") ||
		strings.HasSuffix(lower, ".localhost")
}

// looksLikeAvatarEmail reports whether input is an email address, with an
// optional mailto: prefix. It mirrors looksLikeAvatarEmail.
func looksLikeAvatarEmail(input string) bool {
	trimmed := trimSpace(input)
	if len(trimmed) >= 7 && strings.EqualFold(trimmed[:7], "mailto:") {
		trimmed = trimSpace(trimmed[7:])
	}
	if strings.Contains(trimmed, "://") {
		return false
	}
	at := strings.IndexByte(trimmed, '@')
	if at <= 0 || at >= len(trimmed)-1 {
		return false
	}
	local := trimmed[:at]
	domain := trimmed[at+1:]
	if local == "" || domain == "" {
		return false
	}
	if strings.Contains(domain, "@") {
		return false
	}
	return strings.Contains(domain, ".")
}

// gravatarMetadataURL returns the Gravatar URL for an email, mirroring
// gravatarMetadataUrl. The address is lowercased and hashed with SHA-256.
func gravatarMetadataURL(email string) string {
	normalized := trimSpace(email)
	if len(normalized) >= 7 && strings.EqualFold(normalized[:7], "mailto:") {
		normalized = trimSpace(normalized[7:])
	}
	normalized = strings.ToLower(normalized)
	sum := sha256.Sum256([]byte(normalized))
	return "https://www.gravatar.com/avatar/" + hex.EncodeToString(sum[:]) + "?s={size}&d=404"
}

// AvatarMetadataValue resolves a /avatar argument into the metadata value to
// store, or "" when it is neither an email nor a safe HTTPS URL. It mirrors
// ircAvatarMetadataValue.
func AvatarMetadataValue(input string) string {
	trimmed := trimSpace(input)
	if trimmed == "" {
		return ""
	}
	if looksLikeAvatarEmail(trimmed) {
		return gravatarMetadataURL(trimmed)
	}
	resolved, ok := ResolvedAvatarURL(trimmed, 32)
	if ok && AvatarURLIsSafe(resolved) {
		return trimmed
	}
	return ""
}

// ResolvedAvatarURL substitutes the {size} placeholder with max(16, pixelSize)
// and parses raw as an absolute URL with a non-empty scheme and host, or
// reports false. It mirrors ircResolvedAvatarUrl.
func ResolvedAvatarURL(raw string, pixelSize int) (*url.URL, bool) {
	trimmed := trimSpace(raw)
	if trimmed == "" {
		return nil, false
	}
	size := pixelSize
	if size < 16 {
		size = 16
	}
	substituted := strings.ReplaceAll(trimmed, "{size}", strconv.Itoa(size))
	if len(substituted) > maximumAvatarURLLength {
		return nil, false
	}
	parsed, err := url.Parse(substituted)
	if err != nil || parsed.Scheme == "" || parsed.Host == "" {
		return nil, false
	}
	return parsed, true
}

// AvatarFetchPixelSize quantizes a layout size to the fetch size: 0 disables
// the fetch, otherwise 32 when the layout size is at least as close to 32 as to
// 64 (ties go 32), else 64. It mirrors ircAvatarFetchPixelSize.
func AvatarFetchPixelSize(layoutPixels int) int {
	if layoutPixels <= 0 {
		return 0
	}
	distance32 := layoutPixels - 32
	if distance32 < 0 {
		distance32 = -distance32
	}
	distance64 := layoutPixels - 64
	if distance64 < 0 {
		distance64 = -distance64
	}
	if distance32 <= distance64 {
		return 32
	}
	return 64
}

// AvatarURLIsSafe reports whether url is an HTTPS URL that may be fetched: no
// credentials, a non-local host, port 443 or unset, and a global-unicast
// address when the host is an IP literal. It mirrors ircAvatarUrlIsSafe.
func AvatarURLIsSafe(u *url.URL) bool {
	if u == nil || u.Scheme == "" || u.Host == "" {
		return false
	}
	if !strings.EqualFold(u.Scheme, "https") {
		return false
	}
	rendered := u.String()
	if rendered == "" || len(rendered) > maximumAvatarURLLength {
		return false
	}
	// The Qt code rejects a non-empty userInfo/userName/password; any user
	// component at all is refused here, which is strictly more conservative.
	if u.User != nil {
		return false
	}
	host := strippedHost(u.Hostname())
	if host == "" || hostLooksLocal(host) {
		return false
	}
	if port := u.Port(); port != "" && port != "443" {
		return false
	}
	if address, err := netip.ParseAddr(host); err == nil {
		return !HostAddressIsUnsafe(address)
	}
	return true
}

// HostAddressIsUnsafe reports whether an IP literal is non-global or in a
// range Qt's QHostAddress treats as unsafe to connect to. It mirrors
// ircHostAddressIsUnsafe. The address is unmapped first so an IPv4-mapped IPv6
// literal is judged by its IPv4 rules.
func HostAddressIsUnsafe(address netip.Addr) bool {
	address = address.Unmap()
	if !address.IsValid() ||
		address.IsUnspecified() ||
		address.IsLoopback() ||
		address.IsLinkLocalUnicast() ||
		address.IsLinkLocalMulticast() ||
		address.IsMulticast() ||
		address.IsPrivate() ||
		!address.IsGlobalUnicast() {
		return true
	}
	if address.Is4() {
		bytes := address.As4()
		// 100.64.0.0/10 carrier-grade NAT.
		if bytes[0] == 100 && bytes[1] >= 64 && bytes[1] <= 127 {
			return true
		}
		// 0.0.0.0/8.
		if bytes[0] == 0 {
			return true
		}
	}
	return false
}

// SelectSafeAvatarAddress returns the first global-unicast address, or the
// zero Addr when none are safe. It mirrors ircSelectSafeAvatarAddress.
func SelectSafeAvatarAddress(addresses []netip.Addr) netip.Addr {
	for _, address := range addresses {
		if !HostAddressIsUnsafe(address) {
			return address
		}
	}
	return netip.Addr{}
}

// AvatarURLPinnedToAddress rewrites url's host to the address literal so the
// fetch connects to that IP without a second DNS lookup, preserving the port.
// It mirrors ircAvatarUrlPinnedToAddress and returns (nil, false) when the URL
// or the address is invalid.
func AvatarURLPinnedToAddress(u *url.URL, address netip.Addr) (*url.URL, bool) {
	if u == nil || !address.IsValid() {
		return nil, false
	}
	literal := address.String()
	// Bracket IPv6 (including v4-mapped) literals so String() stays a valid
	// absolute URL.
	if strings.Contains(literal, ":") {
		literal = "[" + literal + "]"
	}
	if port := u.Port(); port != "" {
		literal += ":" + port
	}
	pinned := *u
	pinned.Host = literal
	if pinned.Scheme == "" || pinned.Host == "" {
		return nil, false
	}
	return &pinned, true
}
