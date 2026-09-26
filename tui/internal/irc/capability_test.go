package irc

import (
	"strings"
	"testing"
)

// tokens splits a CAP token list the way the C++ helper does.
func tokens(line string) []string {
	return strings.Fields(line)
}

func assertCapLines(t *testing.T, got []string, want ...string) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("CAP REQ lines = %q, want %q", got, want)
	}
	for index := range want {
		if got[index] != want[index] {
			t.Fatalf("CAP REQ line %d = %q, want %q", index, got[index], want[index])
		}
	}
}

func TestUnwantedAdvertisementProducesNoRequest(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("invite-notify"))

	request := negotiation.TakeRequest()
	if len(request.Lines) != 0 {
		t.Fatalf("lines = %q, want none", request.Lines)
	}
	if request.RequestsSasl {
		t.Fatal("requestsSasl must be false")
	}
	if !negotiation.Settled() {
		t.Fatal("negotiation must be settled")
	}
	if !negotiation.Enabled().IsEmpty() {
		t.Fatal("nothing must be enabled")
	}
}

func TestQuietCapsRequestWhenAdvertised(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("multi-prefix chghost cap-notify echo-message invite-notify"))

	request := negotiation.TakeRequest()
	assertCapLines(t, request.Lines, "multi-prefix chghost cap-notify echo-message")
	if request.RequestsSasl {
		t.Fatal("requestsSasl must be false")
	}
	if negotiation.Settled() {
		t.Fatal("negotiation must not be settled before the ACK")
	}

	granted := negotiation.Acknowledge(tokens("multi-prefix chghost cap-notify echo-message"))
	for _, capability := range []Capability{
		CapabilityMultiPrefix, CapabilityChghost, CapabilityCapNotify, CapabilityEchoMessage,
	} {
		if !granted.Contains(capability) {
			t.Fatalf("granted set missing capability %v", capability)
		}
	}
	if !negotiation.Settled() {
		t.Fatal("negotiation must be settled after the ACK")
	}
}

func TestSaslKeepsItsOwnLine(t *testing.T) {
	negotiation := NewCapabilityNegotiation(true)
	negotiation.Advertise(tokens("sasl=PLAIN,EXTERNAL away-notify batch draft/metadata-2 multi-prefix"))

	request := negotiation.TakeRequest()
	assertCapLines(t, request.Lines, "sasl", "away-notify batch draft/metadata-2 multi-prefix")
	if !request.RequestsSasl {
		t.Fatal("requestsSasl must be true")
	}
	if negotiation.Settled() {
		t.Fatal("negotiation must not be settled before the ACK")
	}
}

func TestMessageTagsKeepsItsOwnLine(t *testing.T) {
	negotiation := NewCapabilityNegotiation(true)
	negotiation.Advertise(tokens("sasl=PLAIN message-tags away-notify batch draft/metadata-2"))

	request := negotiation.TakeRequest()
	assertCapLines(t, request.Lines, "sasl", "message-tags", "away-notify batch draft/metadata-2")
	if !request.RequestsSasl {
		t.Fatal("requestsSasl must be true")
	}

	refused := negotiation.Reject(tokens("message-tags"))
	if !refused.Contains(CapabilityMessageTags) {
		t.Fatal("message-tags must be refused")
	}
	if negotiation.Enabled().Contains(CapabilityMessageTags) {
		t.Fatal("message-tags must not be enabled after the NAK")
	}
	if negotiation.Settled() {
		t.Fatal("sasl is still outstanding, so the negotiation is not settled")
	}
}

func TestServerTimeKeepsItsOwnLine(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("server-time batch multi-prefix"))

	request := negotiation.TakeRequest()
	assertCapLines(t, request.Lines, "server-time", "batch multi-prefix")

	refused := negotiation.Reject(tokens("server-time"))
	if !refused.Contains(CapabilityServerTime) {
		t.Fatal("server-time must be refused")
	}
	if negotiation.Enabled().Contains(CapabilityServerTime) {
		t.Fatal("server-time must not be enabled after the NAK")
	}
	if negotiation.Settled() {
		t.Fatal("batch and multi-prefix are still outstanding")
	}

	granted := negotiation.Acknowledge(tokens("batch multi-prefix"))
	if !granted.Contains(CapabilityBatch) || !granted.Contains(CapabilityMultiPrefix) {
		t.Fatalf("granted = %v, want batch and multi-prefix", granted)
	}
	if negotiation.Enabled().Contains(CapabilityServerTime) {
		t.Fatal("server-time must stay disabled")
	}
	if !negotiation.Settled() {
		t.Fatal("negotiation must be settled")
	}
}

func TestSaslNeedsCredentialsAndPlain(t *testing.T) {
	withoutCredentials := NewCapabilityNegotiation(false)
	withoutCredentials.Advertise(tokens("sasl=PLAIN"))
	denied := withoutCredentials.TakeRequest()
	if len(denied.Lines) != 0 {
		t.Fatalf("lines = %q, want none without credentials", denied.Lines)
	}
	if denied.RequestsSasl {
		t.Fatal("requestsSasl must be false without credentials")
	}
	if denied.SaslMechanism != "" {
		t.Fatalf("mechanism = %q, want empty", denied.SaslMechanism)
	}

	externalOnly := NewCapabilityNegotiation(true)
	externalOnly.Advertise(tokens("sasl=EXTERNAL"))
	external := externalOnly.TakeRequest()
	if len(external.Lines) != 0 {
		t.Fatalf("lines = %q, want none for EXTERNAL-only", external.Lines)
	}
	if external.RequestsSasl {
		t.Fatal("requestsSasl must be false for EXTERNAL-only")
	}
	if external.SaslMechanism != "" {
		t.Fatalf("mechanism = %q, want empty", external.SaslMechanism)
	}

	plain := NewCapabilityNegotiation(true)
	plain.Advertise(tokens("sasl=PLAIN"))
	plainRequest := plain.TakeRequest()
	assertCapLines(t, plainRequest.Lines, "sasl")
	if !plainRequest.RequestsSasl {
		t.Fatal("requestsSasl must be true for PLAIN")
	}
	if plainRequest.SaslMechanism != "PLAIN" {
		t.Fatalf("mechanism = %q, want PLAIN", plainRequest.SaslMechanism)
	}

	bare := NewCapabilityNegotiation(true)
	bare.Advertise(tokens("sasl"))
	bareRequest := bare.TakeRequest()
	assertCapLines(t, bareRequest.Lines, "sasl")
	if !bareRequest.RequestsSasl {
		t.Fatal("requestsSasl must be true for a bare sasl token")
	}
	if bareRequest.SaslMechanism != "PLAIN" {
		t.Fatalf("mechanism = %q, want PLAIN", bareRequest.SaslMechanism)
	}
}

func TestScramSha256IsPreferredWhenAdvertised(t *testing.T) {
	both := NewCapabilityNegotiation(true)
	both.Advertise(tokens("sasl=SCRAM-SHA-256,PLAIN"))
	request := both.TakeRequest()
	assertCapLines(t, request.Lines, "sasl")
	if !request.RequestsSasl {
		t.Fatal("requestsSasl must be true")
	}
	if request.SaslMechanism != "SCRAM-SHA-256" {
		t.Fatalf("mechanism = %q, want SCRAM-SHA-256", request.SaslMechanism)
	}
	again := both.TakeRequest()
	if len(again.Lines) != 0 || again.RequestsSasl || again.SaslMechanism != "" {
		t.Fatalf("second request = %+v, want empty", again)
	}

	reversed := NewCapabilityNegotiation(true)
	reversed.Advertise(tokens("sasl=PLAIN,SCRAM-SHA-256"))
	if got := reversed.TakeRequest().SaslMechanism; got != "SCRAM-SHA-256" {
		t.Fatalf("mechanism = %q, want SCRAM-SHA-256", got)
	}

	scramOnly := NewCapabilityNegotiation(true)
	scramOnly.Advertise(tokens("sasl=SCRAM-SHA-256"))
	only := scramOnly.TakeRequest()
	assertCapLines(t, only.Lines, "sasl")
	if only.SaslMechanism != "SCRAM-SHA-256" {
		t.Fatalf("mechanism = %q, want SCRAM-SHA-256", only.SaslMechanism)
	}

	folded := NewCapabilityNegotiation(true)
	folded.Advertise(tokens("sasl=scram-sha-256"))
	if got := folded.TakeRequest().SaslMechanism; got != "SCRAM-SHA-256" {
		t.Fatalf("mechanism = %q, want SCRAM-SHA-256", got)
	}

	withExternal := NewCapabilityNegotiation(true)
	withExternal.Advertise(tokens("sasl=EXTERNAL,SCRAM-SHA-256"))
	if got := withExternal.TakeRequest().SaslMechanism; got != "SCRAM-SHA-256" {
		t.Fatalf("mechanism = %q, want SCRAM-SHA-256", got)
	}

	withoutCredentials := NewCapabilityNegotiation(false)
	withoutCredentials.Advertise(tokens("sasl=SCRAM-SHA-256,PLAIN"))
	denied := withoutCredentials.TakeRequest()
	if len(denied.Lines) != 0 || denied.RequestsSasl || denied.SaslMechanism != "" {
		t.Fatalf("denied = %+v, want empty", denied)
	}
}

func TestMemberMetadataNeedsBatch(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("draft/metadata-2 away-notify"))
	assertCapLines(t, negotiation.TakeRequest().Lines, "away-notify")

	negotiation.Advertise(tokens("batch"))
	assertCapLines(t, negotiation.TakeRequest().Lines, "batch draft/metadata-2")
}

func TestAcknowledgeAndRejectSettleIndependently(t *testing.T) {
	negotiation := NewCapabilityNegotiation(true)
	negotiation.Advertise(tokens("sasl=PLAIN away-notify batch draft/metadata-2"))
	negotiation.TakeRequest()

	refused := negotiation.Reject(tokens("away-notify batch draft/metadata-2"))
	if !refused.Contains(CapabilityAwayNotify) || !refused.Contains(CapabilityMemberMetadata) {
		t.Fatalf("refused = %v, want away-notify and member metadata", refused)
	}
	if refused.Contains(CapabilitySasl) {
		t.Fatal("sasl must not be refused")
	}
	if !negotiation.Enabled().IsEmpty() {
		t.Fatal("nothing must be enabled yet")
	}
	if negotiation.Settled() {
		t.Fatal("sasl is still outstanding")
	}

	granted := negotiation.Acknowledge(tokens("sasl"))
	if !granted.Contains(CapabilitySasl) {
		t.Fatal("sasl must be granted")
	}
	if !negotiation.Settled() {
		t.Fatal("negotiation must be settled")
	}
	if !negotiation.Enabled().Contains(CapabilitySasl) {
		t.Fatal("sasl must be enabled")
	}
	if negotiation.Enabled().Contains(CapabilityAwayNotify) {
		t.Fatal("away-notify must stay refused")
	}
}

func TestDeletionWithdrawsAnEnabledCapability(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("away-notify batch draft/metadata-2"))
	negotiation.TakeRequest()
	negotiation.Acknowledge(tokens("away-notify batch draft/metadata-2"))
	if !negotiation.Enabled().Contains(CapabilityAwayNotify) {
		t.Fatal("away-notify must be enabled")
	}

	negotiation.Withdraw(tokens("away-notify"))
	if negotiation.Enabled().Contains(CapabilityAwayNotify) {
		t.Fatal("away-notify must be withdrawn")
	}
	if !negotiation.Enabled().Contains(CapabilityMemberMetadata) {
		t.Fatal("member metadata must stay enabled")
	}

	if len(negotiation.TakeRequest().Lines) != 0 {
		t.Fatal("nothing new is requestable after the withdrawal")
	}
}

func TestTimeoutAbandonsOutstandingRequests(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("away-notify batch"))
	negotiation.TakeRequest()
	if negotiation.Settled() {
		t.Fatal("negotiation must not be settled after the request")
	}

	abandoned := negotiation.AbandonOutstanding()
	if !abandoned.Contains(CapabilityAwayNotify) || !abandoned.Contains(CapabilityBatch) {
		t.Fatalf("abandoned = %v, want away-notify and batch", abandoned)
	}
	if !negotiation.Settled() {
		t.Fatal("negotiation must be settled after the timeout")
	}
	if !negotiation.Enabled().IsEmpty() {
		t.Fatal("nothing must be enabled after the timeout")
	}
}

func TestRequestIsIdempotentUntilSomethingNewIsAdvertised(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("away-notify"))
	assertCapLines(t, negotiation.TakeRequest().Lines, "away-notify")
	if len(negotiation.TakeRequest().Lines) != 0 {
		t.Fatal("the second request must be empty")
	}

	negotiation.Acknowledge(tokens("away-notify"))
	if len(negotiation.TakeRequest().Lines) != 0 {
		t.Fatal("the request must stay empty after the ACK")
	}

	negotiation.Reset(false)
	if !negotiation.Enabled().IsEmpty() {
		t.Fatal("reset must clear enabled capabilities")
	}
	if len(negotiation.TakeRequest().Lines) != 0 {
		t.Fatal("reset must clear advertised tokens")
	}
}

func TestDualChatHistoryOfferRequestsStableToken(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("batch chathistory draft/chathistory"))

	request := negotiation.TakeRequest()
	assertCapLines(t, request.Lines, "batch chathistory")
	if strings.Contains(strings.Join(request.Lines, " "), "draft/chathistory") {
		t.Fatalf("lines = %q, must not request draft/chathistory", request.Lines)
	}
}

func TestDraftOnlyChatHistoryRequestsDraftToken(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("batch draft/chathistory"))

	request := negotiation.TakeRequest()
	assertCapLines(t, request.Lines, "batch draft/chathistory")
}

func TestChatHistoryWithoutBatchIsNotRequested(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("chathistory draft/chathistory"))

	if lines := negotiation.TakeRequest().Lines; len(lines) != 0 {
		t.Fatalf("lines = %q, want none without batch", lines)
	}
}

func TestDeletingStableChatHistoryKeepsDraftAdvertised(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("batch chathistory draft/chathistory"))
	negotiation.TakeRequest()
	negotiation.Acknowledge(tokens("batch chathistory"))
	if !negotiation.Enabled().Contains(CapabilityChatHistory) {
		t.Fatal("chat history must be enabled")
	}

	negotiation.Withdraw(tokens("chathistory"))
	assertCapLines(t, negotiation.TakeRequest().Lines, "draft/chathistory")
}

func TestDeletingUnusedDraftChatHistoryKeepsStableEnabled(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("batch chathistory draft/chathistory"))
	negotiation.TakeRequest()
	negotiation.Acknowledge(tokens("batch chathistory"))
	if !negotiation.Enabled().Contains(CapabilityChatHistory) {
		t.Fatal("chat history must be enabled")
	}

	negotiation.Withdraw(tokens("draft/chathistory"))
	if !negotiation.Enabled().Contains(CapabilityChatHistory) {
		t.Fatal("chat history must stay enabled")
	}
	if len(negotiation.TakeRequest().Lines) != 0 {
		t.Fatal("nothing new is requestable")
	}
}

func TestNakOfStableChatHistoryRequestsDraftToken(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("batch chathistory draft/chathistory"))
	assertCapLines(t, negotiation.TakeRequest().Lines, "batch chathistory")
	negotiation.Acknowledge(tokens("batch"))
	negotiation.Reject(tokens("chathistory"))
	assertCapLines(t, negotiation.TakeRequest().Lines, "draft/chathistory")
}

func TestLateAcknowledgeAfterTimeoutIsIgnored(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("away-notify"))
	negotiation.TakeRequest()
	negotiation.AbandonOutstanding()

	if !negotiation.Acknowledge(tokens("away-notify")).IsEmpty() {
		t.Fatal("a late ACK must grant nothing")
	}
	if !negotiation.Enabled().IsEmpty() {
		t.Fatal("nothing must be enabled")
	}
	if !negotiation.Settled() {
		t.Fatal("negotiation must stay settled")
	}
}

func TestLateRejectAfterTimeoutIsIgnored(t *testing.T) {
	negotiation := NewCapabilityNegotiation(true)
	negotiation.Advertise(tokens("sasl"))
	if !negotiation.TakeRequest().RequestsSasl {
		t.Fatal("sasl must be requested")
	}
	negotiation.AbandonOutstanding()

	if !negotiation.Reject(tokens("sasl")).IsEmpty() {
		t.Fatal("a late NAK must refuse nothing")
	}
}

func TestAcknowledgingEitherChatHistorySpellingSettles(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("batch draft/chathistory"))
	assertCapLines(t, negotiation.TakeRequest().Lines, "batch draft/chathistory")

	negotiation.Acknowledge(tokens("batch draft/chathistory"))
	if !negotiation.Settled() {
		t.Fatal("negotiation must be settled")
	}
	if !negotiation.Enabled().Contains(CapabilityChatHistory) {
		t.Fatal("chat history must be enabled")
	}
	if len(negotiation.TakeRequest().Lines) != 0 {
		t.Fatal("nothing new is requestable")
	}
}

func TestAcknowledgedRemovalDisablesTheCapability(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("batch chathistory"))
	negotiation.TakeRequest()
	negotiation.Acknowledge(tokens("batch chathistory"))
	if !negotiation.Enabled().Contains(CapabilityChatHistory) {
		t.Fatal("chat history must be enabled")
	}

	negotiation.Acknowledge(tokens("-chathistory"))
	if negotiation.Enabled().Contains(CapabilityChatHistory) {
		t.Fatal("chat history must be disabled after -chathistory")
	}
	if !negotiation.Enabled().Contains(CapabilityBatch) {
		t.Fatal("batch must stay enabled")
	}
}

func TestStsIsAdvertisedButNeverRequested(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("sts=port=6697,duration=60 multi-prefix cap-notify"))

	request := negotiation.TakeRequest()
	assertCapLines(t, request.Lines, "multi-prefix cap-notify")
	if negotiation.Settled() {
		t.Fatal("negotiation must not be settled before the ACK")
	}
}

func TestLabeledResponseKeepsItsOwnLine(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("message-tags labeled-response away-notify"))

	request := negotiation.TakeRequest()
	assertCapLines(t, request.Lines, "message-tags", "labeled-response", "away-notify")
	if negotiation.Settled() {
		t.Fatal("negotiation must not be settled before the ACK")
	}

	refused := negotiation.Reject(tokens("labeled-response"))
	if !refused.Contains(CapabilityLabeledResponse) {
		t.Fatal("labeled-response must be refused")
	}
	if negotiation.Enabled().Contains(CapabilityLabeledResponse) {
		t.Fatal("labeled-response must not be enabled after the NAK")
	}
	if negotiation.Settled() {
		t.Fatal("message-tags is still outstanding")
	}

	granted := negotiation.Acknowledge(tokens("message-tags away-notify"))
	if !granted.Contains(CapabilityMessageTags) || !granted.Contains(CapabilityAwayNotify) {
		t.Fatalf("granted = %v, want message-tags and away-notify", granted)
	}
	if negotiation.Enabled().Contains(CapabilityLabeledResponse) {
		t.Fatal("labeled-response must stay disabled")
	}
	if !negotiation.Settled() {
		t.Fatal("negotiation must be settled")
	}
}

func TestAccountCapsRequestWhenAdvertised(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("message-tags account-tag account-notify extended-join"))

	assertCapLines(t, negotiation.TakeRequest().Lines,
		"message-tags", "account-tag account-notify extended-join")
}

func TestAccountTagNeedsMessageTags(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("account-tag account-notify extended-join"))

	assertCapLines(t, negotiation.TakeRequest().Lines, "account-notify extended-join")
}

func TestPartialAccountAdvertisementRequestsOnlyPresentCaps(t *testing.T) {
	onlyNotify := NewCapabilityNegotiation(false)
	onlyNotify.Advertise(tokens("account-notify"))
	assertCapLines(t, onlyNotify.TakeRequest().Lines, "account-notify")

	tagAndJoin := NewCapabilityNegotiation(false)
	tagAndJoin.Advertise(tokens("message-tags account-tag extended-join"))
	assertCapLines(t, tagAndJoin.TakeRequest().Lines, "message-tags", "account-tag extended-join")

	none := NewCapabilityNegotiation(false)
	none.Advertise(tokens("invite-notify"))
	if len(none.TakeRequest().Lines) != 0 {
		t.Fatal("no lines expected")
	}
	if !none.Settled() {
		t.Fatal("negotiation must be settled")
	}
}

func TestAccountCapAckEnablesMatchingBits(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("message-tags account-tag account-notify extended-join"))
	negotiation.TakeRequest()

	granted := negotiation.Acknowledge(tokens("message-tags account-tag account-notify extended-join"))
	for _, capability := range []Capability{
		CapabilityAccountTag, CapabilityAccountNotify, CapabilityExtendedJoin,
	} {
		if !granted.Contains(capability) {
			t.Fatalf("granted set missing capability %v", capability)
		}
		if !negotiation.Enabled().Contains(capability) {
			t.Fatalf("enabled set missing capability %v", capability)
		}
	}
	if !negotiation.Settled() {
		t.Fatal("negotiation must be settled")
	}
}

func TestZncPlaybackNeedsBatchAndItsOwnLine(t *testing.T) {
	withoutBatch := NewCapabilityNegotiation(false)
	withoutBatch.Advertise(tokens("znc.in/playback multi-prefix"))
	assertCapLines(t, withoutBatch.TakeRequest().Lines, "multi-prefix")
	if withoutBatch.Enabled().Contains(CapabilityZncPlayback) {
		t.Fatal("znc playback must not be enabled without batch")
	}

	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("znc.in/playback batch server-time"))
	assertCapLines(t, negotiation.TakeRequest().Lines, "server-time", "znc.in/playback", "batch")

	granted := negotiation.Acknowledge(tokens("znc.in/playback batch server-time"))
	for _, capability := range []Capability{
		CapabilityZncPlayback, CapabilityBatch, CapabilityServerTime,
	} {
		if !granted.Contains(capability) {
			t.Fatalf("granted set missing capability %v", capability)
		}
	}
	if !negotiation.Enabled().Contains(CapabilityZncPlayback) {
		t.Fatal("znc playback must be enabled")
	}
	if !negotiation.Settled() {
		t.Fatal("negotiation must be settled")
	}
}

func TestZncPlaybackAbsentIsNotRequested(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("batch server-time multi-prefix"))
	lines := negotiation.TakeRequest().Lines
	for _, line := range lines {
		if strings.Contains(line, "znc.in/playback") {
			t.Fatalf("lines = %q, must not request znc.in/playback", lines)
		}
	}
	negotiation.Acknowledge(tokens("batch server-time multi-prefix"))
	if negotiation.Enabled().Contains(CapabilityZncPlayback) {
		t.Fatal("znc playback must remain disabled")
	}
}

func TestLabeledResponseNeedsMessageTags(t *testing.T) {
	negotiation := NewCapabilityNegotiation(false)
	negotiation.Advertise(tokens("labeled-response away-notify"))
	assertCapLines(t, negotiation.TakeRequest().Lines, "away-notify")

	negotiation.Advertise(tokens("message-tags"))
	assertCapLines(t, negotiation.TakeRequest().Lines, "message-tags", "labeled-response")
}
