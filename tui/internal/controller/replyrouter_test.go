package controller

import (
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// replyRouterTestNow is the fixed clock the tests stamp entries with.
var replyRouterTestNow = time.Date(2026, time.September, 26, 12, 0, 0, 0, time.UTC)

// replyPersistCall records one PersistAvatarURL call.
type replyPersistCall struct {
	networkID string
	url       string
}

// replyFakeHost records the callbacks the router makes and serves the selected
// conversation, capability, and session lookups each test configures.
type replyFakeHost struct {
	transcripts []irc.WhoisTranscriptEvent
	statuses    []irc.StatusEntry
	persisted   []replyPersistCall

	selectedKey irc.ConversationKey
	hasSelected bool

	selfNicks map[string]string

	capabilities map[string]irc.CapabilitySet

	networkIDs map[irc.ComposerSurface]string
	sessions   map[irc.ComposerSurface]*session.Session

	selectedSession   *session.Session
	selectedTarget    string
	selectedCloseable bool
	hasNetworks       bool

	now time.Time
}

var _ ReplyHost = (*replyFakeHost)(nil)

func (h *replyFakeHost) ApplyWhoisTranscript(event irc.WhoisTranscriptEvent) {
	h.transcripts = append(h.transcripts, event)
}

func (h *replyFakeHost) RecordStatus(entry irc.StatusEntry) {
	h.statuses = append(h.statuses, entry)
}

func (h *replyFakeHost) SelectedKey() (irc.ConversationKey, bool) {
	return h.selectedKey, h.hasSelected
}

func (h *replyFakeHost) SelfNick(networkID string) string {
	return h.selfNicks[networkID]
}

func (h *replyFakeHost) PersistAvatarURL(networkID, url string) {
	h.persisted = append(h.persisted, replyPersistCall{networkID: networkID, url: url})
}

func (h *replyFakeHost) Capabilities(networkID string) irc.CapabilitySet {
	return h.capabilities[networkID]
}

func (h *replyFakeHost) QueryNetworkID(surface irc.ComposerSurface) string {
	return h.networkIDs[surface]
}

func (h *replyFakeHost) SessionFor(surface irc.ComposerSurface) *session.Session {
	return h.sessions[surface]
}

func (h *replyFakeHost) SelectedSession() *session.Session { return h.selectedSession }

func (h *replyFakeHost) SelectedTarget() string { return h.selectedTarget }

func (h *replyFakeHost) SelectedIsCloseableDirect() bool { return h.selectedCloseable }

func (h *replyFakeHost) HasNetworks() bool { return h.hasNetworks }

func (h *replyFakeHost) Now() time.Time { return h.now }

// replyRouterTestHarness bundles a router, its recording host, and the reducer
// it reads features and presence from.
type replyRouterTestHarness struct {
	router  *ReplyRouter
	host    *replyFakeHost
	reducer *irc.EventReducer
}

func newReplyRouterTestHarness() *replyRouterTestHarness {
	reducer := irc.NewEventReducer()
	host := &replyFakeHost{now: replyRouterTestNow}
	return &replyRouterTestHarness{
		router:  NewReplyRouter(reducer, host),
		host:    host,
		reducer: reducer,
	}
}

// replyRouterTestConversation builds a conversation destination.
func replyRouterTestConversation(networkID, target string) replyDestination {
	return replyDestination{
		key:    irc.ConversationKey{NetworkID: networkID, NormalizedTarget: target},
		hasKey: true,
	}
}

// replyRouterTestWhoisEntry classifies one WHOIS numeric into its Status entry.
func replyRouterTestWhoisEntry(code string, params ...string) irc.StatusEntry {
	message := irc.Message{Command: code, Params: params}
	return irc.Incoming("net", message, "#", replyRouterTestNow)
}

func TestReplyLabeledStandardReplyCopies(t *testing.T) {
	t.Run("alert fail is copied and the watch is dropped", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		router := harness.router
		router.labeledWatches[replyLabeledWatchKey{networkID: "net", requestLabel: "lr1"}] = replyLabeledWatch{
			kind:        replyLabeledWhois,
			destination: replyRouterTestConversation("net", "#chan"),
		}

		entry := irc.Lifecycle("net", irc.LogSeverityAlert, "FAIL", "WHOIS :No such nick", replyRouterTestNow)
		entry.SetRequestLabel("lr1")
		router.RouteStatusEntry(entry)

		if len(harness.host.transcripts) != 1 {
			t.Fatalf("transcripts = %d, want 1", len(harness.host.transcripts))
		}
		got := harness.host.transcripts[0]
		if got.Destination.NormalizedTarget != "#chan" || got.FormattedBody != "WHOIS :No such nick" {
			t.Fatalf("transcript = %+v, want the copied FAIL body", got)
		}
		if len(router.labeledWatches) != 0 {
			t.Fatalf("labeledWatches = %d, want 0 after an Alert reply", len(router.labeledWatches))
		}
	})

	t.Run("informational 4xx is copied and the watch survives", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		router := harness.router
		key := replyLabeledWatchKey{networkID: "net", requestLabel: "lr2"}
		router.labeledWatches[key] = replyLabeledWatch{
			kind:        replyLabeledWhois,
			destination: replyRouterTestConversation("net", "#chan"),
		}

		entry := irc.Lifecycle("net", irc.LogSeverityInfo, "401", "No such nick: bob", replyRouterTestNow)
		entry.SetRequestLabel("lr2")
		router.RouteStatusEntry(entry)

		if len(harness.host.transcripts) != 1 {
			t.Fatalf("transcripts = %d, want 1", len(harness.host.transcripts))
		}
		if _, ok := router.labeledWatches[key]; !ok {
			t.Fatal("labeled watch dropped for an informational reply")
		}
	})

	t.Run("a 2xx numeric is not copied", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		router := harness.router
		router.labeledWatches[replyLabeledWatchKey{networkID: "net", requestLabel: "lr3"}] = replyLabeledWatch{
			kind:        replyLabeledWhois,
			destination: replyRouterTestConversation("net", "#chan"),
		}

		entry := irc.Lifecycle("net", irc.LogSeverityInfo, "263", "Try again", replyRouterTestNow)
		entry.SetRequestLabel("lr3")
		router.RouteStatusEntry(entry)

		if len(harness.host.transcripts) != 0 {
			t.Fatalf("transcripts = %d, want 0 for a 2xx", len(harness.host.transcripts))
		}
	})
}

func TestReplyOwnMetadataFail(t *testing.T) {
	t.Run("KEY_NOT_SET on a clear watch echoes the cleared message", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		router := harness.router
		destination := replyRouterTestConversation("net", "#chan")
		router.ownMetadataWatches["net"] = map[string]replyOwnMetadataWatch{
			irc.AvatarKey(): {destination: destination, kind: replyOwnMetadataClear},
		}

		router.RouteOwnMetadataFail("net", irc.Message{
			Command: "FAIL",
			Params:  []string{"METADATA", "KEY_NOT_SET", "avatar", "The key was not set"},
		})

		if len(harness.host.transcripts) != 1 ||
			harness.host.transcripts[0].FormattedBody != "Avatar cleared." {
			t.Fatalf("transcripts = %+v, want Avatar cleared.", harness.host.transcripts)
		}
		if len(harness.host.persisted) != 1 ||
			harness.host.persisted[0] != (replyPersistCall{networkID: "net", url: ""}) {
			t.Fatalf("persisted = %+v, want one empty URL for the avatar key", harness.host.persisted)
		}
		if len(router.ownMetadataWatches) != 0 {
			t.Fatalf("ownMetadataWatches = %d, want 0", len(router.ownMetadataWatches))
		}
	})

	t.Run("a generic fail on a set watch reports the reason", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		router := harness.router
		router.ownMetadataWatches["net"] = map[string]replyOwnMetadataWatch{
			irc.StatusKey(): {
				destination: replyDestination{},
				kind:        replyOwnMetadataSet,
				value:       "busy",
			},
		}

		router.RouteOwnMetadataFail("net", irc.Message{
			Command: "FAIL",
			Params:  []string{"METADATA", "VALUE_INVALID", "status", "Too long"},
		})

		if len(harness.host.transcripts) != 0 {
			t.Fatalf("transcripts = %+v, want none for a status-only destination", harness.host.transcripts)
		}
		if len(harness.host.statuses) != 1 {
			t.Fatalf("statuses = %d, want 1", len(harness.host.statuses))
		}
		want := "Could not set standing status: VALUE_INVALID Too long"
		if got := harness.host.statuses[0].Text(); got != want {
			t.Fatalf("status text = %q, want %q", got, want)
		}
		if len(router.ownMetadataWatches) != 0 {
			t.Fatalf("ownMetadataWatches = %d, want 0", len(router.ownMetadataWatches))
		}
	})

	t.Run("an unknown fail code is ignored", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		router := harness.router
		router.ownMetadataWatches["net"] = map[string]replyOwnMetadataWatch{
			irc.StatusKey(): {kind: replyOwnMetadataSet, value: "busy"},
		}

		router.RouteOwnMetadataFail("net", irc.Message{
			Command: "FAIL",
			Params:  []string{"METADATA", "SOMETHING_ELSE", "status", "why"},
		})

		if len(harness.host.statuses) != 0 || len(harness.host.transcripts) != 0 {
			t.Fatalf("router echoed an unknown fail code: %+v / %+v", harness.host.statuses, harness.host.transcripts)
		}
		if len(router.ownMetadataWatches) != 1 {
			t.Fatalf("ownMetadataWatches = %d, want the watch kept", len(router.ownMetadataWatches))
		}
	})
}

func TestReplyOwnMetadataReply(t *testing.T) {
	t.Run("a set only echoes for self and an exact value", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		router := harness.router
		harness.host.selfNicks = map[string]string{"net": "alice"}
		key := replyRouterTestConversation("net", "#chan")
		router.ownMetadataWatches["net"] = map[string]replyOwnMetadataWatch{
			irc.StatusKey(): {destination: key, kind: replyOwnMetadataSet, value: "busy"},
		}

		router.RouteOwnMetadataReply("net", "bob", "status", "busy")
		router.RouteOwnMetadataReply("net", "alice", "status", "wrong")
		if len(harness.host.transcripts) != 0 {
			t.Fatalf("transcripts = %+v, want none before the self value matches", harness.host.transcripts)
		}
		if len(router.ownMetadataWatches) != 1 {
			t.Fatalf("watch dropped on a non-matching reply")
		}

		router.RouteOwnMetadataReply("net", "ALICE", "STATUS", "busy")
		if len(harness.host.transcripts) != 1 ||
			harness.host.transcripts[0].FormattedBody != "Standing status set to busy." {
			t.Fatalf("transcripts = %+v, want the set confirmation", harness.host.transcripts)
		}
		if len(router.ownMetadataWatches) != 0 {
			t.Fatalf("watch not dropped after the confirmation")
		}
	})

	t.Run("a clear requires an empty value and persists the cleared avatar", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		router := harness.router
		harness.host.selfNicks = map[string]string{"net": "alice"}
		key := replyRouterTestConversation("net", "#chan")
		router.ownMetadataWatches["net"] = map[string]replyOwnMetadataWatch{
			irc.AvatarKey(): {destination: key, kind: replyOwnMetadataClear},
		}

		router.RouteOwnMetadataReply("net", "alice", "avatar", "https://example.test/a.png")
		if len(router.ownMetadataWatches) != 1 {
			t.Fatal("watch dropped for a clear reply that carried a value")
		}

		router.RouteOwnMetadataReply("net", "alice", "avatar", "")
		if len(harness.host.transcripts) != 1 ||
			harness.host.transcripts[0].FormattedBody != "Avatar cleared." {
			t.Fatalf("transcripts = %+v, want Avatar cleared.", harness.host.transcripts)
		}
		if len(harness.host.persisted) != 1 || harness.host.persisted[0].url != "" {
			t.Fatalf("persisted = %+v, want the avatar cleared", harness.host.persisted)
		}
		if len(router.ownMetadataWatches) != 0 {
			t.Fatalf("watch not dropped after the confirmation")
		}
	})
}

func TestReplyOwnMetadataError(t *testing.T) {
	cases := []struct {
		label string
		key   string
		text  string
		kind  replyOwnMetadataKind
		want  string
	}{
		{"764", irc.StatusKey(), "status :You may not set that", replyOwnMetadataSet,
			"Could not set standing status: status :You may not set that"},
		{"767", irc.AvatarKey(), "avatar :You may not clear that", replyOwnMetadataClear,
			"Could not clear avatar: avatar :You may not clear that"},
		{"769", irc.StatusKey(), "status :You may not set that", replyOwnMetadataSet,
			"Could not set standing status: status :You may not set that"},
	}
	for _, test := range cases {
		harness := newReplyRouterTestHarness()
		router := harness.router
		router.ownMetadataWatches["net"] = map[string]replyOwnMetadataWatch{
			test.key: {destination: replyDestination{}, kind: test.kind},
		}

		entry := irc.Lifecycle("net", irc.LogSeverityAlert, test.label, test.text, replyRouterTestNow)
		router.RouteStatusEntry(entry)

		if len(harness.host.statuses) != 1 {
			t.Fatalf("label %s: statuses = %d, want 1", test.label, len(harness.host.statuses))
		}
		if got := harness.host.statuses[0].Text(); got != test.want {
			t.Fatalf("label %s: status text = %q, want %q", test.label, got, test.want)
		}
		if len(router.ownMetadataWatches) != 0 {
			t.Fatalf("label %s: watch not dropped", test.label)
		}
	}

	t.Run("an unrelated label is ignored", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		router := harness.router
		router.ownMetadataWatches["net"] = map[string]replyOwnMetadataWatch{
			irc.StatusKey(): {kind: replyOwnMetadataSet},
		}

		entry := irc.Lifecycle("net", irc.LogSeverityAlert, "765", "status :nope", replyRouterTestNow)
		router.RouteStatusEntry(entry)

		if len(harness.host.statuses) != 0 {
			t.Fatalf("statuses = %+v, want none", harness.host.statuses)
		}
		if len(router.ownMetadataWatches) != 1 {
			t.Fatal("watch dropped for an unrelated label")
		}
	})
}

func TestReplyDispatchCtcp(t *testing.T) {
	t.Run("a channel target is refused", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		features := irc.NewServerFeatures()
		features.ApplyToken("CHANTYPES=#")
		harness.reducer.SetServerFeatures("net", features)
		harness.host.networkIDs = map[irc.ComposerSurface]string{irc.SurfaceStatus: "net"}

		outcome := harness.router.DispatchCtcp(irc.Command{Verb: irc.VerbPing, Argument: "#chan"}, irc.SurfaceStatus)
		if outcome != irc.OutcomeRefused {
			t.Fatalf("outcome = %v, want Refused for a channel target", outcome)
		}
	})

	t.Run("an empty nick with nothing selected is refused", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		outcome := harness.router.DispatchCtcp(irc.Command{Verb: irc.VerbTime}, irc.SurfaceConversation)
		if outcome != irc.OutcomeRefused {
			t.Fatalf("outcome = %v, want Refused", outcome)
		}
	})

	t.Run("an empty nick with a selection is wrong scope", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		harness.host.hasSelected = true
		harness.host.selectedKey = irc.ConversationKey{NetworkID: "net", NormalizedTarget: "#chan"}
		outcome := harness.router.DispatchCtcp(irc.Command{Verb: irc.VerbVersion}, irc.SurfaceConversation)
		if outcome != irc.OutcomeWrongScope {
			t.Fatalf("outcome = %v, want WrongScope", outcome)
		}
	})

	t.Run("a second token is refused", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		outcome := harness.router.DispatchCtcp(irc.Command{Verb: irc.VerbPing, Argument: "bob extra"}, irc.SurfaceConversation)
		if outcome != irc.OutcomeRefused {
			t.Fatalf("outcome = %v, want Refused", outcome)
		}
	})
}

func TestReplyDispatchWhois(t *testing.T) {
	t.Run("an empty nick with nothing selected is refused", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		harness.host.hasNetworks = true
		outcome := harness.router.DispatchWhois(irc.Command{Verb: irc.VerbWhois}, irc.SurfaceConversation)
		if outcome != irc.OutcomeRefused {
			t.Fatalf("outcome = %v, want Refused", outcome)
		}
	})

	t.Run("an empty nick with a selection is wrong scope", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		harness.host.hasSelected = true
		harness.host.selectedKey = irc.ConversationKey{NetworkID: "net", NormalizedTarget: "#chan"}
		outcome := harness.router.DispatchWhois(irc.Command{Verb: irc.VerbWhois}, irc.SurfaceConversation)
		if outcome != irc.OutcomeWrongScope {
			t.Fatalf("outcome = %v, want WrongScope", outcome)
		}
	})

	t.Run("a named nick on the conversation surface without a session is not connected", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		harness.host.networkIDs = map[irc.ComposerSurface]string{irc.SurfaceConversation: "net"}
		outcome := harness.router.DispatchWhois(irc.Command{Verb: irc.VerbWhois, Argument: "bob"}, irc.SurfaceConversation)
		if outcome != irc.OutcomeNotConnected {
			t.Fatalf("outcome = %v, want NotConnected", outcome)
		}
	})
}

func TestReplyDispatchStatusAndAvatar(t *testing.T) {
	for name, dispatch := range map[string]func(*ReplyRouter, irc.Command, irc.ComposerSurface) irc.CommandOutcome{
		"status": (*ReplyRouter).DispatchStatus,
		"avatar": (*ReplyRouter).DispatchAvatar,
	} {
		t.Run(name+" refuses with no session and no selection", func(t *testing.T) {
			harness := newReplyRouterTestHarness()
			harness.host.hasNetworks = true
			outcome := dispatch(harness.router, irc.Command{Argument: "hi"}, irc.SurfaceConversation)
			if outcome != irc.OutcomeRefused {
				t.Fatalf("outcome = %v, want Refused", outcome)
			}
		})

		t.Run(name+" reports not connected on the Status surface", func(t *testing.T) {
			harness := newReplyRouterTestHarness()
			harness.host.hasNetworks = true
			outcome := dispatch(harness.router, irc.Command{Argument: "hi"}, irc.SurfaceStatus)
			if outcome != irc.OutcomeNotConnected {
				t.Fatalf("outcome = %v, want NotConnected", outcome)
			}
		})
	}
}

func TestReplyForget(t *testing.T) {
	harness := newReplyRouterTestHarness()
	router := harness.router

	router.whoisWatches[replyWhoisWatchKey{networkID: "net", normalizedNick: "a"}] = replyWhoisWatch{}
	router.whoisWatches[replyWhoisWatchKey{networkID: "net2", normalizedNick: "b"}] = replyWhoisWatch{}
	router.ctcpWatches[replyCtcpWatchKey{networkID: "net", normalizedNick: "a", command: "PING"}] = replyCtcpWatch{}
	router.ctcpWatches[replyCtcpWatchKey{networkID: "net2", normalizedNick: "b", command: "TIME"}] = replyCtcpWatch{}
	router.labeledWatches[replyLabeledWatchKey{networkID: "net", requestLabel: "l1"}] = replyLabeledWatch{kind: replyLabeledWhois}
	router.labeledWatches[replyLabeledWatchKey{networkID: "net2", requestLabel: "l2"}] = replyLabeledWatch{kind: replyLabeledCtcp}
	router.ownMetadataWatches["net"] = map[string]replyOwnMetadataWatch{irc.StatusKey(): {}}
	router.ownMetadataWatches["net2"] = map[string]replyOwnMetadataWatch{irc.StatusKey(): {}}

	router.Forget("net")

	if len(router.whoisWatches) != 1 || len(router.ctcpWatches) != 1 ||
		len(router.labeledWatches) != 1 || len(router.ownMetadataWatches) != 1 {
		t.Fatalf("Forget left the wrong state: %d/%d/%d/%d",
			len(router.whoisWatches), len(router.ctcpWatches),
			len(router.labeledWatches), len(router.ownMetadataWatches))
	}
	if _, ok := router.ownMetadataWatches["net2"]; !ok {
		t.Fatal("Forget dropped the wrong network")
	}

	router.Forget("")
	if len(router.whoisWatches) != 1 {
		t.Fatal("Forget with an empty network id changed state")
	}
}

func TestReplyRequestLabelFinished(t *testing.T) {
	harness := newReplyRouterTestHarness()
	router := harness.router
	key := replyLabeledWatchKey{networkID: "net", requestLabel: "lr1"}
	router.labeledWatches[key] = replyLabeledWatch{kind: replyLabeledWhois}

	router.RequestLabelFinished("", "lr1")
	router.RequestLabelFinished("net", "")
	if _, ok := router.labeledWatches[key]; !ok {
		t.Fatal("an empty request-label-finished argument dropped the watch")
	}

	router.RequestLabelFinished("net", "lr1")
	if len(router.labeledWatches) != 0 {
		t.Fatalf("labeledWatches = %d, want 0", len(router.labeledWatches))
	}
}

func TestReplyNoteNickDelivery(t *testing.T) {
	t.Run("a nick watch is marked ambiguous", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		router := harness.router
		key := replyWhoisWatchKey{networkID: "net", normalizedNick: "bob"}
		router.whoisWatches[key] = replyWhoisWatch{}

		router.NoteNickDelivery("net", "BOB")

		if !router.whoisWatches[key].failedIsAmbiguous {
			t.Fatal("the whois watch was not marked ambiguous")
		}

		entry := replyRouterTestWhoisEntry("401", "me", "bob")
		router.RouteStatusEntry(entry)
		if len(harness.host.transcripts) != 0 {
			t.Fatalf("transcripts = %+v, want the ambiguous failure skipped", harness.host.transcripts)
		}
		if len(router.whoisWatches) != 1 {
			t.Fatal("the ambiguous watch was dropped on a failure")
		}
	})

	t.Run("a channel delivery is ignored", func(t *testing.T) {
		harness := newReplyRouterTestHarness()
		router := harness.router
		features := irc.NewServerFeatures()
		features.ApplyToken("CHANTYPES=#")
		harness.reducer.SetServerFeatures("net", features)
		key := replyWhoisWatchKey{networkID: "net", normalizedNick: "#chan"}
		router.whoisWatches[key] = replyWhoisWatch{}

		router.NoteNickDelivery("net", "#chan")

		if router.whoisWatches[key].failedIsAmbiguous {
			t.Fatal("a channel delivery marked the whois watch ambiguous")
		}
	})
}

func TestReplyRouteWhoisMetadataLines(t *testing.T) {
	harness := newReplyRouterTestHarness()
	router := harness.router
	harness.reducer.Apply(irc.MemberMetadataEvent{
		NetworkID: "net",
		Nick:      "bob",
		Key:       irc.StatusKey(),
		Value:     "standing by",
	}, replyRouterTestNow)

	key := replyWhoisWatchKey{networkID: "net", normalizedNick: "bob"}
	router.whoisWatches[key] = replyWhoisWatch{
		destination: replyRouterTestConversation("net", "bob"),
	}

	router.RouteStatusEntry(replyRouterTestWhoisEntry("311", "me", "bob", "user", "host.example", "*", "Real Name"))

	if len(harness.host.transcripts) != 2 {
		t.Fatalf("transcripts = %+v, want the whois line and its metadata line", harness.host.transcripts)
	}
	if got := harness.host.transcripts[1].FormattedBody; got != "bob status standing by" {
		t.Fatalf("metadata line = %q, want %q", got, "bob status standing by")
	}
	if len(harness.host.statuses) != 1 {
		t.Fatalf("statuses = %d, want 1 metadata lifecycle entry", len(harness.host.statuses))
	}
	if harness.host.statuses[0].Label() != "whois" || harness.host.statuses[0].Severity() != irc.LogSeverityInfo {
		t.Fatalf("lifecycle entry = %+v, want an info whois line", harness.host.statuses[0])
	}

	// A second detail line must not repeat the metadata.
	router.RouteStatusEntry(replyRouterTestWhoisEntry("312", "me", "bob", "irc.example", "test server"))
	if len(harness.host.statuses) != 1 {
		t.Fatalf("statuses = %d, want the metadata emitted only once", len(harness.host.statuses))
	}

	router.RouteStatusEntry(replyRouterTestWhoisEntry("318", "me", "bob"))
	if len(router.whoisWatches) != 0 {
		t.Fatalf("whoisWatches = %d, want the terminal line to erase", len(router.whoisWatches))
	}
}

func TestReplyLabeledWhoisMetadata(t *testing.T) {
	harness := newReplyRouterTestHarness()
	router := harness.router
	harness.reducer.Apply(irc.MemberMetadataEvent{
		NetworkID: "net",
		Nick:      "bob",
		Key:       irc.DisplayNameKey(),
		Value:     "Robert",
	}, replyRouterTestNow)

	watchKey := replyLabeledWatchKey{networkID: "net", requestLabel: "lr9"}
	router.labeledWatches[watchKey] = replyLabeledWatch{
		kind:        replyLabeledWhois,
		destination: replyRouterTestConversation("net", "bob"),
	}

	detail := replyRouterTestWhoisEntry("311", "me", "bob", "user", "host.example", "*", "Real Name")
	detail.SetRequestLabel("lr9")
	router.RouteStatusEntry(detail)

	if len(harness.host.transcripts) != 2 {
		t.Fatalf("transcripts = %+v, want the whois line and its metadata line", harness.host.transcripts)
	}
	if got := harness.host.transcripts[1].FormattedBody; got != "bob is also known as Robert" {
		t.Fatalf("metadata line = %q, want %q", got, "bob is also known as Robert")
	}
	if len(router.labeledWatches) != 1 {
		t.Fatal("the labeled watch was dropped before the terminal line")
	}

	terminal := replyRouterTestWhoisEntry("318", "me", "bob")
	terminal.SetRequestLabel("lr9")
	router.RouteStatusEntry(terminal)
	if len(router.labeledWatches) != 0 {
		t.Fatalf("labeledWatches = %d, want the terminal line to erase", len(router.labeledWatches))
	}
}

func TestReplyRouteLabeledCtcp(t *testing.T) {
	harness := newReplyRouterTestHarness()
	router := harness.router
	watchKey := replyLabeledWatchKey{networkID: "net", requestLabel: "lr5"}
	router.labeledWatches[watchKey] = replyLabeledWatch{
		kind:        replyLabeledCtcp,
		destination: replyRouterTestConversation("net", "bob"),
	}
	router.ctcpWatches[replyCtcpWatchKey{networkID: "net", normalizedNick: "bob", command: "VERSION"}] = replyCtcpWatch{
		destination: replyRouterTestConversation("net", "bob"),
	}

	message := irc.Message{
		Prefix:  &irc.Prefix{Nick: "bob"},
		Command: "NOTICE",
		Params:  []string{"me", "\x01VERSION omairc 1.0\x01"},
	}
	entry := irc.Incoming("net", message, "#", replyRouterTestNow)
	entry.SetRequestLabel("lr5")
	if entry.CtcpReplyLine() == nil {
		t.Fatalf("entry carried no CTCP reply: %+v", entry)
	}
	router.RouteStatusEntry(entry)

	if len(harness.host.transcripts) != 1 {
		t.Fatalf("transcripts = %+v, want one CTCP body", harness.host.transcripts)
	}
	if len(router.labeledWatches) != 0 || len(router.ctcpWatches) != 0 {
		t.Fatalf("labeled=%d ctcp=%d, want both cleared", len(router.labeledWatches), len(router.ctcpWatches))
	}
}
