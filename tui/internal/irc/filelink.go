package irc

import (
	"encoding/base64"
	"net/url"
	"strings"
)

// This file ports the upload-link rules from src/irc/ircfilelink.cpp.
// absoluteFileLink resolves a successful upload's Location. The result is an
// absolute http or https URL with no userinfo. Anything else is refused.
//
// net/url is the parser behind QUrl. bin/check-conventions exempts exactly
// this file for that import.

func hasControlCharacter(text string) bool {
	for index := 0; index < len(text); index++ {
		character := text[index]
		if character < 0x20 || character == 0x7F {
			return true
		}
	}
	return false
}

func webScheme(scheme string) bool {
	return strings.EqualFold(scheme, "https") || strings.EqualFold(scheme, "http")
}

// AbsoluteFileLink resolves location against endpoint. It mirrors
// absoluteFileLink.
func AbsoluteFileLink(endpoint, location string) string {
	if endpoint == "" || location == "" {
		return ""
	}
	if hasControlCharacter(endpoint) || hasControlCharacter(location) {
		return ""
	}
	base, err := url.Parse(endpoint)
	if err != nil || !webScheme(base.Scheme) || base.Hostname() == "" {
		return ""
	}
	ref, err := url.Parse(location)
	if err != nil {
		return ""
	}
	resolved := base.ResolveReference(ref)
	if resolved == nil || !webScheme(resolved.Scheme) || resolved.Hostname() == "" || resolved.User != nil {
		return ""
	}
	resolved.User = nil
	resolved.Fragment = ""
	text := resolved.String()
	if text == "" || strings.ContainsAny(text, " \n\r") {
		return ""
	}
	return text
}

func hostsMatch(left, right string) bool {
	a := strings.TrimRight(left, ".")
	b := strings.TrimRight(right, ".")
	return a != "" && strings.EqualFold(a, b)
}

// FileHostSendsBasicAuth reports whether the IRC password may be sent to
// this upload endpoint. The host must be the server we connected to, and a
// cleartext upload is refused when the IRC connection is encrypted. It
// mirrors fileHostSendsBasicAuth.
func FileHostSendsBasicAuth(serverHost, uploadURL string, serverEncrypted bool) bool {
	if serverHost == "" || uploadURL == "" {
		return false
	}
	parsed, err := url.Parse(uploadURL)
	if err != nil || parsed.Hostname() == "" || parsed.User != nil {
		return false
	}
	https := strings.EqualFold(parsed.Scheme, "https")
	http := strings.EqualFold(parsed.Scheme, "http")
	if !https && !http {
		return false
	}
	if serverEncrypted && !https {
		return false
	}
	return hostsMatch(serverHost, parsed.Hostname())
}

// BasicAuthorization is the Authorization header value, or "" when the pair
// cannot be sent. Callers must not log it. It mirrors basicAuthorizationValue.
func BasicAuthorization(user, secret string) string {
	if user == "" || secret == "" {
		return ""
	}
	if hasControlCharacter(user) || hasControlCharacter(secret) {
		return ""
	}
	token := base64.StdEncoding.EncodeToString([]byte(user + ":" + secret))
	return "Basic " + token
}
