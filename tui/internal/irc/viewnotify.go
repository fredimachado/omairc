package irc

// MemberSurface names which member-panel refresh a view notify asks for.
// It mirrors IrcMemberSurface in src/irc/ircviewnotify.h.
type MemberSurface int

const (
	// MemberSurfaceNone asks for no member-panel change.
	MemberSurfaceNone MemberSurface = iota
	// MemberSurfaceReset asks for a full member-panel reload.
	MemberSurfaceReset
	// MemberSurfaceRow asks for one member row to repaint. Nick names it.
	MemberSurfaceRow
)

// ViewNotify tells the view which surfaces a reduced event dirtied. It mirrors
// IrcViewNotify. The zero value is "nothing changed".
type ViewNotify struct {
	Conversations bool
	Messages      bool
	Members       MemberSurface
	Nick          string
	Selection     bool
	Typing        bool
	// RearmTyping asks the view to restart its typing refresh timer, because
	// the event reset the typing facts the timer watches.
	RearmTyping bool
}

// ViewNotifyNone is the zero notify.
func ViewNotifyNone() ViewNotify { return ViewNotify{} }

// ViewNotifyResetAll reloads every surface, for a fresh welcome.
func ViewNotifyResetAll() ViewNotify {
	return ViewNotify{
		Conversations: true,
		Messages:      true,
		Members:       MemberSurfaceReset,
		Selection:     true,
		Typing:        true,
		RearmTyping:   true,
	}
}

// ViewNotifyChat reloads the conversation list, transcript, and typing.
func ViewNotifyChat() ViewNotify {
	return ViewNotify{
		Conversations: true,
		Messages:      true,
		Typing:        true,
		RearmTyping:   true,
	}
}

// ViewNotifyTopic reloads the selected conversation, not its transcript.
func ViewNotifyTopic() ViewNotify {
	return ViewNotify{
		Selection:   true,
		Typing:      true,
		RearmTyping: true,
	}
}

// ViewNotifyMemberRow repaints one member row by normalized nick.
func ViewNotifyMemberRow(normalizedNick string) ViewNotify {
	return ViewNotify{
		Members: MemberSurfaceRow,
		Nick:    normalizedNick,
	}
}

// ViewNotifyMemberReset reloads the whole member panel. Deliberately broader
// than the row that changed: self-away is network-scoped and rare, and the
// view short-circuits an unchanged list.
func ViewNotifyMemberReset() ViewNotify {
	return ViewNotify{Members: MemberSurfaceReset}
}

// ViewNotifyTypingOnly reloads just the typing indicator.
func ViewNotifyTypingOnly() ViewNotify {
	return ViewNotify{Typing: true, RearmTyping: true}
}

// ViewNotifyTranscript reloads only the transcript.
func ViewNotifyTranscript() ViewNotify {
	return ViewNotify{Messages: true}
}

// ViewNotifyMembership reloads the conversation list, transcript, members,
// selection, and typing: a membership change touches all of them.
func ViewNotifyMembership() ViewNotify {
	return ViewNotify{
		Conversations: true,
		Messages:      true,
		Members:       MemberSurfaceReset,
		Selection:     true,
		Typing:        true,
	}
}

// ClassifyViewNotify maps one event to the surfaces it dirties, using the
// reducer for the presence and metadata facts the mapping depends on. It
// mirrors classifyViewNotify in ircviewnotify.h. selected is the currently
// selected conversation, or nil.
func ClassifyViewNotify(event Event, reducer *EventReducer, selected *ConversationKey) ViewNotify {
	notify := classifyViewNotifyUnchecked(event, reducer)
	if !ChannelNamesSyncing(reducer, selected) {
		return notify
	}
	notify.Members = MemberSurfaceNone
	notify.Selection = false
	notify.Nick = ""
	return notify
}

func classifyViewNotifyUnchecked(event Event, reducer *EventReducer) ViewNotify {
	switch e := event.(type) {
	case WelcomeEvent:
		return ViewNotifyResetAll()
	case MessageEvent, NoticeEvent, ActionEvent:
		return ViewNotifyChat()
	case JoinEvent, PartEvent, QuitEvent, NickEvent, KickEvent, ModeEvent:
		return ViewNotifyMembership()
	case TopicEvent:
		return ViewNotifyTopic()
	case NamesEvent:
		if !e.Complete {
			return ViewNotifyNone()
		}
		notify := ViewNotifyResetAll()
		notify.RearmTyping = false
		return notify
	case AwayEvent:
		key := reducer.ConversationKey(e.NetworkID, e.Nick)
		notify := ViewNotifyMemberRow(key.NormalizedTarget)
		// A direct message row paints this peer's presence from the same
		// facts, so the sidebar has to repaint when their away state lands.
		// Skip that reload when nothing can change: the peer has no direct
		// row and no shared channel for PeerPresence to answer from.
		conversation := reducer.Find(key)
		notify.Conversations = (conversation != nil && !conversation.IsChannel()) ||
			reducer.PeerPresence(e.NetworkID, key.NormalizedTarget) != PeerUnknown
		return notify
	case AccountEvent:
		key := reducer.ConversationKey(e.NetworkID, e.Nick)
		notify := ViewNotifyMemberRow(key.NormalizedTarget)
		// Transcript headers read the account live, so a logout has to
		// repaint existing rows. The sidebar conversation list does not.
		notify.Messages = true
		return notify
	case MemberMetadataEvent:
		key := reducer.ConversationKey(e.NetworkID, e.Nick)
		notify := ViewNotifyMemberRow(key.NormalizedTarget)
		canonical := CanonicalKey(e.Key)
		affectsTranscriptChrome := canonical == AvatarKey() || canonical == BotKey()
		if affectsTranscriptChrome {
			notify.Messages = true
		}
		conversation := reducer.Find(key)
		if affectsTranscriptChrome && conversation != nil && !conversation.IsChannel() {
			notify.Conversations = true
		}
		return notify
	case SelfAwayEvent:
		// Our own away state is overlaid on our member row, so the visible
		// channel's member list has to repaint when it changes.
		return ViewNotifyMemberReset()
	case TypingEvent:
		return ViewNotifyTypingOnly()
	case HistoryEvent:
		// A query replay batch can open the direct message it belongs to, and
		// a transcript-only refresh would leave that conversation out of the
		// sidebar.
		notify := ViewNotifyTranscript()
		notify.Conversations = true
		return notify
	case WhoisTranscriptEvent, ChannelErrorEvent:
		return ViewNotifyTranscript()
	}
	return ViewNotifyNone()
}

// ChannelNamesSyncing reports whether the selected conversation is a channel
// whose NAMES burst is still syncing, so member refreshes stay suppressed.
// It mirrors channelNamesSyncing in ircviewnotify.h.
func ChannelNamesSyncing(reducer *EventReducer, key *ConversationKey) bool {
	if key == nil {
		return false
	}
	conversation := reducer.Find(*key)
	if conversation == nil {
		return false
	}
	channel := conversation.Channel()
	return channel != nil && channel.NamesSyncing
}
