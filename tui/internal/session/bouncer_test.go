package session

import (
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

func bouncerConfig() SessionConfig {
	config := sessionTestConfig(sessionTestNetworkID)
	config.AutojoinChannels = nil
	return config
}

func registerBouncerCaps(fixture *sessionFixture) {
	fixture.t.Helper()
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :soju.im/bouncer-networks " +
		"soju.im/bouncer-networks-notify\r\n" +
		":server CAP omairc ACK :soju.im/bouncer-networks " +
		"soju.im/bouncer-networks-notify\r\n" +
		":server 001 omairc :Welcome\r\n")
}

func bouncerByID(networks []irc.BouncerNetwork, id string) (irc.BouncerNetwork, bool) {
	for _, network := range networks {
		if network.ID == id {
			return network, true
		}
	}
	return irc.BouncerNetwork{}, false
}

func countFrames(frames []string, frame string) int {
	count := 0
	for _, candidate := range frames {
		if candidate == frame {
			count++
		}
	}
	return count
}

func TestBouncerNetworkSnapshotUpdatesAndRemoval(t *testing.T) {
	fixture := newSessionFixture(t, bouncerConfig())
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :soju.im/bouncer-networks " +
		"soju.im/bouncer-networks-notify\r\n" +
		":server CAP omairc ACK :soju.im/bouncer-networks " +
		"soju.im/bouncer-networks-notify\r\n" +
		":server BATCH +bn soju.im/bouncer-networks\r\n" +
		"@batch=bn BOUNCER NETWORK 1 " +
		"name=My\\sAwesome\\sNetwork;host=irc.example;tls=0;" +
		"not-a-real-key=ignored;pass=s3cret\r\n" +
		"@batch=bn BOUNCER NETWORK 2 host=irc.oftc.net;name=OFTC\r\n" +
		":server BATCH -bn\r\n" +
		":server 001 omairc :Welcome\r\n")

	caps := fixture.session.Capabilities()
	if !caps.Contains(irc.CapabilityBouncerNetworks) ||
		!caps.Contains(irc.CapabilityBouncerNetworksNotify) {
		t.Fatalf("capabilities = %v", caps)
	}
	snapshot := fixture.session.BouncerNetworks()
	if len(snapshot) != 2 {
		t.Fatalf("snapshot = %+v", snapshot)
	}
	first, ok := bouncerByID(snapshot, "1")
	if !ok {
		t.Fatal("network 1 missing")
	}
	second, ok := bouncerByID(snapshot, "2")
	if !ok {
		t.Fatal("network 2 missing")
	}
	if first.Name != "My Awesome Network" || first.Host != "irc.example" ||
		first.TLS || first.Port != 6667 || first.Pass != "s3cret" {
		t.Fatalf("network 1 = %+v", first)
	}
	if second.Name != "OFTC" || second.Host != "irc.oftc.net" ||
		!second.TLS || second.Port != 6697 {
		t.Fatalf("network 2 = %+v", second)
	}

	fixture.inject("BOUNCER NETWORK 1 not-a-real-key=nope\r\n" +
		"bouncer network 1 name=;state=connected;error=timed\\sout\r\n" +
		"BOUNCER NETWORK 2 *\r\n")
	updated := fixture.session.BouncerNetworks()
	if len(updated) != 1 {
		t.Fatalf("updated = %+v", updated)
	}
	kept, ok := bouncerByID(updated, "1")
	if !ok || kept.Name != "" || kept.Host != "irc.example" || kept.TLS ||
		kept.Port != 6667 || kept.Pass != "s3cret" ||
		kept.State != irc.BouncerConnected || kept.Error != "timed out" {
		t.Fatalf("kept = %+v ok=%v", kept, ok)
	}
	if _, still := bouncerByID(updated, "2"); still {
		t.Fatal("network 2 must be removed")
	}

	if !fixture.session.ListBouncerNetworks() ||
		!fixture.wrote("BOUNCER LISTNETWORKS\r\n") {
		t.Fatal("LISTNETWORKS was not written")
	}
	fixture.inject(":server BATCH +list soju.im/bouncer-networks\r\n" +
		"@batch=list BOUNCER NETWORK 9 host=irc.libera.chat;name=Libera\r\n" +
		":server BATCH -list\r\n")
	replaced := fixture.session.BouncerNetworks()
	libera, ok := bouncerByID(replaced, "9")
	if len(replaced) != 1 || !ok || libera.Name != "Libera" ||
		libera.Host != "irc.libera.chat" || !libera.TLS || libera.Port != 6697 {
		t.Fatalf("replaced = %+v", replaced)
	}
	if _, still := bouncerByID(replaced, "1"); still {
		t.Fatal("snapshot must drop networks the batch omitted")
	}
}

func TestBouncerBindFollowsSaslAndPrecedesCapEnd(t *testing.T) {
	config := bouncerConfig()
	config.Password = "secret"
	config.BouncerBindNetID = "42"
	fixture := newSessionFixture(t, config)
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sasl=PLAIN soju.im/bouncer-networks " +
		"soju.im/bouncer-networks-notify\r\n" +
		":server CAP omairc ACK :sasl soju.im/bouncer-networks " +
		"soju.im/bouncer-networks-notify\r\n" +
		"AUTHENTICATE +\r\n")
	if !fixture.wrote("AUTHENTICATE PLAIN\r\n") ||
		fixture.wrote("BOUNCER BIND 42\r\n") ||
		fixture.wrote("CAP END\r\n") {
		t.Fatalf("frames before 903 = %q", fixture.frames())
	}

	fixture.inject(":server 903 omairc :SASL successful\r\n")
	frames := fixture.frames()
	authenticate := -1
	bind := -1
	capEnd := -1
	for index, frame := range frames {
		if strings.HasPrefix(frame, "AUTHENTICATE ") &&
			frame != "AUTHENTICATE PLAIN\r\n" &&
			frame != "AUTHENTICATE +\r\n" {
			authenticate = index
		}
		if frame == "BOUNCER BIND 42\r\n" {
			bind = index
		}
		if frame == "CAP END\r\n" {
			capEnd = index
		}
	}
	if authenticate < 0 || bind <= authenticate || capEnd <= bind ||
		countFrames(frames, "BOUNCER BIND 42\r\n") != 1 {
		t.Fatalf("frames after 903 = %q", frames)
	}

	fixture.inject(":server 001 omairc :Welcome\r\n" +
		":server CAP omairc NEW :soju.im/bouncer-networks\r\n" +
		":server CAP omairc ACK :soju.im/bouncer-networks\r\n")
	if fixture.session.State() != StateRegistered ||
		countFrames(fixture.frames(), "BOUNCER BIND 42\r\n") != 1 {
		t.Fatalf("state = %v frames = %q", fixture.session.State(), fixture.frames())
	}
}

func TestBouncerCommandsMatchRepliesAndFailures(t *testing.T) {
	fixture := newSessionFixture(t, bouncerConfig())
	if fixture.session.ListBouncerNetworks() {
		t.Fatal("LISTNETWORKS must wait until the capability is on")
	}
	registerBouncerCaps(fixture)
	if fixture.session.State() != StateRegistered {
		t.Fatalf("state = %v", fixture.session.State())
	}

	added := []irc.BouncerAttributeValue{
		{Key: "host", Value: "irc.oftc.net"},
		{Key: "name", Value: "My Network"},
		{Key: "pass", Value: "s3cret"},
	}
	if !fixture.session.AddBouncerNetwork(added) ||
		!fixture.wrote("BOUNCER ADDNETWORK host=irc.oftc.net;name=My\\sNetwork;pass=s3cret\r\n") {
		t.Fatalf("add frames = %q", fixture.frames())
	}
	fixture.inject("BOUNCER ADDNETWORK 44\r\n")

	changed := []irc.BouncerAttributeValue{{Key: "name", Value: "Renamed"}}
	if !fixture.session.ChangeBouncerNetwork("44", changed) ||
		!fixture.wrote("BOUNCER CHANGENETWORK 44 name=Renamed\r\n") {
		t.Fatal("CHANGENETWORK was not written")
	}
	fixture.inject("BOUNCER CHANGENETWORK 44\r\n")

	if !fixture.session.DeleteBouncerNetwork("44") ||
		!fixture.wrote("BOUNCER DELNETWORK 44\r\n") {
		t.Fatal("DELNETWORK was not written")
	}
	fixture.inject("BOUNCER DELNETWORK 44\r\n")

	again := []irc.BouncerAttributeValue{{Key: "host", Value: "irc.example"}}
	if !fixture.session.AddBouncerNetwork(again) ||
		!fixture.session.ChangeBouncerNetwork("44", changed) ||
		!fixture.session.AddBouncerNetwork(again) ||
		!fixture.session.AddBouncerNetwork(again) ||
		!fixture.session.AddBouncerNetwork(again) ||
		!fixture.session.AddBouncerNetwork(again) {
		t.Fatal("follow-up commands were not written")
	}

	fixture.inject("FAIL BOUNCER UNKNOWN_COMMAND FOO :Unknown subcommand\r\n" +
		"FAIL BOUNCER ACCOUNT_REQUIRED BIND :Authentication required\r\n" +
		"FAIL BOUNCER REGISTRATION_IS_COMPLETED BIND " +
		":Cannot bind to a network after registration\r\n" +
		"FAIL BOUNCER INVALID_NETID CHANGENETWORK 99 :Network not found\r\n" +
		"FAIL BOUNCER INVALID_ATTRIBUTE ADDNETWORK * host :Invalid attribute value\r\n" +
		"FAIL BOUNCER READ_ONLY_ATTRIBUTE ADDNETWORK * state :Read-only attribute\r\n" +
		"FAIL BOUNCER UNKNOWN_ATTRIBUTE ADDNETWORK * extra :Unknown attribute\r\n" +
		"FAIL BOUNCER NEED_ATTRIBUTE ADDNETWORK * host :Missing required attribute\r\n")
	if fixture.session.State() != StateRegistered {
		t.Fatalf("state = %v", fixture.session.State())
	}

	results := fixture.session.BouncerCommandResults()
	if len(results) != 11 {
		t.Fatalf("results = %+v", results)
	}
	if !results[0].OK || results[0].Subcommand != "ADDNETWORK" || results[0].NetID != "44" ||
		!results[1].OK || results[1].Subcommand != "CHANGENETWORK" || results[1].NetID != "44" ||
		!results[2].OK || results[2].Subcommand != "DELNETWORK" || results[2].NetID != "44" {
		t.Fatalf("successes = %+v", results[:3])
	}
	failures := results[3:]
	want := []irc.BouncerCommandResult{
		{Subcommand: "FOO", Code: "UNKNOWN_COMMAND", Description: "Unknown subcommand"},
		{Subcommand: "BIND", Code: "ACCOUNT_REQUIRED", Description: "Authentication required"},
		{Subcommand: "BIND", Code: "REGISTRATION_IS_COMPLETED", Description: "Cannot bind to a network after registration"},
		{Subcommand: "CHANGENETWORK", Code: "INVALID_NETID", NetID: "99", Description: "Network not found"},
		{Subcommand: "ADDNETWORK", Code: "INVALID_ATTRIBUTE", NetID: "*", Attribute: "host", Description: "Invalid attribute value"},
		{Subcommand: "ADDNETWORK", Code: "READ_ONLY_ATTRIBUTE", NetID: "*", Attribute: "state", Description: "Read-only attribute"},
		{Subcommand: "ADDNETWORK", Code: "UNKNOWN_ATTRIBUTE", NetID: "*", Attribute: "extra", Description: "Unknown attribute"},
		{Subcommand: "ADDNETWORK", Code: "NEED_ATTRIBUTE", NetID: "*", Attribute: "host", Description: "Missing required attribute"},
	}
	for index, expected := range want {
		got := failures[index]
		if got.OK || got.Subcommand != expected.Subcommand || got.Code != expected.Code ||
			got.NetID != expected.NetID || got.Attribute != expected.Attribute ||
			got.Description != expected.Description {
			t.Fatalf("failure %d = %+v, want %+v", index, got, expected)
		}
	}
}

func TestBouncerPassNeverReachesStatus(t *testing.T) {
	fixture := newSessionFixture(t, bouncerConfig())
	registerBouncerCaps(fixture)
	added := []irc.BouncerAttributeValue{
		{Key: "host", Value: "irc.example"},
		{Key: "pass", Value: "upstream-pass"},
	}
	if !fixture.session.AddBouncerNetwork(added) {
		t.Fatal("ADDNETWORK was not written")
	}
	fixture.inject("BOUNCER NETWORK 7 host=irc.example;pass=upstream-pass\r\n" +
		"FAIL BOUNCER NEED_ATTRIBUTE ADDNETWORK * host " +
		":Invalid pass=s3cret-value\r\n")
	stored, ok := bouncerByID(fixture.session.BouncerNetworks(), "7")
	if !ok || stored.Pass != "upstream-pass" {
		t.Fatalf("stored = %+v ok=%v", stored, ok)
	}
	if fixture.handler.anyFieldContains("upstream-pass") ||
		fixture.handler.anyFieldContains("s3cret-value") {
		t.Fatalf("status = %q", fixture.handler.statusTexts())
	}
	if got := fixture.handler.textFor("FAIL"); got != "Invalid pass=***" {
		t.Fatalf("FAIL status = %q", got)
	}
}
