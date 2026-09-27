package irc

import (
	"testing"
	"time"
)

// translateLine parses and translates one wire line with the default feature
// set and the fixed reducer clock.
func translateLine(t *testing.T, line string) []Event {
	t.Helper()
	message := mustParse(t, line)
	return Translate(networkA, "omairc", NewServerFeatures(), message, reducerTimestamp)
}

func TestTranslateNumericTable(t *testing.T) {
	cases := []struct {
		name string
		line string
		want EventKind
	}{
		{"self away", ":server 306 omairc :marked away", EventSelfAway},
		{"self back", ":server 305 omairc :back", EventSelfAway},
		{"whois away", ":server 352 omairc #room user host server Alice G :0 Alice", EventAway},
		{"whois account", ":server 330 omairc Alice whoisacct :is logged in as", EventAccount},
		{"channel topic reply", ":server 332 omairc #room :topic", EventTopic},
		{"names reply", ":server 353 omairc = #room :@Alice +Bob", EventNames},
		{"end of names", ":server 366 omairc #room :End of /NAMES list.", EventNames},
		{"mode", ":op!u@h MODE #room +o alice", EventMode},
		{"topic", ":op!u@h TOPIC #room :topic", EventTopic},
		{"part", ":alice!u@h PART #room :bye", EventPart},
		{"quit", ":alice!u@h QUIT :gone", EventQuit},
		{"nick", ":alice!u@h NICK Alicia", EventNick},
		{"kick", ":op!u@h KICK #room bob :bye", EventKick},
		{"account", ":alice!u@h ACCOUNT services", EventAccount},
		{"away", ":alice!u@h AWAY :lunch", EventAway},
		{"metadata 761", ":server 761 omairc Alice status priv :writing docs", EventMemberMetadata},
		{"metadata 766", ":server 766 omairc Alice status :Key not set", EventMemberMetadata},
		{"join failure 471", ":server 471 omairc #room :Cannot join channel (+l)", EventChannelError},
		{"join failure 473", ":server 473 omairc #room :Invite only", EventChannelError},
		{"ambiguous join failure 437", ":server 437 omairc #room :unavailable", EventChannelError},
	}
	for _, testCase := range cases {
		t.Run(testCase.name, func(t *testing.T) {
			events := translateLine(t, testCase.line)
			if len(events) == 0 {
				t.Fatalf("Translate(%q) produced no events", testCase.line)
			}
			requireInt(t, "first kind", int(KindOf(events[0])), int(testCase.want))
		})
	}
}

func TestTranslatesNoticeUpdatesAccountButProducesNoTranscriptEvent(t *testing.T) {
	if events := translateLine(t, ":Alice!u@h NOTICE #room :hi"); len(events) != 0 {
		t.Fatalf("NOTICE produced %d events, want 0", len(events))
	}

	events := translateLine(t, "@account=services :Alice!u@h NOTICE #room :hi")
	requireInt(t, "events", len(events), 1)
	account, ok := events[0].(AccountEvent)
	requireTrue(t, "account event", ok)
	requireString(t, "nick", account.Nick, "Alice")
	requireString(t, "account", account.Account, "services")
}

func TestTranslatesPrivmsgChannelAndDirectDisplayTarget(t *testing.T) {
	events := translateLine(t, ":alice!u@h PRIVMSG #room :hello")
	requireInt(t, "channel events", len(events), 1)
	message, ok := events[0].(MessageEvent)
	requireTrue(t, "channel message", ok)
	requireString(t, "channel conversation", message.Conversation.NormalizedTarget, "#room")
	requireString(t, "channel author", message.Author, "alice")
	requireString(t, "channel body", message.Body, "hello")
	requireString(t, "channel target", message.Target, "#room")

	events = translateLine(t, ":alice!u@h PRIVMSG omairc :hi")
	requireInt(t, "inbound direct events", len(events), 1)
	message = events[0].(MessageEvent)
	requireString(t, "inbound direct conversation", message.Conversation.NormalizedTarget, "alice")
	requireString(t, "inbound direct target", message.Target, "alice")

	events = translateLine(t, ":omairc!u@h PRIVMSG alice :hi")
	requireInt(t, "outbound direct events", len(events), 1)
	message = events[0].(MessageEvent)
	requireString(t, "outbound direct conversation", message.Conversation.NormalizedTarget, "alice")
	requireString(t, "outbound direct target", message.Target, "alice")
}

func TestTranslatesCtcpActionAndDropsOtherCtcp(t *testing.T) {
	events := translateLine(t, ":alice!u@h PRIVMSG #room :\x01ACTION waves\x01")
	requireInt(t, "action events", len(events), 1)
	action, ok := events[0].(ActionEvent)
	requireTrue(t, "action event", ok)
	requireString(t, "action author", action.Author, "alice")
	requireString(t, "action body", action.Body, "waves")

	if events := translateLine(t, ":alice!u@h PRIVMSG #room :\x01VERSION\x01"); len(events) != 0 {
		t.Fatalf("non-ACTION CTCP produced %d events, want 0", len(events))
	}
}

func TestTranslatesJoinExtendedJoinAccount(t *testing.T) {
	events := translateLine(t, ":alice!u@h JOIN #room account :Alice Example")
	requireInt(t, "extended events", len(events), 1)
	join, ok := events[0].(JoinEvent)
	requireTrue(t, "extended join", ok)
	requireString(t, "extended channel", join.Channel, "#room")
	requireString(t, "extended nick", join.Nick, "alice")
	requireTrue(t, "extended account present", join.Account != nil)
	requireString(t, "extended account", *join.Account, "account")

	events = translateLine(t, ":alice!u@h JOIN #room")
	join = events[0].(JoinEvent)
	requireTrue(t, "classic account nil", join.Account == nil)
}

func TestTranslatesNamesAndEndOfNames(t *testing.T) {
	events := translateLine(t, ":server 353 omairc = #room :@Alice +Bob")
	names, ok := events[0].(NamesEvent)
	requireTrue(t, "names event", ok)
	requireString(t, "names channel", names.Channel, "#room")
	requireFalse(t, "names complete", names.Complete)
	requireInt(t, "names count", len(names.Names), 2)
	requireString(t, "names[0] nick", names.Names[0].Nick, "Alice")
	if names.Names[0].Ranks != parsedName(t, "@Alice").Ranks {
		t.Fatalf("names[0] ranks = %b, want %b", names.Names[0].Ranks, parsedName(t, "@Alice").Ranks)
	}
	requireString(t, "names[1] nick", names.Names[1].Nick, "Bob")
	if names.Names[1].Ranks != parsedName(t, "+Bob").Ranks {
		t.Fatalf("names[1] ranks = %b, want %b", names.Names[1].Ranks, parsedName(t, "+Bob").Ranks)
	}

	events = translateLine(t, ":server 366 omairc #room :End of /NAMES list.")
	end, ok := events[0].(NamesEvent)
	requireTrue(t, "end of names event", ok)
	requireString(t, "end channel", end.Channel, "#room")
	requireTrue(t, "end complete", end.Complete)
}

func TestTranslatesAwayNumerics(t *testing.T) {
	events := translateLine(t, ":server 306 omairc :You have been marked as being away")
	self, ok := events[0].(SelfAwayEvent)
	requireTrue(t, "306 event", ok)
	requireTrue(t, "306 away", self.Away)

	events = translateLine(t, ":server 305 omairc :You are no longer away")
	self = events[0].(SelfAwayEvent)
	requireFalse(t, "305 away", self.Away)

	events = translateLine(t, ":server 352 omairc #room user host server Alice G :0 Alice")
	away, ok := events[0].(AwayEvent)
	requireTrue(t, "352 event", ok)
	requireString(t, "352 nick", away.Nick, "Alice")
	requireTrue(t, "352 away", away.Away != nil)

	events = translateLine(t, ":server 352 omairc #room user host server Alice H :0 Alice")
	away = events[0].(AwayEvent)
	requireTrue(t, "352 here", away.Away == nil)

	events = translateLine(t, ":alice!u@h AWAY :lunch")
	away = events[0].(AwayEvent)
	requireTrue(t, "away present", away.Away != nil)
	requireString(t, "away reason", away.Away.Reason, "lunch")

	events = translateLine(t, ":alice!u@h AWAY")
	away = events[0].(AwayEvent)
	requireTrue(t, "away cleared", away.Away == nil)
}

func TestTranslatesMetadataNumerics(t *testing.T) {
	events := translateLine(t, ":server 761 omairc Alice status priv :writing docs")
	metadata, ok := events[0].(MemberMetadataEvent)
	requireTrue(t, "761 event", ok)
	requireString(t, "761 nick", metadata.Nick, "Alice")
	requireString(t, "761 key", metadata.Key, "status")
	requireString(t, "761 value", metadata.Value, "writing docs")

	events = translateLine(t, ":server 766 omairc Alice status :Key not set")
	metadata, ok = events[0].(MemberMetadataEvent)
	requireTrue(t, "766 event", ok)
	requireString(t, "766 nick", metadata.Nick, "Alice")
	requireString(t, "766 key", metadata.Key, "status")
	requireString(t, "766 value", metadata.Value, "")

	if events := translateLine(t, ":server 761 omairc Alice unknown priv :value"); len(events) != 0 {
		t.Fatalf("unknown metadata key produced %d events, want 0", len(events))
	}
	if events := translateLine(t, ":server 761 omairc #room status priv :value"); len(events) != 0 {
		t.Fatalf("channel metadata target produced %d events, want 0", len(events))
	}
}

func TestTranslatesJoinFailureNumerics(t *testing.T) {
	events := translateLine(t, ":server 471 omairc #room :Cannot join channel (+l)")
	channelError, ok := events[0].(ChannelErrorEvent)
	requireTrue(t, "471 event", ok)
	requireString(t, "471 channel", channelError.Channel, "#room")
	requireString(t, "471 body", channelError.Body, "Cannot join channel (+l)")

	events = translateLine(t, ":server 437 omairc #room :unavailable")
	channelError, ok = events[0].(ChannelErrorEvent)
	requireTrue(t, "ambiguous channel event", ok)
	requireString(t, "ambiguous channel", channelError.Channel, "#room")

	if events := translateLine(t, ":server 437 omairc someNick :unavailable"); len(events) != 0 {
		t.Fatalf("ambiguous non-channel target produced %d events, want 0", len(events))
	}
}

func TestTranslatesAccountTagPresentVersusAbsent(t *testing.T) {
	events := translateLine(t, ":Alice!u@h PRIVMSG #room :hi")
	requireInt(t, "absent events", len(events), 1)
	if _, ok := events[0].(MessageEvent); !ok {
		t.Fatalf("absent account tag produced %T, want MessageEvent", events[0])
	}

	events = translateLine(t, "@account=services :Alice!u@h PRIVMSG #room :hi")
	requireInt(t, "present events", len(events), 2)
	account, ok := events[0].(AccountEvent)
	requireTrue(t, "present account event", ok)
	requireString(t, "present account", account.Account, "services")
	if _, ok := events[1].(MessageEvent); !ok {
		t.Fatalf("present account tag second event is %T, want MessageEvent", events[1])
	}

	events = translateLine(t, "@account=* :Alice!u@h PRIVMSG #room :hi")
	account = events[0].(AccountEvent)
	requireString(t, "logout account", account.Account, "*")

	events = translateLine(t, "@account :Alice!u@h PRIVMSG #room :hi")
	requireInt(t, "valueless events", len(events), 2)
	account = events[0].(AccountEvent)
	requireString(t, "valueless account", account.Account, "")
}

func TestConversationForClassifiesTargets(t *testing.T) {
	features := NewServerFeatures()
	fromAlice := mustParse(t, ":alice!u@h PRIVMSG #room :hi")

	if key, ok := ConversationFor(networkA, "#room", fromAlice, "omairc", features); !ok || key.NormalizedTarget != "#room" {
		t.Fatalf("channel ConversationFor = %+v, %v, want #room", key, ok)
	}
	if key, ok := ConversationFor(networkA, "omairc", fromAlice, "omairc", features); !ok || key.NormalizedTarget != "alice" {
		t.Fatalf("inbound direct ConversationFor = %+v, %v, want alice", key, ok)
	}

	fromSelf := mustParse(t, ":omairc!u@h PRIVMSG alice :hi")
	if key, ok := ConversationFor(networkA, "alice", fromSelf, "omairc", features); !ok || key.NormalizedTarget != "alice" {
		t.Fatalf("outbound direct ConversationFor = %+v, %v, want alice", key, ok)
	}

	service := mustParse(t, ":NickServ!s@services PRIVMSG omairc :hi")
	if _, ok := ConversationFor(networkA, "omairc", service, "omairc", features); ok {
		t.Fatal("service identity must not resolve to a conversation")
	}
	noUser := mustParse(t, ":alice PRIVMSG omairc :hi")
	if _, ok := ConversationFor(networkA, "omairc", noUser, "omairc", features); ok {
		t.Fatal("a user-less prefix must not resolve to a conversation")
	}
	noticeTarget := mustParse(t, ":alice!u@h PRIVMSG * :hi")
	if _, ok := ConversationFor(networkA, "*", noticeTarget, "omairc", features); ok {
		t.Fatal("a network notice target must not resolve to a conversation")
	}
	bouncer := mustParse(t, ":*status!u@h PRIVMSG omairc :hi")
	if _, ok := ConversationFor(networkA, "omairc", bouncer, "omairc", features); ok {
		t.Fatal("a non-routable bouncer nick must not resolve to a conversation")
	}
}

func TestTranslatesTypingTagmsg(t *testing.T) {
	events := translateLine(t, "@+typing=active :alice!u@h TAGMSG #omarchy")
	requireInt(t, "active events", len(events), 1)
	typing, ok := events[0].(TypingEvent)
	requireTrue(t, "active typing event", ok)
	requireString(t, "active nick", typing.Nick, "alice")
	requireInt(t, "active phase", int(typing.Phase), int(TypingActive))
	requireString(t, "active conversation", typing.Conversation.NormalizedTarget, "#omarchy")

	events = translateLine(t, "@+typing=PAUSED :alice!u@h TAGMSG omairc")
	requireInt(t, "paused events", len(events), 1)
	typing, ok = events[0].(TypingEvent)
	requireTrue(t, "paused typing event", ok)
	requireInt(t, "paused phase", int(typing.Phase), int(TypingPaused))
	requireString(t, "paused conversation", typing.Conversation.NormalizedTarget, "alice")

	for _, line := range []string{
		"@+typing=draft :alice!u@h TAGMSG #omarchy",
		":alice!u@h TAGMSG #omarchy",
		"@+typing=active TAGMSG #omarchy",
	} {
		if events := translateLine(t, line); len(events) != 0 {
			t.Fatalf("Translate(%q) produced %d events, want 0", line, len(events))
		}
	}

	events = translateLine(t, "@+typing=active :alice!u@h PRIVMSG #omarchy :here")
	requireInt(t, "chat events", len(events), 1)
	if _, ok := events[0].(MessageEvent); !ok {
		t.Fatalf("typing tag on chat produced %T, want MessageEvent", events[0])
	}
}

func TestTranslatesHistoryBatchServerTime(t *testing.T) {
	now := reducerTimestamp
	batch := HistoryBatch{
		Target: "lena",
		Lines:  []Message{mustParse(t, ":lena!u@h PRIVMSG omairc :hi")},
	}
	event, ok := TranslateHistory(networkA, "omairc", NewServerFeatures(), batch, now)
	requireTrue(t, "history ok", ok)
	requireInt(t, "lines", len(event.Lines), 1)
	line := event.Lines[0]
	requireString(t, "author", line.Author, "lena")
	requireString(t, "body", line.Body, "hi")
	requireTrue(t, "serverTime nil", line.ServerTime == nil)
	requireTrue(t, "timestamp fallback", line.Timestamp.Equal(now))

	batch = HistoryBatch{
		Target: "lena",
		Lines:  []Message{mustParse(t, "@time=2024-03-09T16:00:00.620Z :lena!u@h PRIVMSG omairc :hi")},
	}
	event, ok = TranslateHistory(networkA, "omairc", NewServerFeatures(), batch, now)
	requireTrue(t, "history with time ok", ok)
	line = event.Lines[0]
	requireTrue(t, "serverTime present", line.ServerTime != nil)
	want, err := time.Parse(time.RFC3339Nano, "2024-03-09T16:00:00.620Z")
	if err != nil {
		t.Fatalf("parse want time: %v", err)
	}
	requireTrue(t, "serverTime value", line.ServerTime.Equal(want))
	requireTrue(t, "timestamp uses tag", line.Timestamp.Equal(want))

	if _, ok := TranslateHistory(networkA, "omairc", NewServerFeatures(), HistoryBatch{}, now); ok {
		t.Fatal("an empty target must not translate")
	}
}

func TestTranslatesHistoryBouncerQueryPreviousNickFallback(t *testing.T) {
	batch := HistoryBatch{
		Target: "lena",
		Kind:   HistoryBouncerPlayback,
		Lines: []Message{
			mustParse(t, ":oldnick!u@h PRIVMSG lena :mine"),
			mustParse(t, ":lena!u@h PRIVMSG oldnick :before"),
			mustParse(t, ":stranger!u@h PRIVMSG oldnick :elsewhere"),
			mustParse(t, ":lena!u@h PRIVMSG #other :channel"),
			mustParse(t, ":NickServ!s@services PRIVMSG lena :hi"),
		},
	}
	event, ok := TranslateHistory(networkA, "omairc", NewServerFeatures(), batch, reducerTimestamp)
	requireTrue(t, "ok", ok)
	requireInt(t, "lines", len(event.Lines), 2)
	requireString(t, "lines[0] author", event.Lines[0].Author, "oldnick")
	requireString(t, "lines[0] body", event.Lines[0].Body, "mine")
	requireString(t, "lines[1] author", event.Lines[1].Author, "lena")
	requireString(t, "lines[1] body", event.Lines[1].Body, "before")
}

func TestTranslatesHistoryChatKeepsOnlyCurrentNick(t *testing.T) {
	batch := HistoryBatch{
		Target: "lena",
		Lines:  []Message{mustParse(t, ":oldnick!u@h PRIVMSG lena :mine")},
	}
	event, ok := TranslateHistory(networkA, "omairc", NewServerFeatures(), batch, reducerTimestamp)
	requireTrue(t, "ok", ok)
	requireInt(t, "lines", len(event.Lines), 0)
}

func TestTranslatesHistoryChannelBatchKeepsOnlyChat(t *testing.T) {
	batch := HistoryBatch{
		Target: "#omarchy",
		Lines: []Message{
			mustParse(t, ":alice!u@h JOIN :#omarchy"),
			mustParse(t, ":alice!u@h PRIVMSG #omarchy :from history"),
		},
	}
	event, ok := TranslateHistory(networkA, "omairc", NewServerFeatures(), batch, reducerTimestamp)
	requireTrue(t, "ok", ok)
	requireInt(t, "lines", len(event.Lines), 1)
	requireString(t, "body", event.Lines[0].Body, "from history")
}
