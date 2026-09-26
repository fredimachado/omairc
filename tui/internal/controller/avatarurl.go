package controller

import (
	"crypto/sha256"
	"encoding/hex"
	"net"
	"net/url"
	"strconv"
	"strings"
)

// This file ports the avatar metadata policy from src/irc/ircavatarurl.cpp.
// /avatar accepts an HTTPS URL or an email address; an email becomes a Gravatar
// SHA-256 URL, and only safe HTTPS URLs are stored. The Qt code pins a resolved
// address so QNAM cannot re-resolve the host; the fetch itself is deferred with
// the avatar cache, so only the metadata value and the safety predicate are
// ported here.

const maximumAvatarURLLength = 2048

func strippedHost(host string) string {
	return strings.TrimRight(host, ".")
}

func hostLooksLocal(host string) bool {
	lower := strings.ToLower(host)
	return lower == "localhost" ||
		strings.HasSuffix(lower, ".local") ||
		strings.HasSuffix(lower, ".localhost")
}

// looksLikeAvatarEmail reports whether input is an email address, with an
// optional mailto: prefix. It mirrors looksLikeAvatarEmail.
func looksLikeAvatarEmail(input string) bool {
	trimmed := strings.TrimSpace(input)
	if len(trimmed) >= 7 && strings.EqualFold(trimmed[:7], "mailto:") {
		trimmed = strings.TrimSpace(trimmed[7:])
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
// gravatarMetadataUrl.
func gravatarMetadataURL(email string) string {
	normalized := strings.TrimSpace(email)
	if len(normalized) >= 7 && strings.EqualFold(normalized[:7], "mailto:") {
		normalized = strings.TrimSpace(normalized[7:])
	}
	normalized = strings.ToLower(normalized)
	sum := sha256.Sum256([]byte(normalized))
	return "https://www.gravatar.com/avatar/" + hex.EncodeToString(sum[:]) + "?s={size}&d=404"
}

// avatarMetadataValue resolves a /avatar argument into the metadata value to
// store, or "" when it is neither an email nor a safe HTTPS URL. It mirrors
// ircAvatarMetadataValue.
func avatarMetadataValue(input string) string {
	trimmed := strings.TrimSpace(input)
	if trimmed == "" {
		return ""
	}
	if looksLikeAvatarEmail(trimmed) {
		return gravatarMetadataURL(trimmed)
	}
	resolved, ok := resolvedAvatarURL(trimmed, 32)
	if ok && avatarURLIsSafe(resolved) {
		return trimmed
	}
	return ""
}

// resolvedAvatarURL substitutes the {size} placeholder and parses raw as an
// absolute URL, or reports false. It mirrors ircResolvedAvatarUrl.
func resolvedAvatarURL(raw string, pixelSize int) (*url.URL, bool) {
	trimmed := strings.TrimSpace(raw)
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

// avatarURLIsSafe reports whether url is an HTTPS URL that may be fetched: no
// credentials, a non-local host, port 443 or unset, and a global-unicast
// address when the host is an IP literal. It mirrors ircAvatarUrlIsSafe.
func avatarURLIsSafe(parsed *url.URL) bool {
	if parsed == nil || parsed.Scheme == "" || parsed.Host == "" {
		return false
	}
	if !strings.EqualFold(parsed.Scheme, "https") {
		return false
	}
	if parsed.String() == "" || len(parsed.String()) > maximumAvatarURLLength {
		return false
	}
	if parsed.User != nil || parsed.User.Username() != "" {
		return false
	}
	host := strippedHost(parsed.Hostname())
	if host == "" || hostLooksLocal(host) {
		return false
	}
	if port := parsed.Port(); port != "" && port != "443" {
		return false
	}
	if address := net.ParseIP(host); address != nil {
		return !hostAddressIsUnsafe(address)
	}
	return true
}

// hostAddressIsUnsafe reports whether an IP literal is non-global or in a
// range Qt's QHostAddress treats as unsafe to connect to. It mirrors
// ircHostAddressIsUnsafe.
func hostAddressIsUnsafe(address net.IP) bool {
	if address == nil || address.IsUnspecified() || address.IsLoopback() ||
		address.IsLinkLocalUnicast() || address.IsLinkLocalMulticast() ||
		address.IsMulticast() || address.IsPrivate() ||
		!address.IsGlobalUnicast() {
		return true
	}
	if v4 := address.To4(); v4 != nil {
		// 100.64.0.0/10 carrier-grade NAT.
		if v4[0] == 100 && v4[1] >= 64 && v4[1] <= 127 {
			return true
		}
		// 0.0.0.0/8.
		if v4[0] == 0 {
			return true
		}
	}
	return false
}
