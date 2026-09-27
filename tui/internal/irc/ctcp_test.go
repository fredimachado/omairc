package irc

import (
	"testing"
	"time"
)

func TestParsesAndFormatsCtcp(t *testing.T) {
	version, ok := ParseCtcpRequest("\x01VERSION\x01")
	if !ok {
		t.Fatal("VERSION request failed to parse")
	}
	if version.Command != "VERSION" || version.Argument != "" {
		t.Fatalf("version = %+v, want VERSION with no argument", version)
	}
	if got := CtcpPayload(version); got != "\x01VERSION\x01" {
		t.Fatalf("payload = %q, want \\x01VERSION\\x01", got)
	}

	ping, ok := ParseCtcpRequest("\x01ping 42\x01")
	if !ok {
		t.Fatal("ping request failed to parse")
	}
	if ping.Command != "PING" || ping.Argument != "42" {
		t.Fatalf("ping = %+v, want PING 42", ping)
	}
	if got := CtcpPayload(ping); got != "\x01PING 42\x01" {
		t.Fatalf("payload = %q, want \\x01PING 42\\x01", got)
	}

	if _, ok := ParseCtcpRequest("VERSION"); ok {
		t.Fatal("undelimited body unexpectedly parsed")
	}
	if _, ok := ParseCtcpRequest("\x01\x01"); ok {
		t.Fatal("empty payload unexpectedly parsed")
	}

	now := time.UnixMilli(1_700_000_000_042)
	cases := []struct {
		command  string
		nick     string
		argument string
		want     string
	}{
		{"PING", "lena", "1700000000000", "PING reply from lena: 42 ms"},
		{"PING", "lena", "", "PING reply from lena"},
		{"TIME", "lena", "Tue, 15 Sep 2026 12:00:00 +0000", "TIME reply from lena: Tue, 15 Sep 2026 12:00:00 +0000"},
		{"VERSION", "lena", "Omairc 0.4.0", "VERSION reply from lena: Omairc 0.4.0"},
	}
	for _, testCase := range cases {
		got := FormatCtcpReplyText(testCase.command, testCase.nick, testCase.argument, now)
		if got != testCase.want {
			t.Fatalf("FormatCtcpReplyText(%q, %q, %q) = %q, want %q",
				testCase.command, testCase.nick, testCase.argument, got, testCase.want)
		}
	}
}
