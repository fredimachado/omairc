package irc

import (
	"testing"
	"time"
)

func TestFormatListModeNumerics(t *testing.T) {
	now := time.Unix(1, 0).UTC()
	entry := Incoming("net", mustParseListMode(t, ":irc 367 me #room *!*@evil.com alice 1"), "#&", now)
	if entry.Label() != "banlist" {
		t.Fatalf("label = %q, want banlist", entry.Label())
	}
	line := entry.ListModeLine()
	if line == nil {
		t.Fatal("expected list mode line")
	}
	if line.Text() != "ban *!*@evil.com on #room by alice" {
		t.Fatalf("text = %q", line.Text())
	}
	if line.Kind() != ListMaskBan || line.Terminal() {
		t.Fatalf("unexpected kind/progress: %+v", line)
	}

	end := Incoming("net", mustParseListMode(t, ":irc 368 me #room :End of Channel Ban List"), "#&", now)
	if end.ListModeLine() == nil || !end.ListModeLine().Terminal() {
		t.Fatal("368 should be terminal")
	}
	if end.Text() != "End of ban list for #room" {
		t.Fatalf("end text = %q", end.Text())
	}

	quiet := Incoming("net", mustParseListMode(t, ":irc 728 me #room q *!*@evil.com bob 2"), "#&", now)
	if quiet.Label() != "quietlist" {
		t.Fatalf("quiet label = %q", quiet.Label())
	}
	if quiet.ListModeLine() == nil || quiet.ListModeLine().Text() != "quiet *!*@evil.com on #room by bob" {
		t.Fatalf("quiet text = %q", quiet.Text())
	}
}

func mustParseListMode(t *testing.T, line string) Message {
	t.Helper()
	message, err := Parse(line)
	if err != nil {
		t.Fatalf("parse %q: %v", line, err)
	}
	return message
}
