package irc

import (
	"strings"
	"testing"
)

func TestBouncerPassIsRedactedOnBothSides(t *testing.T) {
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
