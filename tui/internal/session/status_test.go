package session

import (
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// statusTestNow is the injected clock for direct status-entry classification.
// Status entries are stamped with the wall clock at classification time (the
// IRCv3 time tag is not consulted), so a fixed value keeps the tests
// deterministic.
var statusTestNow = time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)

// statusTextsOfLabel returns the text of every recorded entry with label.
func statusTextsOfLabel(handler *sessionTestHandler, label string) []string {
	var texts []string
	for _, entry := range handler.status {
		if entry.Label() == label {
			texts = append(texts, entry.Text())
		}
	}
	return texts
}

// TestSessionPingAndWelcomeProduceStatusEntries ports
// SessionTest::pingAndWelcomeProduceStatusEntries.
func TestSessionPingAndWelcomeProduceStatusEntries(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		"PING :abc\r\n" +
		":server 001 omairc :Welcome\r\n")

	if fixture.handler.hasLabel("PING") {
		t.Fatal("PING must not reach Status")
	}
	if !fixture.handler.hasLabel("CAP") {
		t.Fatal("CAP must reach Status")
	}
	if !fixture.handler.anyFieldContains("Server supports: multi-prefix") {
		t.Fatal("the CAP LS text is missing")
	}
	if !fixture.handler.hasLabel("001") {
		t.Fatal("001 must reach Status")
	}
}

// TestSessionStatusKeepListOmitsProtocolDump ports
// SessionTest::statusKeepListOmitsProtocolDump.
func TestSessionStatusKeepListOmitsProtocolDump(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.AutojoinChannels = nil
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(
		":server CAP omairc LS :batch chathistory\r\n" +
			":server CAP omairc ACK :batch chathistory\r\n" +
			":server 001 omairc :Welcome\r\n" +
			":server 372 omairc :- motd line\r\n" +
			":server 376 omairc :End of MOTD\r\n" +
			":omairc!u@h JOIN :#omarchy\r\n" +
			":alice!u@h PRIVMSG #omarchy :hello there\r\n" +
			"PING :abc\r\n" +
			":server PONG :abc\r\n" +
			":server 353 omairc = #omarchy :@omairc alice\r\n" +
			":server 366 omairc #omarchy :End of NAMES\r\n" +
			":alice!u@h PRIVMSG #omarchy :\x01ACTION waves\x01\r\n" +
			":NickServ!NickServ@services NOTICE omairc :Please identify\r\n" +
			":irc.host BATCH +hx chathistory #omarchy\r\n" +
			"@batch=hx :alice!u@h PRIVMSG #omarchy :older replay\r\n" +
			":irc.host BATCH -hx\r\n" +
			":lena!u@h INVITE omairc :#lab\r\n" +
			":server 404 omairc #omarchy :Cannot send to channel\r\n")

	if !fixture.handler.hasLabel("001") || !fixture.handler.hasLabel("CAP") {
		t.Fatal("001 and CAP must reach Status")
	}
	if !fixture.handler.anyFieldContains("Server supports: batch | chathistory") {
		t.Fatal("the CAP LS text is missing")
	}
	if !fixture.handler.anyFieldContains("Acknowledged: batch | chathistory") {
		t.Fatal("the CAP ACK text is missing")
	}
	if !fixture.handler.hasLabel("372") || !fixture.handler.hasLabel("376") {
		t.Fatal("372 and 376 must reach Status")
	}
	if !fixture.handler.anyFieldContains("-NickServ- Please identify") {
		t.Fatal("the NickServ notice is missing")
	}
	if !fixture.handler.hasLabel("INVITE") {
		t.Fatal("INVITE must reach Status")
	}
	if !fixture.handler.anyFieldContains("lena invited you to #lab") {
		t.Fatal("the INVITE text is missing")
	}
	if !fixture.handler.hasLabel("404") {
		t.Fatal("404 must reach Status")
	}
	for _, label := range []string{"PING", "PONG", "JOIN", "PRIVMSG", "353", "ACTION", "CTCP", "BATCH"} {
		if fixture.handler.hasLabel(label) {
			t.Fatalf("%s must not reach Status", label)
		}
	}
	for _, needle := range []string{"hello there", "older replay", "ACTION waves"} {
		if fixture.handler.anyFieldContains(needle) {
			t.Fatalf("%q must not reach Status", needle)
		}
	}
}

// TestSessionIncomingCapMyinfoAndLusersFormatting ports
// SessionTest::incomingCapMyinfoAndLusersFormatting.
func TestSessionIncomingCapMyinfoAndLusersFormatting(t *testing.T) {
	fourParam := irc.IncomingAll("libera",
		mustParse(t, ":server 004 omairc demo.omairc OmaircDemo iw abc"), "", statusTestNow)
	if len(fourParam) != 4 {
		t.Fatalf("entries = %d, want 4", len(fourParam))
	}
	if fourParam[0].Label() != "004" || fourParam[0].Text() != "Host: demo.omairc" {
		t.Fatalf("entry 0 = %q/%q, want 004/Host: demo.omairc", fourParam[0].Label(), fourParam[0].Text())
	}
	if fourParam[1].Text() != "IRCd: OmaircDemo" ||
		fourParam[2].Text() != "User modes: iw" ||
		fourParam[3].Text() != "Channel modes: abc" {
		t.Fatalf("entries = %q", statusTexts(fourParam))
	}

	fiveParam := irc.IncomingAll("libera",
		mustParse(t, ":server 004 omairc demo.omairc OmaircDemo iw abc ABC"), "", statusTestNow)
	if len(fiveParam) != 5 || fiveParam[4].Text() != "Parametric channel modes: ABC" {
		t.Fatalf("fiveParam = %q", statusTexts(fiveParam))
	}

	capLs := irc.Incoming("libera", mustParse(t, ":server CAP omairc LS :batch chathistory echo-message"), "", statusTestNow)
	if capLs.Label() != "CAP" || capLs.Text() != "Server supports: batch | chathistory | echo-message" {
		t.Fatalf("capLs = %q/%q", capLs.Label(), capLs.Text())
	}
	if capLs.Severity() != irc.LogSeverityInfo {
		t.Fatalf("capLs severity = %v, want Info", capLs.Severity())
	}

	capAck := irc.Incoming("libera", mustParse(t, ":server CAP omairc ACK :batch chathistory"), "", statusTestNow)
	if capAck.Text() != "Acknowledged: batch | chathistory" {
		t.Fatalf("capAck = %q", capAck.Text())
	}

	capNickCollision := irc.Incoming("libera", mustParse(t, ":server CAP ACK LS :batch"), "", statusTestNow)
	if capNickCollision.Label() != "CAP" || capNickCollision.Text() != "Server supports: batch" {
		t.Fatalf("capNickCollision = %q/%q", capNickCollision.Label(), capNickCollision.Text())
	}

	capLsContinuation := irc.Incoming("libera", mustParse(t, ":server CAP * LS * :cap-one cap-two"), "", statusTestNow)
	if capLsContinuation.Text() != "Server supports: cap-one | cap-two" {
		t.Fatalf("capLsContinuation = %q", capLsContinuation.Text())
	}

	lusers252 := irc.Incoming("libera", mustParse(t, ":server 252 omairc 1 :IRC Operators online"), "", statusTestNow)
	if lusers252.Label() != "252" || lusers252.Text() != "1 IRC Operators online" {
		t.Fatalf("lusers252 = %q/%q", lusers252.Label(), lusers252.Text())
	}

	lusers265 := irc.Incoming("libera", mustParse(t, ":server 265 omairc 10 20 :Current local users 10, max 20"), "", statusTestNow)
	if lusers265.Label() != "265" || lusers265.Text() != "Current local users 10, max 20" {
		t.Fatalf("lusers265 = %q/%q", lusers265.Label(), lusers265.Text())
	}
}

func statusTexts(entries []irc.StatusEntry) []string {
	texts := make([]string, len(entries))
	for index, entry := range entries {
		texts[index] = entry.Text()
	}
	return texts
}

// TestSessionErgoHistoryReplayPrivmsgStaysOffStatus ports
// SessionTest::ergoHistoryReplayPrivmsgStaysOffStatus.
func TestSessionErgoHistoryReplayPrivmsgStaysOffStatus(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.AutojoinChannels = nil
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(
		":server CAP omairc LS :batch chathistory\r\n" +
			":server CAP omairc ACK :batch chathistory\r\n" +
			":server 001 omairc :Welcome\r\n" +
			":server 005 omairc CHANTYPES=# :are supported by this server\r\n" +
			":omairc!u@h JOIN :#ergo\r\n" +
			":irc.host BATCH +hx chathistory #ergo\r\n" +
			"@batch=hx :HistServ!HistServ@internal PRIVMSG #ergo :alice joined the channel\r\n" +
			"@batch=hx :HistServ!HistServ@internal PRIVMSG #ergo :bob quit (Ping timeout)\r\n" +
			":irc.host BATCH -hx\r\n" +
			":NickServ!NickServ@services PRIVMSG omairc :Please identify\r\n")

	if fixture.handler.anyFieldContains("joined the channel") ||
		fixture.handler.anyFieldContains("quit (Ping timeout)") {
		t.Fatal("replayed history must stay off Status")
	}
	if !fixture.handler.anyFieldContains("Please identify") {
		t.Fatal("the NickServ message must reach Status")
	}
}

// TestSessionOutgoingCapReqKeptCapEndDropped ports
// SessionTest::outgoingCapReqKeptCapEndDropped.
func TestSessionOutgoingCapReqKeptCapEndDropped(t *testing.T) {
	if !irc.StatusKeepsOutgoing([]byte("CAP REQ :multi-prefix\r\n")) {
		t.Fatal("CAP REQ must be kept")
	}
	if irc.StatusKeepsOutgoing([]byte("CAP END\r\n")) {
		t.Fatal("CAP END must be dropped")
	}
	if irc.StatusKeepsOutgoing([]byte("CAP LS 302\r\n")) {
		t.Fatal("CAP LS must be dropped")
	}

	entry := irc.Outgoing("libera", []byte("CAP REQ :multi-prefix chghost\r\n"), "", statusTestNow)
	if entry.Label() != "CAP" || entry.Text() != "Requesting: multi-prefix | chghost" {
		t.Fatalf("entry = %q/%q", entry.Label(), entry.Text())
	}
	if entry.Severity() != irc.LogSeverityInfo {
		t.Fatalf("severity = %v, want Info", entry.Severity())
	}
}

// TestSessionOutgoingWhoisAndCtcpQueriesStayOffStatus ports
// SessionTest::outgoingWhoisAndCtcpQueriesStayOffStatus.
func TestSessionOutgoingWhoisAndCtcpQueriesStayOffStatus(t *testing.T) {
	if irc.StatusKeepsOutgoing([]byte("WHOIS lena lena\r\n")) {
		t.Fatal("WHOIS must be dropped")
	}
	if irc.StatusKeepsOutgoing([]byte("PRIVMSG lena :\x01VERSION\x01\r\n")) ||
		irc.StatusKeepsOutgoing([]byte("PRIVMSG lena :\x01TIME\x01\r\n")) ||
		irc.StatusKeepsOutgoing([]byte("PRIVMSG lena :\x01PING 1\x01\r\n")) {
		t.Fatal("CTCP queries must be dropped")
	}
	if !irc.StatusKeepsOutgoing([]byte("PASS secret\r\n")) {
		t.Fatal("PASS must be kept")
	}
}

// TestSessionConfiguredPasswordNeverAppearsInStatusEntries ports
// SessionTest::configuredPasswordNeverAppearsInStatusEntries.
func TestSessionConfiguredPasswordNeverAppearsInStatusEntries(t *testing.T) {
	passConfig := sessionTestConfig(sessionTestNetworkID)
	passConfig.Password = "hunter2"
	passFixture := newSessionFixture(t, passConfig)
	passFixture.connectTLS()
	passFixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		"PING :abc\r\n" +
		":server 001 omairc :Welcome\r\n")
	if passFixture.handler.hasLabel("PING") {
		t.Fatal("PING must not reach Status")
	}
	if !passFixture.handler.hasLabel("CAP") || !passFixture.handler.hasLabel("001") {
		t.Fatal("CAP and 001 must reach Status")
	}
	if !passFixture.handler.anyFieldContains("Server supports: multi-prefix") {
		t.Fatal("the CAP LS text is missing")
	}
	if !passFixture.handler.hasLabel("PASS") {
		t.Fatal("PASS must reach Status")
	}
	if passFixture.handler.anyFieldContains("hunter2") {
		t.Fatal("the server password leaked into Status")
	}

	saslConfig := sessionTestConfig("network-sasl")
	saslConfig.Password = "hunter2"
	saslFixture := newSessionFixture(t, saslConfig)
	saslFixture.connectTLS()
	saslFixture.inject(":server CAP omairc LS :sasl=PLAIN\r\n" +
		":server CAP omairc ACK :sasl\r\n" +
		"AUTHENTICATE +\r\n" +
		":server 903 omairc :SASL successful\r\n" +
		":server 001 omairc :Welcome\r\n")
	if saslFixture.handler.hasLabel("AUTHENTICATE") || saslFixture.handler.anyFieldContains("AUTHENTICATE") {
		t.Fatal("AUTHENTICATE must not reach Status")
	}
	if !saslFixture.handler.hasLabel("903") || !saslFixture.handler.hasLabel("001") {
		t.Fatal("903 and 001 must reach Status")
	}
	if saslFixture.handler.anyFieldContains("hunter2") {
		t.Fatal("the SASL password leaked into Status")
	}
	for _, entry := range saslFixture.handler.status {
		if entry.Label() == "PASS" && entry.Text() != "PASS ***" {
			t.Fatalf("PASS text = %q, want PASS ***", entry.Text())
		}
	}
}

// TestSessionSaslAccountNeverAppearsInStatusEntries ports
// SessionTest::saslAccountNeverAppearsInStatusEntries.
func TestSessionSaslAccountNeverAppearsInStatusEntries(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.SASLAccount = "joe/libera"
	config.Password = "hunter2"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :sasl=PLAIN\r\n" +
		":server CAP omairc ACK :sasl\r\n" +
		"AUTHENTICATE +\r\n" +
		":server 903 omairc :SASL successful\r\n")

	if fixture.handler.hasLabel("AUTHENTICATE") || fixture.handler.anyFieldContains("AUTHENTICATE") {
		t.Fatal("AUTHENTICATE must not reach Status")
	}
	if !fixture.handler.hasLabel("903") {
		t.Fatal("903 must reach Status")
	}
	if fixture.handler.anyFieldContains("hunter2") || fixture.handler.anyFieldContains("joe/libera") {
		t.Fatal("an SASL secret leaked into Status")
	}
	encoded := base64Encode([]byte("joe/libera\x00joe/libera\x00hunter2"))
	if fixture.handler.anyFieldContains(encoded) {
		t.Fatal("the encoded SASL payload leaked into Status")
	}
}

// TestSessionKeyedJoinIsRedactedInStatusEntries ports
// SessionTest::keyedJoinIsRedactedInStatusEntries.
func TestSessionKeyedJoinIsRedactedInStatusEntries(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n")

	key := "hunter2"
	target, ok := irc.MakeJoinTarget("#secret", &key, irc.NewServerFeatures())
	if !ok {
		t.Fatal("the keyed join target must be valid")
	}
	if !fixture.session.Join(target) {
		t.Fatal("join must succeed")
	}
	if got := fixture.lastFrame(); got != "JOIN #secret hunter2\r\n" {
		t.Fatalf("last frame = %q, want the keyed JOIN", got)
	}
	if fixture.handler.hasLabel("JOIN") || fixture.handler.anyFieldContains("hunter2") {
		t.Fatal("the channel key leaked into Status")
	}

	unkeyed := irc.Outgoing(sessionTestNetworkID, []byte("JOIN #omarchy\r\n"), "", statusTestNow)
	if unkeyed.Text() != "JOIN #omarchy" {
		t.Fatalf("unkeyed text = %q", unkeyed.Text())
	}
	keyed := irc.Outgoing(sessionTestNetworkID, []byte("JOIN #secret hunter2\r\n"), "", statusTestNow)
	if keyed.Text() != "JOIN #secret ***" || strings.Contains(keyed.Text(), "hunter2") {
		t.Fatalf("keyed text = %q, want the redacted key", keyed.Text())
	}
}

// TestSessionServiceIdentifyIsRedactedInStatusEntries ports
// SessionTest::serviceIdentifyIsRedactedInStatusEntries.
func TestSessionServiceIdentifyIsRedactedInStatusEntries(t *testing.T) {
	assertOutgoing := func(want string, line, channelTypes string) {
		t.Helper()
		entry := irc.Outgoing(sessionTestNetworkID, []byte(line), channelTypes, statusTestNow)
		if entry.Text() != want {
			t.Fatalf("Outgoing(%q) = %q, want %q", line, entry.Text(), want)
		}
		if strings.Contains(entry.Text(), "s3cret") || strings.Contains(entry.Text(), "hunter2") {
			t.Fatalf("Outgoing(%q) leaked a secret", line)
		}
	}

	assertOutgoing("PRIVMSG nickserv :IDENTIFY ***", "PRIVMSG nickserv :identify my_nick s3cret\r\n", "")
	assertOutgoing("PRIVMSG NS :IDENTIFY ***", "PRIVMSG NS :IDENTIFY account hunter2\r\n", "#&")
	assertOutgoing("PRIVMSG nickserv :IDENTIFY ***", "PRIVMSG nickserv :identify\tmy_nick\ts3cret\r\n", "")
	assertOutgoing("PRIVMSG nickserv :IDENTIFY ***", "PRIVMSG nickserv IDENTIFY my_nick s3cret\r\n", "")
	assertOutgoing("PRIVMSG nickserv :IDENTIFY ***", "PRIVMSG nickserv :\x01identify my_nick s3cret\x01\r\n", "")
	assertOutgoing("PRIVMSG nickserv :IDENTIFY ***", "PRIVMSG nickserv : \x01identify my_nick s3cret\x01 \r\n", "")
	assertOutgoing("PRIVMSG nickserv :IDENTIFY ***", "PRIVMSG nickserv :identif my_nick s3cret\r\n", "")
	assertOutgoing("PRIVMSG -serv :IDENTIFY ***", "PRIVMSG -serv :identify my_nick s3cret\r\n", "")
	assertOutgoing("PRIVMSG [serv :IDENTIFY ***", "PRIVMSG [serv :identify my_nick s3cret\r\n", "")
	assertOutgoing("PRIVMSG nickserv :SET PASSWORD ***", "PRIVMSG nickserv :set password s3cret\r\n", "")
	assertOutgoing("PRIVMSG nickserv :set email user@example.net", "PRIVMSG nickserv :set email user@example.net\r\n", "")
	assertOutgoing("PASS ***", "PASS\thunter2\r\n", "")
	assertOutgoing("NOTICE NickServ :GHOST ***", "NOTICE NickServ :ghost old_nick s3cret\r\n", "")
	assertOutgoing("OPER admin ***", "OPER admin s3cret\r\n", "")
	assertOutgoing("MODE #omarchy +k ***", "MODE #omarchy +k s3cret\r\n", "")
	assertOutgoing("MODE #omarchy +ok alice ***", "MODE #omarchy +ok alice s3cret\r\n", "")
	assertOutgoing("MODE #omarchy +ko *** alice", "MODE #omarchy +ko s3cret alice\r\n", "")
	assertOutgoing("MODE #omarchy +fk ***", "MODE #omarchy +fk 10 s3cret\r\n", "")
	assertOutgoing("MODE #omarchy +ak ***", "MODE #omarchy +ak s3cret\r\n", "")
	assertOutgoing("MODE #omarchy +o alice", "MODE #omarchy +o alice\r\n", "")
	assertOutgoing("PRIVMSG nickserv,#discuss :IDENTIFY ***", "PRIVMSG nickserv,#discuss :identify my_nick s3cret\r\n", "")
	assertOutgoing("PRIVMSG #discuss,nickserv :IDENTIFY ***", "PRIVMSG #discuss,nickserv :identify my_nick s3cret\r\n", "")
	assertOutgoing("PRIVMSG NickServ@services :IDENTIFY ***", "PRIVMSG NickServ@services :identify my_nick s3cret\r\n", "")
	assertOutgoing("PRIVMSG NickServ!ns@services :IDENTIFY ***", "PRIVMSG NickServ!ns@services :identify my_nick s3cret\r\n", "")
	assertOutgoing("PRIVMSG helper!u@services :IDENTIFY ***", "PRIVMSG helper!u@services :identify my_nick s3cret\r\n", "")

	passTabs := irc.Outgoing(sessionTestNetworkID, []byte("PASS\thunter2\r\n"), "", statusTestNow)
	if passTabs.Label() != "PASS" || strings.Contains(passTabs.Label(), "hunter2") {
		t.Fatalf("PASS label = %q", passTabs.Label())
	}

	keyedModes := irc.Incoming(sessionTestNetworkID, mustParse(t, ":irc 324 omairc #omarchy +k s3cret"), "", statusTestNow)
	if keyedModes.Text() != "#omarchy +k ***" {
		t.Fatalf("keyedModes = %q", keyedModes.Text())
	}
	listedKeyLimit := irc.Incoming(sessionTestNetworkID, mustParse(t, ":irc 324 omairc #omarchy +kl s3cret 40"), "", statusTestNow)
	if listedKeyLimit.Text() != "#omarchy +kl *** 40" {
		t.Fatalf("listedKeyLimit = %q", listedKeyLimit.Text())
	}
	listedLimitKey := irc.Incoming(sessionTestNetworkID, mustParse(t, ":irc 324 omairc #omarchy +lk 40 s3cret"), "", statusTestNow)
	if listedLimitKey.Text() != "#omarchy +lk 40 ***" {
		t.Fatalf("listedLimitKey = %q", listedLimitKey.Text())
	}
	listedModes := irc.Incoming(sessionTestNetworkID, mustParse(t, ":irc 324 omairc #omarchy +nt"), "", statusTestNow)
	if listedModes.Text() != "#omarchy +nt" {
		t.Fatalf("listedModes = %q", listedModes.Text())
	}
	extendedJoin := irc.Incoming(sessionTestNetworkID, mustParse(t, ":alice!u@h JOIN #omarchy alice :Alice"), "", statusTestNow)
	if extendedJoin.Text() != "#omarchy alice Alice" {
		t.Fatalf("extendedJoin = %q", extendedJoin.Text())
	}
	literalStar := irc.Incoming(sessionTestNetworkID, mustParse(t, ":irc MODE ***"), "", statusTestNow)
	if literalStar.Text() != "***" {
		t.Fatalf("literalStar = %q", literalStar.Text())
	}
	incomingPass := irc.Incoming(sessionTestNetworkID, mustParse(t, "PASS hunter2"), "", statusTestNow)
	if incomingPass.Text() != "PASS ***" || strings.Contains(incomingPass.Text(), "hunter2") {
		t.Fatalf("incomingPass = %q", incomingPass.Text())
	}
}

// TestSessionChannelTalkAboutServicesStaysReadable ports
// SessionTest::channelTalkAboutServicesStaysReadable.
func TestSessionChannelTalkAboutServicesStaysReadable(t *testing.T) {
	talk := irc.Outgoing(sessionTestNetworkID,
		[]byte("PRIVMSG #discuss :I told NickServ to IDENTIFY later\r\n"), "", statusTestNow)
	if talk.Text() != "PRIVMSG #discuss :I told NickServ to IDENTIFY later" {
		t.Fatalf("talk = %q", talk.Text())
	}
	channel := irc.Outgoing(sessionTestNetworkID,
		[]byte("PRIVMSG #serv :identify my_nick s3cret\r\n"), "", statusTestNow)
	if channel.Text() != "PRIVMSG #serv :identify my_nick s3cret" {
		t.Fatalf("channel = %q", channel.Text())
	}
	channelWithAt := irc.Outgoing(sessionTestNetworkID,
		[]byte("PRIVMSG #help@services :IDENTIFY examples stay visible\r\n"), "#&", statusTestNow)
	if channelWithAt.Text() != "PRIVMSG #help@services :IDENTIFY examples stay visible" {
		t.Fatalf("channelWithAt = %q", channelWithAt.Text())
	}
	tildeChannel := irc.Outgoing(sessionTestNetworkID,
		[]byte("PRIVMSG ~serv :identify my_nick s3cret\r\n"), "", statusTestNow)
	if tildeChannel.Text() != "PRIVMSG ~serv :identify my_nick s3cret" {
		t.Fatalf("tildeChannel = %q", tildeChannel.Text())
	}

	dollarLine := []byte("PRIVMSG $serv :identify my_nick s3cret\r\n")
	dollarChannel := irc.Outgoing(sessionTestNetworkID, dollarLine, "", statusTestNow)
	if dollarChannel.Text() != "PRIVMSG $serv :identify my_nick s3cret" {
		t.Fatalf("dollarChannel = %q", dollarChannel.Text())
	}
	dollarTyped := irc.Outgoing(sessionTestNetworkID, dollarLine, "$", statusTestNow)
	if dollarTyped.Text() != "PRIVMSG $serv :identify my_nick s3cret" {
		t.Fatalf("dollarTyped = %q", dollarTyped.Text())
	}
	dollarAsNick := irc.Outgoing(sessionTestNetworkID, dollarLine, "#", statusTestNow)
	if dollarAsNick.Text() != "PRIVMSG $serv :IDENTIFY ***" ||
		strings.Contains(dollarAsNick.Text(), "s3cret") {
		t.Fatalf("dollarAsNick = %q", dollarAsNick.Text())
	}
}

// TestSessionNegotiatedChannelTypesClassifyDollarTargets ports
// SessionTest::negotiatedChannelTypesClassifyDollarTargets.
func TestSessionNegotiatedChannelTypesClassifyDollarTargets(t *testing.T) {
	fixture := newSessionFixture(t, sessionTestConfig(sessionTestNetworkID))
	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n" +
		":server 005 omairc CHANTYPES=$ PREFIX=(ov)@+ :are supported\r\n")
	if !fixture.session.SendPrivmsg("$serv", "identify my_nick s3cret") {
		t.Fatal("sendPrivmsg must succeed")
	}
	if got := fixture.lastFrame(); got != "PRIVMSG $serv :identify my_nick s3cret\r\n" {
		t.Fatalf("last frame = %q", got)
	}
	dollarLine := []byte("PRIVMSG $serv :identify my_nick s3cret\r\n")
	if got := irc.Outgoing(sessionTestNetworkID, dollarLine, fixture.session.ChannelTypes(), statusTestNow).Text(); got != "PRIVMSG $serv :identify my_nick s3cret" {
		t.Fatalf("dollar channel text = %q", got)
	}

	fixture.inject(":server 005 omairc CHANTYPES=# PREFIX=(ov)@+ :are supported\r\n")
	if !fixture.session.SendPrivmsg("$serv", "identify my_nick s3cret") {
		t.Fatal("sendPrivmsg must succeed")
	}
	asNick := irc.Outgoing(sessionTestNetworkID, dollarLine, fixture.session.ChannelTypes(), statusTestNow)
	if asNick.Text() != "PRIVMSG $serv :IDENTIFY ***" || strings.Contains(asNick.Text(), "s3cret") {
		t.Fatalf("asNick = %q", asNick.Text())
	}

	fixture.inject(":server 005 omairc AWAYLEN=200 :CHANTYPES=$ are supported\r\n")
	if !fixture.session.SendPrivmsg("$serv", "identify my_nick s3cret") {
		t.Fatal("sendPrivmsg must succeed")
	}
	stillNick := irc.Outgoing(sessionTestNetworkID, dollarLine, fixture.session.ChannelTypes(), statusTestNow)
	if stillNick.Text() != "PRIVMSG $serv :IDENTIFY ***" || strings.Contains(stillNick.Text(), "s3cret") {
		t.Fatalf("stillNick = %q", stillNick.Text())
	}
}

// TestSessionServiceRepliesStayReadable ports
// SessionTest::serviceRepliesStayReadable.
func TestSessionServiceRepliesStayReadable(t *testing.T) {
	notice := irc.Incoming("libera",
		mustParse(t, ":NickServ!NickServ@services NOTICE me :Please identify"), "", statusTestNow)
	if notice.Text() != "-NickServ- Please identify" {
		t.Fatalf("notice = %q", notice.Text())
	}
	privmsg := irc.Incoming("libera",
		mustParse(t, ":NickServ!NickServ@services PRIVMSG me :This nickname is registered."), "", statusTestNow)
	if privmsg.Text() != "This nickname is registered." {
		t.Fatalf("privmsg = %q", privmsg.Text())
	}
}

// TestSessionIncomingInviteNamesNickAndChannel ports
// SessionTest::incomingInviteNamesNickAndChannel.
func TestSessionIncomingInviteNamesNickAndChannel(t *testing.T) {
	invite := irc.Incoming("libera", mustParse(t, ":alice!u@h INVITE omairc :#lab"), "", statusTestNow)
	if invite.Label() != "INVITE" || invite.Text() != "alice invited you to #lab" {
		t.Fatalf("invite = %q/%q", invite.Label(), invite.Text())
	}
	bare := irc.Incoming("libera", mustParse(t, "INVITE omairc :#lab"), "", statusTestNow)
	if bare.Text() != "omairc #lab" {
		t.Fatalf("bare = %q", bare.Text())
	}
}

// TestSessionSelfEchoToServiceIsRedacted ports
// SessionTest::selfEchoToServiceIsRedacted.
func TestSessionSelfEchoToServiceIsRedacted(t *testing.T) {
	cases := []struct {
		line string
		want string
	}{
		{":me!u@h PRIVMSG nickserv :identify my_nick s3cret", "IDENTIFY ***"},
		{":myserv!u@h PRIVMSG nickserv :identify my_nick s3cret", "IDENTIFY ***"},
		{":me!u@h PRIVMSG nickserv IDENTIFY my_nick s3cret", "IDENTIFY ***"},
		{":me!u@h PRIVMSG nickserv :\x01IDENTIFY my_nick s3cret\x01", "IDENTIFY ***"},
		{":me!u@h PRIVMSG nickserv :set password s3cret", "SET PASSWORD ***"},
	}
	for _, testCase := range cases {
		entry := irc.Incoming("libera", mustParse(t, testCase.line), "", statusTestNow)
		if entry.Text() != testCase.want {
			t.Fatalf("Incoming(%q) = %q, want %q", testCase.line, entry.Text(), testCase.want)
		}
		if strings.Contains(entry.Text(), "s3cret") {
			t.Fatalf("Incoming(%q) leaked a secret", testCase.line)
		}
	}

	ctcp := irc.Incoming("libera", mustParse(t, ":me!u@h PRIVMSG nickserv :\x01IDENTIFY my_nick s3cret\x01"), "", statusTestNow)
	if ctcp.Label() != "PRIVMSG" {
		t.Fatalf("ctcp label = %q, want PRIVMSG", ctcp.Label())
	}
	setEmail := irc.Incoming("libera", mustParse(t, ":me!u@h PRIVMSG nickserv :set email user@example.net"), "", statusTestNow)
	if setEmail.Text() != "set email user@example.net" {
		t.Fatalf("setEmail = %q", setEmail.Text())
	}
}

// TestSessionWhoisStatusLinesFormatKnownNumerics ports
// SessionTest::whoisStatusLinesFormatKnownNumerics.
func TestSessionWhoisStatusLinesFormatKnownNumerics(t *testing.T) {
	user := irc.Incoming("libera", mustParse(t, ":irc 311 omairc lena ~lena user/host * :Lena"), "", statusTestNow)
	if user.Label() != "whois" || user.Text() != "lena is ~lena@user/host (Lena)" {
		t.Fatalf("311 = %q/%q", user.Label(), user.Text())
	}
	if user.Severity() != irc.LogSeverityInfo {
		t.Fatalf("311 severity = %v, want Info", user.Severity())
	}

	channels := irc.Incoming("libera", mustParse(t, ":irc 319 omairc lena :#omarchy #desktop"), "", statusTestNow)
	if channels.Label() != "whois" || channels.Text() != "lena is on #omarchy #desktop" {
		t.Fatalf("319 = %q/%q", channels.Label(), channels.Text())
	}

	server := irc.Incoming("libera", mustParse(t, ":irc 312 omairc lena copper.libera.chat :London, UK"), "", statusTestNow)
	if server.Text() != "lena using copper.libera.chat (London, UK)" {
		t.Fatalf("312 = %q", server.Text())
	}

	away := irc.Incoming("libera", mustParse(t, ":irc 301 omairc lena :gone fishing"), "", statusTestNow)
	if away.Text() != "lena is away: gone fishing" {
		t.Fatalf("301 = %q", away.Text())
	}

	idle := irc.Incoming("libera", mustParse(t, ":irc 317 omairc lena 84 :seconds idle"), "", statusTestNow)
	if idle.Text() != "lena idle 84s" {
		t.Fatalf("317 = %q", idle.Text())
	}

	idleSignon := irc.Incoming("libera", mustParse(t, ":irc 317 omairc lena 84 1700000000 :seconds idle"), "", statusTestNow)
	if idleSignon.Text() != "lena idle 84s, signon 1700000000" {
		t.Fatalf("317 signon = %q", idleSignon.Text())
	}

	end := irc.Incoming("libera", mustParse(t, ":irc 318 omairc lena :End of /WHOIS list."), "", statusTestNow)
	if end.Label() != "whois" || end.Text() != "End of WHOIS for lena" {
		t.Fatalf("318 = %q/%q", end.Label(), end.Text())
	}
	if end.WhoisLine() == nil || !end.WhoisLine().Terminal() ||
		end.WhoisLine().Progress() != irc.WhoisTerminal {
		t.Fatalf("318 whois line = %+v, want terminal", end.WhoisLine())
	}

	account := irc.Incoming("libera", mustParse(t, ":irc 330 omairc lena pinkieval :is logged in as"), "", statusTestNow)
	if account.Text() != "lena is logged in as pinkieval" {
		t.Fatalf("330 = %q", account.Text())
	}

	secure := irc.Incoming("libera", mustParse(t, ":irc 671 omairc lena :is using a secure connection"), "", statusTestNow)
	if secure.Text() != "lena is using a secure connection" {
		t.Fatalf("671 = %q", secure.Text())
	}

	unknown := irc.Incoming("libera", mustParse(t, ":irc 335 omairc lena :bot"), "", statusTestNow)
	if unknown.Label() != "335" || unknown.Text() != "lena bot" {
		t.Fatalf("335 = %q/%q", unknown.Label(), unknown.Text())
	}

	shortUser := irc.Incoming("libera", mustParse(t, ":irc 311 omairc lena"), "", statusTestNow)
	if shortUser.Label() != "311" || shortUser.Text() != "lena" {
		t.Fatalf("short 311 = %q/%q", shortUser.Label(), shortUser.Text())
	}

	missing := irc.Incoming("libera", mustParse(t, ":irc 401 omairc lena :No such nick/channel"), "", statusTestNow)
	if missing.Label() != "401" || missing.Text() != "No such nick: lena" {
		t.Fatalf("401 = %q/%q", missing.Label(), missing.Text())
	}
	if missing.Severity() != irc.LogSeverityAlert {
		t.Fatalf("401 severity = %v, want Alert", missing.Severity())
	}
	if missing.WhoisLine() == nil || missing.WhoisLine().Progress() != irc.WhoisFailed {
		t.Fatalf("401 whois line = %+v, want failed", missing.WhoisLine())
	}

	welcome := irc.Incoming("libera", mustParse(t, ":server 001 omairc :Welcome"), "", statusTestNow)
	if welcome.Label() != "001" || welcome.Text() != "Welcome" {
		t.Fatalf("001 = %q/%q", welcome.Label(), welcome.Text())
	}
}

// TestSessionIncomingNoticeStatusLinesWrapSpeaker ports
// SessionTest::incomingNoticeStatusLinesWrapSpeaker.
func TestSessionIncomingNoticeStatusLinesWrapSpeaker(t *testing.T) {
	nickserv := irc.Incoming("libera", mustParse(t, ":NickServ!NickServ@services NOTICE omairc :Please identify"), "", statusTestNow)
	if nickserv.Label() != "NOTICE" || nickserv.Text() != "-NickServ- Please identify" {
		t.Fatalf("nickserv = %q/%q", nickserv.Label(), nickserv.Text())
	}
	auth := irc.Incoming("libera", mustParse(t, "NOTICE AUTH :*** Looking up your hostname..."), "", statusTestNow)
	if auth.Label() != "NOTICE" || auth.Text() != "-AUTH- *** Looking up your hostname..." {
		t.Fatalf("auth = %q/%q", auth.Label(), auth.Text())
	}
	server := irc.Incoming("libera", mustParse(t, ":copper.libera.chat NOTICE * :*** Found your hostname"), "", statusTestNow)
	if server.Text() != "-copper.libera.chat- *** Found your hostname" {
		t.Fatalf("server = %q", server.Text())
	}
	channel := irc.Incoming("libera", mustParse(t, ":alice!u@h NOTICE #omarchy :heads up"), "", statusTestNow)
	if channel.Text() != "-alice- heads up" {
		t.Fatalf("channel = %q", channel.Text())
	}
	bare := irc.Incoming("libera", mustParse(t, "NOTICE * :hello"), "", statusTestNow)
	if bare.Text() != "hello" || strings.HasPrefix(bare.Text(), "-- ") {
		t.Fatalf("bare = %q", bare.Text())
	}
	oneParam := irc.Incoming("libera", mustParse(t, "NOTICE AUTH"), "", statusTestNow)
	if oneParam.Text() != "-AUTH- " || oneParam.Text() == "-AUTH- AUTH" {
		t.Fatalf("oneParam = %q", oneParam.Text())
	}
}

// TestSessionIncomingCtcpNoticeStatusLines ports
// SessionTest::incomingCtcpNoticeStatusLines.
func TestSessionIncomingCtcpNoticeStatusLines(t *testing.T) {
	version := irc.Incoming("libera", mustParse(t, ":lena!u@h NOTICE omairc :\x01VERSION Omairc 0.4.0\x01"), "", statusTestNow)
	if version.Label() != "CTCP" || version.Text() != "VERSION reply from lena: Omairc 0.4.0" {
		t.Fatalf("version = %q/%q", version.Label(), version.Text())
	}
	if version.CtcpReplyLine() == nil ||
		version.CtcpReplyLine().Nick() != "lena" ||
		version.CtcpReplyLine().Command() != "VERSION" {
		t.Fatalf("version ctcp line = %+v", version.CtcpReplyLine())
	}

	// PING reports lag against the injected clock, so a sent timestamp 25ms
	// before statusTestNow renders exactly "25 ms". The server-time tag is
	// deliberately absent: the Qt core stamps the wall clock, never the tag.
	sent := statusTestNow.UnixMilli() - 25
	ping := irc.Incoming("libera",
		mustParse(t, ":lena!u@h NOTICE omairc :\x01PING "+strconv.FormatInt(sent, 10)+"\x01"), "", statusTestNow)
	if ping.Label() != "CTCP" {
		t.Fatalf("ping label = %q, want CTCP", ping.Label())
	}
	if ping.Text() != "PING reply from lena: 25 ms" {
		t.Fatalf("ping text = %q", ping.Text())
	}
	if ping.CtcpReplyLine() == nil || ping.CtcpReplyLine().Command() != "PING" {
		t.Fatalf("ping ctcp line = %+v", ping.CtcpReplyLine())
	}

	clock := irc.Incoming("libera",
		mustParse(t, ":lena!u@h NOTICE omairc :\x01TIME Tue, 15 Sep 2026 12:00:00 +0000\x01"), "", statusTestNow)
	if clock.Label() != "CTCP" || clock.Text() != "TIME reply from lena: Tue, 15 Sep 2026 12:00:00 +0000" {
		t.Fatalf("time = %q/%q", clock.Label(), clock.Text())
	}

	action := irc.Incoming("libera", mustParse(t, ":lena!u@h NOTICE omairc :\x01ACTION waves\x01"), "", statusTestNow)
	if action.Label() != "NOTICE" || action.CtcpReplyLine() != nil {
		t.Fatalf("action = %q/%+v, want a plain NOTICE", action.Label(), action.CtcpReplyLine())
	}
}

// TestSessionIncomingStandardRepliesShowDescriptionOnStatus ports
// SessionTest::incomingStandardRepliesShowDescriptionOnStatus.
func TestSessionIncomingStandardRepliesShowDescriptionOnStatus(t *testing.T) {
	fail := irc.Incoming("libera", mustParse(t, "FAIL * NEED_REGISTRATION :You need to be registered to continue"), "", statusTestNow)
	if fail.Label() != "FAIL" || fail.Text() != "You need to be registered to continue" {
		t.Fatalf("fail = %q/%q", fail.Label(), fail.Text())
	}
	if fail.Severity() != irc.LogSeverityAlert || fail.Source() != irc.LogSourceServer {
		t.Fatalf("fail severity/source = %v/%v", fail.Severity(), fail.Source())
	}

	failWithContext := irc.Incoming("libera",
		mustParse(t, "FAIL ACC REG_INVALID_CALLBACK REGISTER :Email address is not valid"), "", statusTestNow)
	if failWithContext.Label() != "FAIL" || failWithContext.Text() != "Email address is not valid" {
		t.Fatalf("failWithContext = %q/%q", failWithContext.Label(), failWithContext.Text())
	}
	if failWithContext.Severity() != irc.LogSeverityAlert {
		t.Fatalf("failWithContext severity = %v", failWithContext.Severity())
	}

	warn := irc.Incoming("libera", mustParse(t, "WARN REHASH CERTS_EXPIRED :Certificate has expired"), "", statusTestNow)
	if warn.Label() != "WARN" || warn.Text() != "Certificate has expired" ||
		warn.Severity() != irc.LogSeverityInfo {
		t.Fatalf("warn = %q/%q/%v", warn.Label(), warn.Text(), warn.Severity())
	}

	note := irc.Incoming("libera",
		mustParse(t, "NOTE * OPER_MESSAGE :Registering new accounts has been disabled"), "", statusTestNow)
	if note.Label() != "NOTE" || note.Text() != "Registering new accounts has been disabled" ||
		note.Severity() != irc.LogSeverityInfo {
		t.Fatalf("note = %q/%q/%v", note.Label(), note.Text(), note.Severity())
	}
}

// TestSessionInboundFailDoesNotFailTheSession ports
// SessionTest::inboundFailDoesNotFailTheSession.
func TestSessionInboundFailDoesNotFailTheSession(t *testing.T) {
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

	fixture.inject("FAIL JOIN ACCOUNT_REQUIRED #omarchy :You must be logged in\r\n")
	if got := fixture.session.State(); got != StateRegistered {
		t.Fatalf("state = %v, want Registered", got)
	}
	if got := fixture.transport.State(); got != ConnectionEncrypted {
		t.Fatalf("transport = %v, want Encrypted", got)
	}
	if len(fixture.handler.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(fixture.handler.errors))
	}

	sawFail := false
	for _, entry := range fixture.handler.status {
		if entry.Label() != "FAIL" {
			continue
		}
		sawFail = true
		if entry.Text() != "You must be logged in" || entry.Severity() != irc.LogSeverityAlert {
			t.Fatalf("FAIL entry = %q/%v", entry.Text(), entry.Severity())
		}
	}
	if !sawFail {
		t.Fatal("the FAIL line must reach Status")
	}
}

// TestSessionPseudoClientPrivmsgReachesStatus ports
// SessionTest::pseudoClientPrivmsgReachesStatus.
func TestSessionPseudoClientPrivmsgReachesStatus(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.AutojoinChannels = nil
	fixture := newSessionFixture(t, config)
	fixture.registerWithWelcome()

	fixture.inject(":*status!znc@znc.in PRIVMSG omairc :You have 1 network attached\r\n")

	assertStringsEqual(t, statusTextsOfLabel(fixture.handler, "PRIVMSG"),
		[]string{"You have 1 network attached"})
}

// TestSessionPlaintextIdentifyEmitsOneStatusWarning ports
// SessionTest::plaintextIdentifyEmitsOneStatusWarning.
func TestSessionPlaintextIdentifyEmitsOneStatusWarning(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.TLSEnabled = false
	config.NickServPassword = "nick-secret"
	fixture := newSessionFixture(t, config)

	fixture.session.Start()
	fixture.transport.CompleteConnect()
	fixture.inject(":server CAP omairc LS :invite-notify\r\n" +
		":server 001 omairc :Welcome\r\n")

	warnings := 0
	for _, entry := range fixture.handler.status {
		if entry.Label() == "identify" {
			warnings++
			if entry.Text() != "NickServ identify will be sent in clear text" {
				t.Fatalf("identify text = %q", entry.Text())
			}
			if entry.Severity() != irc.LogSeverityAlert {
				t.Fatalf("identify severity = %v, want Alert", entry.Severity())
			}
		}
		if strings.Contains(entry.Text(), "nick-secret") {
			t.Fatal("the NickServ secret leaked into Status")
		}
	}
	if warnings != 1 {
		t.Fatalf("identify warnings = %d, want 1", warnings)
	}
	if fixture.handler.anyFieldContains("PRIVMSG") {
		t.Fatal("an outgoing IDENTIFY must not reach Status")
	}

	identify := irc.Outgoing(sessionTestNetworkID, []byte("PRIVMSG NickServ :IDENTIFY nick-secret\r\n"), "", statusTestNow)
	if identify.Text() != "PRIVMSG NickServ :IDENTIFY ***" ||
		strings.Contains(identify.Text(), "nick-secret") {
		t.Fatalf("identify = %q", identify.Text())
	}
}

// TestSessionAutomaticIdentifyDoesNotAppearInStatusAsSecret ports
// SessionTest::automaticIdentifyDoesNotAppearInStatusAsSecret.
func TestSessionAutomaticIdentifyDoesNotAppearInStatusAsSecret(t *testing.T) {
	config := sessionTestConfig(sessionTestNetworkID)
	config.NickServPassword = "nick-secret"
	fixture := newSessionFixture(t, config)

	fixture.connectTLS()
	fixture.inject(":server CAP omairc LS :invite-notify\r\n" +
		":server 001 omairc :Welcome\r\n")
	if fixture.handler.anyFieldContains("PRIVMSG") || fixture.handler.anyFieldContains("nick-secret") {
		t.Fatal("the automatic IDENTIFY leaked into Status")
	}
	identify := irc.Outgoing(sessionTestNetworkID, []byte("PRIVMSG NickServ :IDENTIFY nick-secret\r\n"), "", statusTestNow)
	if identify.Text() != "PRIVMSG NickServ :IDENTIFY ***" {
		t.Fatalf("identify = %q", identify.Text())
	}
}
