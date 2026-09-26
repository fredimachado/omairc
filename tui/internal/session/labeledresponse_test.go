package session

import (
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// labeledConfig mirrors the sessionConfig() helper in tst_labeledresponse.cpp:
// reconnect is off so a dropped connection never masks a labeling bug.
func labeledConfig() SessionConfig {
	config := sessionTestConfig(sessionTestNetworkID)
	config.ReconnectEnabled = false
	config.AutojoinChannels = nil
	return config
}

func newLabeledFixture(t *testing.T) *sessionFixture {
	t.Helper()
	return newSessionFixture(t, labeledConfig())
}

// registerLabeled enables message-tags + labeled-response, mirroring
// SessionFixture::registerLabeled.
func (f *sessionFixture) registerLabeled() {
	f.t.Helper()
	f.connectTLS()
	f.inject(":server CAP omairc LS :message-tags labeled-response\r\n" +
		":server CAP omairc ACK :message-tags labeled-response\r\n" +
		":server 001 omairc :Welcome\r\n")
}

// registerUnlabeled only negotiates multi-prefix.
func (f *sessionFixture) registerUnlabeled() {
	f.t.Helper()
	f.connectTLS()
	f.inject(":server CAP omairc LS :multi-prefix\r\n" +
		":server 001 omairc :Welcome\r\n")
}

// requestLabelOf extracts the @label= tag value from a frame, or "".
func requestLabelOf(frame string) string {
	if !strings.HasPrefix(frame, "@label=") {
		return ""
	}
	space := strings.IndexByte(frame, ' ')
	if space < 0 {
		return ""
	}
	return frame[len("@label="):space]
}

// commandOf drops the leading tag section from a frame.
func commandOf(frame string) string {
	if !strings.HasPrefix(frame, "@") {
		return frame
	}
	space := strings.IndexByte(frame, ' ')
	if space < 0 {
		return frame
	}
	return frame[space+1:]
}

// hasStatusWithRequestLabel reports whether a Status entry with the given
// correlation label carries the exact text.
func hasStatusWithRequestLabel(handler *sessionTestHandler, requestLabel, text string) bool {
	for _, entry := range handler.status {
		if entry.RequestLabel() == requestLabel && entry.Text() == text {
			return true
		}
	}
	return false
}

// statusTextsWithRequestLabel returns the text of every entry with the label.
func statusTextsWithRequestLabel(handler *sessionTestHandler, requestLabel string) []string {
	var texts []string
	for _, entry := range handler.status {
		if entry.RequestLabel() == requestLabel {
			texts = append(texts, entry.Text())
		}
	}
	return texts
}

// TestLabeledRequestsLabeledResponseOnOwnLine ports
// LabeledResponseTest::requestsLabeledResponseOnOwnLine.
func TestLabeledRequestsLabeledResponseOnOwnLine(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.session.Start()
	fixture.transport.CompleteConnect()
	fixture.inject(":server CAP omairc LS :message-tags labeled-response away-notify\r\n")
	if !fixture.wrote("CAP REQ :message-tags\r\n") ||
		!fixture.wrote("CAP REQ :labeled-response\r\n") ||
		!fixture.wrote("CAP REQ :away-notify\r\n") {
		t.Fatalf("frames = %q, want separate CAP REQ lines", fixture.frames())
	}
}

// TestLabeledDoesNotRequestLabeledResponseWithoutMessageTags ports
// LabeledResponseTest::doesNotRequestLabeledResponseWithoutMessageTags.
func TestLabeledDoesNotRequestLabeledResponseWithoutMessageTags(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.session.Start()
	fixture.transport.CompleteConnect()
	fixture.inject(":server CAP omairc LS :labeled-response away-notify\r\n")
	if fixture.wrote("CAP REQ :labeled-response\r\n") {
		t.Fatal("labeled-response needs message-tags")
	}
	if !fixture.wrote("CAP REQ :away-notify\r\n") {
		t.Fatal("away-notify must still be requested")
	}
}

// TestLabeledWhoisStaysUntaggedWithoutCap ports
// LabeledResponseTest::whoisStaysUntaggedWithoutCap.
func TestLabeledWhoisStaysUntaggedWithoutCap(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerUnlabeled()
	if fixture.session.Capabilities().Contains(irc.CapabilityLabeledResponse) {
		t.Fatal("labeled-response must be off")
	}

	if got := fixture.session.StartLabeledRequest(); got != "" {
		t.Fatalf("label = %q, want empty", got)
	}
	if !fixture.session.Whois("lena", "") {
		t.Fatal("whois must succeed")
	}
	if got := fixture.lastFrame(); got != "WHOIS lena lena\r\n" {
		t.Fatalf("last frame = %q, want the untagged WHOIS", got)
	}
	if got := fixture.session.PendingRequestLabelCount(); got != 0 {
		t.Fatalf("pending labels = %d, want 0", got)
	}
}

// TestLabeledWhoisAndCtcpGetUniqueLabels ports
// LabeledResponseTest::whoisAndCtcpGetUniqueLabels.
func TestLabeledWhoisAndCtcpGetUniqueLabels(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()
	if !fixture.session.Capabilities().Contains(irc.CapabilityLabeledResponse) ||
		!fixture.session.Capabilities().Contains(irc.CapabilityMessageTags) {
		t.Fatal("labeled-response and message-tags must be enabled")
	}

	first := fixture.session.StartLabeledRequest()
	if first == "" {
		t.Fatal("the first label must be non-empty")
	}
	if !fixture.session.Whois("lena", first) {
		t.Fatal("whois must succeed")
	}
	if got := requestLabelOf(fixture.lastFrame()); got != first {
		t.Fatalf("frame label = %q, want %q", got, first)
	}
	if got := commandOf(fixture.lastFrame()); got != "WHOIS lena lena\r\n" {
		t.Fatalf("command = %q, want WHOIS lena lena", got)
	}

	second := fixture.session.StartLabeledRequest()
	if second == "" || second == first {
		t.Fatalf("second label = %q, want a fresh label", second)
	}
	if !fixture.session.Whois("mira", second) {
		t.Fatal("whois must succeed")
	}
	if got := requestLabelOf(fixture.lastFrame()); got != second {
		t.Fatalf("frame label = %q, want %q", got, second)
	}
	if got := fixture.session.PendingRequestLabelCount(); got != 2 {
		t.Fatalf("pending labels = %d, want 2", got)
	}

	ctcp := fixture.session.StartLabeledRequest()
	if ctcp == "" || ctcp == first || ctcp == second {
		t.Fatalf("ctcp label = %q, want a fresh label", ctcp)
	}
	if !fixture.session.SendCtcp("lena", "VERSION", "", ctcp) {
		t.Fatal("sendCtcp must succeed")
	}
	if got := requestLabelOf(fixture.lastFrame()); got != ctcp {
		t.Fatalf("frame label = %q, want %q", got, ctcp)
	}
	if !strings.HasPrefix(commandOf(fixture.lastFrame()), "PRIVMSG lena :") {
		t.Fatalf("command = %q, want a PRIVMSG", commandOf(fixture.lastFrame()))
	}
	if got := fixture.session.PendingRequestLabelCount(); got != 3 {
		t.Fatalf("pending labels = %d, want 3", got)
	}
}

// TestLabeledSingleNumericRoutesToAskingTranscript ports
// LabeledResponseTest::singleLabeledNumericRoutesToAskingTranscript.
func TestLabeledSingleNumericRoutesToAskingTranscript(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()

	label := fixture.session.StartLabeledRequest()
	if !fixture.session.Whois("lena", label) {
		t.Fatal("whois must succeed")
	}
	fixture.inject("@label=" + label + " :irc 401 omairc lena :No such nick/channel\r\n")

	if !hasStatusWithRequestLabel(fixture.handler, label, "No such nick: lena") {
		t.Fatalf("statuses = %q, want the labeled 401", statusTextsWithRequestLabel(fixture.handler, label))
	}
	if fixture.session.HasPendingRequestLabel(label) {
		t.Fatal("the label must be finished by the labeled numeric")
	}
}

// TestLabeledResponseBatchRoutesInnerLines ports
// LabeledResponseTest::labeledResponseBatchRoutesInnerLines.
func TestLabeledResponseBatchRoutesInnerLines(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()

	label := fixture.session.StartLabeledRequest()
	if !fixture.session.Whois("lena", label) {
		t.Fatal("whois must succeed")
	}
	fixture.inject("@label=" + label + " :irc.example BATCH +NMzYSq45x labeled-response\r\n" +
		"@batch=NMzYSq45x :irc 311 omairc lena ~lena user/host * :Lena\r\n" +
		"@batch=NMzYSq45x :irc 319 omairc lena :#omarchy\r\n" +
		"@batch=NMzYSq45x :irc 318 omairc lena :End of /WHOIS list.\r\n" +
		":irc.example BATCH -NMzYSq45x\r\n")

	for _, want := range []string{
		"lena is ~lena@user/host (Lena)",
		"lena is on #omarchy",
		"End of WHOIS for lena",
	} {
		if !hasStatusWithRequestLabel(fixture.handler, label, want) {
			t.Fatalf("statuses = %q, want %q", statusTextsWithRequestLabel(fixture.handler, label), want)
		}
	}
	if fixture.session.HasPendingRequestLabel(label) {
		t.Fatal("closing the labeled batch must finish the label")
	}
}

// TestLabeledUnlabeledLeftoverDoesNotStealLabeledWaiter ports
// LabeledResponseTest::unlabeledLeftoverDoesNotStealLabeledWaiter.
func TestLabeledUnlabeledLeftoverDoesNotStealLabeledWaiter(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()

	label := fixture.session.StartLabeledRequest()
	if !fixture.session.Whois("lena", label) {
		t.Fatal("whois must succeed")
	}

	fixture.inject(":irc 311 omairc lena ~other host * :Other\r\n" +
		":irc 318 omairc lena :End of /WHOIS list.\r\n")
	for _, text := range statusTextsWithRequestLabel(fixture.handler, label) {
		t.Fatalf("an unlabeled line stole the label: %q", text)
	}
	if !fixture.session.HasPendingRequestLabel(label) {
		t.Fatal("the label must stay pending")
	}

	fixture.inject("@label=" + label + " :irc.example BATCH +whois1 labeled-response\r\n" +
		"@batch=whois1 :irc 311 omairc lena ~lena user/host * :Lena\r\n" +
		"@batch=whois1 :irc 318 omairc lena :End of /WHOIS list.\r\n" +
		":irc.example BATCH -whois1\r\n")
	if !hasStatusWithRequestLabel(fixture.handler, label, "lena is ~lena@user/host (Lena)") ||
		!hasStatusWithRequestLabel(fixture.handler, label, "End of WHOIS for lena") {
		t.Fatalf("statuses = %q, want the labeled batch", statusTextsWithRequestLabel(fixture.handler, label))
	}
}

// TestLabeledUnsolicitedLabelDoesNotCrashOrSteal ports
// LabeledResponseTest::unsolicitedLabelDoesNotCrashOrSteal.
func TestLabeledUnsolicitedLabelDoesNotCrashOrSteal(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()

	label := fixture.session.StartLabeledRequest()
	if !fixture.session.Whois("lena", label) {
		t.Fatal("whois must succeed")
	}

	fixture.inject("@label=nope :irc 311 omairc lena ~x h * :Nope\r\n" +
		"@label=nope :irc 318 omairc lena :End of /WHOIS list.\r\n" +
		"@label=nope :irc.example ACK\r\n")
	for _, text := range statusTextsWithRequestLabel(fixture.handler, label) {
		t.Fatalf("an unsolicited label stole the label: %q", text)
	}
	if !fixture.session.HasPendingRequestLabel(label) {
		t.Fatal("the pending label must survive an unsolicited label")
	}

	fixture.inject("@label=" + label + " :irc 401 omairc lena :No such nick/channel\r\n")
	if !hasStatusWithRequestLabel(fixture.handler, label, "No such nick: lena") {
		t.Fatal("the real labeled numeric must route")
	}
}

// TestLabeledAckCompletesWaiterWithoutTranscript ports
// LabeledResponseTest::ackCompletesWaiterWithoutTranscript.
func TestLabeledAckCompletesWaiterWithoutTranscript(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()

	label := fixture.session.StartLabeledRequest()
	if !fixture.session.Whois("lena", label) {
		t.Fatal("whois must succeed")
	}
	if got := fixture.session.PendingRequestLabelCount(); got != 1 {
		t.Fatalf("pending labels = %d, want 1", got)
	}

	fixture.inject("@label=" + label + " :irc.example ACK\r\n")
	if got := fixture.session.PendingRequestLabelCount(); got != 0 {
		t.Fatalf("pending labels = %d, want 0", got)
	}
	if len(fixture.handler.labels) != 1 || fixture.handler.labels[0] != label {
		t.Fatalf("finished labels = %q, want [%s]", fixture.handler.labels, label)
	}

	start := len(fixture.handler.status)
	fixture.inject(":irc 311 omairc lena ~lena user/host * :Lena\r\n" +
		":irc 318 omairc lena :End of /WHOIS list.\r\n")
	for _, entry := range fixture.handler.status[start:] {
		if entry.RequestLabel() != "" {
			t.Fatalf("an unlabeled line carried the finished label: %+v", entry)
		}
	}
}

// TestLabeledFailCopiesIntoAskingTranscript ports
// LabeledResponseTest::labeledFailCopiesIntoAskingTranscript.
func TestLabeledFailCopiesIntoAskingTranscript(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()

	label := fixture.session.StartLabeledRequest()
	if !fixture.session.Whois("lena", label) {
		t.Fatal("whois must succeed")
	}
	fixture.inject("@label=" + label + " :irc FAIL WHOIS TEMPORARILY_UNAVAILABLE lena :try later\r\n")

	found := false
	for _, text := range statusTextsWithRequestLabel(fixture.handler, label) {
		if strings.Contains(text, "try later") {
			found = true
		}
	}
	if !found {
		t.Fatalf("statuses = %q, want the FAIL description", statusTextsWithRequestLabel(fixture.handler, label))
	}
}

// TestLabeledCtcpReplyRoutesToAskingTranscript ports
// LabeledResponseTest::labeledCtcpReplyRoutesToAskingTranscript.
func TestLabeledCtcpReplyRoutesToAskingTranscript(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()

	label := fixture.session.StartLabeledRequest()
	if !fixture.session.SendCtcp("lena", "VERSION", "", label) {
		t.Fatal("sendCtcp must succeed")
	}
	if !strings.HasPrefix(commandOf(fixture.lastFrame()), "PRIVMSG lena :") {
		t.Fatalf("command = %q, want a PRIVMSG", commandOf(fixture.lastFrame()))
	}

	fixture.inject("@label=" + label + " :lena!u@h NOTICE omairc :\x01VERSION Omairc 0.4.0\x01\r\n")
	if !hasStatusWithRequestLabel(fixture.handler, label, "VERSION reply from lena: Omairc 0.4.0") {
		t.Fatalf("statuses = %q, want the labeled CTCP reply", statusTextsWithRequestLabel(fixture.handler, label))
	}
}

// TestLabeledUnlabeledCtcpReplyAfterAckCopiesIntoAskingTranscript ports
// LabeledResponseTest::unlabeledCtcpReplyAfterAckCopiesIntoAskingTranscript.
func TestLabeledUnlabeledCtcpReplyAfterAckCopiesIntoAskingTranscript(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()

	label := fixture.session.StartLabeledRequest()
	if !fixture.session.SendCtcp("lena", "VERSION", "", label) {
		t.Fatal("sendCtcp must succeed")
	}
	if got := fixture.session.PendingRequestLabelCount(); got != 1 {
		t.Fatalf("pending labels = %d, want 1", got)
	}

	fixture.inject("@label=" + label + " :irc.example ACK\r\n")
	if got := fixture.session.PendingRequestLabelCount(); got != 0 {
		t.Fatalf("pending labels = %d, want 0", got)
	}

	fixture.inject(":lena!u@h NOTICE omairc :\x01VERSION Omairc 0.4.0\x01\r\n")
	if !fixture.handler.anyFieldContains("VERSION reply from lena: Omairc 0.4.0") {
		t.Fatal("the unlabeled CTCP reply must still reach Status")
	}
	for _, entry := range fixture.handler.status {
		if entry.RequestLabel() == label {
			t.Fatal("the unlabeled CTCP reply must not carry the finished label")
		}
	}
}

// TestLabeledUnlabeledCtcpReplyAfterEchoMessageCopiesIntoAskingTranscript ports
// LabeledResponseTest::unlabeledCtcpReplyAfterEchoMessageCopiesIntoAskingTranscript.
func TestLabeledUnlabeledCtcpReplyAfterEchoMessageCopiesIntoAskingTranscript(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()

	label := fixture.session.StartLabeledRequest()
	if !fixture.session.SendCtcp("lena", "VERSION", "", label) {
		t.Fatal("sendCtcp must succeed")
	}
	if !strings.HasPrefix(commandOf(fixture.lastFrame()), "PRIVMSG lena :") {
		t.Fatalf("command = %q, want a PRIVMSG", commandOf(fixture.lastFrame()))
	}

	fixture.inject("@label=" + label + " :omairc!u@h PRIVMSG lena :\x01VERSION\x01\r\n")
	if fixture.session.HasPendingRequestLabel(label) {
		t.Fatal("the echo carries the label, so it must finish")
	}

	fixture.inject(":lena!u@h NOTICE omairc :\x01VERSION Omairc 0.4.0\x01\r\n")
	if !fixture.handler.anyFieldContains("VERSION reply from lena: Omairc 0.4.0") {
		t.Fatal("the unlabeled CTCP reply must still reach Status")
	}
}

// TestLabeledCtcp401CopiesIntoAskingTranscript ports
// LabeledResponseTest::labeledCtcp401CopiesIntoAskingTranscript.
func TestLabeledCtcp401CopiesIntoAskingTranscript(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()

	label := fixture.session.StartLabeledRequest()
	if !fixture.session.SendCtcp("missingnick", "VERSION", "", label) {
		t.Fatal("sendCtcp must succeed")
	}
	if !strings.HasPrefix(commandOf(fixture.lastFrame()), "PRIVMSG missingnick :") {
		t.Fatalf("command = %q, want a PRIVMSG", commandOf(fixture.lastFrame()))
	}
	if got := fixture.session.PendingRequestLabelCount(); got != 1 {
		t.Fatalf("pending labels = %d, want 1", got)
	}

	fixture.inject("@label=" + label + " :irc 401 omairc missingnick :No such nick/channel\r\n")
	if !hasStatusWithRequestLabel(fixture.handler, label, "No such nick: missingnick") {
		t.Fatalf("statuses = %q, want the labeled 401", statusTextsWithRequestLabel(fixture.handler, label))
	}
	if got := fixture.session.PendingRequestLabelCount(); got != 0 {
		t.Fatalf("pending labels = %d, want 0", got)
	}

	start := len(fixture.handler.status)
	fixture.inject(":irc 401 omairc missingnick :No such nick/channel\r\n")
	for _, entry := range fixture.handler.status[start:] {
		if entry.RequestLabel() != "" {
			t.Fatal("a later unlabeled 401 must not carry the finished label")
		}
	}
}

// TestLabeledUnsolicitedBatchesShareTheOpenBatchBudget ports
// LabeledResponseTest::unsolicitedLabeledResponseBatchesShareTheOpenBatchBudget.
func TestLabeledUnsolicitedBatchesShareTheOpenBatchBudget(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()
	if got := fixture.session.OpenBatchCount(); got != 0 {
		t.Fatalf("open batches = %d, want 0", got)
	}

	var fill string
	for index := 0; index < 16; index++ {
		fill += ":irc.example BATCH +d" + strconvItoa(index) + " unknown.example/foo\r\n"
	}
	fixture.inject(fill)
	if got := fixture.session.OpenBatchCount(); got != 16 {
		t.Fatalf("open batches = %d, want 16", got)
	}

	var flood string
	for index := 0; index < 40; index++ {
		id := "lr" + strconvItoa(index)
		flood += ":irc.example BATCH +" + id + " labeled-response\r\n" +
			"@batch=" + id + " :irc 311 omairc lena ~x h * :Nope\r\n" +
			"@batch=" + id + " :irc 318 omairc lena :End of /WHOIS list.\r\n"
	}
	fixture.inject(flood)
	if got := fixture.session.OpenBatchCount(); got != 16 {
		t.Fatalf("open batches = %d, want 16", got)
	}

	if !fixture.session.Whois("lena", fixture.session.StartLabeledRequest()) {
		t.Fatal("whois must succeed")
	}
	label := requestLabelOf(fixture.lastFrame())
	if label == "" || !fixture.session.HasPendingRequestLabel(label) {
		t.Fatalf("label = %q, want a pending request label", label)
	}

	fixture.inject("@label=" + label + " :irc.example BATCH +whois1 labeled-response\r\n" +
		"@batch=whois1 :irc 311 omairc lena ~lena user/host * :Lena\r\n" +
		"@batch=whois1 :irc 318 omairc lena :End of /WHOIS list.\r\n" +
		":irc.example BATCH -whois1\r\n")
	if !hasStatusWithRequestLabel(fixture.handler, label, "lena is ~lena@user/host (Lena)") ||
		!hasStatusWithRequestLabel(fixture.handler, label, "End of WHOIS for lena") {
		t.Fatalf("statuses = %q, want the solicited batch", statusTextsWithRequestLabel(fixture.handler, label))
	}
	if got := fixture.session.OpenBatchCount(); got != 16 {
		t.Fatalf("open batches = %d, want 16", got)
	}
}

// TestLabeledTimeoutClearsWaiters ports
// LabeledResponseTest::timeoutClearsWaiters.
func TestLabeledTimeoutClearsWaiters(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()

	label := fixture.session.StartLabeledRequest()
	if label == "" || !fixture.session.Whois("lena", label) {
		t.Fatal("whois must succeed")
	}
	if got := fixture.session.PendingRequestLabelCount(); got != 1 {
		t.Fatalf("pending labels = %d, want 1", got)
	}
	if !fixture.session.HasPendingRequestLabel(label) {
		t.Fatal("the label must be pending")
	}

	fixture.clock.Advance(45000 * time.Millisecond)
	if got := fixture.session.PendingRequestLabelCount(); got != 0 {
		t.Fatalf("pending labels = %d, want 0", got)
	}
	if fixture.session.HasPendingRequestLabel(label) {
		t.Fatal("the timed-out label must be gone")
	}
	if len(fixture.handler.labels) != 1 || fixture.handler.labels[0] != label {
		t.Fatalf("finished labels = %q, want [%s]", fixture.handler.labels, label)
	}
}

// TestLabeledTimeoutExpiresOnlyElapsedLabels ports
// LabeledResponseTest::timeoutExpiresOnlyElapsedLabels.
func TestLabeledTimeoutExpiresOnlyElapsedLabels(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()

	first := fixture.session.StartLabeledRequest()
	if first == "" || !fixture.session.Whois("lena", first) {
		t.Fatal("whois must succeed")
	}
	if fixture.clock.Pending() == 0 {
		t.Fatal("the label timer must be pending")
	}
	if !fixture.session.HasPendingRequestLabel(first) {
		t.Fatal("the first label must be pending")
	}

	fixture.clock.Advance(10000 * time.Millisecond)

	second := fixture.session.StartLabeledRequest()
	if second == "" || second == first {
		t.Fatalf("second label = %q, want a fresh label", second)
	}
	if !fixture.session.Whois("mira", second) {
		t.Fatal("whois must succeed")
	}
	if got := fixture.session.PendingRequestLabelCount(); got != 2 {
		t.Fatalf("pending labels = %d, want 2", got)
	}
	if !fixture.session.HasPendingRequestLabel(first) || !fixture.session.HasPendingRequestLabel(second) {
		t.Fatal("both labels must be pending")
	}

	fixture.clock.Advance(35000 * time.Millisecond)
	if fixture.session.HasPendingRequestLabel(first) {
		t.Fatal("the first label must have expired")
	}
	if !fixture.session.HasPendingRequestLabel(second) {
		t.Fatal("the second label must survive")
	}
	if got := fixture.session.PendingRequestLabelCount(); got != 1 {
		t.Fatalf("pending labels = %d, want 1", got)
	}
	if len(fixture.handler.labels) != 1 || fixture.handler.labels[0] != first {
		t.Fatalf("finished labels = %q, want [%s]", fixture.handler.labels, first)
	}

	fixture.clock.Advance(10000 * time.Millisecond)
	if got := fixture.session.PendingRequestLabelCount(); got != 0 {
		t.Fatalf("pending labels = %d, want 0", got)
	}
	if fixture.session.HasPendingRequestLabel(second) {
		t.Fatal("the second label must have expired")
	}
	if len(fixture.handler.labels) != 2 || fixture.handler.labels[1] != second {
		t.Fatalf("finished labels = %q, want [%s %s]", fixture.handler.labels, first, second)
	}
}

// TestLabeledTimeoutDropsElapsedWatchWithoutStealingNewer ports
// LabeledResponseTest::timeoutDropsElapsedWatchWithoutStealingNewer.
func TestLabeledTimeoutDropsElapsedWatchWithoutStealingNewer(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()

	first := fixture.session.StartLabeledRequest()
	if first == "" || !fixture.session.Whois("lena", first) {
		t.Fatal("whois must succeed")
	}

	fixture.clock.Advance(10000 * time.Millisecond)
	second := fixture.session.StartLabeledRequest()
	if second == "" || second == first || !fixture.session.Whois("mira", second) {
		t.Fatal("the second whois must succeed")
	}

	fixture.clock.Advance(35000 * time.Millisecond)
	if fixture.session.HasPendingRequestLabel(first) {
		t.Fatal("the first label must have expired")
	}
	if !fixture.session.HasPendingRequestLabel(second) {
		t.Fatal("the second label must survive")
	}

	// A late answer for the expired watch must not resurrect it or steal the
	// newer one.
	fixture.inject("@label=" + first + " :irc 318 omairc lena :End of /WHOIS list.\r\n")
	if fixture.session.HasPendingRequestLabel(first) {
		t.Fatal("the expired label must stay gone")
	}
	if !fixture.session.HasPendingRequestLabel(second) {
		t.Fatal("the second label must stay pending")
	}

	fixture.inject("@label=" + second + " :irc.example BATCH +b labeled-response\r\n" +
		"@batch=b :irc 311 omairc mira ~m h * :FromHelp\r\n" +
		"@batch=b :irc 318 omairc mira :End of /WHOIS list.\r\n" +
		":irc.example BATCH -b\r\n")
	if !hasStatusWithRequestLabel(fixture.handler, second, "mira is ~m@h (FromHelp)") ||
		!hasStatusWithRequestLabel(fixture.handler, second, "End of WHOIS for mira") {
		t.Fatalf("statuses = %q, want the second batch", statusTextsWithRequestLabel(fixture.handler, second))
	}
	if fixture.session.HasPendingRequestLabel(second) {
		t.Fatal("the second label must be finished")
	}
}

// TestLabeledDisconnectClearsWaiters ports
// LabeledResponseTest::disconnectClearsWaiters.
func TestLabeledDisconnectClearsWaiters(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()

	label := fixture.session.StartLabeledRequest()
	if label == "" || !fixture.session.Whois("lena", label) {
		t.Fatal("whois must succeed")
	}
	if got := fixture.session.PendingRequestLabelCount(); got != 1 {
		t.Fatalf("pending labels = %d, want 1", got)
	}

	fixture.session.Stop()
	if got := fixture.session.PendingRequestLabelCount(); got != 0 {
		t.Fatalf("pending labels = %d, want 0", got)
	}
}

// TestLabeledTwoWhoisForSameNickStayIndependent ports
// LabeledResponseTest::twoLabeledWhoisForSameNickStayIndependent.
func TestLabeledTwoWhoisForSameNickStayIndependent(t *testing.T) {
	fixture := newLabeledFixture(t)
	fixture.registerLabeled()

	first := fixture.session.StartLabeledRequest()
	if !fixture.session.Whois("mira", first) {
		t.Fatal("the first whois must succeed")
	}
	second := fixture.session.StartLabeledRequest()
	if second == first || !fixture.session.Whois("mira", second) {
		t.Fatal("the second whois must succeed")
	}

	fixture.inject("@label=" + first + " :irc.example BATCH +a labeled-response\r\n" +
		"@batch=a :irc 311 omairc mira ~m h * :FromOmarchy\r\n" +
		"@batch=a :irc 318 omairc mira :End of /WHOIS list.\r\n" +
		":irc.example BATCH -a\r\n")
	if !hasStatusWithRequestLabel(fixture.handler, first, "mira is ~m@h (FromOmarchy)") {
		t.Fatalf("statuses = %q, want the first batch", statusTextsWithRequestLabel(fixture.handler, first))
	}
	if !fixture.session.HasPendingRequestLabel(second) {
		t.Fatal("the second label must stay pending")
	}
	for _, text := range statusTextsWithRequestLabel(fixture.handler, second) {
		if strings.Contains(text, "FromOmarchy") {
			t.Fatal("the second label must not receive the first batch")
		}
	}

	fixture.inject("@label=" + second + " :irc.example BATCH +b labeled-response\r\n" +
		"@batch=b :irc 311 omairc mira ~m h * :FromHelp\r\n" +
		"@batch=b :irc 318 omairc mira :End of /WHOIS list.\r\n" +
		":irc.example BATCH -b\r\n")
	if !hasStatusWithRequestLabel(fixture.handler, second, "mira is ~m@h (FromHelp)") {
		t.Fatalf("statuses = %q, want the second batch", statusTextsWithRequestLabel(fixture.handler, second))
	}
}

// strconvItoa is a tiny local int formatter used to build batch ids.
func strconvItoa(value int) string {
	if value == 0 {
		return "0"
	}
	var out []byte
	for value > 0 {
		out = append([]byte{byte('0' + value%10)}, out...)
		value /= 10
	}
	return string(out)
}
