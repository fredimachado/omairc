package session

import (
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// ctcpVersionReply renders the expected VERSION reply for nick.
func ctcpVersionReply(nick string) string {
	return "NOTICE " + nick + " :\x01VERSION https://omairc.app\x01\r\n"
}

func countFrame(frames []string, frame string) int {
	count := 0
	for _, candidate := range frames {
		if candidate == frame {
			count++
		}
	}
	return count
}

// --- Outbound writers -----------------------------------------------------

// TestSessionSendPrivmsgValidatesTarget ports
// SessionTest::sendPrivmsgValidatesTarget.
func TestSessionSendPrivmsgValidatesTarget(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}

	if !fixture.session.SendPrivmsg("#omarchy", "hello") {
		t.Fatal("channel PRIVMSG must succeed")
	}
	if !fixture.session.SendPrivmsg("lena", "hello") {
		t.Fatal("direct PRIVMSG must succeed")
	}
	if got := fixture.lastFrame(); got != "PRIVMSG lena :hello\r\n" {
		t.Fatalf("last frame = %q, want PRIVMSG lena", got)
	}
	if !fixture.wrote("PRIVMSG #omarchy :hello\r\n") {
		t.Fatal("channel frame missing")
	}

	before := len(fixture.frames())
	invalidTargets := []string{
		"",
		" ",
		"lena smith",
		"lena\t",
		"lena\n",
		"lena\r",
		"\x01lena",
		":lena",
	}
	for _, target := range invalidTargets {
		if fixture.session.SendPrivmsg(target, "hello") {
			t.Fatalf("sendPrivmsg(%q) must fail", target)
		}
	}
	if len(fixture.frames()) != before {
		t.Fatal("invalid targets must not write frames")
	}
}

// TestSessionSendCtcpWritesQuery ports SessionTest::sendCtcpWritesQuery.
func TestSessionSendCtcpWritesQuery(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}

	if !fixture.session.SendCtcp("lena", "VERSION", "", "") {
		t.Fatal("VERSION query must succeed")
	}
	if got := fixture.lastFrame(); got != "PRIVMSG lena :\x01VERSION\x01\r\n" {
		t.Fatalf("last frame = %q, want VERSION query", got)
	}
	if !fixture.session.SendCtcp("lena", "time", "", "") {
		t.Fatal("time query must succeed")
	}
	if got := fixture.lastFrame(); got != "PRIVMSG lena :\x01TIME\x01\r\n" {
		t.Fatalf("last frame = %q, want TIME query", got)
	}
	if !fixture.session.SendCtcp("lena", "PING", "42", "") {
		t.Fatal("PING query must succeed")
	}
	if got := fixture.lastFrame(); got != "PRIVMSG lena :\x01PING 42\x01\r\n" {
		t.Fatalf("last frame = %q, want PING query", got)
	}

	before := len(fixture.frames())
	if fixture.session.SendCtcp("", "VERSION", "", "") ||
		fixture.session.SendCtcp("lena smith", "VERSION", "", "") ||
		fixture.session.SendCtcp("lena", "", "", "") ||
		fixture.session.SendCtcp("lena", "PING PONG", "", "") {
		t.Fatal("invalid CTCP queries must fail")
	}
	if len(fixture.frames()) != before {
		t.Fatal("invalid CTCP queries must not write frames")
	}
}

// TestSessionSendPrivmsgSplitsNearTwoFrames ports
// SessionTest::sendPrivmsgSplitsNearTwoFrames.
func TestSessionSendPrivmsgSplitsNearTwoFrames(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.registerWithWelcome()
	before := len(fixture.frames())

	const prefix = "PRIVMSG #omarchy :"
	first := strings.Repeat("a", 400)
	second := strings.Repeat("b", 200)
	if !fixture.session.SendPrivmsg("#omarchy", first+" "+second) {
		t.Fatal("sendPrivmsg must succeed")
	}

	frames := fixture.writtenSince(before)
	if len(frames) != 2 {
		t.Fatalf("frames = %d, want 2", len(frames))
	}
	if frames[0] != prefix+first+"\r\n" {
		t.Fatalf("frame 0 = %q, want the first chunk", frames[0])
	}
	if frames[1] != prefix+second+"\r\n" {
		t.Fatalf("frame 1 = %q, want the second chunk", frames[1])
	}
	for _, frame := range frames {
		if !strings.HasSuffix(frame, "\r\n") || len(frame) > irc.MaxClassicFrameBytes {
			t.Fatalf("frame %q violates the frame ceiling", frame)
		}
	}
}

// TestSessionSendPrivmsgSplitsEnormousToken ports
// SessionTest::sendPrivmsgSplitsEnormousToken.
func TestSessionSendPrivmsgSplitsEnormousToken(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.registerWithWelcome()
	before := len(fixture.frames())

	const prefix = "PRIVMSG #omarchy :"
	maxBody := irc.MaxClassicFrameBytes - len(prefix) - 2
	token := strings.Repeat("x", maxBody*2+16)
	if !fixture.session.SendPrivmsg("#omarchy", token) {
		t.Fatal("sendPrivmsg must succeed")
	}

	frames := fixture.writtenSince(before)
	if len(frames) != 3 {
		t.Fatalf("frames = %d, want 3", len(frames))
	}
	if frames[0] != prefix+token[:maxBody]+"\r\n" {
		t.Fatalf("frame 0 = %q, want the first chunk", frames[0])
	}
	if frames[1] != prefix+token[maxBody:maxBody*2]+"\r\n" {
		t.Fatalf("frame 1 = %q, want the second chunk", frames[1])
	}
	if frames[2] != prefix+token[maxBody*2:]+"\r\n" {
		t.Fatalf("frame 2 = %q, want the third chunk", frames[2])
	}
	for _, frame := range frames {
		if !strings.HasSuffix(frame, "\r\n") || len(frame) > irc.MaxClassicFrameBytes {
			t.Fatalf("frame %q violates the frame ceiling", frame)
		}
	}
}

// TestSessionSetTopicIsSetOnly ports SessionTest::setTopicIsSetOnly.
func TestSessionSetTopicIsSetOnly(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}

	before := len(fixture.frames())
	if fixture.session.SetTopic("#omarchy", "") || fixture.session.SetTopic("", "hello") {
		t.Fatal("empty channel or topic must fail")
	}
	if len(fixture.frames()) != before {
		t.Fatal("invalid TOPIC must not write a frame")
	}
	for _, frame := range fixture.frames() {
		if strings.Contains(frame, "TOPIC") {
			t.Fatalf("unexpected TOPIC frame %q", frame)
		}
	}

	if !fixture.session.SetTopic("#omarchy", "hello") {
		t.Fatal("setTopic must succeed")
	}
	if got := fixture.lastFrame(); got != "TOPIC #omarchy :hello\r\n" {
		t.Fatalf("last frame = %q, want TOPIC", got)
	}
}

// TestSessionKickWritesOptionalReason ports
// SessionTest::kickWritesOptionalReason.
func TestSessionKickWritesOptionalReason(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n")

	before := len(fixture.frames())
	if fixture.session.Kick("", "alice", "spam") || fixture.session.Kick("#omarchy", "", "spam") {
		t.Fatal("empty channel or nick must fail")
	}
	if len(fixture.frames()) != before {
		t.Fatal("invalid KICK must not write a frame")
	}
	for _, frame := range fixture.frames() {
		if strings.Contains(frame, "KICK") {
			t.Fatalf("unexpected KICK frame %q", frame)
		}
	}

	if !fixture.session.Kick("#omarchy", "alice", "spam") {
		t.Fatal("kick with a reason must succeed")
	}
	if got := fixture.lastFrame(); got != "KICK #omarchy alice :spam\r\n" {
		t.Fatalf("last frame = %q, want KICK with a reason", got)
	}
	if !fixture.session.Kick("#omarchy", "alice", "") {
		t.Fatal("kick without a reason must succeed")
	}
	if got := fixture.lastFrame(); got != "KICK #omarchy alice\r\n" {
		t.Fatalf("last frame = %q, want KICK without a reason", got)
	}
}

// TestSessionInviteWritesNickThenChannel ports
// SessionTest::inviteWritesNickThenChannel.
func TestSessionInviteWritesNickThenChannel(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n")

	before := len(fixture.frames())
	if fixture.session.Invite("", "#lab") || fixture.session.Invite("bob", "") {
		t.Fatal("empty nick or channel must fail")
	}
	if len(fixture.frames()) != before {
		t.Fatal("invalid INVITE must not write a frame")
	}
	for _, frame := range fixture.frames() {
		if strings.Contains(frame, "INVITE") {
			t.Fatalf("unexpected INVITE frame %q", frame)
		}
	}

	if !fixture.session.Invite("bob", "#lab") {
		t.Fatal("invite must succeed")
	}
	if got := fixture.lastFrame(); got != "INVITE bob #lab\r\n" {
		t.Fatalf("last frame = %q, want INVITE", got)
	}
}

// TestSessionSetAwayEncodesOptionalReason ports
// SessionTest::setAwayEncodesOptionalReason.
func TestSessionSetAwayEncodesOptionalReason(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n")

	steps := []struct {
		call func() bool
		want string
	}{
		{func() bool { return fixture.session.SetAway("lunch") }, "AWAY :lunch\r\n"},
		{func() bool { return fixture.session.SetAway("") }, "AWAY\r\n"},
		{func() bool { return fixture.session.SetAway("   ") }, "AWAY\r\n"},
		{func() bool { return fixture.session.ClearAway() }, "AWAY\r\n"},
		{func() bool { return fixture.session.MarkAway("") }, "AWAY :\r\n"},
		{func() bool { return fixture.session.MarkAway("   ") }, "AWAY :\r\n"},
		{func() bool { return fixture.session.MarkAway("lunch") }, "AWAY :lunch\r\n"},
	}
	for index, step := range steps {
		if !step.call() {
			t.Fatalf("step %d failed", index)
		}
		if got := fixture.lastFrame(); got != step.want {
			t.Fatalf("step %d frame = %q, want %q", index, got, step.want)
		}
	}
}

// TestSessionSetOwnMetadataWritesSetAndClear ports
// SessionTest::setOwnMetadataWritesSetAndClear.
func TestSessionSetOwnMetadataWritesSetAndClear(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :batch draft/metadata-2\r\n" +
		":server CAP omairc ACK :batch draft/metadata-2\r\n" +
		":server 001 omairc :Welcome\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}

	if _, ok := fixture.session.SetOwnMetadata("status", "writing"); !ok {
		t.Fatal("setOwnMetadata must succeed")
	}
	if got := fixture.lastFrame(); got != "METADATA * SET status :writing\r\n" {
		t.Fatalf("last frame = %q, want METADATA SET", got)
	}
	if !fixture.session.ClearOwnMetadata("status") {
		t.Fatal("clearOwnMetadata must succeed")
	}
	if got := fixture.lastFrame(); got != "METADATA * SET status\r\n" {
		t.Fatalf("last frame = %q, want METADATA clear", got)
	}

	afterClear := len(fixture.frames())
	if _, ok := fixture.session.SetOwnMetadata("unknown", "x"); ok {
		t.Fatal("an unknown key must fail")
	}
	if _, ok := fixture.session.SetOwnMetadata("", "x"); ok {
		t.Fatal("an empty key must fail")
	}
	if len(fixture.frames()) != afterClear {
		t.Fatal("invalid metadata must not write a frame")
	}

	withoutCap := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	withoutCap.connectTLS()
	withoutCap.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n")
	before := len(withoutCap.frames())
	if _, ok := withoutCap.session.SetOwnMetadata("status", "writing"); ok {
		t.Fatal("setOwnMetadata must fail without the capability")
	}
	if withoutCap.session.ClearOwnMetadata("status") {
		t.Fatal("clearOwnMetadata must fail without the capability")
	}
	if len(withoutCap.frames()) != before {
		t.Fatal("metadata without the capability must not write frames")
	}
}

// TestSessionMetadataCapabilityLimitsSubscriptionsAndValues ports
// SessionTest::metadataCapabilityLimitsSubscriptionsAndValues.
func TestSessionMetadataCapabilityLimitsSubscriptionsAndValues(t *testing.T) {
	limited := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	limited.connectTLS()
	limited.inject(":server CAP omairc LS :batch " +
		"draft/metadata-2=max-subs=2,max-value-bytes=8\r\n" +
		":server CAP omairc ACK :batch draft/metadata-2\r\n" +
		":server 001 omairc :Welcome\r\n")
	if got := limited.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	metadata := limited.session.MetadataCapability()
	if metadata.MaxSubs == nil || *metadata.MaxSubs != 2 {
		t.Fatalf("maxSubs = %v, want 2", metadata.MaxSubs)
	}
	if metadata.MaxValueBytes == nil || *metadata.MaxValueBytes != 8 {
		t.Fatalf("maxValueBytes = %v, want 8", metadata.MaxValueBytes)
	}
	if !framesContain(limited.frames(), "METADATA * SUB status avatar\r\n") {
		t.Fatal("the limited subscription list is missing")
	}
	if framesContain(limited.frames(), "METADATA * SUB status avatar bot") {
		t.Fatal("the subscription list must be truncated")
	}

	if _, ok := limited.session.SetOwnMetadata("status", "abcdefghijk"); !ok {
		t.Fatal("setOwnMetadata must succeed")
	}
	if got := limited.lastFrame(); got != "METADATA * SET status :abcdefgh\r\n" {
		t.Fatalf("last frame = %q, want an 8-byte clamp", got)
	}

	if _, ok := limited.session.SetOwnMetadata("status", "café!!!!"); !ok {
		t.Fatal("setOwnMetadata must succeed")
	}
	last := limited.lastFrame()
	prefix := "METADATA * SET status :"
	if !strings.HasPrefix(last, prefix) || !strings.HasSuffix(last, "\r\n") {
		t.Fatalf("last frame = %q, want the METADATA prefix and CRLF", last)
	}
	if got := len(strings.TrimSuffix(last[len(prefix):], "\r\n")); got != 8 {
		t.Fatalf("clamped value = %d bytes, want 8", got)
	}

	zeroSubs := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	zeroSubs.connectTLS()
	zeroSubs.inject(":server CAP omairc LS :batch draft/metadata-2=max-subs=0\r\n" +
		":server CAP omairc ACK :batch draft/metadata-2\r\n" +
		":server 001 omairc :Welcome\r\n")
	if framesContain(zeroSubs.frames(), "METADATA * SUB") {
		t.Fatal("max-subs=0 must not subscribe")
	}

	malformed := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	malformed.connectTLS()
	malformed.inject(":server CAP omairc LS :batch " +
		"draft/metadata-2=max-subs=-3,max-value-bytes=nope,max-value-bytes=99999\r\n" +
		":server CAP omairc ACK :batch draft/metadata-2\r\n" +
		":server 001 omairc :Welcome\r\n")
	if got := malformed.session.MetadataCapability().MaxSubs; got != nil {
		t.Fatalf("maxSubs = %v, want nil", got)
	}
	if got := malformed.session.MetadataCapability().MaxValueBytes; got == nil || *got != 99999 {
		t.Fatalf("maxValueBytes = %v, want 99999", got)
	}
	if !framesContain(malformed.frames(),
		"METADATA * SUB status avatar bot display-name pronouns homepage color\r\n") {
		t.Fatal("the full subscription list is missing")
	}
	if got := irc.EffectiveMaxValueBytes(malformed.session.MetadataCapability().MaxValueBytes); got != irc.MaximumValueBytes {
		t.Fatalf("effective max = %d, want %d", got, irc.MaximumValueBytes)
	}
	huge := strings.Repeat("x", 600)
	if got := len(irc.Clamped(huge)); got != irc.MaximumValueBytes {
		t.Fatalf("clamped huge = %d, want %d", got, irc.MaximumValueBytes)
	}
	if _, ok := malformed.session.SetOwnMetadata("status", huge); !ok {
		t.Fatal("setOwnMetadata must succeed")
	}
	hugeFrame := malformed.lastFrame()
	setPrefix := "METADATA * SET status :"
	wireBudget := irc.MaxClassicFrameBytes - len(setPrefix) - 2
	want := irc.MaximumValueBytes
	if wireBudget < want {
		want = wireBudget
	}
	if got := len(strings.TrimSuffix(hugeFrame[len(setPrefix):], "\r\n")); got != want {
		t.Fatalf("huge value = %d bytes, want %d", got, want)
	}
	if got := len(hugeFrame); got != irc.MaxClassicFrameBytes {
		t.Fatalf("huge frame = %d bytes, want %d", got, irc.MaxClassicFrameBytes)
	}

	legacy := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	legacy.connectTLS()
	legacy.inject(":server CAP omairc LS :batch draft/metadata-2\r\n" +
		":server CAP omairc ACK :batch draft/metadata-2\r\n" +
		":server 001 omairc :Welcome\r\n")
	if legacy.session.MetadataCapability().MaxSubs != nil ||
		legacy.session.MetadataCapability().MaxValueBytes != nil {
		t.Fatal("legacy metadata limits must be unset")
	}
	if !framesContain(legacy.frames(),
		"METADATA * SUB status avatar bot display-name pronouns homepage color\r\n") {
		t.Fatal("the legacy subscription list is missing")
	}

	zeroValue := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	zeroValue.connectTLS()
	zeroValue.inject(":server CAP omairc LS :batch draft/metadata-2=max-value-bytes=0\r\n" +
		":server CAP omairc ACK :batch draft/metadata-2\r\n" +
		":server 001 omairc :Welcome\r\n")
	if got := zeroValue.session.MetadataCapability().MaxValueBytes; got == nil || *got != 0 {
		t.Fatalf("maxValueBytes = %v, want 0", got)
	}
	if got := irc.EffectiveMaxValueBytes(zeroValue.session.MetadataCapability().MaxValueBytes); got != 0 {
		t.Fatalf("effective max = %d, want 0", got)
	}
	beforeZeroSet := len(zeroValue.frames())
	if _, ok := zeroValue.session.SetOwnMetadata("status", "blocked"); ok {
		t.Fatal("a zero value ceiling must block non-empty values")
	}
	if len(zeroValue.frames()) != beforeZeroSet {
		t.Fatal("the blocked value must not write a frame")
	}
	if !zeroValue.session.ClearOwnMetadata("status") {
		t.Fatal("clearing must still succeed")
	}
	if got := zeroValue.lastFrame(); got != "METADATA * SET status\r\n" {
		t.Fatalf("last frame = %q, want METADATA clear", got)
	}
}

// TestSessionWhoisWritesDoubledNick ports
// SessionTest::whoisWritesDoubledNick.
func TestSessionWhoisWritesDoubledNick(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}

	if !fixture.session.Whois("lena", "") {
		t.Fatal("whois must succeed")
	}
	if got := fixture.lastFrame(); got != "WHOIS lena lena\r\n" {
		t.Fatalf("last frame = %q, want WHOIS lena lena", got)
	}

	before := len(fixture.frames())
	if fixture.session.Whois("", "") || fixture.session.Whois("   ", "") {
		t.Fatal("empty nick must fail")
	}
	if len(fixture.frames()) != before {
		t.Fatal("an empty WHOIS must not write a frame")
	}
}

// --- CTCP handling --------------------------------------------------------

// TestSessionCtcpFromServerPrefixGetsNoReply ports
// SessionTest::ctcpFromServerPrefixGetsNoReply.
func TestSessionCtcpFromServerPrefixGetsNoReply(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server 001 omairc :Welcome\r\n")

	fixture.inject(":services.example.net PRIVMSG omairc :\x01VERSION\x01\r\n" +
		":services PRIVMSG omairc :\x01VERSION\x01\r\n" +
		":NickServ!NickServ@services PRIVMSG omairc :\x01VERSION\x01\r\n")
	for _, nick := range []string{"services.example.net", "services", "NickServ"} {
		if fixture.wrote(ctcpVersionReply(nick)) {
			t.Fatalf("a server-prefixed CTCP must not be answered for %q", nick)
		}
	}
}

// TestSessionCtcpToFoldedSelfNickIsAnswered ports
// SessionTest::ctcpToFoldedSelfNickIsAnswered.
func TestSessionCtcpToFoldedSelfNickIsAnswered(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.Nick = "omairc[m]"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server 001 omairc[m] :Welcome\r\n" +
		":server 005 omairc[m] CASEMAPPING=rfc1459 :are supported\r\n")

	fixture.inject(":alice!u@h PRIVMSG omairc{m} :\x01PING one\x01\r\n")
	if !fixture.wrote("NOTICE alice :\x01PING one\x01\r\n") {
		t.Fatal("a folded self nick must be answered")
	}
}

// TestSessionAnswersCtcpRequests ports SessionTest::answersCtcpRequests.
func TestSessionAnswersCtcpRequests(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server 001 omairc :Welcome\r\n")

	fixture.inject(":MetaNova!u@h PRIVMSG omairc :\x01PING token\x01\r\n" +
		":alice!u@h PRIVMSG omairc :\x01TIME\x01\r\n" +
		":bob!u@h PRIVMSG omairc :\x01VERSION\x01\r\n")

	if !fixture.wrote("NOTICE MetaNova :\x01PING token\x01\r\n") {
		t.Fatal("the PING request must be echoed")
	}
	if !fixture.wrote(ctcpVersionReply("bob")) {
		t.Fatal("the VERSION request must be answered")
	}
	wroteTime := false
	for _, frame := range fixture.frames() {
		if strings.HasPrefix(frame, "NOTICE alice :\x01TIME ") {
			wroteTime = true
		}
	}
	if !wroteTime {
		t.Fatal("the TIME request must be answered")
	}
}

// TestSessionAnswersNickOnlyCtcpRequests ports
// SessionTest::answersNickOnlyCtcpRequests.
func TestSessionAnswersNickOnlyCtcpRequests(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server 001 omairc :Welcome\r\n")

	fixture.inject(":lena PRIVMSG omairc :\x01VERSION\x01\r\n")
	if !fixture.wrote(ctcpVersionReply("lena")) {
		t.Fatal("a nick-only prefix must be answered")
	}
}

// TestSessionRateLimitsCtcpVersionRepliesPerNick ports
// SessionTest::rateLimitsCtcpVersionRepliesPerNick.
func TestSessionRateLimitsCtcpVersionRepliesPerNick(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server 001 omairc :Welcome\r\n")

	probe := ":MetaNova!u@h PRIVMSG omairc :\x01VERSION\x01\r\n"
	reply := ctcpVersionReply("MetaNova")
	otherProbe := ":alice!u@h PRIVMSG omairc :\x01VERSION\x01\r\n"
	otherReply := ctcpVersionReply("alice")

	fixture.inject(probe + probe)
	if got := countFrame(fixture.frames(), reply); got != 1 {
		t.Fatalf("MetaNova replies = %d, want 1", got)
	}

	fixture.inject(otherProbe)
	if got := countFrame(fixture.frames(), otherReply); got != 1 {
		t.Fatalf("alice replies = %d, want 1", got)
	}

	ctcpEntries := func() int {
		count := 0
		for _, entry := range fixture.handler.status {
			if entry.Label() == "CTCP" {
				count++
			}
		}
		return count
	}
	if got := ctcpEntries(); got != 3 {
		t.Fatalf("CTCP status entries = %d, want 3", got)
	}

	// The C++ test sleeps 5500ms; advance the fake clock instead.
	fixture.clock.Advance(5500 * time.Millisecond)
	fixture.inject(probe)
	if got := countFrame(fixture.frames(), reply); got != 2 {
		t.Fatalf("MetaNova replies after the interval = %d, want 2", got)
	}
	if got := ctcpEntries(); got != 4 {
		t.Fatalf("CTCP status entries = %d, want 4", got)
	}
}

// TestSessionDropsOversizedCtcpPingPayload ports
// SessionTest::dropsOversizedCtcpPingPayload.
func TestSessionDropsOversizedCtcpPingPayload(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server 001 omairc :Welcome\r\n")

	tooLong := strings.Repeat("x", 33)
	exact := strings.Repeat("y", 32)
	fixture.inject(":MetaNova!u@h PRIVMSG omairc :\x01PING " + tooLong + "\x01\r\n")
	if fixture.wrote("NOTICE MetaNova :\x01PING " + tooLong + "\x01\r\n") {
		t.Fatal("an oversized PING must not be echoed")
	}
	if fixture.wrote("NOTICE MetaNova :\x01PING " + strings.Repeat("x", 32) + "\x01\r\n") {
		t.Fatal("a truncated PING must not be echoed")
	}
	if !fixture.handler.hasLabel("CTCP") || !fixture.handler.anyFieldContains("PING") {
		t.Fatal("the oversized PING must still reach Status")
	}

	fixture.inject(":MetaNova!u@h PRIVMSG omairc :\x01PING " + exact + "\x01\r\n")
	if !fixture.wrote("NOTICE MetaNova :\x01PING " + exact + "\x01\r\n") {
		t.Fatal("a 32-byte PING must be echoed")
	}
}

// TestSessionDoesNotAnswerChannelCtcpRequests ports
// SessionTest::doesNotAnswerChannelCtcpRequests.
func TestSessionDoesNotAnswerChannelCtcpRequests(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server 001 omairc :Welcome\r\n")
	before := len(fixture.frames())

	fixture.inject(":MetaNova!u@h PRIVMSG #omarchy :\x01PING token\x01\r\n")
	if len(fixture.frames()) != before {
		t.Fatal("a channel CTCP must not be answered")
	}
}

// --- Nick handling --------------------------------------------------------

// TestSessionWelcomeAssignsNickFrom001 ports
// SessionTest::welcomeAssignsNickFrom001.
func TestSessionWelcomeAssignsNickFrom001(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.Nick = "omairc-very-long-name"
	fixture := newSessionFixture(t, config)
	if got := fixture.session.Nick(); got != "omairc-very-long-name" {
		t.Fatalf("nick = %q, want the configured nick", got)
	}

	fixture.connectTLS()
	fixture.inject(":server CAP omairc-very-long-name LS :multi-prefix\r\n" +
		":server 001 omairc-truncated :Welcome\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if got := fixture.session.Nick(); got != "omairc-truncated" {
		t.Fatalf("nick = %q, want omairc-truncated", got)
	}
}

// TestSessionEmptyWelcomeKeepsConfigNick ports
// SessionTest::emptyWelcomeKeepsConfigNick.
func TestSessionEmptyWelcomeKeepsConfigNick(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	if got := fixture.session.Nick(); got != "omairc" {
		t.Fatalf("nick = %q, want omairc", got)
	}

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if got := fixture.session.Nick(); got != "omairc" {
		t.Fatalf("nick = %q, want omairc", got)
	}
}

// TestSessionSelfNickUpdatesSessionNick ports
// SessionTest::selfNickUpdatesSessionNick.
func TestSessionSelfNickUpdatesSessionNick(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.Nick = "omairc-very-long-name"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc-very-long-name LS :multi-prefix\r\n" +
		":server 001 omairc-truncated :Welcome\r\n" +
		":Alice!u@h NICK :Alicia\r\n")
	if got := fixture.session.Nick(); got != "omairc-truncated" {
		t.Fatalf("nick = %q, want omairc-truncated", got)
	}

	fixture.inject(":omairc-truncated!u@h NICK :fred\r\n")
	if got := fixture.session.Nick(); got != "fred" {
		t.Fatalf("nick = %q, want fred", got)
	}
}
