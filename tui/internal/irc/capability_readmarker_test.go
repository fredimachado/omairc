package irc

import (
	"strings"
	"testing"
)

func TestDualReadMarkerOfferRequestsDraftToken(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("draft/read-marker soju.im/read"))
	request := negotiation.TakeRequest()
	assertCapLines(t, request.Lines, "draft/read-marker")
	for _, line := range request.Lines {
		if strings.Contains(strings.ToLower(line), "soju.im/read") {
			t.Fatalf("lines = %q, must not request soju.im/read when draft/read-marker is advertised", request.Lines)
		}
	}
}

func TestSojuOnlyReadMarkerRequestsSojuToken(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("soju.im/read"))
	request := negotiation.TakeRequest()
	assertCapLines(t, request.Lines, "soju.im/read")
}
