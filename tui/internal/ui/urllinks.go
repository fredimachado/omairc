package ui

import (
	"regexp"
	"strings"

	"github.com/fredimachado/omairc/tui/internal/openurl"
)

// This file ports the URL policy and the Ctrl+Shift+O link sheet from
// OmaircWindow.qml: isAllowedHttpUrl, trimHttpUrlMatch, httpUrlSpans,
// httpUrlAt, inviteChannelAt, refreshLinkMatches, appendLinkUrlsFromText, and
// openAllowedUrl. It reads only controller snapshots and the OS opener seam,
// never internal/irc.

// allowedURLSchemes is the scheme allowlist. It mirrors allowedUrlSchemes in
// src/OmaircWindow.qml.
var allowedURLSchemes = map[string]bool{"http": true, "https": true}

// isAllowedHTTPURL mirrors isAllowedHttpUrl in src/OmaircWindow.qml: scheme
// http or https (case-insensitive) followed by "://", a non-empty remainder,
// and no space, newline, or carriage return anywhere. A hostname is not
// validated separately, so "https:///foo" is allowed.
func isAllowedHTTPURL(raw string) bool {
	if raw == "" {
		return false
	}
	colon := strings.IndexByte(raw, ':')
	if colon <= 0 {
		return false
	}
	scheme := strings.ToLower(raw[:colon])
	if !allowedURLSchemes[scheme] {
		return false
	}
	if colon+3 > len(raw) || raw[colon:colon+3] != "://" {
		return false
	}
	if len(raw) == colon+3 {
		return false
	}
	if strings.ContainsAny(raw, "\n\r ") {
		return false
	}
	return true
}

// trimHTTPURLMatch mirrors trimHttpUrlMatch in src/OmaircWindow.qml: strip
// trailing ".,;:!?" and a trailing ")" or "]" only while the close count
// exceeds the open count inside the prefix, so balanced pairs such as
// "IRC_(protocol)" stay intact.
func trimHTTPURLMatch(raw string) string {
	end := len(raw)
	for end > 0 {
		ch := raw[end-1]
		if strings.IndexByte(".,;:!?", ch) >= 0 {
			end--
			continue
		}
		if ch == ')' {
			open := 0
			closed := 0
			for index := 0; index < end; index++ {
				switch raw[index] {
				case '(':
					open++
				case ')':
					closed++
				}
			}
			if closed > open {
				end--
				continue
			}
		}
		if ch == ']' {
			open := 0
			closed := 0
			for index := 0; index < end; index++ {
				switch raw[index] {
				case '[':
					open++
				case ']':
					closed++
				}
			}
			if closed > open {
				end--
				continue
			}
		}
		break
	}
	return raw[:end]
}

// urlSpan is one matched URL within a line. start and end are byte offsets
// into the line; url is the trimmed match.
type urlSpan struct {
	start int
	end   int
	url   string
}

// httpURLPattern mirrors the QML literal /https?:\/\/[^\s<>"']+/gi.
var httpURLPattern = regexp.MustCompile(`(?i)https?://[^\s<>"']+`)

// httpURLSpans returns every http(s) match in text, in order, with any
// trailing punctuation trimmed off. A match that trims to nothing is dropped.
func httpURLSpans(text string) []urlSpan {
	if text == "" {
		return nil
	}
	matches := httpURLPattern.FindAllStringIndex(text, -1)
	spans := make([]urlSpan, 0, len(matches))
	for _, match := range matches {
		trimmed := trimHTTPURLMatch(text[match[0]:match[1]])
		if len(trimmed) == 0 {
			continue
		}
		spans = append(spans, urlSpan{
			start: match[0],
			end:   match[0] + len(trimmed),
			url:   trimmed,
		})
	}
	return spans
}

// httpURLAt mirrors httpUrlAt in src/OmaircWindow.qml: the allowed URL span
// containing index, else "".
func httpURLAt(text string, index int) string {
	if text == "" || index < 0 || index >= len(text) {
		return ""
	}
	for _, span := range httpURLSpans(text) {
		if index >= span.start && index < span.end && isAllowedHTTPURL(span.url) {
			return span.url
		}
	}
	return ""
}

// inviteMarker is the INVITE text the status console renders, mirroring
// IrcStatusEntry's "nick invited you to #channel".
const inviteMarker = " invited you to "

// inviteChannelAt mirrors inviteChannelAt in src/OmaircWindow.qml: the channel
// after the last " invited you to " when index falls inside that channel, else
// "".
func inviteChannelAt(text string, index int) string {
	if text == "" || index < 0 || index >= len(text) {
		return ""
	}
	at := strings.LastIndex(text, inviteMarker)
	if at < 0 {
		return ""
	}
	start := at + len(inviteMarker)
	channel := text[start:]
	if channel == "" {
		return ""
	}
	if index >= start && index < start+len(channel) {
		return channel
	}
	return ""
}

// inviteChannelAfter returns the non-empty channel after the last
// " invited you to " in text, else "". Only the invite line shape yields a
// channel, so a plain chat line is not misread as an invite.
func inviteChannelAfter(text string) string {
	at := strings.LastIndex(text, inviteMarker)
	if at < 0 {
		return ""
	}
	channel := text[at+len(inviteMarker):]
	if channel == "" {
		return ""
	}
	return channel
}

// Link-match kinds.
const (
	linkKindURL     = "url"
	linkKindInvite  = "invite"
	linkKindChannel = "channel"
)

// linkMatch is one row of the link sheet. row is the source transcript or
// console row, used to reveal the row the URL came from.
type linkMatch struct {
	kind  string
	value string
	label string
	row   int
}

// appendLinkURLsFromText mirrors appendLinkUrlsFromText in src/OmaircWindow.qml:
// append the allowed URLs of text, last-in-text first. query filters on the
// lower-cased URL when non-empty.
func appendLinkURLsFromText(matches []linkMatch, text string, row int, query string) []linkMatch {
	if text == "" {
		return matches
	}
	spans := httpURLSpans(text)
	for index := len(spans) - 1; index >= 0; index-- {
		url := spans[index].url
		if !isAllowedHTTPURL(url) {
			continue
		}
		if query != "" && !strings.Contains(strings.ToLower(url), query) {
			continue
		}
		matches = append(matches, linkMatch{
			kind:  linkKindURL,
			value: url,
			label: url,
			row:   row,
		})
	}
	return matches
}

// appendChannelNames mirrors appendChannelNamesFromText in src/OmaircWindow.qml.
// Channel names come from the controller, which reads CHANTYPES from the
// focused network. A name that sits on an allowed URL is left to that URL.
// Names are appended last-in-text first.
func (m *Model) appendChannelNames(matches []linkMatch, text string, row int, query string) []linkMatch {
	if m == nil || m.ctrl == nil || text == "" {
		return matches
	}
	spans := m.ctrl.ChannelNameSpans(text)
	for index := len(spans) - 1; index >= 0; index-- {
		name := spans[index].Name
		if name == "" || httpURLAt(text, spans[index].Start) != "" {
			continue
		}
		if query != "" && !strings.Contains(strings.ToLower(name), query) {
			continue
		}
		matches = append(matches, linkMatch{
			kind:  linkKindChannel,
			value: name,
			label: name,
			row:   row,
		})
	}
	return matches
}

// linkMatches mirrors refreshLinkMatches in src/OmaircWindow.qml: the sheet's
// rows for the current surface, newest row first and last-in-text first within
// a row. On Status the console lines are scanned; otherwise the transcript
// rows are, skipping event rows. An invite channel on a Status line becomes an
// invite row whose label is the channel.
func (m *Model) linkMatches() []linkMatch {
	if m == nil || m.ctrl == nil {
		return nil
	}
	query := strings.ToLower(strings.TrimSpace(m.link.input.Value()))
	var matches []linkMatch
	if m.ctrl.ConsoleOpen() {
		rows := m.ctrl.ConsoleText(m.ctrl.FocusedNetworkID())
		for row := len(rows) - 1; row >= 0; row-- {
			text := m.ctrl.PlainIrcText(rows[row])
			matches = appendLinkURLsFromText(matches, text, row, query)
			channel := inviteChannelAfter(text)
			if channel == "" {
				continue
			}
			if query != "" && !strings.Contains(strings.ToLower(channel), query) {
				continue
			}
			matches = append(matches, linkMatch{
				kind:  linkKindInvite,
				value: channel,
				label: channel,
				row:   row,
			})
		}
		return matches
	}
	topic := m.ctrl.PlainIrcText(m.ctrl.Topic())
	matches = appendLinkURLsFromText(matches, topic, -1, query)
	matches = m.appendChannelNames(matches, topic, -1, query)
	messages := m.ctrl.Messages()
	for row := len(messages) - 1; row >= 0; row-- {
		if messages[row].Kind == "event" {
			continue
		}
		body := m.ctrl.PlainIrcText(messages[row].Body)
		matches = appendLinkURLsFromText(matches, body, row, query)
		matches = m.appendChannelNames(matches, body, row, query)
	}
	return matches
}

// openAllowedURL mirrors openAllowedUrl in src/OmaircWindow.qml: fail closed
// on a disallowed URL, record the last opened URL, and hand an allowed URL to
// the OS opener unless the test/suppression latch is set. It reports whether
// the URL was allowed.
func (m *Model) openAllowedURL(raw string) bool {
	if !isAllowedHTTPURL(raw) {
		return false
	}
	m.lastOpenedURL = raw
	if !m.suppressExternalOpen {
		_ = openurl.Open(raw)
	}
	return true
}
