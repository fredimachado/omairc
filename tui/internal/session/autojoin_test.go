package session

import (
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// keyedTarget builds a validated JOIN target for channel with key.
func keyedTarget(t *testing.T, channel, key string) irc.JoinTarget {
	t.Helper()
	target, ok := irc.MakeJoinTarget(channel, &key, irc.NewServerFeatures())
	if !ok {
		t.Fatalf("MakeJoinTarget(%q, %q) failed", channel, key)
	}
	return target
}

// bareTarget builds a validated JOIN target with no key.
func bareTarget(t *testing.T, channel string) irc.JoinTarget {
	t.Helper()
	target, ok := irc.MakeJoinTarget(channel, nil, irc.NewServerFeatures())
	if !ok {
		t.Fatalf("MakeJoinTarget(%q) failed", channel)
	}
	return target
}

// TestSessionKeyedAutojoinSurvivesReconnectThenPartForgets ports
// SessionTest::keyedAutojoinSurvivesReconnectThenPartForgets.
func TestSessionKeyedAutojoinSurvivesReconnectThenPartForgets(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.registerWithWelcome()

	if !fixture.session.Join(keyedTarget(t, "#private", "hunter2")) {
		t.Fatal("the keyed join must succeed")
	}
	if got := fixture.lastFrame(); got != "JOIN #private hunter2\r\n" {
		t.Fatalf("last frame = %q, want the keyed JOIN", got)
	}
	fixture.inject(":omairc!u@h JOIN :#private\r\n")
	if fixture.handler.anyFieldContains("hunter2") {
		t.Fatal("the channel key leaked into Status")
	}

	before := len(fixture.frames())
	fixture.reconnectRegistered()
	assertStringsEqual(t, joinFrames(fixture.writtenSince(before)),
		[]string{"JOIN #private hunter2\r\n"})
	fixture.inject(":omairc!u@h JOIN :#private\r\n")
	if fixture.handler.anyFieldContains("hunter2") {
		t.Fatal("the channel key leaked into Status after the reconnect")
	}

	if !fixture.session.Part("#private") {
		t.Fatal("part must succeed")
	}
	fixture.inject(":omairc!u@h PART #private\r\n")

	before = len(fixture.frames())
	fixture.reconnectRegistered()
	assertStringsEqual(t, joinFrames(fixture.writtenSince(before)), nil)
}

// TestSessionBareJoinDoesNotWipeStoredKey ports
// SessionTest::bareJoinDoesNotWipeStoredKey.
func TestSessionBareJoinDoesNotWipeStoredKey(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.registerWithWelcome()

	if !fixture.session.Join(keyedTarget(t, "#private", "hunter2")) {
		t.Fatal("the keyed join must succeed")
	}
	fixture.inject(":omairc!u@h JOIN :#private\r\n")

	if !fixture.session.Join(bareTarget(t, "#private")) {
		t.Fatal("the bare join must succeed")
	}
	if got := fixture.lastFrame(); got != "JOIN #private\r\n" {
		t.Fatalf("last frame = %q, want the bare JOIN", got)
	}
	fixture.inject(":omairc!u@h JOIN :#private\r\n")

	before := len(fixture.frames())
	fixture.reconnectRegistered()
	assertStringsEqual(t, joinFrames(fixture.writtenSince(before)),
		[]string{"JOIN #private hunter2\r\n"})
}

// TestSessionJoinWithNewKeyUpdatesAfterSuccess ports
// SessionTest::joinWithNewKeyUpdatesAfterSuccess.
func TestSessionJoinWithNewKeyUpdatesAfterSuccess(t *testing.T) {
	fixture := newSessionFixture(t, historyConfig())
	fixture.registerWithWelcome()

	if !fixture.session.Join(keyedTarget(t, "#private", "hunter2")) {
		t.Fatal("the first keyed join must succeed")
	}
	fixture.inject(":omairc!u@h JOIN :#private\r\n")

	if !fixture.session.Join(keyedTarget(t, "#private", "newkey")) {
		t.Fatal("the second keyed join must succeed")
	}
	if got := fixture.lastFrame(); got != "JOIN #private newkey\r\n" {
		t.Fatalf("last frame = %q, want the new key", got)
	}
	fixture.inject(":omairc!u@h JOIN :#private\r\n")

	before := len(fixture.frames())
	fixture.reconnectRegistered()
	assertStringsEqual(t, joinFrames(fixture.writtenSince(before)),
		[]string{"JOIN #private newkey\r\n"})
}

// TestSessionWelcomeSendsStoredChannelKey ports
// SessionTest::welcomeSendsStoredChannelKey.
func TestSessionWelcomeSendsStoredChannelKey(t *testing.T) {
	config := historyConfig()
	config.AutojoinChannels = []string{"#omarchy", "#private"}
	config.AutojoinKeys = map[string]string{"#private": "hunter2"}
	fixture := newSessionFixture(t, config)
	fixture.registerWithWelcome()

	assertStringsEqual(t, joinFrames(fixture.frames()),
		[]string{"JOIN #omarchy\r\n", "JOIN #private hunter2\r\n"})
	if fixture.handler.anyFieldContains("hunter2") {
		t.Fatal("the channel key leaked into Status")
	}
}

// TestSessionBadChannelKeyStaysOnStatusWithoutRetry ports
// SessionTest::badChannelKeyStaysOnStatusWithoutRetry.
func TestSessionBadChannelKeyStaysOnStatusWithoutRetry(t *testing.T) {
	config := historyConfig()
	config.AutojoinChannels = []string{"#secret"}
	config.AutojoinKeys = map[string]string{"#secret": "hunter2"}
	fixture := newSessionFixture(t, config)
	fixture.registerWithWelcome()

	assertStringsEqual(t, joinFrames(fixture.frames()),
		[]string{"JOIN #secret hunter2\r\n"})

	framesAfterWelcome := len(fixture.frames())
	fixture.inject(":server 475 omairc #secret :Cannot join channel (+k)\r\n")
	if len(fixture.frames()) != framesAfterWelcome {
		t.Fatal("a bad channel key must not be retried")
	}
	if !fixture.handler.hasLabel("475") {
		t.Fatal("475 must reach Status")
	}
	if fixture.handler.anyFieldContains("hunter2") {
		t.Fatal("the channel key leaked into Status")
	}

	before := len(fixture.frames())
	fixture.reconnectRegistered()
	assertStringsEqual(t, joinFrames(fixture.writtenSince(before)),
		[]string{"JOIN #secret\r\n"})
}
