package irc

import (
	"strings"
	"time"
)

// HistoryBatch is one completed replay batch. The lines are still raw protocol
// so the translator can reuse the same rules it applies to live traffic. It
// mirrors IrcHistoryBatch.
type HistoryBatch struct {
	Target string
	Lines  []Message
	Kind   HistoryKind
}

// ConversationFor resolves the conversation an inbound message belongs to, or
// ok=false when the message has none. It mirrors ircConversationFor.
func ConversationFor(networkID, target string, message Message, currentNick string, features ServerFeatures) (ConversationKey, bool) {
	// A bouncer addresses its playback markers to the channel, so the sender
	// is read before the target is classified.
	if message.Prefix != nil && message.Prefix.Nick != "" && !NickIsRoutable(message.Prefix.Nick) {
		return ConversationKey{}, false
	}
	if features.IsChannel(target) {
		return ircKey(networkID, target, features), true
	}
	if ircIsNetworkNoticeTarget(target) || !ircHasUserPrefix(message) || ircIsServiceUser(message, features) {
		return ConversationKey{}, false
	}
	sender := ircAuthor(message)
	if sender == "" {
		return ConversationKey{}, false
	}
	if ircSame(features, target, currentNick) {
		return ircKey(networkID, sender, features), true
	}
	if ircSame(features, sender, currentNick) {
		return ircKey(networkID, target, features), true
	}
	return ConversationKey{}, false
}

// Translate turns one inbound IRC message into the events the reducer
// consumes. now is the wall-clock fallback when the IRCv3 time tag is missing
// or unparseable; it replaces QDateTime::currentDateTimeUtc(). It mirrors
// IrcEventTranslator::translate.
func Translate(networkID, currentNick string, features ServerFeatures, message Message, now time.Time) []Event {
	var events []Event
	command := uppercaseASCII(message.Command)
	sender := ircAuthor(message)
	timestamp := timestampFor(message, now)

	// The account tag names the sender. Record it before command handling so
	// NOTICE, which produces no transcript event, still updates the nick.
	if account, present := ircPresentTagValue(message, "account"); present {
		if ircHasUserPrefix(message) && sender != "" {
			events = append(events, AccountEvent{NetworkID: networkID, Nick: sender, Account: account})
		}
	}

	if command == "NOTICE" {
		return events
	}

	switch {
	case command == "PRIVMSG" && len(message.Params) >= 2:
		wireTarget := ircParameter(message, 0)
		conversation, ok := ConversationFor(networkID, wireTarget, message, currentNick, features)
		if !ok {
			return events
		}
		body := ircParameter(message, 1)
		if ctcp, ok := ParseCtcpRequest(body); ok && ctcp.Command != "ACTION" {
			return events
		}
		displayTarget := wireTarget
		if !features.IsChannel(wireTarget) && !ircSame(features, sender, currentNick) {
			displayTarget = sender
		}
		msgid := MsgID{Value: ircTagValueOrEmpty(message, "msgid")}
		const actionPrefix = "\x01ACTION "
		if strings.HasPrefix(body, actionPrefix) && strings.HasSuffix(body, "\x01") {
			events = append(events, ActionEvent{
				Conversation: conversation,
				Author:       sender,
				Body:         body[len(actionPrefix) : len(body)-1],
				Timestamp:    timestamp,
				Target:       displayTarget,
				MsgID:        msgid,
			})
		} else {
			events = append(events, MessageEvent{
				Conversation: conversation,
				Author:       sender,
				Body:         body,
				Timestamp:    timestamp,
				Target:       displayTarget,
				MsgID:        msgid,
			})
		}
	case command == "TAGMSG" && len(message.Params) != 0:
		value, ok := ircTagValue(message, "+typing")
		if !ok || sender == "" {
			return events
		}
		phase, ok := TypingPhaseFromTag(value)
		if !ok {
			return events
		}
		conversation, ok := ConversationFor(networkID, ircParameter(message, 0), message, currentNick, features)
		if !ok {
			return events
		}
		events = append(events, TypingEvent{
			Conversation: conversation,
			Nick:         sender,
			Phase:        phase,
			ReceivedAt:   timestamp,
		})
	case command == "JOIN" && len(message.Params) != 0:
		var account *string
		if len(message.Params) >= 2 {
			value := ircParameter(message, 1)
			account = &value
		}
		events = append(events, JoinEvent{
			NetworkID: networkID,
			Channel:   ircParameter(message, 0),
			Nick:      sender,
			Account:   account,
		})
	case command == "ACCOUNT" && sender != "":
		account := ""
		if len(message.Params) != 0 {
			account = ircParameter(message, 0)
		}
		events = append(events, AccountEvent{NetworkID: networkID, Nick: sender, Account: account})
	case command == "330" && len(message.Params) >= 3:
		nick := ircParameter(message, 1)
		if nick != "" {
			events = append(events, AccountEvent{NetworkID: networkID, Nick: nick, Account: ircParameter(message, 2)})
		}
	case command == "PART" && len(message.Params) != 0:
		events = append(events, PartEvent{
			NetworkID: networkID,
			Channel:   ircParameter(message, 0),
			Nick:      sender,
			Reason:    ircParameter(message, 1),
		})
	case command == "QUIT":
		events = append(events, QuitEvent{NetworkID: networkID, Nick: sender, Reason: ircParameter(message, 0)})
	case command == "NICK" && len(message.Params) != 0:
		events = append(events, NickEvent{NetworkID: networkID, OldNick: sender, NewNick: ircParameter(message, 0)})
	case command == "KICK" && len(message.Params) >= 2:
		events = append(events, KickEvent{
			NetworkID: networkID,
			Channel:   ircParameter(message, 0),
			Target:    ircParameter(message, 1),
			Author:    sender,
			Reason:    ircParameter(message, 2),
		})
	case command == "TOPIC" && len(message.Params) >= 2:
		events = append(events, TopicEvent{
			NetworkID: networkID,
			Channel:   ircParameter(message, 0),
			Topic:     ircParameter(message, 1),
			Author:    sender,
		})
	case command == "332" && len(message.Params) >= 3:
		events = append(events, TopicEvent{
			NetworkID: networkID,
			Channel:   ircParameter(message, 1),
			Topic:     ircParameter(message, 2),
		})
	case command == "353" && len(message.Params) >= 4:
		var names []Name
		for _, token := range strings.Split(ircParameter(message, 3), " ") {
			if token == "" {
				continue
			}
			parsed, ok := features.ParseNamesToken(token)
			if !ok {
				continue
			}
			names = append(names, Name{Nick: parsed.Nick, Ranks: parsed.Ranks})
		}
		events = append(events, NamesEvent{
			NetworkID: networkID,
			Channel:   ircParameter(message, 2),
			Names:     names,
			Complete:  false,
		})
	case command == "366" && len(message.Params) >= 2:
		events = append(events, NamesEvent{
			NetworkID: networkID,
			Channel:   ircParameter(message, 1),
			Complete:  true,
		})
	case command == "MODE" && len(message.Params) >= 2:
		events = append(events, ModeEvent{
			NetworkID: networkID,
			Target:    ircParameter(message, 0),
			Author:    sender,
			Mode:      ircParameter(message, 1),
			Arguments: ircRemainingParameters(message, 2),
		})
	case command == "AWAY" && sender != "":
		var away *Away
		if len(message.Params) != 0 {
			away = &Away{Reason: ircParameter(message, 0)}
		}
		events = append(events, AwayEvent{NetworkID: networkID, Nick: sender, Away: away})
	case command == "306":
		events = append(events, SelfAwayEvent{NetworkID: networkID, Away: true})
	case command == "305":
		events = append(events, SelfAwayEvent{NetworkID: networkID, Away: false})
	case command == "352" && len(message.Params) >= 7:
		nick := ircParameter(message, 5)
		away := strings.HasPrefix(ircParameter(message, 6), "G")
		if nick != "" {
			var state *Away
			if away {
				state = &Away{}
			}
			events = append(events, AwayEvent{NetworkID: networkID, Nick: nick, Away: state})
		}
	case command == "CHGHOST":
		// Members store nick plus ranks. User and host are not modeled.
	case command == "METADATA":
		events = appendMemberMetadata(events, networkID, message, 0, features)
	case command == "761":
		events = appendMemberMetadata(events, networkID, message, 1, features)
	case command == "766":
		events = append766KeyNotSet(events, networkID, message, features)
	case (isJoinFailureNumeric(command) || isAmbiguousJoinFailureNumeric(command)) && len(message.Params) >= 3:
		channel := ircParameter(message, 1)
		if !isAmbiguousJoinFailureNumeric(command) || features.IsChannel(channel) {
			events = append(events, ChannelErrorEvent{
				NetworkID: networkID,
				Channel:   channel,
				Body:      ircParameter(message, 2),
			})
		}
	}

	return events
}

// TranslateHistory replays a batch of raw lines into one HistoryEvent. It
// mirrors IrcEventTranslator::translateHistory.
func TranslateHistory(networkID, currentNick string, features ServerFeatures, batch HistoryBatch, now time.Time) (HistoryEvent, bool) {
	if batch.Target == "" {
		return HistoryEvent{}, false
	}
	conversation := ircKey(networkID, batch.Target, features)
	// Channel batches already key off the channel. CHATHISTORY keeps the
	// live nick check; only a bouncer query needs the previous-nick rule.
	bouncerQuery := batch.Kind == HistoryBouncerPlayback && !features.IsChannel(batch.Target)
	event := HistoryEvent{Conversation: conversation, Target: batch.Target, Kind: batch.Kind}
	for _, line := range batch.Lines {
		serverTime := ircServerTimeOf(line)
		kept := false
		for _, translated := range Translate(networkID, currentNick, features, line, now) {
			switch value := translated.(type) {
			case MessageEvent:
				if value.Conversation != conversation {
					continue
				}
				event.Lines = append(event.Lines, ReplayLine{
					Author:     value.Author,
					Body:       value.Body,
					Timestamp:  value.Timestamp,
					Kind:       MessageKindChat,
					MsgID:      value.MsgID,
					ServerTime: serverTime,
				})
				kept = true
			case ActionEvent:
				if value.Conversation != conversation {
					continue
				}
				event.Lines = append(event.Lines, ReplayLine{
					Author:     value.Author,
					Body:       value.Body,
					Timestamp:  value.Timestamp,
					Kind:       MessageKindEmote,
					MsgID:      value.MsgID,
					ServerTime: serverTime,
				})
				kept = true
			}
		}
		if kept || !bouncerQuery {
			continue
		}
		if replay, ok := bouncerQueryReplayLine(batch.Target, features, line, serverTime, now); ok {
			event.Lines = append(event.Lines, replay)
		}
	}
	return event, true
}

// ircParameter returns one message parameter, or "" when the index is past the
// end. The Go Message already holds decoded UTF-8 strings, so no wire decode
// happens here where the C++ parameter() called ircWireText.
func ircParameter(message Message, index int) string {
	if index < len(message.Params) {
		return message.Params[index]
	}
	return ""
}

func ircAuthor(message Message) string {
	return PrefixNick(message)
}

func ircKey(networkID, target string, features ServerFeatures) ConversationKey {
	return ConversationKey{
		NetworkID:        networkID,
		NormalizedTarget: features.CaseMapping().Normalize(target),
	}
}

func ircSame(features ServerFeatures, left, right string) bool {
	return features.CaseMapping().Equals(left, right)
}

func ircHasUserPrefix(message Message) bool {
	return message.Prefix != nil && message.Prefix.Nick != "" && message.Prefix.User != ""
}

func ircIsNetworkNoticeTarget(target string) bool {
	return target == "" || target == "*" || strings.EqualFold(target, "AUTH")
}

func ircIsServiceUser(message Message, features ServerFeatures) bool {
	if message.Prefix == nil {
		return false
	}
	return IsServiceIdentity(message.Prefix.Nick, message.Prefix.Host, features.ChannelTypes())
}

func ircRemainingParameters(message Message, start int) []string {
	var result []string
	for index := start; index < len(message.Params); index++ {
		result = append(result, ircParameter(message, index))
	}
	return result
}

// ircTagValue returns the value of a tag that carried one. A valueless tag is
// absent, mirroring tagValue in irceventtranslator.cpp.
func ircTagValue(message Message, name string) (string, bool) {
	for _, tag := range message.Tags {
		if tag.Name == name && tag.Value != nil {
			return *tag.Value, true
		}
	}
	return "", false
}

// ircTagValueOrEmpty returns the value of a tag, or "" when it is absent or
// valueless, mirroring tagValue(...).value_or(QString{}).
func ircTagValueOrEmpty(message Message, name string) string {
	value, ok := ircTagValue(message, name)
	if !ok {
		return ""
	}
	return value
}

// ircPresentTagValue distinguishes "server omitted the tag" from "server sent
// it empty": present is true even when the value is missing or empty, while a
// missing tag stays absent so callers do not treat it as an explicit logout.
func ircPresentTagValue(message Message, name string) (string, bool) {
	for _, tag := range message.Tags {
		if tag.Name != name {
			continue
		}
		if tag.Value == nil {
			return "", true
		}
		return *tag.Value, true
	}
	return "", false
}

// ircServerTimeOf parses the IRCv3 time tag, or returns nil when it is absent,
// empty, or unparseable. The result is UTC.
func ircServerTimeOf(message Message) *time.Time {
	raw, ok := ircTagValue(message, "time")
	if !ok || raw == "" {
		return nil
	}
	parsed, err := time.Parse(time.RFC3339Nano, raw)
	if err != nil {
		parsed, err = time.Parse(time.RFC3339, raw)
		if err != nil {
			return nil
		}
	}
	utc := parsed.UTC()
	return &utc
}

// timestampFor prefers the parsed server time tag and falls back to now, the
// caller-supplied wall clock.
func timestampFor(message Message, now time.Time) time.Time {
	if serverTime := ircServerTimeOf(message); serverTime != nil {
		return *serverTime
	}
	return now
}

// appendMemberMetadata reads `<Target> <Key> <Visibility> [<Value>]` for
// METADATA and 761.
func appendMemberMetadata(events []Event, networkID string, message Message, targetIndex int, features ServerFeatures) []Event {
	if len(message.Params) < targetIndex+3 {
		return events
	}
	target := ircParameter(message, targetIndex)
	key := ircParameter(message, targetIndex+1)
	if target == "" || features.IsChannel(target) {
		return events
	}
	canonical := CanonicalKey(key)
	if canonical == "" {
		return events
	}
	value := Clamped(ircParameter(message, targetIndex+3))
	if value == "" {
		return events
	}
	return append(events, MemberMetadataEvent{
		NetworkID: networkID,
		Nick:      target,
		Key:       canonical,
		Value:     value,
	})
}

// append766KeyNotSet handles `766 RPL_KEYNOTSET`: `<Target> <Key> [:reason]`.
// It clears one known key for GET misses, successful SET unset, and CLEAR
// batches. The trailing text is never the key — replies that omit `<Key>` are
// ignored.
func append766KeyNotSet(events []Event, networkID string, message Message, features ServerFeatures) []Event {
	const targetIndex = 1
	if len(message.Params) < targetIndex+2 {
		return events
	}
	target := ircParameter(message, targetIndex)
	key := ircParameter(message, targetIndex+1)
	if target == "" || features.IsChannel(target) {
		return events
	}
	canonical := CanonicalKey(key)
	if canonical == "" {
		return events
	}
	return append(events, MemberMetadataEvent{
		NetworkID: networkID,
		Nick:      target,
		Key:       canonical,
	})
}

func isJoinFailureNumeric(command string) bool {
	switch command {
	case "403", "405", "448", "471", "473", "474", "475", "476", "477", "479", "489", "520":
		return true
	}
	return false
}

func isAmbiguousJoinFailureNumeric(command string) bool {
	switch command {
	case "437", "480", "485":
		return true
	}
	return false
}

// bouncerQueryReplayLine files a znc.in/playback query line on the batch
// conversation. A playback query buffer is named for the peer, so after a nick
// change the same buffer still has PRIVMSG to the previous nick and echoes
// from that nick. A channel target stays with its channel batch, and live
// PRIVMSG still has to name the current nick.
func bouncerQueryReplayLine(batchTarget string, features ServerFeatures, message Message, serverTime *time.Time, now time.Time) (ReplayLine, bool) {
	command := uppercaseASCII(message.Command)
	if command != "PRIVMSG" || len(message.Params) < 2 {
		return ReplayLine{}, false
	}
	if message.Prefix != nil && message.Prefix.Nick != "" && !NickIsRoutable(message.Prefix.Nick) {
		return ReplayLine{}, false
	}
	wireTarget := ircParameter(message, 0)
	if features.IsChannel(wireTarget) || ircIsNetworkNoticeTarget(wireTarget) ||
		!ircHasUserPrefix(message) || ircIsServiceUser(message, features) {
		return ReplayLine{}, false
	}
	sender := ircAuthor(message)
	if sender == "" ||
		(!ircSame(features, sender, batchTarget) && !ircSame(features, wireTarget, batchTarget)) {
		return ReplayLine{}, false
	}
	body := ircParameter(message, 1)
	if ctcp, ok := ParseCtcpRequest(body); ok && ctcp.Command != "ACTION" {
		return ReplayLine{}, false
	}
	msgid := MsgID{Value: ircTagValueOrEmpty(message, "msgid")}
	const actionPrefix = "\x01ACTION "
	if strings.HasPrefix(body, actionPrefix) && strings.HasSuffix(body, "\x01") {
		return ReplayLine{
			Author:     sender,
			Body:       body[len(actionPrefix) : len(body)-1],
			Timestamp:  timestampFor(message, now),
			Kind:       MessageKindEmote,
			MsgID:      msgid,
			ServerTime: serverTime,
		}, true
	}
	return ReplayLine{
		Author:     sender,
		Body:       body,
		Timestamp:  timestampFor(message, now),
		Kind:       MessageKindChat,
		MsgID:      msgid,
		ServerTime: serverTime,
	}, true
}
