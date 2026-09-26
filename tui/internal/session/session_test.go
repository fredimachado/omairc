package session

import (
	"crypto/pbkdf2"
	"crypto/sha256"
	"encoding/base64"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// This file ports the phase-2 subset of tests/session/tst_session.cpp. It
// uses the loopback transport and a FakeClock so every timer (capability
// timeout, ping watchdog, reconnect backoff, labeled-response timeout) fires
// deterministically without sleeping. The C++ suite injected four separate
// FakeReconnectTimer instances; here one FakeClock drives them all, and tests
// advance virtual time by the exact delay the session armed.

const sessionTestNetworkID = "network-a"

// sessionTestConfig mirrors the anonymous config() helper in tst_session.cpp.
func sessionTestConfig(networkID string) SessionConfig {
	return SessionConfig{
		NetworkID:                networkID,
		Host:                     "irc.example",
		Port:                     6697,
		TLSEnabled:               true,
		Nick:                     "omairc",
		Username:                 "omairc",
		Realname:                 "Omairc User",
		AutojoinChannels:         []string{"#omarchy", "&local"},
		ReconnectEnabled:         true,
		ReconnectBaseDelayMs:     250,
		ReconnectMaximumDelayMs:  1000,
		CapabilityTimeoutMs:      10000,
		PingTimeoutMs:            60000,
		LabeledResponseTimeoutMs: 45000,
	}
}

// sessionTestPlaintextConfig mirrors plaintextConfig() in tst_session.cpp.
func sessionTestPlaintextConfig() SessionConfig {
	config := sessionTestConfig(sessionTestNetworkID)
	config.Port = 6667
	config.TLSEnabled = false
	config.AutojoinChannels = nil
	return config
}

type sessionRecordedError struct {
	networkID string
	kind      ErrorKind
	message   string
}

type sessionRecordedReconnect struct {
	networkID string
	delayMs   int
	attempt   int
}

type sessionRecordedAutojoin struct {
	networkID string
	channels  []string
	keys      map[string]string
}

// sessionTestHandler records every Handler callback, standing in for the
// QSignalSpy + StatusCollector helpers of the C++ fixture.
type sessionTestHandler struct {
	states     []SessionState
	errors     []sessionRecordedError
	registered int
	messages   []irc.Message
	batches    []irc.HistoryBatch
	status     []irc.StatusEntry
	caps       []irc.CapabilitySet
	labels     []string
	reconnects []sessionRecordedReconnect
	autojoins  []sessionRecordedAutojoin

	// reconnectLimit, when positive, stops the owning session once that many
	// reconnects have been scheduled (mirrors FixtureOptions).
	reconnectLimit int
	session        *Session
}

var _ Handler = (*sessionTestHandler)(nil)

func (h *sessionTestHandler) StateChanged(state SessionState) {
	h.states = append(h.states, state)
}

func (h *sessionTestHandler) ErrorOccurred(networkID string, kind ErrorKind, message string) {
	h.errors = append(h.errors, sessionRecordedError{networkID, kind, message})
}

func (h *sessionTestHandler) Registered(networkID string) { h.registered++ }

func (h *sessionTestHandler) ReconnectScheduled(networkID string, delayMs, attempt int) {
	h.reconnects = append(h.reconnects, sessionRecordedReconnect{networkID, delayMs, attempt})
	if h.reconnectLimit > 0 && len(h.reconnects) >= h.reconnectLimit && h.session != nil {
		h.session.Stop()
	}
}

func (h *sessionTestHandler) MessageReceived(networkID string, message irc.Message) {
	h.messages = append(h.messages, message)
}

func (h *sessionTestHandler) HistoryBatchReceived(networkID string, batch irc.HistoryBatch) {
	h.batches = append(h.batches, batch)
}

func (h *sessionTestHandler) StatusEntry(entry irc.StatusEntry) {
	h.status = append(h.status, entry)
}

func (h *sessionTestHandler) CapabilitiesChanged(networkID string, capabilities irc.CapabilitySet) {
	h.caps = append(h.caps, capabilities)
}

func (h *sessionTestHandler) RequestLabelFinished(networkID, requestLabel string) {
	h.labels = append(h.labels, requestLabel)
}

func (h *sessionTestHandler) AutojoinChannelsChanged(networkID string, channels []string, keys map[string]string) {
	h.autojoins = append(h.autojoins, sessionRecordedAutojoin{networkID, channels, keys})
}

// hasLabel reports whether any recorded Status entry carried label.
func (h *sessionTestHandler) hasLabel(label string) bool {
	for _, entry := range h.status {
		if entry.Label() == label {
			return true
		}
	}
	return false
}

// anyFieldContains reports whether any Status entry's text, label, or network
// id contains needle.
func (h *sessionTestHandler) anyFieldContains(needle string) bool {
	for _, entry := range h.status {
		if strings.Contains(entry.Text(), needle) ||
			strings.Contains(entry.Label(), needle) ||
			strings.Contains(entry.NetworkID(), needle) {
			return true
		}
	}
	return false
}

// statusTexts returns the text of every recorded Status entry.
func (h *sessionTestHandler) statusTexts() []string {
	texts := make([]string, len(h.status))
	for index, entry := range h.status {
		texts[index] = entry.Text()
	}
	return texts
}

// sessionFixture bundles one session, its loopback transport, and the fake
// clock, mirroring the C++ Fixture struct.
type sessionFixture struct {
	t         *testing.T
	transport *LoopbackTransport
	clock     *FakeClock
	session   *Session
	handler   *sessionTestHandler
}

func newSessionFixture(t *testing.T, config SessionConfig) *sessionFixture {
	t.Helper()
	fixture := &sessionFixture{
		t:         t,
		transport: NewLoopbackTransport(),
		clock:     NewFakeClock(time.Unix(0, 0)),
	}
	fixture.session = NewSession(config, fixture.transport, fixture.clock)
	fixture.handler = &sessionTestHandler{session: fixture.session}
	fixture.session.SetHandler(fixture.handler)
	return fixture
}

func (f *sessionFixture) inject(text string) {
	f.t.Helper()
	f.transport.InjectBytes([]byte(text))
}

// connectTLS starts the session and completes the TLS connect, mirroring
// Fixture::connectTls.
func (f *sessionFixture) connectTLS() {
	f.t.Helper()
	f.session.Start()
	f.transport.CompleteConnect()
}

// connectPlaintext starts the session and completes a plaintext connect.
func (f *sessionFixture) connectPlaintext() {
	f.t.Helper()
	f.session.Start()
	f.transport.CompleteConnect()
}

// registerWithWelcome mirrors Fixture::registerWithWelcome.
func (f *sessionFixture) registerWithWelcome() {
	f.t.Helper()
	f.connectTLS()
	f.inject(":server CAP omairc LS :multi-prefix\r\n:server 001 omairc :Welcome\r\n")
}

// frames returns every written frame as a string.
func (f *sessionFixture) frames() []string {
	raw := f.transport.WrittenFrames()
	frames := make([]string, len(raw))
	for index, frame := range raw {
		frames[index] = string(frame)
	}
	return frames
}

// wrote reports whether one exact frame was written.
func (f *sessionFixture) wrote(frame string) bool {
	for _, candidate := range f.frames() {
		if candidate == frame {
			return true
		}
	}
	return false
}

// lastFrame returns the most recent written frame, or "".
func (f *sessionFixture) lastFrame() string {
	frames := f.frames()
	if len(frames) == 0 {
		return ""
	}
	return frames[len(frames)-1]
}

// writtenSince returns the frames written from index onward.
func (f *sessionFixture) writtenSince(index int) []string {
	frames := f.frames()
	if index > len(frames) {
		return nil
	}
	return frames[index:]
}

// fireReconnect advances virtual time by the last scheduled reconnect delay.
func (f *sessionFixture) fireReconnect() {
	f.t.Helper()
	if len(f.handler.reconnects) == 0 {
		f.t.Fatal("no reconnect scheduled")
	}
	delay := f.handler.reconnects[len(f.handler.reconnects)-1].delayMs
	f.clock.Advance(time.Duration(delay) * time.Millisecond)
}

// reconnectRegistered mirrors the anonymous reconnectRegistered helper.
func (f *sessionFixture) reconnectRegistered() {
	f.t.Helper()
	f.transport.RemoteClose()
	f.fireReconnect()
	f.transport.CompleteConnect()
	f.inject(":server CAP omairc LS :multi-prefix\r\n:server 001 omairc :Welcome\r\n")
}

// mustParse parses a line that must succeed.
func mustParse(t *testing.T, line string) irc.Message {
	t.Helper()
	message, err := irc.Parse(line)
	if err != nil {
		t.Fatalf("Parse(%q) failed: %v", line, err)
	}
	return message
}

// base64Encode is the standard Base64 encoding used by the SASL framing.
func base64Encode(raw []byte) string {
	return base64.StdEncoding.EncodeToString(raw)
}

func assertFramesEqual(t *testing.T, got, want []string) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("frames = %q, want %q", got, want)
	}
	for index := range want {
		if got[index] != want[index] {
			t.Fatalf("frame %d = %q, want %q", index, got[index], want[index])
		}
	}
}

func joinFrames(frames []string) []string {
	var joins []string
	for _, frame := range frames {
		if strings.HasPrefix(frame, "JOIN ") {
			joins = append(joins, frame)
		}
	}
	return joins
}

func capLsCount(frames []string) int {
	count := 0
	for _, frame := range frames {
		if frame == "CAP LS 302\r\n" {
			count++
		}
	}
	return count
}

func framesContain(frames []string, needle string) bool {
	for _, frame := range frames {
		if strings.Contains(frame, needle) {
			return true
		}
	}
	return false
}

// decodeAuthenticatePayload strips the AUTHENTICATE framing and Base64-decodes
// the body, mirroring decodedSaslPayload / decodeAuthenticatePlain.
func decodeAuthenticatePayload(t *testing.T, frame string) []byte {
	t.Helper()
	const prefix = "AUTHENTICATE "
	if !strings.HasPrefix(frame, prefix) {
		t.Fatalf("frame %q does not start with %q", frame, prefix)
	}
	body := strings.TrimSuffix(frame[len(prefix):], "\r\n")
	decoded, err := base64.StdEncoding.DecodeString(body)
	if err != nil {
		t.Fatalf("base64 decode of %q failed: %v", body, err)
	}
	return decoded
}

// authenticateBody strips the AUTHENTICATE framing, keeping the Base64 body.
func authenticateBody(frame string) string {
	const prefix = "AUTHENTICATE "
	return strings.TrimSuffix(frame[len(prefix):], "\r\n")
}

// scramServerFinalFor recomputes the RFC 7677 server-final signature, mirroring
// the anonymous scramServerFinal helper in tst_session.cpp.
func scramServerFinalFor(t *testing.T, clientFirst, serverFirst string) string {
	t.Helper()
	if !strings.HasPrefix(clientFirst, "n,,") {
		t.Fatalf("clientFirst = %q, want the n,, prefix", clientFirst)
	}
	bare := clientFirst[3:]
	nonceAt := strings.Index(serverFirst, "r=")
	if nonceAt < 0 {
		t.Fatalf("serverFirst = %q, want an r= attribute", serverFirst)
	}
	comma := strings.IndexByte(serverFirst[nonceAt:], ',')
	if comma < 0 {
		t.Fatalf("serverFirst = %q, want a comma after r=", serverFirst)
	}
	serverNonce := serverFirst[nonceAt+2 : nonceAt+comma]
	withoutProof := "c=biws,r=" + serverNonce
	authMessage := bare + "," + serverFirst + "," + withoutProof
	salt, err := base64.StdEncoding.DecodeString(scramTestSalt)
	if err != nil {
		t.Fatalf("salt decode failed: %v", err)
	}
	salted, err := pbkdf2.Key(sha256.New, "pencil", salt, 4096, sha256.Size)
	if err != nil {
		t.Fatalf("pbkdf2 failed: %v", err)
	}
	serverKey := hmacSHA256([]byte("Server Key"), salted)
	signature := hmacSHA256([]byte(authMessage), serverKey)
	return "v=" + base64.StdEncoding.EncodeToString(signature)
}

// saslWire renders raw SASL bytes as the concatenated AUTHENTICATE frames a
// server would send, mirroring saslWire in tst_session.cpp.
func saslWire(raw []byte) string {
	var builder strings.Builder
	for _, frame := range encodeSASLFrames(raw) {
		builder.Write(frame)
	}
	return builder.String()
}

// startScramExchange mirrors beginScramExchange: it drives the session into a
// SCRAM-SHA-256 exchange and returns the raw client-first message.
func startScramExchange(t *testing.T, fixture *sessionFixture) []byte {
	t.Helper()
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sasl=SCRAM-SHA-256,PLAIN\r\n" +
		":server CAP omairc ACK :sasl\r\n" +
		"AUTHENTICATE +\r\n")
	return decodeAuthenticatePayload(t, fixture.lastFrame())
}

// --- Registration and capability negotiation ------------------------------

// TestSessionRegistersAndAutojoins ports
// SessionTest::registersAndAutojoins.
func TestSessionRegistersAndAutojoins(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))

	fixture.connectTLS()
	if got := fixture.session.State(); got != StateCapLs {
		t.Fatalf("state = %v, want CapLs", got)
	}
	assertFramesEqual(t, fixture.frames(), []string{"CAP LS 302\r\n"})

	fixture.inject(":server CAP omairc LS :multi-prefix chghost cap-notify echo-message\r\n")
	if got := fixture.session.State(); got != StateRegistering {
		t.Fatalf("state = %v, want Registering", got)
	}
	assertFramesEqual(t, fixture.writtenSince(1), []string{
		"CAP REQ :multi-prefix chghost cap-notify echo-message\r\n",
		"NICK omairc\r\n",
		"USER omairc 8 * :Omairc User\r\n",
	})
	if !fixture.session.Capabilities().IsEmpty() {
		t.Fatal("capabilities must be empty before the ACK")
	}

	fixture.inject(":server CAP omairc ACK :multi-prefix chghost cap-notify echo-message\r\n")
	if !fixture.wrote("CAP END\r\n") {
		t.Fatal("CAP END must follow the ACK")
	}
	for _, capability := range []irc.Capability{
		irc.CapabilityMultiPrefix, irc.CapabilityChghost,
		irc.CapabilityCapNotify, irc.CapabilityEchoMessage,
	} {
		if !fixture.session.Capabilities().Contains(capability) {
			t.Fatalf("capabilities missing %v", capability)
		}
	}

	fixture.inject(":server 001 omairc :Welcome\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if got := fixture.session.Nick(); got != "omairc" {
		t.Fatalf("nick = %q, want omairc", got)
	}
	if fixture.handler.registered != 1 {
		t.Fatalf("registered = %d, want 1", fixture.handler.registered)
	}
	assertFramesEqual(t, fixture.writtenSince(5), []string{
		"JOIN #omarchy\r\n",
		"JOIN &local\r\n",
	})
}

// TestSessionNegotiatesPresenceCapabilities ports
// SessionTest::negotiatesPresenceCapabilities.
func TestSessionNegotiatesPresenceCapabilities(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.Password = "secret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS * :sasl=PLAIN away-notify\r\n" +
		":server CAP omairc LS :batch draft/metadata-2 multi-prefix\r\n")
	assertFramesEqual(t, fixture.frames(), []string{
		"CAP LS 302\r\n",
		"CAP REQ :sasl\r\n",
		"CAP REQ :away-notify batch draft/metadata-2 multi-prefix\r\n",
		"NICK omairc\r\n",
		"USER omairc 8 * :Omairc User\r\n",
	})

	fixture.inject(":server CAP omairc ACK :away-notify batch draft/metadata-2 multi-prefix\r\n")
	if fixture.wrote("CAP END\r\n") {
		t.Fatal("CAP END must wait for the SASL ACK")
	}

	fixture.inject(":server CAP omairc ACK :sasl\r\n" +
		"AUTHENTICATE +\r\n" +
		":server 903 omairc :SASL successful\r\n")
	if !fixture.wrote("CAP END\r\n") {
		t.Fatal("CAP END must follow the SASL 903")
	}

	fixture.inject(":server 001 omairc :Welcome\r\n")
	if !fixture.wrote("METADATA * SUB status avatar bot display-name pronouns homepage color\r\n") {
		t.Fatal("member metadata subscription missing")
	}

	fixture.inject(":server 366 omairc #omarchy :End of /NAMES\r\n")
	if !fixture.wrote("WHO #omarchy\r\n") {
		t.Fatal("away-notify probe missing")
	}

	enabled := fixture.session.Capabilities()
	for _, capability := range []irc.Capability{
		irc.CapabilityAwayNotify, irc.CapabilityBatch, irc.CapabilityMemberMetadata,
	} {
		if !enabled.Contains(capability) {
			t.Fatalf("capabilities missing %v", capability)
		}
	}
	if len(fixture.handler.caps) == 0 {
		t.Fatal("capabilitiesChanged must have fired")
	}
}

// TestSessionNegotiatesMessageTagsOnOwnLine ports
// SessionTest::negotiatesMessageTagsOnOwnLine.
func TestSessionNegotiatesMessageTagsOnOwnLine(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.Password = "secret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sasl=PLAIN message-tags " +
		"away-notify batch draft/metadata-2\r\n")
	assertFramesEqual(t, fixture.frames(), []string{
		"CAP LS 302\r\n",
		"CAP REQ :sasl\r\n",
		"CAP REQ :message-tags\r\n",
		"CAP REQ :away-notify batch draft/metadata-2\r\n",
		"NICK omairc\r\n",
		"USER omairc 8 * :Omairc User\r\n",
	})
}

// TestSessionRefusedPresenceCapabilitiesStayOffWithoutFailing ports
// SessionTest::refusedPresenceCapabilitiesStayOffWithoutFailing.
func TestSessionRefusedPresenceCapabilitiesStayOffWithoutFailing(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :away-notify batch draft/metadata-2\r\n")
	assertFramesEqual(t, fixture.writtenSince(1), []string{
		"CAP REQ :away-notify batch draft/metadata-2\r\n",
		"NICK omairc\r\n",
		"USER omairc 8 * :Omairc User\r\n",
	})

	fixture.inject(":server CAP omairc NAK :away-notify batch draft/metadata-2\r\n")
	if !fixture.wrote("CAP END\r\n") {
		t.Fatal("CAP END must follow the NAK")
	}
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
	if !fixture.session.Capabilities().IsEmpty() {
		t.Fatal("refused capabilities must stay off")
	}

	fixture.inject(":server 001 omairc :Welcome\r\n" +
		":server 366 omairc #omarchy :End of /NAMES\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if fixture.wrote("METADATA * SUB status avatar bot display-name pronouns homepage color\r\n") {
		t.Fatal("metadata subscription must not be sent")
	}
	if fixture.wrote("WHO #omarchy\r\n") {
		t.Fatal("away probe must not be sent")
	}
}

// TestSessionRefusedMessageTagsStayOffWithoutFailing ports
// SessionTest::refusedMessageTagsStayOffWithoutFailing.
func TestSessionRefusedMessageTagsStayOffWithoutFailing(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :message-tags away-notify\r\n")
	assertFramesEqual(t, fixture.writtenSince(1), []string{
		"CAP REQ :message-tags\r\n",
		"CAP REQ :away-notify\r\n",
		"NICK omairc\r\n",
		"USER omairc 8 * :Omairc User\r\n",
	})

	fixture.inject(":server CAP omairc NAK :message-tags\r\n")
	if fixture.wrote("CAP END\r\n") {
		t.Fatal("CAP END must wait for the outstanding away-notify")
	}
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
	if fixture.session.Capabilities().Contains(irc.CapabilityMessageTags) {
		t.Fatal("refused message-tags must stay off")
	}

	fixture.inject(":server CAP omairc ACK :away-notify\r\n")
	if !fixture.wrote("CAP END\r\n") {
		t.Fatal("CAP END must follow the away-notify ACK")
	}

	fixture.inject(":server 001 omairc :Welcome\r\n")
	if !fixture.session.Capabilities().Contains(irc.CapabilityAwayNotify) {
		t.Fatal("away-notify must be enabled")
	}
	if fixture.session.Capabilities().Contains(irc.CapabilityMessageTags) {
		t.Fatal("message-tags must stay off")
	}
}

// TestSessionUnansweredPresenceRequestStillRegisters ports
// SessionTest::unansweredPresenceRequestStillRegisters.
func TestSessionUnansweredPresenceRequestStillRegisters(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :away-notify\r\n")
	if fixture.clock.Pending() == 0 {
		t.Fatal("capability timer must be pending")
	}
	if fixture.wrote("CAP END\r\n") {
		t.Fatal("CAP END must wait for the ACK")
	}

	fixture.clock.Advance(10000 * time.Millisecond)
	if !fixture.wrote("CAP END\r\n") {
		t.Fatal("CAP END must follow the capability timeout")
	}
	if !fixture.session.Capabilities().IsEmpty() {
		t.Fatal("unanswered capabilities must stay off")
	}
}

// TestSessionLateCapAckAfterTimeoutIsIgnored ports
// SessionTest::lateCapAckAfterTimeoutIsIgnored.
func TestSessionLateCapAckAfterTimeoutIsIgnored(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :away-notify\r\n")
	fixture.clock.Advance(10000 * time.Millisecond)
	if !fixture.session.Capabilities().IsEmpty() {
		t.Fatal("capabilities must be empty after the timeout")
	}

	fixture.inject(":server CAP omairc ACK :away-notify\r\n" +
		":server 001 omairc :Welcome\r\n")
	if fixture.session.Capabilities().Contains(irc.CapabilityAwayNotify) {
		t.Fatal("a late ACK must not re-enable away-notify")
	}
}

// TestSessionLateCapNakAfterTimeoutIsIgnored ports
// SessionTest::lateCapNakAfterTimeoutIsIgnored.
func TestSessionLateCapNakAfterTimeoutIsIgnored(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.Password = "s3cret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sasl\r\n")
	fixture.clock.Advance(10000 * time.Millisecond)

	fixture.inject(":server CAP omairc NAK :sasl\r\n")
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
}

// TestSessionCapNakBeforeCapListIsIgnored ports
// SessionTest::capNakBeforeCapListIsIgnored.
func TestSessionCapNakBeforeCapListIsIgnored(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))

	fixture.connectTLS()
	fixture.inject(":server CAP omairc NAK :away-notify\r\n")
	if fixture.wrote("CAP END\r\n") {
		t.Fatal("a NAK before CAP LS must not register")
	}

	fixture.inject(":server CAP omairc LS :away-notify\r\n" +
		":server CAP omairc ACK :away-notify\r\n")
	if !fixture.wrote("CAP END\r\n") {
		t.Fatal("CAP END must follow the real ACK")
	}
	if !fixture.session.Capabilities().Contains(irc.CapabilityAwayNotify) {
		t.Fatal("away-notify must be enabled")
	}
}

// TestSessionRegistersWhenCapIsUnsupported ports
// SessionTest::registersWhenCapIsUnsupported.
func TestSessionRegistersWhenCapIsUnsupported(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))

	fixture.connectTLS()
	fixture.inject(":server 421 omairc CAP :Unknown command\r\n")
	assertFramesEqual(t, fixture.writtenSince(1), []string{
		"NICK omairc\r\n",
		"USER omairc 8 * :Omairc User\r\n",
	})

	fixture.inject(":server 001 omairc :Welcome\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
}

// TestSessionWithdrawnCapabilityIsPublished ports
// SessionTest::withdrawnCapabilityIsPublished.
func TestSessionWithdrawnCapabilityIsPublished(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :away-notify\r\n" +
		":server CAP omairc ACK :away-notify\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":server 366 omairc #omarchy :End of /NAMES\r\n")
	if !fixture.wrote("WHO #omarchy\r\n") {
		t.Fatal("away probe missing")
	}
	if !fixture.session.Capabilities().Contains(irc.CapabilityAwayNotify) {
		t.Fatal("away-notify must be enabled")
	}

	fixture.handler.caps = nil
	fixture.inject(":server CAP omairc DEL :away-notify\r\n" +
		":server 366 omairc #desktop :End of /NAMES\r\n")
	if len(fixture.handler.caps) != 1 {
		t.Fatalf("capabilitiesChanged = %d, want 1", len(fixture.handler.caps))
	}
	if fixture.session.Capabilities().Contains(irc.CapabilityAwayNotify) {
		t.Fatal("withdrawn away-notify must stay off")
	}
	if fixture.wrote("WHO #desktop\r\n") {
		t.Fatal("withdrawn away-notify must not probe")
	}
}
