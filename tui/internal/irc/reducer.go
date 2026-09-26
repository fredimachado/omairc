package irc

import (
	"sort"
	"strings"
	"time"
	"unicode"
)

// MessageKind classifies a transcript row. It mirrors IrcMessageKind.
type MessageKind int

const (
	// KindMessage is a normal chat line.
	KindMessage MessageKind = iota
	// KindNotice is a NOTICE line.
	KindNotice
	// KindAction is a CTCP ACTION (emote) line.
	KindAction
	// KindEvent is a join/part/quit/mode/state line, also used for errors in
	// the persisted kind token.
	KindEvent
	// KindError is a local error line.
	KindError
	// KindWhois is one formatted WHOIS transcript line.
	KindWhois
)

// Origin tells a live inbound line from replay (CHATHISTORY or bouncer
// playback). It mirrors IrcOrigin.
type Origin int

const (
	// OriginLive is an inbound line as it arrived.
	OriginLive Origin = iota
	// OriginReplay is a line spliced from a replay batch.
	OriginReplay
)

// ReducedMessage is one row in a conversation transcript. It mirrors
// IrcReducedMessage.
type ReducedMessage struct {
	Author      string
	Body        string
	Timestamp   time.Time
	Kind        MessageKind
	Collapsible bool
	Origin      Origin
	MsgID       MsgID
	Sequence    int64
}

// MemberState is one channel member as stored: the display nick and its rank
// bits. It mirrors IrcMemberState.
type MemberState struct {
	DisplayNick string
	Ranks       PrefixSet
}

// MemberView is the displayed row for one channel member, joining membership
// with the per-network presence facts. It mirrors IrcMemberView.
type MemberView struct {
	Nick        string
	Label       string
	Ranks       PrefixSet
	Away        *Away
	Status      string
	Avatar      string
	Bot         bool
	DisplayName string
	Pronouns    string
	Homepage    string
	Color       string
	// Account is empty when unknown, logged out, or the same as the nick
	// under the network case mapping.
	Account string
}

// IsAway reports whether the member is marked away.
func (v MemberView) IsAway() bool { return v.Away != nil }

// PeerPresence is what the reducer knows about a nick's availability from
// membership plus away facts. It mirrors IrcPeerPresence.
type PeerPresence int

const (
	// PeerUnknown means no shared channel proves the nick is online.
	PeerUnknown PeerPresence = iota
	// PeerOnline means the nick shares a channel and is not away.
	PeerOnline
	// PeerAway means the nick shares a channel and is away.
	PeerAway
)

// OrderedMember is one channel member in panel order: Priority is the index of
// the member's highest rank in the server's PREFIX order, so a lower value is a
// higher privilege and members without a rank come last. It mirrors
// IrcOrderedMember.
type OrderedMember struct {
	Nick     string
	Priority int
}

// TranscriptAnchor names a logical transcript position: the trimmed prefix
// plus the index of the self-join line a replay batch splices above. It mirrors
// IrcTranscriptAnchor.
type TranscriptAnchor struct {
	Sequence int64
}

// ChannelState is the channel half of a conversation. It mirrors
// IrcChannelState.
type ChannelState struct {
	Members          map[string]MemberState
	Topic            string
	Joined           bool
	NamesSyncing     bool
	NamesSyncStarted time.Time
	HistoryAnchor    *TranscriptAnchor
}

// DirectMessageState is the direct-message half of a conversation. It mirrors
// the empty IrcDirectMessageState.
type DirectMessageState struct{}

// ConversationState is one conversation transcript plus its live state. It
// mirrors IrcConversationState.
type ConversationState struct {
	Key          ConversationKey
	Target       string
	Messages     []ReducedMessage
	Typing       map[string]TypingHint
	Unread       int
	Mentions     int
	UnreadMark   *int64
	Muted        bool
	Trimmed      int
	MessageIDs   map[MsgID]struct{}
	SpliceEpoch  int
	NextSequence int64

	// channel is non-nil only for channels; a nil channel means a direct
	// message.
	channel *ChannelState
}

// IsChannel reports whether this conversation is a channel.
func (c *ConversationState) IsChannel() bool { return c.channel != nil }

// Channel returns the channel state, or nil for a direct message.
func (c *ConversationState) Channel() *ChannelState { return c.channel }

// PeopleCount returns the number of stored channel members, or 0 for a direct
// message.
func (c *ConversationState) PeopleCount() int {
	if c.channel == nil {
		return 0
	}
	return len(c.channel.Members)
}

// Store maps a conversation key to its state. It mirrors
// IrcEventReducer::Store.
type Store map[ConversationKey]*ConversationState

// StaleNamesSyncMs is how long a NAMES sync may run before it is released.
// It mirrors IrcEventReducer::kStaleNamesSyncMs.
const StaleNamesSyncMs = 30000

// MaxMessages caps one conversation transcript. It mirrors
// IrcEventReducer::kMaxMessages.
const MaxMessages = 2000

// EventReducer folds translated Events into conversation state. It mirrors
// IrcEventReducer.
type EventReducer struct {
	conversations  Store
	features       map[string]ServerFeatures
	currentNicks   map[string]string
	selfNicks      map[string]map[string]struct{}
	highlightWords map[string][]string
	presence       map[string]NetworkPresence
	selfAway       map[string]struct{}
	selected       *ConversationKey
	// windowInactive is inverted so the zero value matches the C++ member
	// initializer `bool m_windowActive = true`: a reducer is focused until
	// something says otherwise.
	windowInactive      bool
	mentionArrival      *MentionArrival
	inboxArrival        *InboxArrival
	mutedKeys           map[ConversationKey]struct{}
	log                 ConversationLog
	pendingPlayback     map[ConversationKey]HistoryEvent
	keptPlaybackChannel map[ConversationKey]struct{}
	keptReplay          []KeptReplay
	rememberedQueries   []RememberedQuery
	queryRestorePending map[string]struct{}
}

// defaultServerFeatures is returned for a network the caller never configured.
// It matches a default-constructed IrcServerFeatures.
var defaultServerFeatures = NewServerFeatures()

// NewEventReducer returns an empty reducer with the window treated as active,
// matching a default-constructed IrcEventReducer.
func NewEventReducer() *EventReducer {
	reducer := &EventReducer{}
	reducer.ensure()
	return reducer
}

// ensure initializes every map so a zero-value EventReducer is usable too.
func (r *EventReducer) ensure() {
	if r.conversations == nil {
		r.conversations = make(Store)
	}
	if r.features == nil {
		r.features = make(map[string]ServerFeatures)
	}
	if r.currentNicks == nil {
		r.currentNicks = make(map[string]string)
	}
	if r.selfNicks == nil {
		r.selfNicks = make(map[string]map[string]struct{})
	}
	if r.highlightWords == nil {
		r.highlightWords = make(map[string][]string)
	}
	if r.presence == nil {
		r.presence = make(map[string]NetworkPresence)
	}
	if r.selfAway == nil {
		r.selfAway = make(map[string]struct{})
	}
	if r.mutedKeys == nil {
		r.mutedKeys = make(map[ConversationKey]struct{})
	}
	if r.pendingPlayback == nil {
		r.pendingPlayback = make(map[ConversationKey]HistoryEvent)
	}
	if r.keptPlaybackChannel == nil {
		r.keptPlaybackChannel = make(map[ConversationKey]struct{})
	}
	if r.queryRestorePending == nil {
		r.queryRestorePending = make(map[string]struct{})
	}
}

// SetServerFeatures records the ISUPPORT feature set for one network. It
// mirrors IrcEventReducer::setServerFeatures.
func (r *EventReducer) SetServerFeatures(networkID string, features ServerFeatures) {
	r.ensure()
	r.features[networkID] = features
}

// ServerFeatures returns the feature set for one network, or the default
// feature set when the caller never set one. It mirrors
// IrcEventReducer::serverFeatures.
func (r *EventReducer) ServerFeatures(networkID string) ServerFeatures {
	if features, ok := r.features[networkID]; ok {
		return features
	}
	return defaultServerFeatures
}

// SetHighlightWords replaces the highlight words for one network. An empty
// network or an empty word list clears them. It mirrors
// IrcEventReducer::setHighlightWords.
func (r *EventReducer) SetHighlightWords(networkID string, words []string) {
	r.ensure()
	if networkID == "" {
		return
	}
	if len(words) == 0 {
		delete(r.highlightWords, networkID)
		return
	}
	copied := append([]string(nil), words...)
	r.highlightWords[networkID] = copied
}

// ConversationKey builds the key for one network and target, normalizing the
// target under the network case mapping. It mirrors
// IrcEventReducer::conversationKey.
func (r *EventReducer) ConversationKey(networkID, target string) ConversationKey {
	return ConversationKey{NetworkID: networkID, NormalizedTarget: r.normalize(networkID, target)}
}

// MarkSelected records the selected conversation and clears its unread and
// mention counts. It mirrors IrcEventReducer::markSelected.
func (r *EventReducer) MarkSelected(key ConversationKey) {
	r.ensure()
	if r.selected != nil && *r.selected == key {
		return
	}
	selected := key
	r.selected = &selected
	if conversation := r.findMutable(key); conversation != nil {
		if conversation.Unread == 0 {
			conversation.UnreadMark = nil
		}
		conversation.Unread = 0
		conversation.Mentions = 0
	}
}

// MarkRead clears the unread and mention counts, keeping the "New messages"
// boundary. It reports whether anything changed. It mirrors
// IrcEventReducer::markRead.
func (r *EventReducer) MarkRead(key ConversationKey) bool {
	conversation := r.findMutable(key)
	if conversation == nil || (conversation.Unread == 0 && conversation.Mentions == 0) {
		return false
	}
	conversation.Unread = 0
	conversation.Mentions = 0
	return true
}

// SetWindowActive records whether the window has focus. It is flag-only;
// consuming unread on focus regain is the controller's job. It mirrors
// IrcEventReducer::setWindowActive.
func (r *EventReducer) SetWindowActive(active bool) {
	r.windowInactive = !active
}

// ClearSelection forgets the selected conversation. It mirrors
// IrcEventReducer::clearSelection.
func (r *EventReducer) ClearSelection() {
	r.selected = nil
}

// Selected returns the selected conversation, if any. It mirrors
// IrcEventReducer::selected.
func (r *EventReducer) Selected() (ConversationKey, bool) {
	if r.selected == nil {
		return ConversationKey{}, false
	}
	return *r.selected, true
}

// SetConversationLog wires the transcript log used for hydration and
// persistence. A nil log disables both. It mirrors
// IrcEventReducer::setConversationLog.
func (r *EventReducer) SetConversationLog(log ConversationLog) {
	r.log = log
}

// Apply folds one translated event into the store. The now argument is the
// caller's clock, used only where the C++ core would read the wall clock. It
// mirrors IrcEventReducer::apply plus the injected clock.
func (r *EventReducer) Apply(event Event, now time.Time) {
	r.ensure()
	r.mentionArrival = nil
	r.inboxArrival = nil
	event = dereferenceEvent(event)
	switch value := event.(type) {
	case WelcomeEvent:
		r.reduceWelcome(value)
	case MessageEvent:
		r.reduceMessage(value)
	case NoticeEvent:
		r.reduceNotice(value)
	case ActionEvent:
		r.reduceAction(value)
	case JoinEvent:
		r.reduceJoin(value)
	case PartEvent:
		r.reducePart(value)
	case QuitEvent:
		r.reduceQuit(value)
	case NickEvent:
		r.reduceNick(value)
	case KickEvent:
		r.reduceKick(value)
	case TopicEvent:
		r.reduceTopic(value)
	case NamesEvent:
		r.reduceNames(value, now)
	case ModeEvent:
		r.reduceMode(value)
	case AwayEvent:
		r.reduceAway(value)
	case SelfAwayEvent:
		r.reduceSelfAway(value)
	case MemberMetadataEvent:
		r.reduceMemberMetadata(value)
	case AccountEvent:
		r.reduceAccount(value)
	case TypingEvent:
		r.reduceTyping(value)
	case HistoryEvent:
		r.reduceHistory(value)
	case WhoisTranscriptEvent:
		r.reduceWhoisTranscript(value)
	case ChannelErrorEvent:
		r.reduceChannelError(value)
	}
}

// TakeKeptReplay returns the replay lines kept since the last call and resets
// the pending list. It mirrors IrcEventReducer::takeKeptReplay.
func (r *EventReducer) TakeKeptReplay() []KeptReplay {
	kept := r.keptReplay
	r.keptReplay = nil
	return kept
}

// TakeRememberedQueries returns the bouncer queries worth remembering since the
// last call and resets the pending list. It mirrors
// IrcEventReducer::takeRememberedQueries.
func (r *EventReducer) TakeRememberedQueries() []RememberedQuery {
	remembered := r.rememberedQueries
	r.rememberedQueries = nil
	return remembered
}

// ReleasePendingQueryPlayback marks MOTD-end restore finished for a network and
// splices held query batches into directs that now exist. It reports whether
// anything changed. It mirrors
// IrcEventReducer::releasePendingQueryPlayback.
func (r *EventReducer) ReleasePendingQueryPlayback(networkID string) bool {
	if networkID == "" {
		return false
	}
	delete(r.queryRestorePending, networkID)
	var keys []ConversationKey
	for key := range r.pendingPlayback {
		if key.NetworkID != networkID {
			continue
		}
		if featuresIsChannel(r.ServerFeatures(networkID), key.NormalizedTarget) {
			continue
		}
		keys = append(keys, key)
	}
	sort.Slice(keys, func(i, j int) bool { return conversationKeyLess(keys[i], keys[j]) })

	changed := false
	for _, key := range keys {
		event, ok := r.pendingPlayback[key]
		if !ok {
			continue
		}
		delete(r.pendingPlayback, key)
		conversation := r.findMutable(event.Conversation)
		if conversation == nil {
			// Open only when a peer wrote. A self-only batch with no
			// conversation is finished and must not move the clock.
			if !r.replayFromPeer(event) {
				continue
			}
			if r.replayOnlyRepeatsPersistedIDs(event) {
				continue
			}
			conversation = r.EnsureConversation(event.Conversation, event.Target, CauseInboundOther)
			if conversation == nil {
				continue
			}
		}
		r.spliceHistory(conversation, event, historyAnchorConsume)
		changed = true
	}
	return changed
}

// PlaybackBatchKept reports whether a bouncer playback batch for this channel
// was spliced or deduped on the current connection. It mirrors
// IrcEventReducer::playbackBatchKept.
func (r *EventReducer) PlaybackBatchKept(networkID, target string) bool {
	if networkID == "" || target == "" {
		return false
	}
	_, ok := r.keptPlaybackChannel[r.ConversationKey(networkID, target)]
	return ok
}

// ReleaseStaleNamesSync stops a NAMES sync that has run past
// StaleNamesSyncMs. It reports whether a sync was released. It mirrors
// IrcEventReducer::releaseStaleNamesSync.
func (r *EventReducer) ReleaseStaleNamesSync(key *ConversationKey, now time.Time) bool {
	if key == nil {
		return false
	}
	conversation := r.findMutable(*key)
	if conversation == nil || conversation.channel == nil || !conversation.channel.NamesSyncing {
		return false
	}
	if now.Sub(conversation.channel.NamesSyncStarted) < time.Duration(StaleNamesSyncMs)*time.Millisecond {
		return false
	}
	stopNamesSync(conversation.channel)
	return true
}

// DropDirectMessage removes a direct message conversation. It reports whether
// one was removed. It mirrors IrcEventReducer::dropDirectMessage.
func (r *EventReducer) DropDirectMessage(key ConversationKey) bool {
	conversation := r.findMutable(key)
	if conversation == nil || conversation.IsChannel() {
		return false
	}
	if r.selected != nil && *r.selected == key {
		r.selected = nil
	}
	delete(r.conversations, key)
	return true
}

// DropChannel removes a channel conversation. It reports whether one was
// removed. It mirrors IrcEventReducer::dropChannel.
func (r *EventReducer) DropChannel(key ConversationKey) bool {
	conversation := r.findMutable(key)
	if conversation == nil || !conversation.IsChannel() {
		return false
	}
	if r.selected != nil && *r.selected == key {
		r.selected = nil
	}
	delete(r.conversations, key)
	return true
}

// ForgetNetwork drops every fact about one network: conversations, features,
// nicks, presence, mutes, and held playback. It mirrors
// IrcEventReducer::forgetNetwork.
func (r *EventReducer) ForgetNetwork(networkID string) {
	r.ensure()
	if networkID == "" {
		return
	}
	r.dropPendingPlayback(networkID)
	r.dropKeptPlayback(networkID)
	delete(r.queryRestorePending, networkID)
	for key := range r.conversations {
		if key.NetworkID == networkID {
			delete(r.conversations, key)
		}
	}
	delete(r.features, networkID)
	delete(r.currentNicks, networkID)
	delete(r.selfNicks, networkID)
	delete(r.highlightWords, networkID)
	delete(r.presence, networkID)
	delete(r.selfAway, networkID)
	for key := range r.mutedKeys {
		if key.NetworkID == networkID {
			delete(r.mutedKeys, key)
		}
	}
	if r.selected != nil && r.selected.NetworkID == networkID {
		r.selected = nil
	}
}

// ClearMessages empties a transcript while keeping its msgid set, so a bouncer
// replay at the same millisecond cannot put a line back. It advances the splice
// epoch and drops any history anchor. It mirrors IrcEventReducer::clearMessages.
func (r *EventReducer) ClearMessages(key ConversationKey) {
	conversation := r.findMutable(key)
	if conversation == nil {
		return
	}
	conversation.Trimmed += len(conversation.Messages)
	conversation.Messages = nil
	conversation.SpliceEpoch++
	if conversation.channel != nil {
		conversation.channel.HistoryAnchor = nil
	}
}

// SetMuted records the muted flag for a conversation, clearing mentions when it
// becomes muted. It mirrors IrcEventReducer::setMuted.
func (r *EventReducer) SetMuted(key ConversationKey, muted bool) {
	r.ensure()
	if key.NetworkID == "" || key.NormalizedTarget == "" {
		return
	}
	if muted {
		r.mutedKeys[key] = struct{}{}
	} else {
		delete(r.mutedKeys, key)
	}
	if conversation := r.findMutable(key); conversation != nil {
		conversation.Muted = muted
		if muted {
			conversation.Mentions = 0
		}
	}
}

// EnsureConversation returns the conversation for key, creating it when the
// cause permits. An existing conversation always wins, before the cause is
// consulted, so CauseQuietSend and CauseInboundSelf can never invent one. It
// mirrors IrcEventReducer::ensureConversation.
func (r *EventReducer) EnsureConversation(key ConversationKey, displayTarget string, cause ConversationCause) *ConversationState {
	r.ensure()
	if found := r.findMutable(key); found != nil {
		return found
	}
	features := r.ServerFeatures(key.NetworkID)
	targetIsChannel := features.IsChannel(key.NormalizedTarget)
	targetLooksLikeService := TargetLooksLikeService(displayTarget, features)
	if !ConversationCauseInserts(cause, targetIsChannel, targetLooksLikeService) {
		return nil
	}
	conversation := &ConversationState{
		Key:        key,
		Target:     displayTarget,
		Typing:     make(map[string]TypingHint),
		MessageIDs: make(map[MsgID]struct{}),
	}
	_, conversation.Muted = r.mutedKeys[key]
	if targetIsChannel {
		conversation.channel = &ChannelState{Members: make(map[string]MemberState)}
	}
	r.conversations[key] = conversation
	r.hydrateFromLog(conversation)
	return conversation
}

// Conversations returns the conversation store. It mirrors
// IrcEventReducer::conversations.
func (r *EventReducer) Conversations() Store {
	return r.conversations
}

// Find returns a conversation, or nil when absent. It mirrors
// IrcEventReducer::find.
func (r *EventReducer) Find(key ConversationKey) *ConversationState {
	return r.findMutable(key)
}

// MemberView joins a channel member with the network presence facts, or reports
// false when the key is not a channel or the nick is not a member. It mirrors
// IrcEventReducer::memberView.
func (r *EventReducer) MemberView(key ConversationKey, normalizedNick string) (MemberView, bool) {
	conversation := r.Find(key)
	var channel *ChannelState
	if conversation != nil {
		channel = conversation.channel
	}
	if channel == nil {
		return MemberView{}, false
	}
	member, ok := channel.Members[normalizedNick]
	if !ok {
		return MemberView{}, false
	}

	facts := r.presenceOf(key.NetworkID, normalizedNick)
	features := r.ServerFeatures(key.NetworkID)
	// Self-away arrives as a network-level numeric and a server need not echo
	// our own AWAY back through away-notify. Overlay it on our own row so the
	// identity footer and the member list agree.
	away := facts.Away
	if away == nil && r.SelfAway(key.NetworkID) && r.isSelf(key.NetworkID, member.DisplayNick) {
		away = &Away{}
	}
	return MemberView{
		Nick:        member.DisplayNick,
		Label:       WireText([]byte(features.MemberLabel(member.Ranks, member.DisplayNick))),
		Ranks:       member.Ranks,
		Away:        away,
		Status:      facts.Status(),
		Avatar:      facts.Avatar(),
		Bot:         facts.IsBot(),
		DisplayName: facts.Metadata(DisplayNameKey()),
		Pronouns:    facts.Metadata(PronounsKey()),
		Homepage:    facts.Metadata(HomepageKey()),
		Color:       facts.Metadata(ColorKey()),
		Account:     r.DisplayAccount(key.NetworkID, member.DisplayNick),
	}, true
}

// NickPresence returns the stored presence for a nick, normalizing it first.
// It mirrors IrcEventReducer::nickPresence.
func (r *EventReducer) NickPresence(networkID, nick string) NickPresence {
	if networkID == "" || nick == "" {
		return NickPresence{}
	}
	return r.presenceOf(networkID, r.normalize(networkID, nick))
}

// DisplayAccount returns the stored account only when it adds information: it
// is empty when unknown, logged out, or the same as the nick under the network
// case mapping. It mirrors IrcEventReducer::displayAccount.
func (r *EventReducer) DisplayAccount(networkID, nick string) string {
	account := r.NickPresence(networkID, nick).Account
	if account == "" || nick == "" {
		return ""
	}
	if featuresCaseMapping(r.ServerFeatures(networkID)).Equals(account, nick) {
		return ""
	}
	return account
}

// PeerPresence reports what the network proves about a nick: Unknown without a
// shared channel, else Away or Online. It mirrors
// IrcEventReducer::peerPresence.
func (r *EventReducer) PeerPresence(networkID, normalizedNick string) PeerPresence {
	if normalizedNick == "" || !r.isVisible(networkID, normalizedNick) {
		return PeerUnknown
	}
	presence, ok := r.presence[networkID]
	if !ok {
		return PeerOnline
	}
	if presence.Lookup(normalizedNick).Away != nil {
		return PeerAway
	}
	return PeerOnline
}

// OrderedMembers returns the channel members sorted by PREFIX rank priority,
// then by normalized nick. It mirrors IrcEventReducer::orderedMembers.
func (r *EventReducer) OrderedMembers(key ConversationKey) []OrderedMember {
	conversation := r.Find(key)
	if conversation == nil || conversation.channel == nil {
		return nil
	}
	features := r.ServerFeatures(key.NetworkID)
	ordered := make([]OrderedMember, 0, len(conversation.channel.Members))
	for nick, member := range conversation.channel.Members {
		ordered = append(ordered, OrderedMember{Nick: nick, Priority: features.RankPriority(member.Ranks)})
	}
	sort.Slice(ordered, func(i, j int) bool {
		if ordered[i].Priority != ordered[j].Priority {
			return ordered[i].Priority < ordered[j].Priority
		}
		return ordered[i].Nick < ordered[j].Nick
	})
	return ordered
}

// Mentions reports whether a body names the current nick or a highlight word.
// It mirrors IrcEventReducer::mentions.
func (r *EventReducer) Mentions(networkID, body string) bool {
	return r.isMention(networkID, body)
}

// IsTranscriptHighlight reports whether a transcript row should be washed:
// another author's Message/Action whose body is a nick or highlight hit. It
// does not tag every direct message. It mirrors
// IrcEventReducer::isTranscriptHighlight.
func (r *EventReducer) IsTranscriptHighlight(networkID, author string, kind MessageKind, body string) bool {
	if kind != KindMessage && kind != KindAction {
		return false
	}
	if r.isSelf(networkID, author) {
		return false
	}
	return r.Mentions(networkID, body)
}

// TypingNicks returns the display nicks whose typing indicator should paint,
// ordered by normalized nick like the C++ std::map. It mirrors
// IrcEventReducer::typingNicks.
func (r *EventReducer) TypingNicks(key ConversationKey, now time.Time) []string {
	conversation := r.Find(key)
	if conversation == nil {
		return nil
	}
	normalized := make([]string, 0, len(conversation.Typing))
	for nick := range conversation.Typing {
		normalized = append(normalized, nick)
	}
	sort.Strings(normalized)
	var nicks []string
	for _, nick := range normalized {
		hint := conversation.Typing[nick]
		if r.isSelf(key.NetworkID, hint.DisplayNick) {
			continue
		}
		if !TypingShowsIndicator(hint, now) {
			continue
		}
		nicks = append(nicks, hint.DisplayNick)
	}
	return nicks
}

// DirectPeerIsTyping reports whether the direct message peer is typing. It
// mirrors IrcEventReducer::directPeerIsTyping.
func (r *EventReducer) DirectPeerIsTyping(key ConversationKey, now time.Time) bool {
	conversation := r.Find(key)
	if conversation == nil || conversation.IsChannel() {
		return false
	}
	hint, ok := conversation.Typing[key.NormalizedTarget]
	if !ok {
		return false
	}
	return TypingShowsIndicator(hint, now)
}

// ClearTypingFacts drops every typing hint on one network. It mirrors
// IrcEventReducer::clearTypingFacts.
func (r *EventReducer) ClearTypingFacts(networkID string) {
	for _, conversation := range r.conversations {
		if conversation.Key.NetworkID == networkID {
			conversation.Typing = make(map[string]TypingHint)
		}
	}
}

// ClearPresenceFacts drops away and/or metadata facts for every nick on one
// network. It mirrors IrcEventReducer::clearPresenceFacts.
func (r *EventReducer) ClearPresenceFacts(networkID string, away, metadata bool) {
	presence, ok := r.presence[networkID]
	if !ok {
		return
	}
	if away {
		presence.ClearAway()
	}
	if metadata {
		presence.ClearMetadata()
	}
	r.presence[networkID] = presence
}

// SelfAway reports whether the network-level self-away flag is set. It mirrors
// IrcEventReducer::selfAway.
func (r *EventReducer) SelfAway(networkID string) bool {
	_, ok := r.selfAway[networkID]
	return ok
}

// TakeMentionArrival returns the mention planted by the most recent Apply and
// resets it. ok is false when there is none. It mirrors
// IrcEventReducer::takeMentionArrival.
func (r *EventReducer) TakeMentionArrival() (MentionArrival, bool) {
	if r.mentionArrival == nil {
		return MentionArrival{}, false
	}
	arrival := *r.mentionArrival
	r.mentionArrival = nil
	return arrival, true
}

// TakeInboxArrival returns the inbox item planted by the most recent Apply and
// resets it. ok is false when there is none. It mirrors
// IrcEventReducer::takeInboxArrival.
func (r *EventReducer) TakeInboxArrival() (InboxArrival, bool) {
	if r.inboxArrival == nil {
		return InboxArrival{}, false
	}
	arrival := *r.inboxArrival
	r.inboxArrival = nil
	return arrival, true
}

// findMutable returns a conversation, or nil when absent. It mirrors
// IrcEventReducer::findMutable.
func (r *EventReducer) findMutable(key ConversationKey) *ConversationState {
	return r.conversations[key]
}

// normalize case-maps an identifier under a network's mapping and decodes it as
// wire text. It mirrors IrcEventReducer::normalize.
func (r *EventReducer) normalize(networkID, identifier string) string {
	return WireText([]byte(featuresCaseMapping(r.ServerFeatures(networkID)).Normalize(identifier)))
}

// equals compares two identifiers under a network's case mapping. It mirrors
// IrcEventReducer::equals.
func (r *EventReducer) equals(networkID, left, right string) bool {
	return featuresCaseMapping(r.ServerFeatures(networkID)).Equals(left, right)
}

// isSelf reports whether a nick is the current nick on a network. It mirrors
// IrcEventReducer::isSelf.
func (r *EventReducer) isSelf(networkID, nick string) bool {
	current, ok := r.currentNicks[networkID]
	return ok && r.equals(networkID, current, nick)
}

// isMention reports whether a body is a nick or highlight hit. It mirrors
// IrcEventReducer::isMention.
func (r *EventReducer) isMention(networkID, body string) bool {
	return r.isNickMention(networkID, body) || r.isHighlightHit(networkID, body)
}

// isNickMention reports whether a body contains the current nick as a word. It
// mirrors IrcEventReducer::isNickMention.
func (r *EventReducer) isNickMention(networkID, body string) bool {
	normalizedBody := r.normalize(networkID, body)
	current, ok := r.currentNicks[networkID]
	if !ok {
		return false
	}
	return containsWord(normalizedBody, r.normalize(networkID, current))
}

// isHighlightHit reports whether a body contains a highlight word as a word. It
// mirrors IrcEventReducer::isHighlightHit.
func (r *EventReducer) isHighlightHit(networkID, body string) bool {
	normalizedBody := r.normalize(networkID, body)
	words, ok := r.highlightWords[networkID]
	if !ok {
		return false
	}
	for _, word := range words {
		if containsWord(normalizedBody, r.normalize(networkID, word)) {
			return true
		}
	}
	return false
}

// featuresIsChannel adapts IsChannel, which has a pointer receiver, so it can
// be called on the value ServerFeatures returns.
func featuresIsChannel(features ServerFeatures, target string) bool {
	return features.IsChannel(target)
}

// featuresCaseMapping adapts CaseMapping, which has a pointer receiver, so it
// can be called on the value ServerFeatures returns.
func featuresCaseMapping(features ServerFeatures) CaseMapping {
	return features.CaseMapping()
}

// presenceOf returns the stored presence for a normalized nick.
func (r *EventReducer) presenceOf(networkID, normalizedNick string) NickPresence {
	presence, ok := r.presence[networkID]
	if !ok {
		return NickPresence{}
	}
	return presence.Lookup(normalizedNick)
}

// appendChat admits one live chat line, dedupes by msgid, persists it, caps the
// transcript, clears the author's typing hint, and notes the arrival. It
// mirrors IrcEventReducer::appendChat.
func (r *EventReducer) appendChat(key ConversationKey, displayTarget, author, body string, timestamp time.Time, kind MessageKind, msgid MsgID) {
	self := r.isSelf(key.NetworkID, author)
	cause := CauseInboundOther
	if self {
		cause = CauseInboundSelf
	}
	conversation := r.EnsureConversation(key, displayTarget, cause)
	if conversation == nil {
		return
	}
	if !msgid.IsEmpty() {
		if _, exists := conversation.MessageIDs[msgid]; exists {
			return
		}
		conversation.MessageIDs[msgid] = struct{}{}
	}
	admitMessage(conversation, ReducedMessage{
		Author:    author,
		Body:      body,
		Timestamp: timestamp,
		Kind:      kind,
		Origin:    OriginLive,
		MsgID:     msgid,
	})
	last := conversation.Messages[len(conversation.Messages)-1]
	r.persistMessage(conversation, last)
	r.capMessages(conversation)
	r.clearTyping(conversation, r.normalize(key.NetworkID, author))
	r.noteChatArrival(conversation, key, author, body, kind, msgid, last.Sequence, OriginLive, nil)
}

// noteChatArrival plants the mention/inbox arrivals and advances the unread
// accounting for one admitted chat line. It mirrors
// IrcEventReducer::noteChatArrival.
func (r *EventReducer) noteChatArrival(conversation *ConversationState, key ConversationKey, author, body string, kind MessageKind, msgid MsgID, sequence int64, origin Origin, history *HistoryEvent) {
	// A previous nick inside a bouncer query is our own backlog, as is
	// channel playback from a nick this network has welcomed or changed away
	// from. Live PRIVMSG still uses only the current nick.
	self := r.isSelf(key.NetworkID, author) ||
		(history != nil && (r.bouncerQueryOwnLine(*history, author) || r.bouncerChannelOwnLine(*history, author)))
	nickHit := r.isNickMention(key.NetworkID, body)
	highlightHit := !nickHit && r.isHighlightHit(key.NetworkID, body)
	reason, hasReason := classifyChatLine(conversation, kind, self, nickHit, highlightHit)
	if hasReason && !conversation.Muted {
		r.mentionArrival = &MentionArrival{
			Author:    author,
			Body:      body,
			NetworkID: key.NetworkID,
			Target:    conversation.Target,
			MsgID:     msgid,
		}
	}
	selected := r.selected != nil && *r.selected == key
	// Inbox is the waiting list for conversations the user is not looking
	// at. Selection still skips it while the window is unfocused. Direct
	// messages never enter the inbox.
	if origin == OriginLive && hasReason && !conversation.Muted &&
		!selected && reason != chatLineDirect {
		r.inboxArrival = &InboxArrival{
			Kind:      inboxKindFor(reason),
			Actor:     author,
			Body:      body,
			NetworkID: key.NetworkID,
			Target:    conversation.Target,
			MsgID:     msgid,
		}
	}
	if self {
		return
	}
	// Selected live chat is unread only while unfocused. Replay while
	// unfocused is backlog, not "new since you left", so it must not plant
	// the mark mid-splice. Focused replay still plants the mark without
	// bumping unread.
	skipLiveFocused := selected && origin == OriginLive && !r.windowInactive
	skipReplayUnfocused := selected && origin != OriginLive && r.windowInactive
	if skipLiveFocused || skipReplayUnfocused {
		return
	}
	holdFocusedReplayMark := selected && origin != OriginLive && conversation.UnreadMark != nil
	if conversation.Unread == 0 && !holdFocusedReplayMark {
		mark := sequence
		conversation.UnreadMark = &mark
	}
	if selected && origin != OriginLive {
		return
	}
	conversation.Unread++
	if hasReason && (reason == chatLineNickMention || reason == chatLineHighlight) && !conversation.Muted {
		conversation.Mentions++
	}
}

// appendEvent adds one event line, collapsing it into a trailing collapsible
// event row when possible. It persists the row and caps the transcript. It
// mirrors IrcEventReducer::appendEvent.
func (r *EventReducer) appendEvent(conversation *ConversationState, body string, collapsible bool) {
	if collapsible && len(conversation.Messages) > 0 {
		last := &conversation.Messages[len(conversation.Messages)-1]
		if last.Kind == KindEvent && last.Collapsible {
			last.Body = collapseEventBody(last.Body, body)
			return
		}
	}
	admitMessage(conversation, ReducedMessage{
		Body:        body,
		Kind:        KindEvent,
		Collapsible: collapsible,
		Origin:      OriginLive,
	})
	r.persistMessage(conversation, conversation.Messages[len(conversation.Messages)-1])
	r.capMessages(conversation)
}

// appendWhois adds one WHOIS transcript row. It mirrors
// IrcEventReducer::appendWhois.
func (r *EventReducer) appendWhois(conversation *ConversationState, body string) {
	admitMessage(conversation, ReducedMessage{
		Body:   body,
		Kind:   KindWhois,
		Origin: OriginLive,
	})
	r.capMessages(conversation)
}

// appendError adds one error row. It mirrors IrcEventReducer::appendError.
func (r *EventReducer) appendError(conversation *ConversationState, body string) {
	admitMessage(conversation, ReducedMessage{
		Body:   body,
		Kind:   KindError,
		Origin: OriginLive,
	})
	r.capMessages(conversation)
}

// hydrateFromLog loads the stored transcript tail into a new conversation. It
// is a no-op without a log. It mirrors IrcEventReducer::hydrateFromLog.
func (r *EventReducer) hydrateFromLog(conversation *ConversationState) {
	if r.log == nil {
		return
	}
	lines := r.log.ReadTail(
		conversation.Key.NetworkID,
		conversation.Key.NormalizedTarget,
		featuresCaseMapping(r.ServerFeatures(conversation.Key.NetworkID)),
		MaxMessages,
	)
	for _, line := range lines {
		kind, ok := kindFromToken(line.Kind)
		if !ok {
			continue
		}
		msgid := MsgID{Value: line.MsgID}
		if !msgid.IsEmpty() {
			if _, exists := conversation.MessageIDs[msgid]; exists {
				continue
			}
			conversation.MessageIDs[msgid] = struct{}{}
		}
		admitMessage(conversation, ReducedMessage{
			Author:    line.Author,
			Body:      line.Body,
			Timestamp: line.Timestamp,
			Kind:      kind,
			Origin:    OriginReplay,
			MsgID:     msgid,
		})
	}
	r.capMessages(conversation)
}

// persistMessage appends one message to the conversation log when it is a
// persistable kind. It is a no-op without a log. It mirrors
// IrcEventReducer::persistMessage.
func (r *EventReducer) persistMessage(conversation *ConversationState, message ReducedMessage) {
	if r.log == nil || !persistableKind(message.Kind) {
		return
	}
	kind := kindToken(message.Kind)
	if kind == "" {
		return
	}
	r.log.Append(
		conversation.Key.NetworkID,
		conversation.Key.NormalizedTarget,
		featuresCaseMapping(r.ServerFeatures(conversation.Key.NetworkID)),
		TranscriptLine{
			Timestamp: message.Timestamp,
			Author:    message.Author,
			Kind:      kind,
			Body:      message.Body,
			MsgID:     message.MsgID.Value,
		},
	)
}

// capMessages trims the transcript to MaxMessages, dropping trimmed msgids and
// accounting for the removed prefix. It mirrors IrcEventReducer::capMessages.
func (r *EventReducer) capMessages(conversation *ConversationState) {
	extra := len(conversation.Messages) - MaxMessages
	if extra <= 0 {
		return
	}
	for index := 0; index < extra; index++ {
		msgid := conversation.Messages[index].MsgID
		if !msgid.IsEmpty() {
			delete(conversation.MessageIDs, msgid)
		}
	}
	conversation.Messages = conversation.Messages[extra:]
	conversation.Trimmed += extra
	if conversation.channel != nil && conversation.channel.HistoryAnchor != nil {
		if conversation.channel.HistoryAnchor.Sequence < int64(conversation.Trimmed) {
			conversation.channel.HistoryAnchor = nil
		}
	}
}

// peekSpliceIndex returns where a replay batch should land without consuming
// the anchor. A direct message and a joined channel without an anchor splice at
// the tail; an unjoined channel without an anchor cannot splice. It mirrors
// IrcEventReducer::peekSpliceIndex.
func (r *EventReducer) peekSpliceIndex(conversation *ConversationState) (int, bool) {
	channel := conversation.channel
	if channel == nil {
		return len(conversation.Messages), true
	}
	if channel.HistoryAnchor == nil {
		if channel.Joined {
			return len(conversation.Messages), true
		}
		return 0, false
	}
	index := channel.HistoryAnchor.Sequence - int64(conversation.Trimmed)
	if index < 0 || index > int64(len(conversation.Messages)) {
		return 0, false
	}
	return int(index), true
}

// takeSpliceIndex returns where a replay batch should land, dropping an anchor
// that no longer names a row. It mirrors IrcEventReducer::takeSpliceIndex.
func (r *EventReducer) takeSpliceIndex(conversation *ConversationState) (int, bool) {
	channel := conversation.channel
	if channel == nil {
		return len(conversation.Messages), true
	}
	if channel.HistoryAnchor == nil {
		return 0, false
	}
	index := channel.HistoryAnchor.Sequence - int64(conversation.Trimmed)
	// A usable anchor stays until a replay line actually lands.
	if index < 0 || index > int64(len(conversation.Messages)) {
		channel.HistoryAnchor = nil
		return 0, false
	}
	return int(index), true
}

// bouncerQueryOwnLine reports whether an author in a bouncer query batch is our
// own previous nick rather than the peer. It mirrors
// IrcEventReducer::bouncerQueryOwnLine.
func (r *EventReducer) bouncerQueryOwnLine(event HistoryEvent, author string) bool {
	if event.Kind != HistoryBouncerPlayback || author == "" {
		return false
	}
	if featuresIsChannel(r.ServerFeatures(event.Conversation.NetworkID), event.Conversation.NormalizedTarget) {
		return false
	}
	return !r.equals(event.Conversation.NetworkID, author, event.Target)
}

// rememberSelfNick records a nick this network has welcomed or changed away
// from. It mirrors IrcEventReducer::rememberSelfNick.
func (r *EventReducer) rememberSelfNick(networkID, nick string) {
	r.ensure()
	if networkID == "" || nick == "" {
		return
	}
	nicks := r.selfNicks[networkID]
	if nicks == nil {
		nicks = make(map[string]struct{})
		r.selfNicks[networkID] = nicks
	}
	nicks[nick] = struct{}{}
}

// bouncerChannelOwnLine reports whether an author in a bouncer channel batch is
// one of our own nicks. It mirrors
// IrcEventReducer::bouncerChannelOwnLine.
func (r *EventReducer) bouncerChannelOwnLine(event HistoryEvent, author string) bool {
	if event.Kind != HistoryBouncerPlayback || author == "" {
		return false
	}
	if !featuresIsChannel(r.ServerFeatures(event.Conversation.NetworkID), event.Conversation.NormalizedTarget) {
		return false
	}
	nicks, ok := r.selfNicks[event.Conversation.NetworkID]
	if !ok {
		return false
	}
	mapping := featuresCaseMapping(r.ServerFeatures(event.Conversation.NetworkID))
	for selfNick := range nicks {
		if mapping.Equals(selfNick, author) {
			return true
		}
	}
	return false
}

// replayFromPeer reports whether a batch holds a line from the peer. It mirrors
// IrcEventReducer::replayFromPeer.
func (r *EventReducer) replayFromPeer(event HistoryEvent) bool {
	bouncerQuery := event.Kind == HistoryBouncerPlayback &&
		!featuresIsChannel(r.ServerFeatures(event.Conversation.NetworkID), event.Conversation.NormalizedTarget)
	for _, line := range event.Lines {
		if line.Author == "" {
			continue
		}
		if bouncerQuery {
			if !r.bouncerQueryOwnLine(event, line.Author) {
				return true
			}
			continue
		}
		if !r.isSelf(event.Conversation.NetworkID, line.Author) {
			return true
		}
	}
	return false
}

// replayOnlyRepeatsPersistedIDs reports whether every line carries a msgid the
// transcript log already stores, so opening a missing query would add nothing.
// It mirrors IrcEventReducer::replayOnlyRepeatsPersistedIds.
func (r *EventReducer) replayOnlyRepeatsPersistedIDs(event HistoryEvent) bool {
	if r.log == nil || len(event.Lines) == 0 {
		return false
	}
	for _, line := range event.Lines {
		if line.MsgID.IsEmpty() {
			return false
		}
	}
	stored := r.log.ReadTail(
		event.Conversation.NetworkID,
		event.Conversation.NormalizedTarget,
		featuresCaseMapping(r.ServerFeatures(event.Conversation.NetworkID)),
		MaxMessages,
	)
	for _, line := range event.Lines {
		known := false
		for _, saved := range stored {
			if saved.MsgID == line.MsgID.Value {
				known = true
				break
			}
		}
		if !known {
			return false
		}
	}
	return true
}

// absorbPendingQueryPlayback appends a query batch onto a held one and splices
// when the combined batch has a peer line. It reports whether it handled the
// event. It mirrors IrcEventReducer::absorbPendingQueryPlayback.
func (r *EventReducer) absorbPendingQueryPlayback(event HistoryEvent) bool {
	held, ok := r.pendingPlayback[event.Conversation]
	if !ok {
		return false
	}
	if len(event.Lines) > 0 {
		held.Lines = append(held.Lines, event.Lines...)
		// Keep the extended batch in the store for every path that does not
		// erase it, exactly like the C++ reference into the map value.
		r.pendingPlayback[event.Conversation] = held
	}
	if !r.replayFromPeer(held) {
		if _, pending := r.queryRestorePending[event.Conversation.NetworkID]; !pending {
			delete(r.pendingPlayback, event.Conversation)
		}
		return true
	}
	conversation := r.findMutable(event.Conversation)
	if conversation == nil {
		if r.replayOnlyRepeatsPersistedIDs(held) {
			if _, pending := r.queryRestorePending[event.Conversation.NetworkID]; !pending {
				delete(r.pendingPlayback, event.Conversation)
			}
			return true
		}
		conversation = r.EnsureConversation(event.Conversation, held.Target, CauseInboundOther)
		if conversation == nil {
			return true
		}
	}
	delete(r.pendingPlayback, event.Conversation)
	r.spliceHistory(conversation, held, historyAnchorConsume)
	return true
}

// historyAnchorUse selects whether a splice consumes the history anchor or
// keeps it (shifted by what landed).
type historyAnchorUse int

const (
	historyAnchorConsume historyAnchorUse = iota
	historyAnchorKeep
)

// spliceHistory merges a replay batch into a transcript position, dedupes known
// msgids and matching rows, notes arrivals, and records kept replay lines. It
// mirrors IrcEventReducer::spliceHistory.
func (r *EventReducer) spliceHistory(conversation *ConversationState, event HistoryEvent, anchorUse historyAnchorUse) {
	var at int
	if anchorUse == historyAnchorConsume {
		index, ok := r.takeSpliceIndex(conversation)
		if !ok {
			return
		}
		at = index
	} else {
		index, ok := r.peekSpliceIndex(conversation)
		if !ok {
			return
		}
		at = index
	}
	previousSize := len(conversation.Messages)

	mapping := featuresCaseMapping(r.ServerFeatures(event.Conversation.NetworkID))
	sameReplayLine := func(message ReducedMessage, line ReplayLine, kind MessageKind) bool {
		if message.Kind != kind || message.Body != line.Body {
			return false
		}
		if message.Timestamp.IsZero() || line.Timestamp.IsZero() {
			return false
		}
		if message.Timestamp.UTC().UnixMilli() != line.Timestamp.UTC().UnixMilli() {
			return false
		}
		return mapping.Equals(message.Author, line.Author)
	}

	type pendingPlaybackNote struct {
		sequence   int64
		serverTime *time.Time
	}
	var run []ReducedMessage
	var pendingNotes []pendingPlaybackNote
	sawSelf := false

	matchedSequence := func(predicate func(ReducedMessage) bool) (int64, bool) {
		for _, message := range conversation.Messages {
			if predicate(message) {
				return message.Sequence, true
			}
		}
		for _, message := range run {
			if predicate(message) {
				return message.Sequence, true
			}
		}
		return 0, false
	}

	for _, line := range event.Lines {
		if !sawSelf && r.bouncerQueryOwnLine(event, line.Author) {
			sawSelf = true
		}
		queueNote := func(sequence int64) {
			pendingNotes = append(pendingNotes, pendingPlaybackNote{sequence: sequence, serverTime: line.ServerTime})
		}
		if !line.MsgID.IsEmpty() {
			if _, exists := conversation.MessageIDs[line.MsgID]; exists {
				if sequence, ok := matchedSequence(func(message ReducedMessage) bool {
					return message.MsgID == line.MsgID
				}); ok {
					queueNote(sequence)
				}
				continue
			}
		}
		kind := KindMessage
		if line.Kind == MessageKindEmote {
			kind = KindAction
		}
		if sequence, ok := matchedSequence(func(message ReducedMessage) bool {
			if !line.MsgID.IsEmpty() && !message.MsgID.IsEmpty() && message.MsgID != line.MsgID {
				return false
			}
			return sameReplayLine(message, line, kind)
		}); ok {
			queueNote(sequence)
			continue
		}
		if !line.MsgID.IsEmpty() {
			conversation.MessageIDs[line.MsgID] = struct{}{}
		}
		message := ReducedMessage{
			Author:    line.Author,
			Body:      line.Body,
			Timestamp: line.Timestamp,
			Kind:      kind,
			Origin:    OriginReplay,
			MsgID:     line.MsgID,
		}
		message.Sequence = conversation.NextSequence
		conversation.NextSequence++
		run = append(run, message)
		queueNote(message.Sequence)
	}

	if sawSelf {
		target := event.Target
		if target == "" {
			target = conversation.Target
		}
		r.rememberedQueries = append(r.rememberedQueries, RememberedQuery{
			NetworkID: event.Conversation.NetworkID,
			Target:    target,
		})
	}
	if len(run) > 0 {
		conversation.Messages = insertMessages(conversation.Messages, at, run)
		for _, message := range run {
			r.persistMessage(conversation, message)
		}
		// Consume drops the anchor only once a replay line has landed. Keep
		// still names the join line, shifted forward by what landed.
		if conversation.channel != nil {
			if anchorUse == historyAnchorConsume {
				conversation.channel.HistoryAnchor = nil
			} else if conversation.channel.HistoryAnchor != nil {
				conversation.channel.HistoryAnchor.Sequence += int64(len(run))
			}
		}
		if at != previousSize {
			conversation.SpliceEpoch++
		}
		r.capMessages(conversation)
		for _, message := range run {
			r.noteChatArrival(conversation, event.Conversation, message.Author, message.Body, message.Kind, message.MsgID, message.Sequence, OriginReplay, &event)
		}
	}

	keptPlaybackLine := false
	for _, pending := range pendingNotes {
		stillPresent := false
		for _, message := range conversation.Messages {
			if message.Sequence == pending.sequence {
				stillPresent = true
				break
			}
		}
		if !stillPresent {
			continue
		}
		if pending.serverTime != nil && !pending.serverTime.IsZero() {
			r.keptReplay = append(r.keptReplay, KeptReplay{
				NetworkID:  event.Conversation.NetworkID,
				Target:     event.Target,
				ServerTime: pending.serverTime.UTC(),
			})
		}
		keptPlaybackLine = true
	}
	if keptPlaybackLine && event.Kind == HistoryBouncerPlayback && conversation.channel != nil {
		r.keptPlaybackChannel[conversation.Key] = struct{}{}
	}
}

// holdPendingPlayback stores or extends a held query or channel batch. It
// mirrors IrcEventReducer::holdPendingPlayback.
func (r *EventReducer) holdPendingPlayback(event HistoryEvent) {
	held, ok := r.pendingPlayback[event.Conversation]
	if !ok {
		if len(event.Lines) == 0 {
			return
		}
		r.pendingPlayback[event.Conversation] = event
		return
	}
	if len(event.Lines) == 0 {
		return
	}
	held.Lines = append(held.Lines, event.Lines...)
	r.pendingPlayback[event.Conversation] = held
}

// dropKeptPlayback forgets kept-playback channels for one network. It mirrors
// IrcEventReducer::dropKeptPlayback.
func (r *EventReducer) dropKeptPlayback(networkID string) {
	if networkID == "" {
		return
	}
	for key := range r.keptPlaybackChannel {
		if key.NetworkID == networkID {
			delete(r.keptPlaybackChannel, key)
		}
	}
}

// releasePendingPlayback splices a held batch into a conversation that now
// exists, keeping the anchor. It mirrors
// IrcEventReducer::releasePendingPlayback.
func (r *EventReducer) releasePendingPlayback(conversation *ConversationState) {
	event, ok := r.pendingPlayback[conversation.Key]
	if !ok {
		return
	}
	delete(r.pendingPlayback, conversation.Key)
	r.spliceHistory(conversation, event, historyAnchorKeep)
}

// dropPendingPlayback forgets every held batch for one network. It mirrors
// IrcEventReducer::dropPendingPlayback.
func (r *EventReducer) dropPendingPlayback(networkID string) {
	if networkID == "" {
		return
	}
	for key := range r.pendingPlayback {
		if key.NetworkID == networkID {
			delete(r.pendingPlayback, key)
		}
	}
}

// reduceWelcome resets a network on a new connection and marks open-direct
// restore pending. It mirrors IrcEventReducer::reduce(IrcWelcomeEvent).
func (r *EventReducer) reduceWelcome(event WelcomeEvent) {
	r.dropPendingPlayback(event.NetworkID)
	r.dropKeptPlayback(event.NetworkID)
	r.queryRestorePending[event.NetworkID] = struct{}{}
	if current, ok := r.currentNicks[event.NetworkID]; ok {
		r.rememberSelfNick(event.NetworkID, current)
	}
	r.rememberSelfNick(event.NetworkID, event.CurrentNick)
	r.currentNicks[event.NetworkID] = event.CurrentNick
	if presence, ok := r.presence[event.NetworkID]; ok {
		presence.Clear()
		r.presence[event.NetworkID] = presence
	}
	delete(r.selfAway, event.NetworkID)
	for _, conversation := range r.conversations {
		if conversation.Key.NetworkID != event.NetworkID {
			continue
		}
		if conversation.channel != nil {
			conversation.channel.Members = make(map[string]MemberState)
			conversation.channel.Joined = false
			conversation.channel.HistoryAnchor = nil
			stopNamesSync(conversation.channel)
		}
		conversation.Typing = make(map[string]TypingHint)
	}
}

// reduceMessage admits a PRIVMSG. It mirrors
// IrcEventReducer::reduce(IrcMessageEvent).
func (r *EventReducer) reduceMessage(event MessageEvent) {
	r.appendChat(event.Conversation, displayTarget(event.Conversation, event.Target),
		event.Author, event.Body, event.Timestamp, KindMessage, event.MsgID)
}

// reduceNotice admits a NOTICE. It mirrors
// IrcEventReducer::reduce(IrcNoticeEvent).
func (r *EventReducer) reduceNotice(event NoticeEvent) {
	r.appendChat(event.Conversation, displayTarget(event.Conversation, event.Target),
		event.Author, event.Body, event.Timestamp, KindNotice, event.MsgID)
}

// reduceAction admits a CTCP ACTION. It mirrors
// IrcEventReducer::reduce(IrcActionEvent).
func (r *EventReducer) reduceAction(event ActionEvent) {
	r.appendChat(event.Conversation, displayTarget(event.Conversation, event.Target),
		event.Author, event.Body, event.Timestamp, KindAction, event.MsgID)
}

// reduceJoin records a JOIN, its account, and a self-join's history anchor. It
// mirrors IrcEventReducer::reduce(IrcJoinEvent).
func (r *EventReducer) reduceJoin(event JoinEvent) {
	normalizedNick := r.normalize(event.NetworkID, event.Nick)
	if event.Account != nil && normalizedNick != "" {
		r.setPresenceAccount(event.NetworkID, normalizedNick, *event.Account)
	}

	key := r.ConversationKey(event.NetworkID, event.Channel)
	conversation := r.EnsureConversation(key, event.Channel, CauseChannelState)
	if conversation == nil {
		return
	}
	channel := conversation.channel
	if channel == nil {
		return
	}
	var ranks PrefixSet
	if existing, ok := channel.Members[normalizedNick]; ok {
		ranks = existing.Ranks
	}
	channel.Members[normalizedNick] = MemberState{DisplayNick: event.Nick, Ranks: ranks}
	self := r.isSelf(event.NetworkID, event.Nick)
	if self {
		channel.Joined = true
		channel.HistoryAnchor = &TranscriptAnchor{
			Sequence: int64(conversation.Trimmed) + int64(len(conversation.Messages)),
		}
	}
	body := event.Nick + " joined"
	if event.Account != nil {
		if shown := r.DisplayAccount(event.NetworkID, event.Nick); shown != "" {
			body = event.Nick + " (" + shown + ") joined"
		}
	}
	r.appendEvent(conversation, body, !self)
	if self {
		r.releasePendingPlayback(conversation)
	}
}

// reduceAccount records an account-notify change. It mirrors
// IrcEventReducer::reduce(IrcAccountEvent).
func (r *EventReducer) reduceAccount(event AccountEvent) {
	normalizedNick := r.normalize(event.NetworkID, event.Nick)
	if normalizedNick == "" {
		return
	}
	r.setPresenceAccount(event.NetworkID, normalizedNick, event.Account)
}

// reducePart records a PART and forgets the departing nick's presence when no
// shared channel remains. It mirrors
// IrcEventReducer::reduce(IrcPartEvent).
func (r *EventReducer) reducePart(event PartEvent) {
	key := r.ConversationKey(event.NetworkID, event.Channel)
	conversation := r.findMutable(key)
	if conversation == nil || conversation.channel == nil {
		return
	}
	channel := conversation.channel
	self := r.isSelf(event.NetworkID, event.Nick)
	departed := []string{r.normalize(event.NetworkID, event.Nick)}
	delete(channel.Members, departed[0])
	if self {
		channel.Joined = false
		channel.HistoryAnchor = nil
		for member := range channel.Members {
			departed = append(departed, member)
		}
		channel.Members = make(map[string]MemberState)
		stopNamesSync(channel)
	}
	r.forgetUnseen(event.NetworkID, departed)
	r.appendEvent(conversation, event.Nick+" left", true)
	if self {
		conversation.Typing = make(map[string]TypingHint)
	} else {
		r.clearTyping(conversation, departed[0])
	}
}

// reduceQuit records a QUIT in every shared channel. It mirrors
// IrcEventReducer::reduce(IrcQuitEvent).
func (r *EventReducer) reduceQuit(event QuitEvent) {
	normalizedNick := r.normalize(event.NetworkID, event.Nick)
	for _, conversation := range r.conversations {
		if conversation.Key.NetworkID != event.NetworkID {
			continue
		}
		channel := conversation.channel
		if channel == nil {
			continue
		}
		if _, ok := channel.Members[normalizedNick]; !ok {
			continue
		}
		delete(channel.Members, normalizedNick)
		r.appendEvent(conversation, event.Nick+" quit", true)
	}
	r.forgetUnseen(event.NetworkID, []string{normalizedNick})
	r.clearTypingEverywhere(event.NetworkID, normalizedNick)
}

// reduceNick moves membership, presence, typing, and any direct conversation
// from the old nick to the new one. It mirrors
// IrcEventReducer::reduce(IrcNickEvent).
func (r *EventReducer) reduceNick(event NickEvent) {
	oldNormalized := r.normalize(event.NetworkID, event.OldNick)
	newNormalized := r.normalize(event.NetworkID, event.NewNick)
	if r.isSelf(event.NetworkID, event.OldNick) {
		r.rememberSelfNick(event.NetworkID, event.OldNick)
		r.currentNicks[event.NetworkID] = event.NewNick
	}
	r.rekeyPresence(event.NetworkID, oldNormalized, newNormalized)
	r.rekeyTyping(event.NetworkID, oldNormalized, newNormalized, event.NewNick)

	for _, conversation := range r.conversations {
		if conversation.Key.NetworkID != event.NetworkID {
			continue
		}
		channel := conversation.channel
		if channel == nil {
			continue
		}
		member, ok := channel.Members[oldNormalized]
		if !ok {
			continue
		}
		updated := member
		updated.DisplayNick = event.NewNick
		delete(channel.Members, oldNormalized)
		channel.Members[newNormalized] = updated
		r.appendEvent(conversation, event.OldNick+" is now "+event.NewNick, true)
	}

	oldKey := ConversationKey{NetworkID: event.NetworkID, NormalizedTarget: oldNormalized}
	newKey := ConversationKey{NetworkID: event.NetworkID, NormalizedTarget: newNormalized}
	direct, ok := r.conversations[oldKey]
	if !ok || direct.IsChannel() {
		return
	}

	direct.Target = event.NewNick
	r.appendEvent(direct, event.OldNick+" is now "+event.NewNick, true)
	if oldKey == newKey {
		return
	}

	moved := direct
	delete(r.conversations, oldKey)
	moved.Key = newKey
	moved.Target = event.NewNick
	if _, muted := r.mutedKeys[oldKey]; muted {
		delete(r.mutedKeys, oldKey)
		r.mutedKeys[newKey] = struct{}{}
	}
	if existing, found := r.conversations[newKey]; !found {
		r.conversations[newKey] = moved
	} else {
		// Both sides may already hold the same msgid. Admitting a duplicate
		// would leave one id covering two messages.
		for _, message := range moved.Messages {
			if !message.MsgID.IsEmpty() {
				if _, exists := existing.MessageIDs[message.MsgID]; exists {
					continue
				}
				existing.MessageIDs[message.MsgID] = struct{}{}
			}
			oldSequence := message.Sequence
			message.Sequence = existing.NextSequence
			existing.NextSequence++
			if existing.UnreadMark == nil && moved.UnreadMark != nil && oldSequence == *moved.UnreadMark {
				mark := message.Sequence
				existing.UnreadMark = &mark
			}
			existing.Messages = append(existing.Messages, message)
		}
		r.capMessages(existing)
		existing.Unread += moved.Unread
		existing.Mentions += moved.Mentions
		existing.Muted = existing.Muted || moved.Muted
		for nick, hint := range moved.Typing {
			existing.Typing[nick] = hint
		}
		existing.Target = event.NewNick
	}
	if r.selected != nil && *r.selected == oldKey {
		selected := newKey
		r.selected = &selected
	}
}

// reduceKick records a KICK and, for a self-kick, raises an inbox arrival. It
// mirrors IrcEventReducer::reduce(IrcKickEvent).
func (r *EventReducer) reduceKick(event KickEvent) {
	key := r.ConversationKey(event.NetworkID, event.Channel)
	conversation := r.findMutable(key)
	if conversation == nil || conversation.channel == nil {
		return
	}
	channel := conversation.channel
	self := r.isSelf(event.NetworkID, event.Target)
	departed := []string{r.normalize(event.NetworkID, event.Target)}
	delete(channel.Members, departed[0])
	if self {
		channel.Joined = false
		channel.HistoryAnchor = nil
		for member := range channel.Members {
			departed = append(departed, member)
		}
		channel.Members = make(map[string]MemberState)
		stopNamesSync(channel)
	}
	r.forgetUnseen(event.NetworkID, departed)
	r.appendEvent(conversation, event.Target+" was kicked", false)
	if self {
		conversation.Typing = make(map[string]TypingHint)
		preview := event.Reason
		if preview == "" {
			preview = "You were kicked"
		}
		r.inboxArrival = &InboxArrival{
			Kind:      InboxKick,
			Actor:     event.Author,
			Body:      preview,
			NetworkID: event.NetworkID,
			Target:    event.Channel,
		}
	} else {
		r.clearTyping(conversation, departed[0])
	}
}

// reduceTopic records a channel topic. It mirrors
// IrcEventReducer::reduce(IrcTopicEvent).
func (r *EventReducer) reduceTopic(event TopicEvent) {
	key := r.ConversationKey(event.NetworkID, event.Channel)
	conversation := r.EnsureConversation(key, event.Channel, CauseChannelState)
	if conversation == nil {
		return
	}
	if conversation.channel != nil {
		conversation.channel.Topic = event.Topic
	}
}

// reduceNames merges a NAMES batch into channel membership, starting and
// finishing the sync. It mirrors IrcEventReducer::reduce(IrcNamesEvent).
func (r *EventReducer) reduceNames(event NamesEvent, now time.Time) {
	key := r.ConversationKey(event.NetworkID, event.Channel)
	conversation := r.EnsureConversation(key, event.Channel, CauseChannelState)
	if conversation == nil {
		return
	}
	channel := conversation.channel
	if channel == nil {
		return
	}
	if !channel.NamesSyncing {
		startNamesSync(channel, now)
	}
	for _, name := range event.Names {
		channel.Members[r.normalize(event.NetworkID, name.Nick)] = MemberState{
			DisplayNick: name.Nick,
			Ranks:       name.Ranks,
		}
	}
	if event.Complete {
		stopNamesSync(channel)
	}
}

// reduceMode records a channel MODE and applies prefix rank changes. It mirrors
// IrcEventReducer::reduce(IrcModeEvent).
func (r *EventReducer) reduceMode(event ModeEvent) {
	key := r.ConversationKey(event.NetworkID, event.Target)
	features := r.ServerFeatures(event.NetworkID)
	if !features.IsChannel(key.NormalizedTarget) {
		return
	}
	conversation := r.EnsureConversation(key, event.Target, CauseChannelState)
	if conversation == nil {
		return
	}
	r.appendEvent(conversation, event.Author+" set mode "+event.Mode, false)
	channel := conversation.channel
	if channel == nil {
		return
	}
	for _, change := range features.PrefixChanges(event.Mode, event.Arguments) {
		normalized := r.normalize(event.NetworkID, WireText([]byte(change.Nick)))
		member, ok := channel.Members[normalized]
		if !ok {
			continue
		}
		member.Ranks = features.Apply(member.Ranks, change)
		channel.Members[normalized] = member
	}
}

// reduceAway records an away-notify change. It mirrors
// IrcEventReducer::reduce(IrcAwayEvent).
func (r *EventReducer) reduceAway(event AwayEvent) {
	r.setPresenceAway(event.NetworkID, r.normalize(event.NetworkID, event.Nick), event.Away)
}

// reduceSelfAway records the local user's own away state. It mirrors
// IrcEventReducer::reduce(IrcSelfAwayEvent).
func (r *EventReducer) reduceSelfAway(event SelfAwayEvent) {
	if event.Away {
		r.selfAway[event.NetworkID] = struct{}{}
	} else {
		delete(r.selfAway, event.NetworkID)
	}
}

// reduceMemberMetadata records one metadata value. It mirrors
// IrcEventReducer::reduce(IrcMemberMetadataEvent).
func (r *EventReducer) reduceMemberMetadata(event MemberMetadataEvent) {
	r.setPresenceMetadata(event.NetworkID, r.normalize(event.NetworkID, event.Nick), event.Key, event.Value)
}

// reduceTyping records or clears one typing hint. It mirrors
// IrcEventReducer::reduce(IrcTypingEvent).
func (r *EventReducer) reduceTyping(event TypingEvent) {
	conversation := r.findMutable(event.Conversation)
	normalizedNick := r.normalize(event.Conversation.NetworkID, event.Nick)
	if conversation == nil || r.isSelf(event.Conversation.NetworkID, event.Nick) {
		if conversation != nil {
			r.clearTyping(conversation, normalizedNick)
		}
		return
	}
	hint, ok := StoredTypingHint(event.Phase, event.ReceivedAt, event.Nick)
	if !ok {
		r.clearTyping(conversation, normalizedNick)
		return
	}
	r.pruneExpiredTyping(conversation, event.ReceivedAt)
	conversation.Typing[normalizedNick] = hint
}

// reduceHistory routes a replay batch: bouncer channel batches wait for the
// self-join, bouncer query batches absorb into a held batch, and everything
// else splices now. It mirrors IrcEventReducer::reduce(IrcHistoryEvent).
func (r *EventReducer) reduceHistory(event HistoryEvent) {
	channelTarget := featuresIsChannel(r.ServerFeatures(event.Conversation.NetworkID), event.Conversation.NormalizedTarget)
	conversation := r.findMutable(event.Conversation)
	if event.Kind == HistoryBouncerPlayback && channelTarget {
		var channel *ChannelState
		if conversation != nil {
			channel = conversation.channel
		}
		if channel == nil || !channel.Joined {
			r.holdPendingPlayback(event)
			return
		}
		r.spliceHistory(conversation, event, historyAnchorKeep)
		return
	}
	if event.Kind == HistoryBouncerPlayback && !channelTarget && r.absorbPendingQueryPlayback(event) {
		return
	}
	if conversation == nil {
		if channelTarget {
			return
		}
		if !r.replayFromPeer(event) {
			if event.Kind == HistoryBouncerPlayback {
				if _, pending := r.queryRestorePending[event.Conversation.NetworkID]; pending {
					r.holdPendingPlayback(event)
				}
			}
			return
		}
		if r.replayOnlyRepeatsPersistedIDs(event) {
			return
		}
		conversation = r.EnsureConversation(event.Conversation, event.Target, CauseInboundOther)
		if conversation == nil {
			return
		}
	}
	if conversation == nil {
		return
	}
	r.spliceHistory(conversation, event, historyAnchorConsume)
}

// reduceWhoisTranscript appends a formatted WHOIS block to its conversation. It
// mirrors IrcEventReducer::reduce(IrcWhoisTranscriptEvent).
func (r *EventReducer) reduceWhoisTranscript(event WhoisTranscriptEvent) {
	conversation := r.findMutable(event.Destination)
	if conversation == nil {
		return
	}
	r.appendWhois(conversation, event.FormattedBody)
}

// reduceChannelError appends a channel-scoped error line. It mirrors
// IrcEventReducer::reduce(IrcChannelErrorEvent).
func (r *EventReducer) reduceChannelError(event ChannelErrorEvent) {
	conversation := r.findMutable(r.ConversationKey(event.NetworkID, event.Channel))
	if conversation == nil {
		return
	}
	r.appendError(conversation, event.Body)
}

// clearTyping drops one nick's typing hint.
func (r *EventReducer) clearTyping(conversation *ConversationState, normalizedNick string) {
	delete(conversation.Typing, normalizedNick)
}

// clearTypingEverywhere drops one nick's typing hint on every conversation of a
// network. It mirrors IrcEventReducer::clearTypingEverywhere.
func (r *EventReducer) clearTypingEverywhere(networkID, normalizedNick string) {
	for _, conversation := range r.conversations {
		if conversation.Key.NetworkID == networkID {
			r.clearTyping(conversation, normalizedNick)
		}
	}
}

// rekeyTyping moves a typing hint to a nick's new spelling. It mirrors
// IrcEventReducer::rekeyTyping.
func (r *EventReducer) rekeyTyping(networkID, oldNormalized, newNormalized, newDisplay string) {
	for _, conversation := range r.conversations {
		if conversation.Key.NetworkID != networkID {
			continue
		}
		hint, ok := conversation.Typing[oldNormalized]
		if !ok {
			continue
		}
		hint.DisplayNick = newDisplay
		delete(conversation.Typing, oldNormalized)
		conversation.Typing[newNormalized] = hint
	}
}

// pruneExpiredTyping drops hints that are no longer retained. It mirrors
// IrcEventReducer::pruneExpiredTyping.
func (r *EventReducer) pruneExpiredTyping(conversation *ConversationState, now time.Time) {
	for nick, hint := range conversation.Typing {
		if !TypingHintRetained(hint, now) {
			delete(conversation.Typing, nick)
		}
	}
}

// forgetUnseen drops a nick's presence when it is no longer visible in any
// channel. It mirrors IrcEventReducer::forgetUnseen.
func (r *EventReducer) forgetUnseen(networkID string, normalizedNicks []string) {
	presence, ok := r.presence[networkID]
	if !ok {
		return
	}
	for _, nick := range normalizedNicks {
		if presence.Knows(nick) && !r.isVisible(networkID, nick) {
			presence.Forget(nick)
		}
	}
	r.presence[networkID] = presence
}

// isVisible reports whether a normalized nick is a member of any channel on a
// network. It mirrors IrcEventReducer::isVisible.
func (r *EventReducer) isVisible(networkID, normalizedNick string) bool {
	for _, conversation := range r.conversations {
		if conversation.Key.NetworkID != networkID {
			continue
		}
		channel := conversation.channel
		if channel == nil {
			continue
		}
		if _, ok := channel.Members[normalizedNick]; ok {
			return true
		}
	}
	return false
}

// setPresenceAway stores a nick's away state, keeping the presence map usable.
func (r *EventReducer) setPresenceAway(networkID, normalizedNick string, away *Away) {
	presence := r.presence[networkID]
	presence.SetAway(normalizedNick, away)
	r.presence[networkID] = presence
}

// setPresenceMetadata stores one metadata value for a nick.
func (r *EventReducer) setPresenceMetadata(networkID, normalizedNick, key, value string) {
	presence := r.presence[networkID]
	presence.SetMetadata(normalizedNick, key, value)
	r.presence[networkID] = presence
}

// setPresenceAccount stores a nick's services account.
func (r *EventReducer) setPresenceAccount(networkID, normalizedNick, account string) {
	presence := r.presence[networkID]
	presence.SetAccount(normalizedNick, account)
	r.presence[networkID] = presence
}

// rekeyPresence moves a nick's presence to a new normalized nick.
func (r *EventReducer) rekeyPresence(networkID, oldNormalized, newNormalized string) {
	presence := r.presence[networkID]
	presence.Rekey(oldNormalized, newNormalized)
	r.presence[networkID] = presence
}

// dereferenceEvent unwraps a pointer event so Apply can switch on values. A nil
// pointer becomes a nil Event.
func dereferenceEvent(event Event) Event {
	switch value := event.(type) {
	case *WelcomeEvent:
		if value == nil {
			return nil
		}
		return *value
	case *MessageEvent:
		if value == nil {
			return nil
		}
		return *value
	case *NoticeEvent:
		if value == nil {
			return nil
		}
		return *value
	case *ActionEvent:
		if value == nil {
			return nil
		}
		return *value
	case *JoinEvent:
		if value == nil {
			return nil
		}
		return *value
	case *PartEvent:
		if value == nil {
			return nil
		}
		return *value
	case *QuitEvent:
		if value == nil {
			return nil
		}
		return *value
	case *NickEvent:
		if value == nil {
			return nil
		}
		return *value
	case *KickEvent:
		if value == nil {
			return nil
		}
		return *value
	case *TopicEvent:
		if value == nil {
			return nil
		}
		return *value
	case *NamesEvent:
		if value == nil {
			return nil
		}
		return *value
	case *ModeEvent:
		if value == nil {
			return nil
		}
		return *value
	case *AwayEvent:
		if value == nil {
			return nil
		}
		return *value
	case *SelfAwayEvent:
		if value == nil {
			return nil
		}
		return *value
	case *MemberMetadataEvent:
		if value == nil {
			return nil
		}
		return *value
	case *AccountEvent:
		if value == nil {
			return nil
		}
		return *value
	case *TypingEvent:
		if value == nil {
			return nil
		}
		return *value
	case *HistoryEvent:
		if value == nil {
			return nil
		}
		return *value
	case *WhoisTranscriptEvent:
		if value == nil {
			return nil
		}
		return *value
	case *ChannelErrorEvent:
		if value == nil {
			return nil
		}
		return *value
	}
	return event
}

// chatLineReason classifies why a chat line is worth surfacing.
type chatLineReason int

const (
	chatLineNickMention chatLineReason = iota
	chatLineHighlight
	chatLineDirect
)

// classifyChatLine decides whether a chat line is a mention, highlight, or
// direct message. It mirrors the anonymous classifyChatLine.
func classifyChatLine(conversation *ConversationState, kind MessageKind, self, nickHit, highlightHit bool) (chatLineReason, bool) {
	if kind != KindMessage && kind != KindAction {
		return 0, false
	}
	if self {
		return 0, false
	}
	if nickHit {
		return chatLineNickMention, true
	}
	if highlightHit {
		return chatLineHighlight, true
	}
	if !conversation.IsChannel() {
		return chatLineDirect, true
	}
	return 0, false
}

// inboxKindFor maps a chat-line reason to its inbox kind. It mirrors the
// anonymous inboxKindFor.
func inboxKindFor(reason chatLineReason) InboxKind {
	switch reason {
	case chatLineNickMention:
		return InboxMention
	case chatLineHighlight:
		return InboxHighlight
	case chatLineDirect:
		return InboxDirect
	}
	return InboxMention
}

// kindToken maps a message kind to its persisted token. It mirrors the
// anonymous kindToken.
func kindToken(kind MessageKind) string {
	switch kind {
	case KindAction:
		return "action"
	case KindEvent, KindError:
		return "event"
	case KindWhois:
		return "whois"
	case KindNotice:
		return "notice"
	case KindMessage:
		return "message"
	}
	return ""
}

// kindFromToken resolves a persisted token, recognizing only the kinds the log
// stores. It mirrors the anonymous kindFromToken.
func kindFromToken(token string) (MessageKind, bool) {
	switch token {
	case "action":
		return KindAction, true
	case "event":
		return KindEvent, true
	case "notice":
		return KindNotice, true
	case "message":
		return KindMessage, true
	}
	return 0, false
}

// persistableKind reports whether a kind is written to the transcript log. It
// mirrors the anonymous persistableKind.
func persistableKind(kind MessageKind) bool {
	switch kind {
	case KindMessage, KindNotice, KindAction, KindEvent:
		return true
	case KindError, KindWhois:
		return false
	}
	return false
}

// admitMessage appends a message and assigns its sequence. It mirrors the
// anonymous admitMessage.
func admitMessage(conversation *ConversationState, message ReducedMessage) {
	message.Sequence = conversation.NextSequence
	conversation.NextSequence++
	conversation.Messages = append(conversation.Messages, message)
}

// displayTarget falls back to the normalized target when the event carries no
// display target. It mirrors the anonymous displayTarget.
func displayTarget(key ConversationKey, target string) string {
	if target == "" {
		return key.NormalizedTarget
	}
	return target
}

// startNamesSync clears membership and starts the sync clock. It mirrors the
// anonymous startNamesSync.
func startNamesSync(channel *ChannelState, now time.Time) {
	channel.Members = make(map[string]MemberState)
	channel.NamesSyncing = true
	channel.NamesSyncStarted = now
}

// stopNamesSync clears the sync flag and clock. It mirrors the anonymous
// stopNamesSync.
func stopNamesSync(channel *ChannelState) {
	channel.NamesSyncing = false
	channel.NamesSyncStarted = time.Time{}
}

// collapseEventBody merges two collapsible event bodies sharing a suffix. It
// mirrors the anonymous collapseEventBody.
func collapseEventBody(existing, incoming string) string {
	suffixes := []string{" joined", " left", " quit"}
	for _, suffix := range suffixes {
		if strings.HasSuffix(existing, suffix) && strings.HasSuffix(incoming, suffix) {
			return strings.TrimSuffix(existing, suffix) + ", " + strings.TrimSuffix(incoming, suffix) + suffix
		}
	}
	return existing + ", " + incoming
}

// containsWord reports whether normalizedWord appears in normalizedBody on
// identifier boundaries. It mirrors the anonymous containsWord.
func containsWord(normalizedBody, normalizedWord string) bool {
	if normalizedWord == "" {
		return false
	}
	body := []rune(normalizedBody)
	word := []rune(normalizedWord)
	if len(word) > len(body) {
		return false
	}
	for position := 0; position+len(word) <= len(body); position++ {
		matched := true
		for offset := range word {
			if body[position+offset] != word[offset] {
				matched = false
				break
			}
		}
		if !matched {
			continue
		}
		end := position + len(word)
		leftBoundary := position == 0 || !isIdentifierRune(body[position-1])
		rightBoundary := end == len(body) || !isIdentifierRune(body[end])
		if leftBoundary && rightBoundary {
			return true
		}
	}
	return false
}

// isIdentifierRune reports whether a rune counts as a word character for
// mention boundaries. It mirrors the anonymous isIdentifierCharacter.
func isIdentifierRune(character rune) bool {
	if unicode.IsLetter(character) || unicode.IsNumber(character) {
		return true
	}
	switch character {
	case '-', '_', '[', ']', '\\', '`', '^', '{', '}', '|', '~':
		return true
	}
	return false
}

// insertMessages returns messages with run inserted at at, clamped to the valid
// range.
func insertMessages(messages []ReducedMessage, at int, run []ReducedMessage) []ReducedMessage {
	if len(run) == 0 {
		return messages
	}
	if at < 0 {
		at = 0
	}
	if at > len(messages) {
		at = len(messages)
	}
	result := make([]ReducedMessage, 0, len(messages)+len(run))
	result = append(result, messages[:at]...)
	result = append(result, run...)
	result = append(result, messages[at:]...)
	return result
}
