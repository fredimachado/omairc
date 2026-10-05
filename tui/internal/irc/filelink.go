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

// FileHostSendsBasicAuth reports whether the IRC password may be sent to
// this upload endpoint. The scheme must be http or https, the URL must have
// no userinfo, and a cleartext upload is refused when the IRC connection is
// encrypted. serverHost is unused; a different https host still authenticates.
// It mirrors fileHostSendsBasicAuth.
func FileHostSendsBasicAuth(serverHost, uploadURL string, serverEncrypted bool) bool {
	if uploadURL == "" {
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
	return true
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
