package session

import (
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

// TestSessionPlaintextStsPortReconnectsWithTlsAndCachesOnSecureDuration.
// SessionTest::plaintextStsPortReconnectsWithTlsAndCachesOnSecureDuration /
// client/src/session/tests/sts.rs::plaintext_sts_port_reconnects_with_tls_and_caches_on_secure_duration.
func TestSessionPlaintextStsPortReconnectsWithTlsAndCachesOnSecureDuration(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestPlaintextConfig())
	fixture.session.Start()
	fixture.transport.CompleteConnect()
	if fixture.transport.TLSRequested() {
		t.Fatal("the first connect must be plaintext")
	}
	if got := fixture.transport.ConnectedPort(); got != 6667 {
		t.Fatalf("port = %d, want 6667", got)
	}

	fixture.inject(":server CAP omairc LS :sts=port=6697,duration=60 multi-prefix\r\n" +
		":server 001 omairc :should-not-register-plaintext\r\n")

	if !fixture.transport.TLSRequested() {
		t.Fatal("the STS upgrade must request TLS")
	}
	if got := fixture.transport.ConnectedPort(); got != 6697 {
		t.Fatalf("port = %d, want 6697", got)
	}
	if got := fixture.session.Port(); got != 6697 {
		t.Fatalf("session port = %d, want 6697", got)
	}
	if !fixture.session.TLSEnabled() {
		t.Fatal("tlsEnabled must be true after the upgrade")
	}
	if fixture.wrote("CAP REQ :multi-prefix\r\n") || fixture.wrote("NICK omairc\r\n") {
		t.Fatal("the plaintext connection must not finish registering")
	}
	if !fixture.handler.hasLabel("sts") {
		t.Fatal("the STS upgrade must be logged")
	}
	if !fixture.handler.anyFieldContains("Upgraded to TLS on port 6697") {
		t.Fatal("the STS upgrade text is missing")
	}

	fixture.transport.CompleteConnect()
	fixture.inject(":server CAP omairc LS :sts=duration=60\r\n" +
		":server 001 omairc :Welcome\r\n")
	if got := capLsCount(fixture.frames()); got != 2 {
		t.Fatalf("CAP LS count = %d, want 2", got)
	}
	cached, ok := fixture.session.sts.Lookup("irc.example", fixture.clock.Now())
	if !ok {
		t.Fatal("the STS policy must be cached after the secure connect")
	}
	if cached.Port != 6697 {
		t.Fatalf("cached port = %d, want 6697", cached.Port)
	}
}

// TestSessionPlaintextStsWithoutPortStaysPlaintext.
// SessionTest::plaintextStsWithoutPortStaysPlaintext /
// client/src/session/tests/sts.rs::plaintext_sts_without_port_stays_plaintext.
func TestSessionPlaintextStsWithoutPortStaysPlaintext(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestPlaintextConfig())
	fixture.session.Start()
	fixture.transport.CompleteConnect()
	fixture.inject(":server CAP omairc LS :sts=duration=60\r\n" +
		":server 001 omairc :Welcome\r\n")
	if fixture.transport.TLSRequested() {
		t.Fatal("an STS advertisement without a port must not upgrade")
	}
	if got := fixture.transport.ConnectedPort(); got != 6667 {
		t.Fatalf("port = %d, want 6667", got)
	}
	if !fixture.wrote("NICK omairc\r\n") {
		t.Fatal("the session must register in plaintext")
	}
}

// TestSessionPlaintextCapNewStsPortReconnectsWithTls.
// SessionTest::plaintextCapNewStsPortReconnectsWithTls /
// client/src/session/tests/sts.rs::plaintext_cap_new_sts_port_reconnects_with_tls.
func TestSessionPlaintextCapNewStsPortReconnectsWithTls(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestPlaintextConfig())
	fixture.session.Start()
	fixture.transport.CompleteConnect()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n")
	if !fixture.wrote("CAP REQ :multi-prefix\r\n") {
		t.Fatal("multi-prefix must be requested")
	}
	fixture.inject(":server CAP omairc NEW :sts=port=6697\r\n")
	if !fixture.transport.TLSRequested() {
		t.Fatal("a CAP NEW STS port must upgrade")
	}
	if got := fixture.transport.ConnectedPort(); got != 6697 {
		t.Fatalf("port = %d, want 6697", got)
	}
}

// TestSessionTlsStsDurationRefreshesCacheWithoutReconnect.
// SessionTest::tlsStsDurationRefreshesCacheWithoutReconnect /
// client/src/session/tests/sts.rs::tls_sts_duration_refreshes_cache_without_reconnect.
func TestSessionTlsStsDurationRefreshesCacheWithoutReconnect(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.AutojoinChannels = nil
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sts=duration=86400\r\n")
	if got := capLsCount(fixture.frames()); got != 1 {
		t.Fatalf("CAP LS count = %d, want 1", got)
	}
	if !fixture.transport.TLSRequested() {
		t.Fatal("TLS must stay requested")
	}
	if got := fixture.transport.ConnectedPort(); got != 6697 {
		t.Fatalf("port = %d, want 6697", got)
	}
	cached, ok := fixture.session.sts.Lookup("irc.example", fixture.clock.Now())
	if !ok {
		t.Fatal("the STS duration must refresh the cache")
	}
	if cached.Port != 6697 || cached.DurationSeconds != 86400 {
		t.Fatalf("cached = %+v, want port 6697 duration 86400", cached)
	}
}

// TestSessionTlsStsDurationZeroClearsCache.
// SessionTest::tlsStsDurationZeroClearsCache /
// client/src/session/tests/sts.rs::tls_sts_duration_zero_clears_cache.
func TestSessionTlsStsDurationZeroClearsCache(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.AutojoinChannels = nil
	fixture := newSessionFixture(t, config)
	_ = fixture.session.sts.Store("irc.example", 6697, 60, fixture.clock.Now())

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sts=duration=0\r\n" +
		":server 001 omairc :Welcome\r\n")
	if _, ok := fixture.session.sts.Lookup("irc.example", fixture.clock.Now()); ok {
		t.Fatal("a zero duration must clear the cache")
	}
}

// TestSessionCachedHostOpensTlsWhenProfileDisablesIt.
// SessionTest::cachedHostOpensTlsWhenProfileDisablesIt /
// client/src/session/tests/sts.rs::cached_host_opens_tls_when_profile_disables_it.
func TestSessionCachedHostOpensTlsWhenProfileDisablesIt(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestPlaintextConfig())
	_ = fixture.session.sts.Store("irc.example", 6697, 60, fixture.clock.Now())

	fixture.session.Start()
	if !fixture.transport.TLSRequested() {
		t.Fatal("a cached policy must force TLS")
	}
	if got := fixture.transport.ConnectedPort(); got != 6697 {
		t.Fatalf("port = %d, want 6697", got)
	}
	if !fixture.session.TLSEnabled() {
		t.Fatal("tlsEnabled must be true")
	}
	if got := fixture.session.Port(); got != 6697 {
		t.Fatalf("session port = %d, want 6697", got)
	}
}

// TestSessionCapDelStsDoesNotClearCache.
// SessionTest::capDelStsDoesNotClearCache /
// client/src/session/tests/sts.rs::cap_del_sts_does_not_clear_cache.
func TestSessionCapDelStsDoesNotClearCache(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.AutojoinChannels = nil
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sts=duration=86400\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":server CAP omairc DEL :sts\r\n")
	cached, ok := fixture.session.sts.Lookup("irc.example", fixture.clock.Now())
	if !ok {
		t.Fatal("CAP DEL must not clear the STS cache")
	}
	if cached.Port != 6697 {
		t.Fatalf("cached port = %d, want 6697", cached.Port)
	}
}

// TestSessionExpiredCachedPolicyDoesNotUpgradeOnStart.
// client/src/session/tests/sts.rs::an_expired_cached_policy_does_not_upgrade_on_start.
func TestSessionExpiredCachedPolicyDoesNotUpgradeOnStart(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestPlaintextConfig())
	_ = fixture.session.sts.Store("irc.example", 6697, 60, fixture.clock.Now())
	fixture.clock.Advance(61 * time.Second)

	fixture.session.Start()
	if fixture.transport.TLSRequested() {
		t.Fatal("an expired policy must not force TLS")
	}
	if got := fixture.transport.ConnectedPort(); got != 6667 {
		t.Fatalf("port = %d, want 6667", got)
	}
}

// TestSessionDroppedSecureConnectionRefreshesExpiry.
// client/src/session/tests/sts.rs::a_dropped_secure_connection_refreshes_the_expiry.
func TestSessionDroppedSecureConnectionRefreshesExpiry(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.AutojoinChannels = nil
	fixture := newSessionFixture(t, config)
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sts=duration=60\r\n")
	fixture.clock.Advance(30 * time.Second)

	fixture.transport.RemoteClose()

	cached, ok := fixture.session.sts.Lookup("irc.example", fixture.clock.Now())
	if !ok {
		t.Fatal("the policy is still valid")
	}
	if cached.ExpiryEpochSeconds != 90 {
		t.Fatalf("expiry = %d, want 90 (30s in plus 60s duration)", cached.ExpiryEpochSeconds)
	}
}

// TestSessionStoppingRefreshesExpiry.
// client/src/session/tests/sts.rs::stopping_the_session_refreshes_the_expiry.
func TestSessionStoppingRefreshesExpiry(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.AutojoinChannels = nil
	fixture := newSessionFixture(t, config)
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sts=duration=60\r\n")
	fixture.clock.Advance(30 * time.Second)

	fixture.session.Stop()

	cached, ok := fixture.session.sts.Lookup("irc.example", fixture.clock.Now())
	if !ok {
		t.Fatal("the policy is still valid")
	}
	if cached.ExpiryEpochSeconds != 90 {
		t.Fatalf("expiry = %d, want 90", cached.ExpiryEpochSeconds)
	}
}

// TestSessionManagerNetworksSharePolicyCache.
// client/src/session/tests/sts.rs::networks_on_one_manager_share_the_policy_cache.
func TestSessionManagerNetworksSharePolicyCache(t *testing.T) {
	manager := NewManager()
	clock := NewFakeClock(time.Unix(0, 0))
	learnerTransport := NewLoopbackTransport()
	tlsConfig := sessionTestConfig("learner")
	tlsConfig.AutojoinChannels = nil
	learner, err := manager.Create(tlsConfig, learnerTransport, clock)
	if err != nil {
		t.Fatal(err)
	}
	learner.Start()
	learnerTransport.CompleteConnect()
	learnerTransport.InjectBytes([]byte(":server CAP omairc LS :sts=duration=60\r\n"))

	followerTransport := NewLoopbackTransport()
	followerConfig := sessionTestPlaintextConfig()
	followerConfig.NetworkID = "follower"
	follower, err := manager.Create(followerConfig, followerTransport, clock)
	if err != nil {
		t.Fatal(err)
	}
	follower.Start()
	if !followerTransport.TLSRequested() {
		t.Fatal("the other network reuses the cached policy")
	}
	if got := followerTransport.ConnectedPort(); got != 6697 {
		t.Fatalf("port = %d, want 6697", got)
	}
}

// TestSessionPolicyLearnedBeforeRestartAppliesAfterIt.
// client/src/session/tests/sts.rs::a_policy_learned_before_a_restart_applies_after_it.
func TestSessionPolicyLearnedBeforeRestartAppliesAfterIt(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	clock := NewFakeClock(time.Unix(0, 0))

	firstTransport := NewLoopbackTransport()
	firstSTS := NewSTSStore()
	firstSTS.AttachDisk(storage.NewIniSTSStore(path))
	tlsConfig := sessionTestConfig(sessionTestNetworkID)
	tlsConfig.AutojoinChannels = nil
	first := NewSessionWithSTS(tlsConfig, firstTransport, clock, firstSTS)
	first.Start()
	firstTransport.CompleteConnect()
	firstTransport.InjectBytes([]byte(":server CAP omairc LS :sts=duration=60\r\n"))

	secondTransport := NewLoopbackTransport()
	secondSTS := NewSTSStore()
	secondSTS.AttachDisk(storage.NewIniSTSStore(path))
	second := NewSessionWithSTS(sessionTestPlaintextConfig(), secondTransport, clock, secondSTS)
	second.Start()
	if !secondTransport.TLSRequested() {
		t.Fatal("the saved policy forces TLS after a restart")
	}
	if got := secondTransport.ConnectedPort(); got != 6697 {
		t.Fatalf("port = %d, want 6697", got)
	}
}

// TestSessionCorruptSTSFileDoesNotWeakenInMemoryPolicy.
// Acceptance: a corrupt file must not cause a plaintext connect for a host
// with a valid in-memory policy.
func TestSessionCorruptSTSFileDoesNotWeakenInMemoryPolicy(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "sts")
	if err := os.WriteFile(path, []byte("this line has no equals sign\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	fixture := newSessionFixture(t, sessionTestPlaintextConfig())
	fixture.session.sts.AttachDisk(storage.NewIniSTSStore(path))
	_ = fixture.session.sts.Store("irc.example", 6697, 60, fixture.clock.Now())

	fixture.session.Start()
	if !fixture.transport.TLSRequested() {
		t.Fatal("in-memory policy must still force TLS when the file is corrupt")
	}
	if got := fixture.transport.ConnectedPort(); got != 6697 {
		t.Fatalf("port = %d, want 6697", got)
	}
}

// TestSessionAnswersPingImmediately.
// SessionTest::answersPingImmediately.
func TestSessionAnswersPingImmediately(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject("PING :server-token\r\n")
	if got := fixture.lastFrame(); got != "PONG :server-token\r\n" {
		t.Fatalf("last frame = %q, want PONG", got)
	}
	if fixture.clock.Pending() != 0 {
		t.Fatal("no watchdog is armed before registration")
	}
}

// TestSessionSilentSocketAfterWelcomeSendsClientPing.
// SessionTest::silentSocketAfterWelcomeSendsClientPing.
func TestSessionSilentSocketAfterWelcomeSendsClientPing(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.registerWithWelcome()
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if fixture.clock.Pending() == 0 {
		t.Fatal("the ping watchdog must be armed")
	}

	fixture.inject(":bob!u@h PRIVMSG #omarchy :hi\r\n")

	before := len(fixture.frames())
	fixture.clock.Advance(60000 * time.Millisecond)
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	assertFramesEqual(t, fixture.writtenSince(before), []string{"PING :omairc-watchdog\r\n"})
	if fixture.clock.Pending() == 0 {
		t.Fatal("the probe must re-arm the watchdog")
	}
}

// TestSessionUnansweredClientPingReconnects.
// SessionTest::unansweredClientPingReconnects.
func TestSessionUnansweredClientPingReconnects(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.registerWithWelcome()
	fixture.clock.Advance(60000 * time.Millisecond)
	if got := fixture.lastFrame(); got != "PING :omairc-watchdog\r\n" {
		t.Fatalf("last frame = %q, want the watchdog probe", got)
	}

	fixture.clock.Advance(60000 * time.Millisecond)

	last := fixture.handler.errors[len(fixture.handler.errors)-1]
	if last.kind != ErrorNetwork {
		t.Fatalf("error kind = %v, want Network", last.kind)
	}
	if last.message != "Ping timeout" {
		t.Fatalf("error = %q, want Ping timeout", last.message)
	}
	if got := fixture.session.State(); got != StateReconnecting {
		t.Fatalf("state = %v, want Reconnecting", got)
	}
	if len(fixture.handler.reconnects) != 1 {
		t.Fatalf("reconnects = %d, want 1", len(fixture.handler.reconnects))
	}
}

// TestSessionMatchingPongKeepsSessionRegistered.
// SessionTest::matchingPongKeepsSessionRegistered.
func TestSessionMatchingPongKeepsSessionRegistered(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.registerWithWelcome()
	fixture.clock.Advance(60000 * time.Millisecond)

	fixture.inject(":server PONG irc.example :omairc-watchdog\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
	if fixture.clock.Pending() == 0 {
		t.Fatal("the watchdog must be re-armed")
	}

	before := len(fixture.frames())
	fixture.clock.Advance(60000 * time.Millisecond)
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	assertFramesEqual(t, fixture.writtenSince(before), []string{"PING :omairc-watchdog\r\n"})
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
}

// TestSessionAnswersServerPingAfterWelcome.
// SessionTest::answersServerPingAfterWelcome.
func TestSessionAnswersServerPingAfterWelcome(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.registerWithWelcome()
	fixture.inject("PING :server-token\r\n")
	if got := fixture.lastFrame(); got != "PONG :server-token\r\n" {
		t.Fatalf("last frame = %q, want PONG", got)
	}
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if fixture.wrote("PING :omairc-watchdog\r\n") {
		t.Fatal("a server ping must not trigger the watchdog")
	}
}

// TestSessionRegistrationRefusalFailsVisibly.
// SessionTest::registrationRefusalFailsVisibly.
func TestSessionRegistrationRefusalFailsVisibly(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server 432 * omairc :Erroneous nickname\r\n")
	if got := fixture.session.State(); got != StateFailed {
		t.Fatalf("state = %v, want Failed", got)
	}
	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	if fixture.handler.errors[0].kind != ErrorRegistration {
		t.Fatalf("error kind = %v, want Registration", fixture.handler.errors[0].kind)
	}
	if got := fixture.handler.errors[0].message; got != "Erroneous nickname" {
		t.Fatalf("error = %q, want the server sentence", got)
	}
	if got := fixture.handler.textFor("432"); got != "omairc Erroneous nickname" {
		t.Fatalf("status = %q, want the nick and the sentence", got)
	}
	if len(fixture.handler.reconnects) != 0 {
		t.Fatalf("reconnects = %d, want 0", len(fixture.handler.reconnects))
	}
}

// TestSessionRegistrationRefusalWithoutSentenceKeepsNumeric.
// SessionTest::registrationRefusalWithoutSentenceKeepsNumeric.
func TestSessionRegistrationRefusalWithoutSentenceKeepsNumeric(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server 432 * omairc :\r\n")
	if got := fixture.session.State(); got != StateFailed {
		t.Fatalf("state = %v, want Failed", got)
	}
	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	if got := fixture.handler.errors[0].message; got != "IRC registration was refused (432)" {
		t.Fatalf("error = %q, want the numeric fallback", got)
	}
	if got := fixture.handler.textFor("432"); got != "432" {
		t.Fatalf("status = %q, want the numeric", got)
	}
}

// TestSessionRegistrationRefusalWithoutColonKeepsNumeric.
// SessionTest::registrationRefusalWithoutColonKeepsNumeric.
func TestSessionRegistrationRefusalWithoutColonKeepsNumeric(t *testing.T) {
	lines := []string{
		":server 432 * omairc\r\n",
		":server 432 * omairc :\r\n",
		":server 432 * omairc :IDENTIFY hunter2\r\n",
	}
	for _, line := range lines {
		t.Run(line, func(t *testing.T) {
			fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
			fixture.connectTLS()
			fixture.inject(line)
			if got := fixture.session.State(); got != StateFailed {
				t.Fatalf("state = %v, want Failed", got)
			}
			if len(fixture.handler.errors) != 1 {
				t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
			}
			if got := fixture.handler.errors[0].message; got != "IRC registration was refused (432)" {
				t.Fatalf("error = %q, want the numeric fallback", got)
			}
			if strings.Contains(fixture.handler.errors[0].message, "omairc") ||
				strings.Contains(fixture.handler.errors[0].message, "hunter2") {
				t.Fatal("the nick or secret must not stand in for a missing sentence")
			}
			if got := fixture.handler.textFor("432"); got != "432" {
				t.Fatalf("status = %q, want the numeric", got)
			}
			if fixture.handler.anyFieldContains("hunter2") {
				t.Fatal("Status stored the secret sentence")
			}
		})
	}
}

// TestSessionRegistrationRefusalKeepsOneWordSentence.
// SessionTest::registrationRefusalKeepsOneWordSentence.
func TestSessionRegistrationRefusalKeepsOneWordSentence(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server 432 * omairc :Unavailable\r\n")
	if got := fixture.session.State(); got != StateFailed {
		t.Fatalf("state = %v, want Failed", got)
	}
	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	if got := fixture.handler.errors[0].message; got != "Unavailable" {
		t.Fatalf("error = %q, want the one-word sentence", got)
	}
	if got := fixture.handler.textFor("432"); got != "omairc Unavailable" {
		t.Fatalf("status = %q, want the nick and the sentence", got)
	}
}

// TestSessionRegistrationRefusalDropsSecretSentence.
// SessionTest::registrationRefusalDropsSecretSentence.
func TestSessionRegistrationRefusalDropsSecretSentence(t *testing.T) {
	sentences := []string{
		"IDENTIFY hunter2",
		"Closing Link: PASS hunter2",
		"PAS hunter2",
		"PASS:hunter2",
	}
	for _, sentence := range sentences {
		t.Run(sentence, func(t *testing.T) {
			fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
			fixture.connectTLS()
			fixture.inject(":server 432 * omairc :" + sentence + "\r\n")
			if got := fixture.session.State(); got != StateFailed {
				t.Fatalf("state = %v, want Failed", got)
			}
			if len(fixture.handler.errors) != 1 {
				t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
			}
			if got := fixture.handler.errors[0].message; got != "IRC registration was refused (432)" {
				t.Fatalf("error = %q, want the numeric fallback", got)
			}
			if strings.Contains(fixture.handler.errors[0].message, "hunter2") {
				t.Fatal("the secret sentence must not be shown")
			}
			if len(fixture.handler.reconnects) != 0 {
				t.Fatalf("reconnects = %d, want 0", len(fixture.handler.reconnects))
			}
			if fixture.handler.anyFieldContains("hunter2") {
				t.Fatal("Status stored the secret sentence")
			}
			statusText := fixture.handler.textFor("432")
			if sentence == "IDENTIFY hunter2" {
				if statusText != "432" {
					t.Fatalf("status = %q, want the numeric", statusText)
				}
			} else if statusText != "omairc PASS ***" {
				t.Fatalf("status = %q, want the nick and the masked preview", statusText)
			}
		})
	}
}

// TestSessionNickInUseBeforeWelcomeRetriesThenRegisters.
// SessionTest::nickInUseBeforeWelcomeRetriesThenRegisters.
func TestSessionNickInUseBeforeWelcomeRetriesThenRegisters(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n")
	if got := fixture.session.State(); got != StateRegistering {
		t.Fatalf("state = %v, want Registering", got)
	}
	if !fixture.wrote("NICK omairc\r\n") {
		t.Fatal("the configured nick must be sent")
	}

	fixture.inject(":server 433 * omairc :Nickname in use\r\n")
	if got := fixture.session.State(); got != StateRegistering {
		t.Fatalf("state = %v, want Registering", got)
	}
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
	if got := fixture.lastFrame(); got != "NICK omairc_\r\n" {
		t.Fatalf("last frame = %q, want NICK omairc_", got)
	}
	if got := fixture.session.Nick(); got != "omairc_" {
		t.Fatalf("nick = %q, want omairc_", got)
	}
	if got := fixture.handler.textFor("433"); got != "omairc Nickname in use" {
		t.Fatalf("status = %q, want the nick and the sentence", got)
	}

	fixture.inject(":server 433 * omairc_ :Nickname in use\r\n")
	if got := fixture.session.State(); got != StateRegistering {
		t.Fatalf("state = %v, want Registering", got)
	}
	if got := fixture.lastFrame(); got != "NICK omairc2\r\n" {
		t.Fatalf("last frame = %q, want NICK omairc2", got)
	}
	if got := fixture.session.Nick(); got != "omairc2" {
		t.Fatalf("nick = %q, want omairc2", got)
	}

	fixture.inject(":server 001 omairc2 :Welcome\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if got := fixture.session.Nick(); got != "omairc2" {
		t.Fatalf("nick = %q, want omairc2", got)
	}
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
}

// TestSessionNickInUseFallbacksExhaustedFails.
// SessionTest::nickInUseFallbacksExhaustedFails.
func TestSessionNickInUseFallbacksExhaustedFails(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n")

	fixture.inject(":server 433 * omairc :Nickname in use\r\n" +
		":server 433 * omairc_ :Nickname in use\r\n" +
		":server 433 * omairc2 :Nickname in use\r\n")

	if got := fixture.session.State(); got != StateFailed {
		t.Fatalf("state = %v, want Failed", got)
	}
	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	if fixture.handler.errors[0].kind != ErrorRegistration {
		t.Fatalf("error kind = %v, want Registration", fixture.handler.errors[0].kind)
	}
	if got := fixture.handler.errors[0].message; got != "Nickname in use" {
		t.Fatalf("error = %q, want the server sentence", got)
	}
	if got := fixture.lastFrame(); got != "NICK omairc2\r\n" {
		t.Fatalf("last frame = %q, want NICK omairc2", got)
	}
	if len(fixture.handler.reconnects) != 0 {
		t.Fatalf("reconnects = %d, want 0", len(fixture.handler.reconnects))
	}
}

// TestSessionNickInUseAfterWelcomeKeepsSession.
// SessionTest::nickInUseAfterWelcomeKeepsSession.
func TestSessionNickInUseAfterWelcomeKeepsSession(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}

	fixture.inject(":server 433 omairc othernick :Nickname is already in use\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if got := fixture.transport.State(); got != ConnectionEncrypted {
		t.Fatalf("transport = %v, want Encrypted", got)
	}
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
	if len(fixture.handler.messages) != 1 {
		t.Fatalf("messages = %d, want 1", len(fixture.handler.messages))
	}
	if got := fixture.handler.messages[0].Command; got != "433" {
		t.Fatalf("last command = %q, want 433", got)
	}
	if got := fixture.handler.textFor("433"); got != "othernick Nickname is already in use" {
		t.Fatalf("status = %q, want a normal 433 line", got)
	}
	if fixture.handler.anyFieldContains("IRC registration was refused") {
		t.Fatal("a 433 after welcome must not use the registration fallback")
	}
}

// TestSessionNickInUseFallbackDropsSecretStatusSentence.
// SessionTest::nickInUseFallbackDropsSecretStatusSentence.
func TestSessionNickInUseFallbackDropsSecretStatusSentence(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 433 * omairc :IDENTIFY hunter2\r\n")
	if got := fixture.session.State(); got != StateRegistering {
		t.Fatalf("state = %v, want Registering", got)
	}
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
	if got := fixture.lastFrame(); got != "NICK omairc_\r\n" {
		t.Fatalf("last frame = %q, want NICK omairc_", got)
	}
	if fixture.handler.anyFieldContains("hunter2") {
		t.Fatal("Status stored the secret sentence")
	}
	if fixture.handler.anyFieldContains("IRC registration was refused") {
		t.Fatal("a 433 that still falls back must not use the registration fallback")
	}
	if got := fixture.handler.textFor("433"); got != "433" {
		t.Fatalf("status = %q, want the numeric", got)
	}
}

// TestSessionNickInUseAfterWelcomeDropsSecretStatusSentence.
// SessionTest::nickInUseAfterWelcomeDropsSecretStatusSentence.
func TestSessionNickInUseAfterWelcomeDropsSecretStatusSentence(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":server 433 omairc othernick :Closing Link: PASS hunter2\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
	if fixture.handler.anyFieldContains("hunter2") {
		t.Fatal("Status stored the secret sentence")
	}
	if fixture.handler.anyFieldContains("IRC registration was refused") {
		t.Fatal("a 433 after welcome must not use the registration fallback")
	}
	if !fixture.handler.anyFieldContains("PASS ***") {
		t.Fatal("Status must store the masked preview")
	}
}

// TestSessionUnavailableResourceAfterWelcomeKeepsSession.
// SessionTest::unavailableResourceAfterWelcomeKeepsSession.
func TestSessionUnavailableResourceAfterWelcomeKeepsSession(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}

	fixture.inject(":server 437 omairc othernick :Nick/channel is temporarily unavailable\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if got := fixture.transport.State(); got != ConnectionEncrypted {
		t.Fatalf("transport = %v, want Encrypted", got)
	}
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}
}

// TestSessionUnavailableResourceBeforeWelcomeFails.
// SessionTest::unavailableResourceBeforeWelcomeFails.
func TestSessionUnavailableResourceBeforeWelcomeFails(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server 437 * omairc :Nick/channel is temporarily unavailable\r\n")
	if got := fixture.session.State(); got != StateFailed {
		t.Fatalf("state = %v, want Failed", got)
	}
	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	if fixture.handler.errors[0].kind != ErrorRegistration {
		t.Fatalf("error kind = %v, want Registration", fixture.handler.errors[0].kind)
	}
	if got := fixture.handler.errors[0].message; got != "Nick/channel is temporarily unavailable" {
		t.Fatalf("error = %q, want the server sentence", got)
	}
	if len(fixture.handler.reconnects) != 0 {
		t.Fatalf("reconnects = %d, want 0", len(fixture.handler.reconnects))
	}
}

// TestSessionRegistrationErrorBeforeWelcomeShowsReasonAndStops.
// SessionTest::registrationErrorBeforeWelcomeShowsReasonAndStops.
func TestSessionRegistrationErrorBeforeWelcomeShowsReasonAndStops(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	written := len(fixture.frames())
	fixture.inject(":server ERROR :Closing Link: 127.0.0.1 (Invalid nickname)\r\n")
	if got := fixture.session.State(); got != StateFailed {
		t.Fatalf("state = %v, want Failed", got)
	}
	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	if fixture.handler.errors[0].kind != ErrorRegistration {
		t.Fatalf("error kind = %v, want Registration", fixture.handler.errors[0].kind)
	}
	if got := fixture.handler.errors[0].message; got != "Closing Link: 127.0.0.1 (Invalid nickname)" {
		t.Fatalf("error = %q, want the server sentence", got)
	}
	if len(fixture.handler.reconnects) != 0 {
		t.Fatalf("reconnects = %d, want 0", len(fixture.handler.reconnects))
	}
	if got := len(fixture.frames()); got != written {
		t.Fatalf("frames = %d, want %d (no further nick)", got, written)
	}
}

// TestSessionRegistrationErrorBeforeWelcomeDropsSecretSentence.
// SessionTest::registrationErrorBeforeWelcomeDropsSecretSentence.
func TestSessionRegistrationErrorBeforeWelcomeDropsSecretSentence(t *testing.T) {
	sentences := []string{
		"IDENTIFY hunter2",
		"Closing Link: PASS hunter2",
		"PAS hunter2",
		"PASS:hunter2",
	}
	for _, sentence := range sentences {
		t.Run(sentence, func(t *testing.T) {
			fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
			fixture.connectTLS()
			fixture.inject(":server ERROR :" + sentence + "\r\n")
			if got := fixture.session.State(); got != StateFailed {
				t.Fatalf("state = %v, want Failed", got)
			}
			if len(fixture.handler.errors) != 1 {
				t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
			}
			if fixture.handler.errors[0].kind != ErrorRegistration {
				t.Fatalf("error kind = %v, want Registration", fixture.handler.errors[0].kind)
			}
			if got := fixture.handler.errors[0].message; got != "IRC server reported an error" {
				t.Fatalf("error = %q, want the generic fallback", got)
			}
			if strings.Contains(fixture.handler.errors[0].message, "hunter2") {
				t.Fatal("the secret sentence must not be shown")
			}
			if len(fixture.handler.reconnects) != 0 {
				t.Fatalf("reconnects = %d, want 0", len(fixture.handler.reconnects))
			}
			if fixture.handler.anyFieldContains("hunter2") {
				t.Fatal("Status stored the secret sentence")
			}
			statusText := fixture.handler.textFor("ERROR")
			if sentence == "IDENTIFY hunter2" {
				if statusText != "ERROR" {
					t.Fatalf("status = %q, want the command", statusText)
				}
			} else if !strings.Contains(statusText, "PASS ***") {
				t.Fatalf("status = %q, want the masked preview", statusText)
			}
		})
	}
}

// TestSessionRegistrationErrorAfterWelcomeReconnectsWithReason.
// SessionTest::registrationErrorAfterWelcomeReconnectsWithReason.
func TestSessionRegistrationErrorAfterWelcomeReconnectsWithReason(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.registerWithWelcome()
	fixture.inject(":server ERROR :Closing Link: 127.0.0.1 (Quit)\r\n")
	if got := fixture.session.State(); got != StateReconnecting {
		t.Fatalf("state = %v, want Reconnecting", got)
	}
	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	if fixture.handler.errors[0].kind != ErrorNetwork {
		t.Fatalf("error kind = %v, want Network", fixture.handler.errors[0].kind)
	}
	if got := fixture.handler.errors[0].message; got != "Closing Link: 127.0.0.1 (Quit)" {
		t.Fatalf("error = %q, want the server sentence", got)
	}
	if len(fixture.handler.reconnects) != 1 {
		t.Fatalf("reconnects = %d, want 1", len(fixture.handler.reconnects))
	}
}

// TestSessionConnectionTimeoutSchedulesReconnect.
// SessionTest::connectionTimeoutSchedulesReconnect.
func TestSessionConnectionTimeoutSchedulesReconnect(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.session.Start()
	fixture.transport.TimeoutConnect()

	if got := fixture.session.State(); got != StateReconnecting {
		t.Fatalf("state = %v, want Reconnecting", got)
	}
	if len(fixture.handler.reconnects) != 1 || fixture.handler.reconnects[0].delayMs != 250 {
		t.Fatalf("reconnects = %+v, want one at 250ms", fixture.handler.reconnects)
	}
	if fixture.handler.errors[0].kind != ErrorNetwork {
		t.Fatalf("error kind = %v, want Network", fixture.handler.errors[0].kind)
	}
}

// TestSessionRemoteCloseSchedulesReconnect.
// SessionTest::remoteCloseSchedulesReconnect.
func TestSessionRemoteCloseSchedulesReconnect(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.transport.RemoteClose()

	if got := fixture.session.State(); got != StateReconnecting {
		t.Fatalf("state = %v, want Reconnecting", got)
	}
	if len(fixture.handler.reconnects) != 1 {
		t.Fatalf("reconnects = %d, want 1", len(fixture.handler.reconnects))
	}
	if got := fixture.handler.reconnects[0].delayMs; got != 250 {
		t.Fatalf("delay = %d, want 250", got)
	}
}

// fakeReachability is the Go shape of FakeReachabilitySource. The session
// consumes the channel on a goroutine, so becomeReachable sends once and
// barrier sends a second value to guarantee the first was processed.
type fakeReachability struct {
	channel chan struct{}
}

func newFakeReachability() *fakeReachability {
	return &fakeReachability{channel: make(chan struct{})}
}

func (r *fakeReachability) Reachable() <-chan struct{} { return r.channel }

func (r *fakeReachability) becomeReachable() { r.channel <- struct{}{} }

// barrier blocks until the session's reachability goroutine has processed the
// previous signal (it processes signals strictly in order).
func (r *fakeReachability) barrier() { r.channel <- struct{}{} }

// TestSessionReachabilityStartsReconnectWithoutWaiting.
// SessionTest::reachabilityStartsReconnectWithoutWaiting.
func TestSessionReachabilityStartsReconnectWithoutWaiting(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	reachability := newFakeReachability()
	fixture.session.SetReachabilitySource(reachability)

	fixture.connectTLS()
	fixture.transport.RemoteClose()
	if got := fixture.session.State(); got != StateReconnecting {
		t.Fatalf("state = %v, want Reconnecting", got)
	}
	if len(fixture.handler.reconnects) != 1 || fixture.handler.reconnects[0].delayMs != 250 {
		t.Fatalf("reconnects = %+v, want one at 250ms", fixture.handler.reconnects)
	}
	if got := fixture.session.ReconnectAttempt(); got != 1 {
		t.Fatalf("attempt = %d, want 1", got)
	}

	reachability.becomeReachable()
	reachability.barrier()

	if got := fixture.session.State(); got != StateConnecting {
		t.Fatalf("state = %v, want Connecting", got)
	}
	if got := fixture.transport.State(); got != ConnectionConnecting {
		t.Fatalf("transport = %v, want Connecting", got)
	}
	if len(fixture.handler.reconnects) != 1 {
		t.Fatalf("reconnects = %d, want 1", len(fixture.handler.reconnects))
	}
	if got := fixture.session.ReconnectAttempt(); got != 1 {
		t.Fatalf("attempt = %d, want 1", got)
	}
}

// TestSessionReachabilityIgnoredUnlessReconnecting.
// SessionTest::reachabilityIgnoredUnlessReconnecting.
func TestSessionReachabilityIgnoredUnlessReconnecting(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	reachability := newFakeReachability()
	fixture.session.SetReachabilitySource(reachability)

	fixture.registerWithWelcome()
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}

	reachability.becomeReachable()
	reachability.barrier()
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if got := fixture.transport.State(); got != ConnectionEncrypted {
		t.Fatalf("transport = %v, want Encrypted", got)
	}

	fixture.transport.RemoteClose()
	fixture.session.Stop()
	reachability.becomeReachable()
	reachability.barrier()
	if got := fixture.session.State(); got != StateIdle {
		t.Fatalf("state = %v, want Idle", got)
	}
	if got := fixture.transport.State(); got != ConnectionDisconnected {
		t.Fatalf("transport = %v, want Disconnected", got)
	}
}

// TestSessionReconnectCanBeCancelled.
// SessionTest::reconnectCanBeCancelled.
func TestSessionReconnectCanBeCancelled(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.transport.RemoteClose()
	if fixture.clock.Pending() == 0 {
		t.Fatal("the reconnect timer must be pending")
	}

	fixture.session.Stop()
	if got := fixture.session.State(); got != StateIdle {
		t.Fatalf("state = %v, want Idle", got)
	}
	if fixture.clock.Pending() != 0 {
		t.Fatal("the reconnect timer must be cancelled")
	}
	before := len(fixture.frames())
	fixture.clock.Advance(250 * time.Millisecond)
	if len(fixture.frames()) != before {
		t.Fatal("a cancelled reconnect must not write frames")
	}
	if got := fixture.transport.State(); got != ConnectionDisconnected {
		t.Fatalf("transport = %v, want Disconnected", got)
	}
}

// TestSessionReconnectDelayIsBoundedExponential.
// SessionTest::reconnectDelayIsBoundedExponential.
func TestSessionReconnectDelayIsBoundedExponential(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.handler.reconnectLimit = 3

	fixture.connectTLS()
	fixture.transport.RemoteClose()
	if got := reconnectDelays(fixture.handler.reconnects); !equalInts(got, []int{250}) {
		t.Fatalf("delays = %v, want [250]", got)
	}

	fixture.fireReconnect()
	if got := fixture.transport.State(); got != ConnectionConnecting {
		t.Fatalf("transport = %v, want Connecting", got)
	}
	fixture.transport.CompleteConnect()
	fixture.transport.RemoteClose()
	if got := reconnectDelays(fixture.handler.reconnects); !equalInts(got, []int{250, 500}) {
		t.Fatalf("delays = %v, want [250 500]", got)
	}

	fixture.fireReconnect()
	fixture.transport.CompleteConnect()
	fixture.transport.RemoteClose()
	if got := reconnectDelays(fixture.handler.reconnects); !equalInts(got, []int{250, 500, 1000}) {
		t.Fatalf("delays = %v, want [250 500 1000]", got)
	}
	if got := fixture.session.State(); got != StateIdle {
		t.Fatalf("state = %v, want Idle", got)
	}
	if fixture.clock.Pending() != 0 {
		t.Fatal("the session must have stopped")
	}
}

// TestSessionReconnectKeepsRetryingUntilStop.
// SessionTest::reconnectKeepsRetryingUntilStop.
func TestSessionReconnectKeepsRetryingUntilStop(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.transport.RemoteClose()

	for index := 0; index < 3; index++ {
		fixture.fireReconnect()
		fixture.transport.CompleteConnect()
		fixture.transport.RemoteClose()
	}

	if got := fixture.session.State(); got != StateReconnecting {
		t.Fatalf("state = %v, want Reconnecting", got)
	}
	delays := reconnectDelays(fixture.handler.reconnects)
	if len(delays) <= 3 {
		t.Fatalf("delays = %v, want more than 3", delays)
	}
	if delays[len(delays)-1] != 1000 {
		t.Fatalf("last delay = %d, want 1000", delays[len(delays)-1])
	}

	fixture.session.Stop()
	if got := fixture.session.State(); got != StateIdle {
		t.Fatalf("state = %v, want Idle", got)
	}
	if fixture.clock.Pending() != 0 {
		t.Fatal("the reconnect timer must be cancelled")
	}
}

// TestSessionQuitStopsReconnectWait.
// SessionTest::quitStopsReconnectWait.
func TestSessionQuitStopsReconnectWait(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.transport.RemoteClose()
	if got := fixture.session.State(); got != StateReconnecting {
		t.Fatalf("state = %v, want Reconnecting", got)
	}
	if fixture.clock.Pending() == 0 {
		t.Fatal("the reconnect timer must be pending")
	}

	if !fixture.session.Quit("") {
		t.Fatal("quit must succeed")
	}
	if got := fixture.session.State(); got != StateIdle {
		t.Fatalf("state = %v, want Idle", got)
	}
	if fixture.clock.Pending() != 0 {
		t.Fatal("the reconnect timer must be cancelled")
	}
	written := len(fixture.frames())
	fixture.clock.Advance(250 * time.Millisecond)
	if got := fixture.session.State(); got != StateIdle {
		t.Fatalf("state = %v, want Idle", got)
	}
	if len(fixture.frames()) != written {
		t.Fatal("a cancelled reconnect must not write frames")
	}
	if got := fixture.transport.State(); got != ConnectionDisconnected {
		t.Fatalf("transport = %v, want Disconnected", got)
	}
}

// TestSessionRetryableNetworkErrorIsEmittedOnceUntilWelcome.
// SessionTest::retryableNetworkErrorIsEmittedOnceUntilWelcome.
func TestSessionRetryableNetworkErrorIsEmittedOnceUntilWelcome(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.transport.RemoteClose()

	for index := 0; index < 2; index++ {
		fixture.fireReconnect()
		fixture.transport.CompleteConnect()
		fixture.transport.RemoteClose()
	}

	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	if fixture.handler.errors[0].kind != ErrorNetwork {
		t.Fatalf("error kind = %v, want Network", fixture.handler.errors[0].kind)
	}

	fixture.fireReconnect()
	fixture.transport.CompleteConnect()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}

	fixture.transport.RemoteClose()
	if len(fixture.handler.errors) != 2 {
		t.Fatalf("errors = %d, want 2", len(fixture.handler.errors))
	}
	if fixture.handler.errors[1].kind != ErrorNetwork {
		t.Fatalf("error kind = %v, want Network", fixture.handler.errors[1].kind)
	}
}

// TestSessionRetryableErrorsStayDedupedAcrossKindsUntilWelcome.
// SessionTest::retryableErrorsStayDedupedAcrossKindsUntilWelcome.
func TestSessionRetryableErrorsStayDedupedAcrossKindsUntilWelcome(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.transport.RemoteClose()

	if len(fixture.handler.errors) != 1 || fixture.handler.errors[0].kind != ErrorNetwork {
		t.Fatalf("errors = %+v, want one Network error", fixture.handler.errors)
	}

	fixture.fireReconnect()
	fixture.transport.FailConnect("TLS handshake failed")

	if len(fixture.handler.errors) != 2 {
		t.Fatalf("errors = %d, want 2", len(fixture.handler.errors))
	}
	if fixture.handler.errors[1].kind != ErrorTLS {
		t.Fatalf("error kind = %v, want TLS", fixture.handler.errors[1].kind)
	}
	if got := fixture.session.State(); got != StateReconnecting {
		t.Fatalf("state = %v, want Reconnecting", got)
	}

	fixture.fireReconnect()
	fixture.transport.CompleteConnect()
	fixture.transport.RemoteClose()

	if len(fixture.handler.errors) != 2 {
		t.Fatalf("errors = %d, want 2", len(fixture.handler.errors))
	}
	if fixture.handler.errors[0].kind != ErrorNetwork || fixture.handler.errors[1].kind != ErrorTLS {
		t.Fatalf("error kinds = %v/%v, want Network/TLS",
			fixture.handler.errors[0].kind, fixture.handler.errors[1].kind)
	}
}

// TestSessionQuitRejectsInvalidReasonWhileRegistered.
// SessionTest::quitRejectsInvalidReasonWhileRegistered.
func TestSessionQuitRejectsInvalidReasonWhileRegistered(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.registerWithWelcome()
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}

	if fixture.session.Quit("bad\nreason") {
		t.Fatal("quit with an invalid reason must fail")
	}
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if fixture.wrote("QUIT :bad\nreason\r\n") {
		t.Fatal("the invalid QUIT must not be sent")
	}

	fixture.transport.RemoteClose()
	if got := fixture.session.State(); got != StateReconnecting {
		t.Fatalf("state = %v, want Reconnecting", got)
	}
	if !fixture.session.Quit("") {
		t.Fatal("quit while reconnecting must succeed")
	}
	if got := fixture.session.State(); got != StateIdle {
		t.Fatalf("state = %v, want Idle", got)
	}
}

// TestSessionDestructionWhileConnectingIsSafe.
// Destruction while connecting must stay safe. Go has no parent/child
// ownership, so the analogue is that dropping the last reference must not leave
// a dangling callback.
func TestSessionDestructionWhileConnectingIsSafe(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.session.Start()
	if got := fixture.transport.State(); got != ConnectionConnecting {
		t.Fatalf("transport = %v, want Connecting", got)
	}

	fixture.session = nil
	runtime.GC()
}

// TestSessionManagerStartsTwoLiveNetworks.
// SessionTest::managerStartsTwoLiveNetworks.
func TestSessionManagerStartsTwoLiveNetworks(t *testing.T) {
	manager := NewManager()
	firstTransport := NewLoopbackTransport()
	secondTransport := NewLoopbackTransport()
	first, err := manager.Create(sessionTestConfig("network-a"), firstTransport, NewFakeClock(time.Unix(0, 0)))
	if err != nil {
		t.Fatalf("create network-a failed: %v", err)
	}
	second, err := manager.Create(sessionTestConfig("network-b"), secondTransport, NewFakeClock(time.Unix(0, 0)))
	if err != nil {
		t.Fatalf("create network-b failed: %v", err)
	}

	if manager.Find("network-a") != first {
		t.Fatal("find must return the created session")
	}
	if !manager.Activate("network-a") {
		t.Fatal("activate network-a must succeed")
	}
	if !manager.Activate("network-b") {
		t.Fatal("activate network-b must succeed")
	}
	if first.State() != StateConnecting || second.State() != StateConnecting {
		t.Fatalf("states = %v/%v, want Connecting/Connecting", first.State(), second.State())
	}
	if firstTransport.State() != ConnectionConnecting || secondTransport.State() != ConnectionConnecting {
		t.Fatal("both transports must be connecting")
	}

	manager.Stop("network-a")
	if second.State() != StateConnecting {
		t.Fatalf("network-b state = %v, want Connecting", second.State())
	}
}

// TestSessionManagerCreateStaysAddOnly.
// SessionTest::managerCreateStaysAddOnly.
func TestSessionManagerCreateStaysAddOnly(t *testing.T) {
	manager := NewManager()
	first, err := manager.Create(sessionTestConfig("network-a"), NewLoopbackTransport(), NewFakeClock(time.Unix(0, 0)))
	if err != nil {
		t.Fatalf("create failed: %v", err)
	}
	if _, err := manager.Create(sessionTestConfig("network-a"), NewLoopbackTransport(), NewFakeClock(time.Unix(0, 0))); err == nil {
		t.Fatal("a duplicate network id must be rejected")
	}
	if manager.Find("network-a") != first {
		t.Fatal("the original session must stay registered")
	}
}

// TestSessionManagerDiscardUnregistersImmediately.
// SessionTest::managerDiscardUnregistersImmediately.
func TestSessionManagerDiscardUnregistersImmediately(t *testing.T) {
	manager := NewManager()
	session, err := manager.Create(sessionTestConfig("network-a"), NewLoopbackTransport(), NewFakeClock(time.Unix(0, 0)))
	if err != nil {
		t.Fatalf("create failed: %v", err)
	}

	if !manager.Activate("network-a") {
		t.Fatal("activate must succeed")
	}
	manager.Discard("network-a")
	if manager.Find("network-a") != nil {
		t.Fatal("discard must unregister immediately")
	}
	manager.Discard("network-a")

	replacement, err := manager.Create(sessionTestConfig("network-a"), NewLoopbackTransport(), NewFakeClock(time.Unix(0, 0)))
	if err != nil {
		t.Fatalf("replacement create failed: %v", err)
	}
	if replacement == session {
		t.Fatal("the replacement must be a new session")
	}
	if manager.Find("network-a") != replacement {
		t.Fatal("find must return the replacement")
	}
	if session.State() != StateIdle {
		t.Fatalf("discarded session state = %v, want Idle", session.State())
	}
}

// TestSessionMalformedInputSurfacesProtocolError.
// SessionTest::malformedInputSurfacesProtocolError.
func TestSessionMalformedInputSurfacesProtocolError(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()

	fixture.inject(string([]byte("BAD\x00FRAME\r\n")))
	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	if fixture.handler.errors[0].kind != ErrorProtocol {
		t.Fatalf("error kind = %v, want Protocol", fixture.handler.errors[0].kind)
	}
	message := fixture.handler.errors[0].message
	if !strings.Contains(message, "invalid character") ||
		!strings.Contains(message, "Preview:") ||
		!strings.Contains(message, "BAD") {
		t.Fatalf("error = %q, want invalid character with a BAD preview", message)
	}
	if got := fixture.session.State(); got != StateCapLs {
		t.Fatalf("state = %v, want CapLs", got)
	}

	check := func(index int, raw []byte, wantSubstring string, wantRedacted string) {
		t.Helper()
		fixture.inject(string(raw))
		if len(fixture.handler.errors) != index {
			t.Fatalf("errors = %d, want %d", len(fixture.handler.errors), index)
		}
		got := fixture.handler.errors[index-1].message
		if !strings.Contains(got, wantSubstring) {
			t.Fatalf("error = %q, want it to contain %q", got, wantSubstring)
		}
		if wantRedacted != "" && !strings.Contains(got, wantRedacted) {
			t.Fatalf("error = %q, want it to contain %q", got, wantRedacted)
		}
		if strings.Contains(got, "hunter2") || strings.Contains(got, "s3cret") ||
			strings.Contains(got, "my_nick") {
			t.Fatalf("error = %q leaked a secret", got)
		}
	}

	check(2, []byte("\x00PASS hunter2\r\n"), "invalid character", "PASS ***")
	check(3, []byte("PASS\x00hunter2\r\n"), "invalid character", "PASS ***")
	check(4, []byte("P\x00ASS hunter2\r\n"), "invalid character", "PASS ***")
	check(5, []byte("\x01PASS hunter2\r\n"), "invalid command", "PASS ***")
	check(6, []byte("PASS\x7F hunter2\r\n"), "invalid command", "PASS ***")
	check(7, []byte("P@SS hunter2\r\n"), "invalid command", "PASS ***")
	check(8, []byte("PA@SS hunter2\r\n"), "invalid command", "PASS ***")
	check(9, []byte("32@4 omairc #omarchy +k s3cret\r\n"), "invalid command", "+k ***")
	check(10, []byte("PRIVMSG nickserv :identif\x00 my_nick s3cret\r\n"), "invalid character", "IDENTIFY ***")
	check(11, []byte("P\x00ASS\x00hunter2\r\n"), "invalid character", "PASS ***")
	check(12, []byte("@bad tag=foo PRIVMSG nickserv :identify my_nick s3cret\r\n"), "invalid command", "IDENTIFY ***")
}

// TestSessionOverlongFrameLogsPreviewWithoutSecrets.
// SessionTest::overlongFrameLogsPreviewWithoutSecrets.
func TestSessionOverlongFrameLogsPreviewWithoutSecrets(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()

	overlong := []byte("PING :")
	overlong = append(overlong, strings.Repeat("z", irc.MaxInboundClassicFrameBytes-len(overlong)-1)...)
	overlong = append(overlong, "\r\n"...)
	fixture.inject(string(overlong))

	if len(fixture.handler.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(fixture.handler.errors))
	}
	message := fixture.handler.errors[0].message
	if !strings.Contains(message, "too many bytes") ||
		!strings.Contains(message, strconv.Itoa(irc.MaxInboundClassicFrameBytes-1)+" bytes") ||
		!strings.Contains(message, "Preview: PING :") {
		t.Fatalf("error = %q, want an overlong preview", message)
	}

	checkOverlong := func(index int, prefix, redacted, secret string) {
		t.Helper()
		frame := []byte(prefix)
		frame = append(frame, strings.Repeat("x", irc.MaxInboundClassicFrameBytes-len(frame)-1)...)
		frame = append(frame, "\r\nPING :ok\r\n"...)
		fixture.inject(string(frame))
		if len(fixture.handler.errors) != index {
			t.Fatalf("errors = %d, want %d", len(fixture.handler.errors), index)
		}
		got := fixture.handler.errors[index-1].message
		if !strings.Contains(got, "too many bytes") {
			t.Fatalf("error = %q, want too many bytes", got)
		}
		if !strings.Contains(got, redacted) {
			t.Fatalf("error = %q, want it to contain %q", got, redacted)
		}
		if strings.Contains(got, secret) {
			t.Fatalf("error = %q leaked the secret", got)
		}
	}

	checkOverlong(2, "PASS hunter2 ", "PASS ***", "hunter2")
	checkOverlong(3, "JOIN #secret hunter2 ", "JOIN #secret ***", "hunter2")
	checkOverlong(4, "PRIVMSG nickserv :identify my_nick s3cret ", "PRIVMSG nickserv :IDENTIFY ***", "s3cret")
	checkOverlong(5, "MODE #omarchy +k s3cret ", "MODE #omarchy +k ***", "s3cret")
	checkOverlong(6, "PASS\thunter2 ", "PASS ***", "hunter2")
	checkOverlong(7, "PRIVMSG nickserv :\x01identify my_nick s3cret\x01 ", "PRIVMSG nickserv :IDENTIFY ***", "s3cret")
}

// reconnectDelays extracts the scheduled delays.
func reconnectDelays(reconnects []sessionRecordedReconnect) []int {
	delays := make([]int, len(reconnects))
	for index, reconnect := range reconnects {
		delays[index] = reconnect.delayMs
	}
	return delays
}

func equalInts(left, right []int) bool {
	if len(left) != len(right) {
		return false
	}
	for index := range left {
		if left[index] != right[index] {
			return false
		}
	}
	return true
}
