package irc

import (
	"strings"
	"testing"
	"time"
)

func TestBouncerPassIsRedactedOnBothSides(t *testing.T) {
	if !AllowsTranscript("hi pass=x", "") {
		t.Fatal("a channel line containing pass= must stay in the transcript")
	}
	if _, ok := RedactWireLine("hi pass=x", ""); ok {
		t.Fatal("hi pass=x must not be redacted")
	}
	channel, err := Parse(":alice!a@h PRIVMSG #chan :hi pass=x")
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := RedactMessage(channel, ""); ok {
		t.Fatal("a channel PRIVMSG containing pass= must not be redacted")
	}
	if _, ok := RedactWireLine("FAIL CHATHISTORY INVALID_TARGET #chan :hi pass=x", ""); ok {
		t.Fatal("pass= outside BOUNCER must not be redacted")
	}

	now := time.Date(2026, 9, 4, 0, 0, 0, 0, time.UTC)
	network, err := Parse("BOUNCER NETWORK 42 host=irc.example;pass=s3cret")
	if err != nil {
		t.Fatal(err)
	}
	assertPassMasked(t, Incoming("network-a", network, "", now).Text())
	assertPassMasked(t, Outgoing("network-a", []byte("BOUNCER ADDNETWORK host=irc.example;pass=s3cret;name=OFTC\r\n"), "", now).Text())
	assertPassMasked(t, Outgoing("network-a", []byte("BOUNCER CHANGENETWORK 44 host=irc.example;pass=s3cret\r\n"), "", now).Text())

	outbound, ok := RedactWireLine(
		"BOUNCER ADDNETWORK host=irc.example;pass=s3cret;name=OFTC", "")
	if !ok || !strings.Contains(outbound, "pass=***") || strings.Contains(outbound, "s3cret") {
		t.Fatalf("outbound = %q ok=%v", outbound, ok)
	}

	inbound, err := Parse("BOUNCER NETWORK 42 pass=s3cret")
	if err != nil {
		t.Fatal(err)
	}
	masked, ok := RedactMessage(inbound, "")
	inboundText := strings.Join(masked.Parameters, " ")
	if !ok || !strings.Contains(inboundText, "pass=***") || strings.Contains(inboundText, "s3cret") {
		t.Fatalf("inbound = %+v ok=%v", masked, ok)
	}

	mixed, ok := RedactWireLine("BOUNCER ADDNETWORK Pass=Secret", "")
	if !ok || !strings.Contains(mixed, "Pass=***") || strings.Contains(mixed, "Secret") {
		t.Fatalf("mixed = %q ok=%v", mixed, ok)
	}

	if _, ok := RedactWireLine("BOUNCER NETWORK 1 compass=north", ""); ok {
		t.Fatal("compass= must stay readable")
	}
	passLine, ok := RedactWireLine("PASS hunter2", "")
	if !ok || passLine != "PASS ***" {
		t.Fatalf("PASS = %q ok=%v", passLine, ok)
	}
}

func assertPassMasked(t *testing.T, text string) {
	t.Helper()
	if !strings.Contains(text, "pass=***") || strings.Contains(text, "s3cret") {
		t.Fatalf("status = %q", text)
	}
}
