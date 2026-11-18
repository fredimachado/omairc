package irc

import (
	"strings"
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
		{"setname", ":alice!u@h SETNAME :Alice Lee", EventNickFacts},
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
	requireInt(t, "extended events", len(events), 2) // the join and its userhost fact
	_, ok := events[1].(NickFactsEvent)
	requireTrue(t, "userhost fact", ok)
	join, ok := events[0].(JoinEvent)
	requireTrue(t, "extended join", ok)
	requireString(t, "extended channel", join.Channel, "#room")
	requireString(t, "extended nick", join.Nick, "alice")
	requireTrue(t, "extended account present", join.Account != nil)
	requireString(t, "extended account", *join.Account, "account")
	requireTrue(t, "extended realname present", join.Realname != nil)
	requireString(t, "extended realname", *join.Realname, "Alice Example")

	events = translateLine(t, ":alice!u@h JOIN #room")
	join = events[0].(JoinEvent)
	requireTrue(t, "classic account nil", join.Account == nil)
	requireTrue(t, "classic realname nil", join.Realname == nil)
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
	requireTrue(t, "352 realname set", away.Realname != nil)
	requireString(t, "352 realname", *away.Realname, "Alice")
	requireFalse(t, "352 is not whois-only", away.RealnameOnly)

	events = translateLine(t, ":server 352 omairc #room user host server Alice H :0 Alice")
	away = events[0].(AwayEvent)
	requireTrue(t, "352 here", away.Away == nil)

	events = translateLine(t, ":server 352 omairc #room user host server Alice H* :0 Alice Vale")
	requireInt(t, "352 facts count", len(events), 2)
	away = events[0].(AwayEvent)
	requireTrue(t, "352 realname set", away.Realname != nil)
	requireString(t, "352 realname", *away.Realname, "Alice Vale")
	facts, ok := events[1].(NickFactsEvent)
	requireTrue(t, "352 facts", ok)
	requireTrue(t, "352 realname stays on the away event", facts.Realname == nil)
	requireTrue(t, "352 operator set", facts.ServerOperator != nil && *facts.ServerOperator)
	requireFalse(t, "352 here is not away", away.Away != nil)

	events = translateLine(t, ":server 311 omairc Alice user host * :Alice Example")
	away = events[0].(AwayEvent)
	requireTrue(t, "311 realname only", away.RealnameOnly)
	requireTrue(t, "311 leaves away alone", away.Away == nil)
	requireString(t, "311 realname", *away.Realname, "Alice Example")

	events = translateLine(t, ":server 313 omairc Alice :is an IRC operator")
	requireInt(t, "313 events", len(events), 1)
	facts, ok = events[0].(NickFactsEvent)
	requireTrue(t, "313 facts", ok)
	requireTrue(t, "313 leaves realname", facts.Realname == nil)
	requireTrue(t, "313 operator", facts.ServerOperator != nil && *facts.ServerOperator)

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
	requireInt(t, "lines", len(event.Lines), 2)
	requireString(t, "join author", event.Lines[0].Author, "")
	requireInt(t, "join kind", int(event.Lines[0].Kind), int(MessageKindEvent))
	requireString(t, "join body", event.Lines[0].Body, "alice joined")
	requireString(t, "body", event.Lines[1].Body, "from history")
}

func roomMemberLookup() JoinedMemberChannel {
	return func(channel, sender string) (string, bool) {
		if !strings.EqualFold(sender, "bot") || !strings.EqualFold(channel, "#room") {
			return "", false
		}
		return "#room", true
	}
}

func translateJoined(t *testing.T, line string, joined JoinedMemberChannel) []Event {
	t.Helper()
	message := mustParse(t, line)
	return Translate(networkA, "omairc", NewServerFeatures(), message, reducerTimestamp, joined)
}

func TestChannelContextPrivateLineIsOnlyVisibleToYou(t *testing.T) {
	joined := roomMemberLookup()
	marked := translateJoined(t, "@+draft/channel-context=#room :bot!u@h PRIVMSG omairc :just you", joined)
	requireInt(t, "marked events", len(marked), 1)
	message, ok := marked[0].(MessageEvent)
	requireTrue(t, "marked message", ok)
	requireString(t, "marked conversation", message.Conversation.NormalizedTarget, "#room")
	requireString(t, "marked target", message.Target, "#room")
	requireString(t, "marked body", message.Body, "just you")
	requireTrue(t, "marked only you", message.OnlyVisibleToYou)

	unprefixed := translateJoined(t, "@draft/channel-context=#room :bot!u@h PRIVMSG omairc :also you", joined)
	requireInt(t, "unprefixed events", len(unprefixed), 1)
	unprefixedMessage := unprefixed[0].(MessageEvent)
	requireTrue(t, "unprefixed only you", unprefixedMessage.OnlyVisibleToYou)

	spelled := translateJoined(t, "@+draft/channel-context=#Room :bot!u@h PRIVMSG omairc :canonical", joined)
	requireInt(t, "spelled events", len(spelled), 1)
	spelledMessage := spelled[0].(MessageEvent)
	requireString(t, "canonical target", spelledMessage.Target, "#room")
	requireTrue(t, "canonical only you", spelledMessage.OnlyVisibleToYou)

	ordinary := translateJoined(t, ":alice!u@h PRIVMSG #room :everyone", joined)
	requireInt(t, "ordinary events", len(ordinary), 1)
	ordinaryMessage := ordinary[0].(MessageEvent)
	requireString(t, "ordinary conversation", ordinaryMessage.Conversation.NormalizedTarget, "#room")
	requireFalse(t, "ordinary only you", ordinaryMessage.OnlyVisibleToYou)

	taggedChannel := translateJoined(t, "@+draft/channel-context=#room :alice!u@h PRIVMSG #room :still public", joined)
	requireInt(t, "tagged channel events", len(taggedChannel), 1)
	requireFalse(t, "tagged channel only you", taggedChannel[0].(MessageEvent).OnlyVisibleToYou)

	status := translateJoined(t, ":op!u@h PRIVMSG @#room :ops", joined)
	requireInt(t, "status events", len(status), 0)
	requireTrue(t, "status stays on Status", StatusKeepsIncoming(mustParse(t, ":op!u@h PRIVMSG @#room :ops"), "omairc", ""))
	requireFalse(t, "private delivery stays off Status", StatusKeepsIncoming(
		mustParse(t, "@+draft/channel-context=#room :bot!u@h PRIVMSG omairc :just you"), "omairc", ""))

	notice := translateJoined(t, "@+draft/channel-context=#room :bot!u@h NOTICE omairc :heads up", joined)
	requireInt(t, "notice events", len(notice), 1)
	noticeEvent, ok := notice[0].(NoticeEvent)
	requireTrue(t, "notice event", ok)
	requireTrue(t, "notice only you", noticeEvent.OnlyVisibleToYou)
	requireString(t, "notice target", noticeEvent.Target, "#room")
	if events := translateLine(t, ":alice!u@h NOTICE omairc :dm notice"); len(events) != 0 {
		t.Fatalf("DM NOTICE produced %d events, want 0", len(events))
	}

	batch := HistoryBatch{
		Target: "#room",
		Lines: []Message{
			mustParse(t, "@+draft/channel-context=#room :bot!u@h PRIVMSG omairc :replayed"),
			mustParse(t, ":alice!u@h PRIVMSG #room :public replay"),
			mustParse(t, "@+draft/channel-context=#room :bot!u@h NOTICE omairc :notice replay"),
			mustParse(t, ":op!u@h PRIVMSG @#room :ops replay"),
		},
	}
	history, ok := TranslateHistory(networkA, "omairc", NewServerFeatures(), batch, reducerTimestamp, joined)
	requireTrue(t, "history", ok)
	requireInt(t, "history lines", len(history.Lines), 2)
	requireTrue(t, "replay marked", history.Lines[0].OnlyVisibleToYou)
	requireInt(t, "replay kind", int(history.Lines[0].Kind), int(MessageKindChat))
	requireString(t, "replay body", history.Lines[0].Body, "replayed")
	requireFalse(t, "public replay", history.Lines[1].OnlyVisibleToYou)
	requireString(t, "public body", history.Lines[1].Body, "public replay")
}

func TestChannelContextStaysADirectMessageWhenUntrusted(t *testing.T) {
	accept := func(channel, _ string) (string, bool) { return channel, true }
	expectDirect := func(t *testing.T, events []Event, sender, body string) {
		t.Helper()
		requireInt(t, body+" events", len(events), 1)
		message, ok := events[0].(MessageEvent)
		requireTrue(t, body+" message", ok)
		requireString(t, body+" author", message.Author, sender)
		requireString(t, body+" body", message.Body, body)
		requireString(t, body+" conversation", message.Conversation.NormalizedTarget, strings.ToLower(sender))
		requireString(t, body+" target", message.Target, sender)
		requireFalse(t, body+" only you", message.OnlyVisibleToYou)
	}

	expectDirect(t, translateJoined(t, "@+draft/channel-context=#room :eve!u@h PRIVMSG omairc :unjoined", nil), "eve", "unjoined")
	expectDirect(t, translateJoined(t, "@+draft/channel-context=#room :eve!u@h PRIVMSG omairc :not a member", roomMemberLookup()), "eve", "not a member")
	requireInt(t, "channel-context length lock", ChannelContextLimit, 50)
	atLimit := "@+draft/channel-context=#" + strings.Repeat("x", ChannelContextLimit-1) + " :bot!u@h PRIVMSG omairc :at limit"
	limited := translateJoined(t, atLimit, accept)
	requireInt(t, "at-limit events", len(limited), 1)
	limitedMessage, ok := limited[0].(MessageEvent)
	requireTrue(t, "at-limit message", ok)
	requireTrue(t, "exactly ChannelContextLimit runes still relocates", limitedMessage.OnlyVisibleToYou)
	overlong := "@+draft/channel-context=#" + strings.Repeat("x", ChannelContextLimit) + " :eve!u@h PRIVMSG omairc :overlong"
	expectDirect(t, translateJoined(t, overlong, accept), "eve", "overlong")
	farOverlong := "@+draft/channel-context=#" + strings.Repeat("x", 60) + " :eve!u@h PRIVMSG omairc :far overlong"
	expectDirect(t, translateJoined(t, farOverlong, accept), "eve", "far overlong")
	expectDirect(t, translateJoined(t, "@+draft/channel-context=#a,#b :eve!u@h PRIVMSG omairc :comma", accept), "eve", "comma")
	expectDirect(t, translateJoined(t, "@+draft/channel-context=#my\\sroom :eve!u@h PRIVMSG omairc :space", accept), "eve", "space")
	expectDirect(t, translateJoined(t, "@+draft/channel-context=#room\x01x :eve!u@h PRIVMSG omairc :control", accept), "eve", "control")
	expectDirect(t, translateJoined(t, "@+draft/channel-context=omairc :eve!u@h PRIVMSG omairc :own nick", accept), "eve", "own nick")

	action := translateJoined(t, "@+draft/channel-context=#room :eve!u@h PRIVMSG omairc :\x01ACTION waves\x01", nil)
	requireInt(t, "action events", len(action), 1)
	actionEvent, ok := action[0].(ActionEvent)
	requireTrue(t, "action event", ok)
	requireString(t, "action conversation", actionEvent.Conversation.NormalizedTarget, "eve")
	requireString(t, "action body", actionEvent.Body, "waves")
	requireFalse(t, "action only you", actionEvent.OnlyVisibleToYou)
	if events := translateJoined(t, "@+draft/channel-context=#room :eve!u@h NOTICE omairc :heads up", nil); len(events) != 0 {
		t.Fatalf("unjoined NOTICE produced %d events, want 0", len(events))
	}

	history, ok := TranslateHistory(networkA, "omairc", NewServerFeatures(), HistoryBatch{
		Target: "alice",
		Lines: []Message{
			mustParse(t, "@+draft/channel-context=#room :alice!u@h PRIVMSG omairc :kept in query"),
		},
	}, reducerTimestamp, accept)
	requireTrue(t, "query history", ok)
	requireString(t, "query target", history.Conversation.NormalizedTarget, "alice")
	requireInt(t, "query lines", len(history.Lines), 1)
	requireString(t, "query body", history.Lines[0].Body, "kept in query")
	requireInt(t, "query kind", int(history.Lines[0].Kind), int(MessageKindChat))
	requireFalse(t, "query only you", history.Lines[0].OnlyVisibleToYou)

	unjoined, ok := TranslateHistory(networkA, "omairc", NewServerFeatures(), HistoryBatch{
		Target: "#room",
		Lines: []Message{
			mustParse(t, "@+draft/channel-context=#room :bot!u@h PRIVMSG omairc :dropped"),
			mustParse(t, ":alice!u@h PRIVMSG #room :public only"),
		},
	}, reducerTimestamp)
	requireTrue(t, "channel history", ok)
	requireInt(t, "channel lines", len(unjoined.Lines), 1)
	requireString(t, "public only", unjoined.Lines[0].Body, "public only")
	requireFalse(t, "public only flag", unjoined.Lines[0].OnlyVisibleToYou)
}

func TestSetnameTranslatesToRealnameFact(t *testing.T) {
	events := translateLine(t, ":alice!u@h SETNAME :Alice Lee")
	requireInt(t, "events", len(events), 1)
	facts, ok := events[0].(NickFactsEvent)
	requireTrue(t, "NickFactsEvent", ok)
	requireString(t, "nick", facts.Nick, "alice")
	requireTrue(t, "realname set", facts.Realname != nil)
	requireString(t, "realname", *facts.Realname, "Alice Lee")
	requireTrue(t, "operator untouched", facts.ServerOperator == nil)
}

func TestSetnameWithEmptyParamClearsRealname(t *testing.T) {
	for _, line := range []string{":alice!u@h SETNAME :", ":alice!u@h SETNAME"} {
		events := translateLine(t, line)
		requireInt(t, line+" events", len(events), 1)
		facts, ok := events[0].(NickFactsEvent)
		requireTrue(t, line+" NickFactsEvent", ok)
		requireTrue(t, line+" realname set", facts.Realname != nil)
		requireString(t, line+" realname", *facts.Realname, "")
	}
}

func TestSetnameWithoutPrefixIsIgnored(t *testing.T) {
	requireInt(t, "events", len(translateLine(t, "SETNAME :Alice Lee")), 0)
}

func TestInviteToSelfEmitsNoEvent(t *testing.T) {
	requireInt(t, "bare", len(translateLine(t, ":op!u@h INVITE omairc #room")), 0)
	requireInt(t, "trailing", len(translateLine(t, ":op!u@h INVITE omairc :#room")), 0)
}

func TestInviteToOtherEmitsInviteEvent(t *testing.T) {
	events := translateLine(t, ":op!u@h INVITE bob #room")
	requireInt(t, "events", len(events), 1)
	invite, ok := events[0].(InviteEvent)
	requireTrue(t, "InviteEvent", ok)
	requireString(t, "network", invite.NetworkID, networkA)
	requireString(t, "channel", invite.Channel, "#room")
	requireString(t, "inviter", invite.Inviter, "op")
	requireString(t, "target", invite.Target, "bob")
}

func TestInviteToOtherNickCaseMappedSelfIsSelf(t *testing.T) {
	translateAs := func(nick, line string) []Event {
		return Translate(networkA, nick, NewServerFeatures(), mustParse(t, line), reducerTimestamp)
	}
	requireInt(t, "ASCII case", len(translateAs("me", ":op!u@h INVITE ME #room")), 0)
	requireInt(t, "rfc1459 brackets", len(translateAs("a[b", ":op!u@h INVITE A{B #room")), 0)
	requireInt(t, "different nick", len(translateAs("me", ":op!u@h INVITE mee #room")), 1)
}

func TestInviteWithoutChannelEmitsNothing(t *testing.T) {
	requireInt(t, "no channel", len(translateLine(t, ":op!u@h INVITE bob")), 0)
	requireInt(t, "not a channel", len(translateLine(t, ":op!u@h INVITE bob notachannel")), 0)
	requireInt(t, "no inviter", len(translateLine(t, "INVITE bob #room")), 0)
}

func TestInviteToOtherIsNotKeptForStatus(t *testing.T) {
	requireTrue(t, "to us", StatusKeepsIncoming(mustParse(t, ":rio!r@host INVITE me #debian"), "me", "#&"))
	requireTrue(t, "to us, other case", StatusKeepsIncoming(mustParse(t, ":rio!r@host INVITE ME #debian"), "me", "#&"))
	requireFalse(t, "to bob", StatusKeepsIncoming(mustParse(t, ":rio!r@host INVITE bob #debian"), "me", "#&"))
	requireTrue(t, "short line keeps old behavior", StatusKeepsIncoming(mustParse(t, "INVITE #debian"), "me", "#&"))
}

func userhostFacts(events []Event) [][2]string {
	var facts [][2]string
	for _, event := range events {
		if fact, ok := event.(NickFactsEvent); ok && fact.Userhost != nil {
			facts = append(facts, [2]string{fact.Nick, *fact.Userhost})
		}
	}
	return facts
}

func requireUserhostFacts(t *testing.T, label string, got [][2]string, want ...[2]string) {
	t.Helper()
	if len(got) != len(want) {
		t.Fatalf("%s = %v, want %v", label, got, want)
	}
	for index := range want {
		if got[index] != want[index] {
			t.Fatalf("%s = %v, want %v", label, got, want)
		}
	}
}

func TestNamesWithUserhostYieldCorrectNicksAndUserhostFacts(t *testing.T) {
	events := translateLine(t, ":server 353 omairc = #room :@+alice!a@h.example bob!b@h2 carol")
	names, ok := events[0].(NamesEvent)
	requireTrue(t, "NamesEvent first", ok)
	requireInt(t, "names", len(names.Names), 3)
	requireString(t, "alice", names.Names[0].Nick, "alice")
	requireString(t, "bob", names.Names[1].Nick, "bob")
	requireString(t, "carol", names.Names[2].Nick, "carol")
	if names.Names[0] != parsedName(t, "@+alice") {
		t.Fatalf("alice = %+v, want the ranks of @+alice", names.Names[0])
	}
	requireUserhostFacts(t, "facts", userhostFacts(events),
		[2]string{"alice", "a@h.example"}, [2]string{"bob", "b@h2"})
	requireInt(t, "plain names add no event", len(translateLine(t, ":server 353 omairc = #room :@Alice +Bob")), 1)
}

func TestJoinCarriesThePrefixUserhostAsAFact(t *testing.T) {
	events := translateLine(t, ":alice!u@h.example JOIN #room")
	_, ok := events[0].(JoinEvent)
	requireTrue(t, "join first", ok)
	requireUserhostFacts(t, "join", userhostFacts(events), [2]string{"alice", "u@h.example"})
	requireInt(t, "server prefix", len(userhostFacts(translateLine(t, ":server JOIN #room"))), 0)
}

func TestWho352SetsUserhost(t *testing.T) {
	events := translateLine(t, ":server 352 omairc #room user host.example server Alice H :0 Alice")
	requireUserhostFacts(t, "352", userhostFacts(events), [2]string{"Alice", "user@host.example"})
}

func TestChghostTranslatesToAUserhostFact(t *testing.T) {
	events := translateLine(t, ":alice!u@h CHGHOST newuser new.host")
	requireInt(t, "events", len(events), 1)
	requireUserhostFacts(t, "chghost", userhostFacts(events), [2]string{"alice", "newuser@new.host"})
	requireInt(t, "one param", len(translateLine(t, ":alice!u@h CHGHOST onlyone")), 0)
	requireInt(t, "no sender", len(translateLine(t, "CHGHOST a b")), 0)
}

// draft/multiline (#478): the session hands the translator one PRIVMSG or NOTICE
// whose body holds the "\n"s, so nothing downstream knows about batches. The Rust
// twins are in `client/src/irc/translator/tests/chat.rs`.

func assembledMultiline(t *testing.T, open, command, target string, lines ...string) Message {
	t.Helper()
	var parsed []Message
	for _, text := range lines {
		parsed = append(parsed, mustParse(t, "@batch=ml "+command+" "+target+" :"+text))
	}
	message, ok := AssembleMultiline(mustParse(t, open), parsed)
	requireTrue(t, "assembled", ok)
	return message
}

func TestAssembledMultilinePrivmsgIsOneMessageWithHardBreaks(t *testing.T) {
	message := assembledMultiline(t, "@msgid=ml1 :mira!u@h BATCH +ml draft/multiline #room",
		"PRIVMSG", "#room", "one", "two", "three")
	events := Translate(networkA, "omairc", NewServerFeatures(), message, reducerTimestamp)
	requireInt(t, "events", len(events), 1)
	event, ok := events[0].(MessageEvent)
	requireTrue(t, "message event", ok)
	requireString(t, "author", event.Author, "mira")
	requireString(t, "body", event.Body, "one\ntwo\nthree")
	requireString(t, "msgid", event.MsgID.Value, "ml1")
	requireString(t, "conversation", event.Conversation.NormalizedTarget, "#room")
}

func TestAssembledMultilineNoticeIsOneNoticeWithHardBreaks(t *testing.T) {
	message := assembledMultiline(t,
		"@msgid=ml2;+draft/channel-context=#room :bot!u@h BATCH +ml draft/multiline omairc",
		"NOTICE", "omairc", "one", "two")
	events := translateMessageJoined(message, roomMemberLookup())
	requireInt(t, "events", len(events), 1)
	notice, ok := events[0].(NoticeEvent)
	requireTrue(t, "notice event", ok)
	requireString(t, "body", notice.Body, "one\ntwo")
	requireString(t, "msgid", notice.MsgID.Value, "ml2")
}

func translateMessageJoined(message Message, joined JoinedMemberChannel) []Event {
	return Translate(networkA, "omairc", NewServerFeatures(), message, reducerTimestamp, joined)
}

func TestAssembledMultilineKeepsItsBreaksInAReplayLine(t *testing.T) {
	message := assembledMultiline(t,
		"@msgid=ml3;time=2024-03-09T16:00:00.620Z :mira!u@h BATCH +ml draft/multiline #room",
		"PRIVMSG", "#room", "one", "two")
	batch := HistoryBatch{Target: "#room", Lines: []Message{message}, Kind: HistoryChat}
	event, ok := TranslateHistory(networkA, "omairc", NewServerFeatures(), batch, reducerTimestamp)
	requireTrue(t, "history ok", ok)
	requireInt(t, "lines", len(event.Lines), 1)
	requireString(t, "body", event.Lines[0].Body, "one\ntwo")
	requireString(t, "msgid", event.Lines[0].MsgID.Value, "ml3")
}
