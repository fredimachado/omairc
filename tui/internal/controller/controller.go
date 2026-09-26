package controller

import (
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// Controller is the Go port of IrcController for the Phase 2 state model. It
// owns the sessions, the event reducer, the selection, and the cached
// snapshots. See doc.go for scope and seams.go for the deferred behaviour.
//
// A Controller is not safe for concurrent use.
type Controller struct {
	clock session.Clock

	manager       *session.Manager
	reducer       *irc.EventReducer
	statusConsole *StatusConsole

	currentNicks map[string]string
	lastErrors   map[string]string
	capabilities map[string]irc.CapabilitySet

	selected         *irc.ConversationKey
	selectedTarget   string
	consoleNetworkID string
	consoleOpen      bool
	networkOrder     []string
	connectionStatus string

	conversationEpoch int
	peerMetadataEpoch int
	peerAccountEpoch  int

	conversations []ConversationSnapshot
	messages      []MessageSnapshot
	members       []MemberSnapshot

	// OnMentionArrived is called for each mention the reducer hands up. It is
	// nil-able; Phase 9 wires the inbox and notifications.
	OnMentionArrived func(author, body, networkID, target, msgid string)
	// OnInboxArrived is called for each inbox item the reducer hands up. It is
	// nil-able; Phase 9 owns the inbox store and model.
	OnInboxArrived func(arrival irc.InboxArrival)
	// OnSelectionChanged fires when the selected conversation, the focused
	// network, or the Status surface changes.
	OnSelectionChanged func()
	// OnStatusChanged fires when a connection state, last error, or self-away
	// fact that the identity footer reads changes.
	OnStatusChanged func()
	// OnCapabilitiesChanged fires when the negotiated capability set changes or
	// the selection moves to a network with a different set.
	OnCapabilitiesChanged func()
}

// New returns an empty controller with a real clock, an empty reducer, and an
// empty default-capacity Status console.
func New() *Controller {
	c := &Controller{
		clock:            session.RealClock{},
		manager:          session.NewManager(),
		reducer:          irc.NewEventReducer(),
		statusConsole:    NewStatusConsole(0),
		currentNicks:     make(map[string]string),
		lastErrors:       make(map[string]string),
		capabilities:     make(map[string]irc.CapabilitySet),
		connectionStatus: "Offline",
	}
	// IrcEventReducer starts with the window treated as active
	// (src/irc/irceventreducer.h:395). NewEventReducer leaves the Go field at
	// its zero value, so the controller restores the Qt default here.
	c.reducer.SetWindowActive(true)
	return c
}

// Controller implements the session event receiver; AddSession wires it.
var _ session.Handler = (*Controller)(nil)

// SetClock installs the clock the controller reads for event timestamps and
// typing expiry. A nil clock restores the real clock.
func (c *Controller) SetClock(clock session.Clock) {
	if clock == nil {
		clock = session.RealClock{}
	}
	c.clock = clock
}

func (c *Controller) now() time.Time {
	if c.clock == nil {
		return time.Time{}
	}
	return c.clock.Now()
}

// Reducer returns the event reducer the controller folds into.
func (c *Controller) Reducer() *irc.EventReducer {
	return c.reducer
}

// Console returns the per-network Status console.
func (c *Controller) Console() *StatusConsole {
	return c.statusConsole
}

// ConversationEpoch increments whenever the conversation list snapshot is
// rebuilt.
func (c *Controller) ConversationEpoch() int { return c.conversationEpoch }

// PeerMetadataEpoch increments whenever peer metadata changes.
func (c *Controller) PeerMetadataEpoch() int { return c.peerMetadataEpoch }

// PeerAccountEpoch increments whenever an account fact moves.
func (c *Controller) PeerAccountEpoch() int { return c.peerAccountEpoch }

// Session returns the live session for networkID, or nil.
func (c *Controller) Session(networkID string) *session.Session {
	return c.manager.Find(networkID)
}

// NetworkIDs returns every registered network id, sorted.
func (c *Controller) NetworkIDs() []string {
	return c.manager.NetworkIDs()
}

// --- Session handler ------------------------------------------------------

// StateChanged refreshes the focused connection status. The session Handler
// signature carries no network id, so the controller derives it from the
// focused network; per-network status is read live from the session.
func (c *Controller) StateChanged(state session.SessionState) {
	c.refreshConnectionStatus()
}

// ErrorOccurred records the last error for the network and notifies.
func (c *Controller) ErrorOccurred(networkID string, kind session.ErrorKind, message string) {
	c.setLastError(networkID, message)
	c.notifyStatusChanged()
}

// Registered resets the network's features, records the assigned nick, and
// folds the welcome into the reducer. It mirrors the registered lambda in
// IrcController::addSession (src/irc/irccontroller.cpp:449-461).
func (c *Controller) Registered(networkID string) {
	nick := c.currentNicks[networkID]
	if s := c.manager.Find(networkID); s != nil {
		nick = s.Nick()
		c.currentNicks[networkID] = nick
	}
	c.reducer.SetServerFeatures(networkID, irc.NewServerFeatures())
	c.Apply(irc.WelcomeEvent{NetworkID: networkID, CurrentNick: nick})
	c.refreshConnectionStatus()
}

// ReconnectScheduled refreshes the status text. The backoff itself is the
// session's.
func (c *Controller) ReconnectScheduled(networkID string, delayMs, attempt int) {
	c.refreshConnectionStatus()
}

// MessageReceived translates and folds one inbound line.
func (c *Controller) MessageReceived(networkID string, message irc.Message) {
	c.handleMessage(networkID, message)
}

// HistoryBatchReceived replays one history batch into the reducer. Playback
// trimming is Phase 11.
func (c *Controller) HistoryBatchReceived(networkID string, batch irc.HistoryBatch) {
	features := c.reducer.ServerFeatures(networkID)
	event, ok := irc.TranslateHistory(networkID, c.currentNicks[networkID], features, batch, c.now())
	if !ok {
		return
	}
	c.Apply(event)
}

// StatusEntry records one classified Status line.
func (c *Controller) StatusEntry(entry irc.StatusEntry) {
	c.statusConsole.Append(entry)
}

// CapabilitiesChanged stores the set and clears presence/typing facts that a
// dropped capability can no longer maintain. It mirrors IrcController::
// handleCapabilities (src/irc/irccontroller.cpp:931-967).
func (c *Controller) CapabilitiesChanged(networkID string, capabilities irc.CapabilitySet) {
	c.handleCapabilities(networkID, capabilities)
}

// RequestLabelFinished is a Phase 6 ReplyRouter seam.
func (c *Controller) RequestLabelFinished(networkID, requestLabel string) {}

// AutojoinChannelsChanged is a Phase 7/9 profile seam.
func (c *Controller) AutojoinChannelsChanged(networkID string, channels []string, keys map[string]string) {
}

// --- Apply / Publish ------------------------------------------------------

// Apply folds one translated event into the reducer and republishes the
// surfaces it dirtied. It mirrors IrcController::apply
// (src/irc/irccontroller.cpp:2015-2135).
func (c *Controller) Apply(event irc.Event) {
	if event == nil {
		return
	}
	// A redundant account tag never lands: the reducer would re-store the same
	// value and the view would repaint for nothing.
	if account, ok := event.(irc.AccountEvent); ok {
		stored := c.reducer.NickPresence(account.NetworkID, account.Nick).Account
		if servicesAccountMatches(stored, account.Account) {
			return
		}
	}
	kind := irc.KindOf(event)
	typingOnly := kind == irc.EventTyping
	selfAwayOnly := kind == irc.EventSelfAway

	c.reducer.Apply(event, c.now())
	c.noteKeptReplay()

	if _, ok := event.(irc.MemberMetadataEvent); ok {
		c.peerMetadataEpoch++
	}
	if accountsMoved(kind, event) {
		c.peerAccountEpoch++
	}

	if selfAwayOnly {
		c.Publish(irc.ClassifyViewNotify(event, c.reducer, c.selected))
		return
	}

	// A first conversation claims the selection. The Qt controller reads the
	// reducer's std::map begin(), i.e. the (network, normalized target)
	// minimum; Go maps iterate randomly, so pick it explicitly.
	if c.selected == nil && !typingOnly && len(c.reducer.Conversations()) > 0 {
		key, conversation := firstConversation(c.reducer)
		c.selected = &key
		c.selectedTarget = conversation.Target
		c.reducer.MarkSelected(key)
	}
	c.adoptReducerSelection()

	releasedStale := c.reducer.ReleaseStaleNamesSync(c.selected, c.now())
	notify := irc.ClassifyViewNotify(event, c.reducer, c.selected)
	if releasedStale {
		notify.Conversations = true
		notify.Messages = true
		notify.Members = irc.MemberSurfaceReset
		notify.Selection = true
	}
	c.Publish(notify)

	if mention, ok := c.reducer.TakeMentionArrival(); ok && c.OnMentionArrived != nil {
		c.OnMentionArrived(mention.Author, mention.Body, mention.NetworkID,
			mention.Target, mention.MsgID.Value)
	}
	if arrival, ok := c.reducer.TakeInboxArrival(); ok && c.OnInboxArrived != nil {
		c.OnInboxArrived(arrival)
	}
}

// Publish is the only place the snapshots refresh. It rebuilds the surfaces a
// notify names and bumps the matching epochs. It mirrors IrcController::publish
// (src/irc/irccontroller.cpp:2137-2158).
func (c *Controller) Publish(notify irc.ViewNotify) {
	if notify.Conversations {
		c.rebuildConversations()
		c.conversationEpoch++
	}
	if notify.Messages {
		c.rebuildMessages()
	}
	if notify.Members != irc.MemberSurfaceNone {
		c.rebuildMembers()
	}
	if notify.Selection {
		c.notifySelectionChanged()
	}
}

// adoptReducerSelection pulls a selection the reducer moved on its own (a NICK
// rekey, for example) back into the controller. It mirrors
// IrcController::adoptReducerSelection (src/irc/irccontroller.cpp:2006-2014).
func (c *Controller) adoptReducerSelection() {
	key, ok := c.reducer.Selected()
	if !ok {
		return
	}
	c.selected = &key
}

// selectionNotify reloads the transcript and member panel alongside the
// selection, matching the model selects in IrcController::selectConversation.
func selectionNotify() irc.ViewNotify {
	return irc.ViewNotify{
		Messages:  true,
		Members:   irc.MemberSurfaceReset,
		Selection: true,
	}
}

// reloadModels republishes everything, keeping a syncing channel's member panel
// suppressed. It mirrors IrcController::reloadModels
// (src/irc/irccontroller.cpp:2342-2352).
func (c *Controller) reloadModels() {
	if irc.ChannelNamesSyncing(c.reducer, c.selected) {
		c.Publish(irc.ViewNotify{Conversations: true})
		return
	}
	c.Publish(irc.ViewNotifyResetAll())
}

// --- Selection ------------------------------------------------------------

// SelectedNetworkID returns the selected conversation's network, or "".
func (c *Controller) SelectedNetworkID() string {
	if c.selected == nil {
		return ""
	}
	return c.selected.NetworkID
}

// SelectedTarget returns the selected conversation's display target, falling
// back to the last requested target while the conversation is unknown.
func (c *Controller) SelectedTarget() string {
	if c.selected != nil {
		if conversation := c.reducer.Find(*c.selected); conversation != nil {
			return conversation.Target
		}
	}
	return c.selectedTarget
}

// SelectedConversationID returns the stable id of the selected conversation,
// or "".
func (c *Controller) SelectedConversationID() string {
	if c.selected == nil {
		return ""
	}
	return irc.ConversationID(*c.selected)
}

// FocusedNetworkID returns the network the identity footer and connection
// status follow: the Status surface when it is open, else the selected
// conversation's network, else the last Status network.
func (c *Controller) FocusedNetworkID() string {
	if c.consoleOpen {
		return c.consoleNetworkID
	}
	if c.selected != nil {
		return c.selected.NetworkID
	}
	return c.consoleNetworkID
}

// SelectConversation selects one network and target, opening no transcript for
// a conversation that does not exist yet. It mirrors
// IrcController::selectConversation (src/irc/irccontroller.cpp:1068-1105).
func (c *Controller) SelectConversation(networkID, target string) {
	if networkID == "" || target == "" {
		return
	}
	c.consoleNetworkID = networkID
	c.consoleOpen = false

	key := c.reducer.ConversationKey(networkID, target)
	c.selected = &key
	c.selectedTarget = target
	c.reducer.MarkSelected(key)
	c.Publish(selectionNotify())
	c.refreshConnectionStatus()
	c.notifyCapabilitiesChanged()
}

// SelectConversationByID resolves a irc.ConversationID and selects it. It
// reports false when the id is malformed.
func (c *Controller) SelectConversationByID(conversationID string) bool {
	key, ok := irc.ParseConversationID(conversationID)
	if !ok {
		return false
	}
	target := key.NormalizedTarget
	if conversation := c.reducer.Find(key); conversation != nil {
		target = conversation.Target
	}
	c.SelectConversation(key.NetworkID, target)
	return true
}

// OpenStatus opens the Status surface for a network. It mirrors
// IrcController::openStatus (src/irc/irccontroller.cpp:1117-1128).
func (c *Controller) OpenStatus(networkID string) {
	if networkID == "" {
		return
	}
	c.consoleNetworkID = networkID
	c.consoleOpen = true
	c.refreshConnectionStatus()
	c.notifySelectionChanged()
}

// ClearConversationSelection forgets the selection, leaving the Status surface
// as the focused network. It mirrors IrcController::clearConversationSelection
// (src/irc/irccontroller.cpp:1252-1271).
func (c *Controller) ClearConversationSelection() {
	c.selected = nil
	c.selectedTarget = ""
	c.reducer.ClearSelection()
	c.Publish(selectionNotify())
	c.notifyCapabilitiesChanged()
	c.refreshConnectionStatus()
}

// OpenDirectMessage opens or creates a direct message with nick on the selected
// network. It returns false when nothing is selected, nick is empty, or the
// cause may not invent the conversation.
func (c *Controller) OpenDirectMessage(nick string) bool {
	if c.selected == nil || nick == "" {
		return false
	}
	networkID := c.selected.NetworkID
	key := c.reducer.ConversationKey(networkID, nick)
	if c.reducer.EnsureConversation(key, nick, irc.CauseUserOpen) == nil {
		return false
	}
	c.rememberOpenDirect(networkID, nick)
	c.Publish(irc.ViewNotify{Conversations: true})
	c.SelectConversation(networkID, nick)
	return true
}

// CloseDirectMessage drops the selected direct message and selects its
// neighbor. It returns false when a channel or nothing is selected.
func (c *Controller) CloseDirectMessage() bool {
	if c.selected == nil {
		return false
	}
	if conversation := c.reducer.Find(*c.selected); conversation != nil && conversation.IsChannel() {
		return false
	}
	c.dropSelectedDirectAndReselect()
	return true
}

// DropConversationAndReselect removes one conversation and, when it was the
// selected one, selects its sidebar neighbor or clears the selection. It
// mirrors IrcController::dropConversationAndReselect
// (src/irc/irccontroller.cpp:1196-1233).
func (c *Controller) DropConversationAndReselect(key irc.ConversationKey, forgetDirect bool) {
	conversation := c.reducer.Find(key)
	channel := conversation != nil && conversation.IsChannel()
	wasSelected := c.selected != nil && *c.selected == key
	displayTarget := key.NormalizedTarget
	if conversation != nil && conversation.Target != "" {
		displayTarget = conversation.Target
	} else if wasSelected {
		displayTarget = c.selectedTarget
	}

	var nextKey irc.ConversationKey
	hasNext := false
	if wasSelected {
		ordered := irc.SidebarOrderWithNetworks(c.reducer, c.networkOrder)
		if next, ok := irc.NeighborAfterDrop(ordered, key); ok {
			nextKey = next
			hasNext = true
		}
	}

	c.reducer.SetMuted(key, false)
	if forgetDirect {
		c.forgetOpenDirect(key.NetworkID, displayTarget)
	}
	if channel {
		c.reducer.DropChannel(key)
	} else {
		c.reducer.DropDirectMessage(key)
	}
	c.reloadModels()
	if !wasSelected {
		return
	}
	if hasNext {
		target := nextKey.NormalizedTarget
		if neighbor := c.reducer.Find(nextKey); neighbor != nil {
			target = neighbor.Target
		}
		c.SelectConversation(nextKey.NetworkID, target)
		return
	}
	c.ClearConversationSelection()
}

func (c *Controller) dropSelectedDirectAndReselect() {
	if c.selected == nil {
		return
	}
	c.DropConversationAndReselect(*c.selected, true)
}

// --- Send ----------------------------------------------------------------

// SendMessage sends the selected conversation a plain PRIVMSG. Slash commands
// are a Phase 7 concern: no parsing happens here, so a leading slash is sent
// literally. An empty message is refused.
func (c *Controller) SendMessage(text string) bool {
	if strings.TrimSpace(text) == "" {
		return false
	}
	return c.sendSelectedMessage(text)
}

// SendToTarget sends a plain PRIVMSG to an arbitrary network and target. It
// uses the QuietSend cause, so it never invents a conversation. It mirrors
// IrcController::sendToTarget (src/irc/irccontroller.cpp:1291-1329).
func (c *Controller) SendToTarget(networkID, target, text string) bool {
	if networkID == "" || target == "" || text == "" {
		c.setLastError(networkID, "Missing network, target, or text")
		c.notifyStatusChanged()
		return false
	}
	s := c.manager.Find(networkID)
	if s == nil {
		c.setLastError(networkID, "That network is not configured")
		c.notifyStatusChanged()
		return false
	}
	if s.State() != session.StateRegistered {
		c.setLastError(networkID, "Not connected")
		c.notifyStatusChanged()
		return false
	}
	if !s.SendPrivmsg(target, text) {
		c.setLastError(networkID, "Failed to send message")
		c.notifyStatusChanged()
		return false
	}
	c.rememberOpenDirect(networkID, target)
	c.reducer.EnsureConversation(c.reducer.ConversationKey(networkID, target), target, irc.CauseQuietSend)
	c.echoIfPresent(s, target, text)
	c.setLastError(networkID, "")
	c.notifyStatusChanged()
	return true
}

func (c *Controller) sendSelectedMessage(body string) bool {
	if c.selected == nil || body == "" {
		return false
	}
	s := c.manager.Find(c.selected.NetworkID)
	if s == nil {
		return false
	}
	target := c.SelectedTarget()
	if !s.SendPrivmsg(target, body) {
		return false
	}
	c.rememberOpenDirect(s.NetworkID(), target)
	c.echoLocal(irc.KindMessage, body)
	return true
}

// echoLocal folds the user's own line into the selected transcript, unless the
// server already echoes it. It mirrors IrcController::echoLocal
// (src/irc/irccontroller.cpp:1988-2003).
func (c *Controller) echoLocal(kind irc.MessageKind, body string) {
	if c.selected == nil || body == "" {
		return
	}
	if c.capabilities[c.selected.NetworkID].Contains(irc.CapabilityEchoMessage) {
		return
	}
	target := c.SelectedTarget()
	now := c.now()
	nick := c.CurrentNick()
	if kind == irc.KindAction {
		c.Apply(irc.ActionEvent{Conversation: *c.selected, Author: nick, Body: body, Timestamp: now, Target: target})
		return
	}
	c.Apply(irc.MessageEvent{Conversation: *c.selected, Author: nick, Body: body, Timestamp: now, Target: target})
}

// echoIfPresent echoes an existing conversation's own line. It mirrors
// IrcController::echoIfPresent (src/irc/irccontroller.cpp:1931-1952): a
// `notice` wire echoes as NOTICE, and an echo-message network gets no local
// copy at all.
func (c *Controller) echoIfPresent(s *session.Session, target, body string) {
	if c.capabilities[s.NetworkID()].Contains(irc.CapabilityEchoMessage) {
		return
	}
	key := c.reducer.ConversationKey(s.NetworkID(), target)
	if c.reducer.Find(key) == nil {
		return
	}
	c.Apply(irc.MessageEvent{Conversation: key, Author: s.Nick(), Body: body, Timestamp: c.now(), Target: target})
}

// --- Status and identity --------------------------------------------------

// ConnectionStatus returns the focused network's connection status.
func (c *Controller) ConnectionStatus() string {
	return c.ConnectionStatusFor(c.FocusedNetworkID())
}

// ConnectionStatusFor returns one network's connection status. A network with
// no session is "Offline"; an empty id returns the last focused status.
func (c *Controller) ConnectionStatusFor(networkID string) string {
	if s := c.manager.Find(networkID); s != nil {
		return stateText(s.State())
	}
	if networkID == "" {
		return c.connectionStatus
	}
	return "Offline"
}

// LastError returns the focused network's last error, or "".
func (c *Controller) LastError() string {
	return c.LastErrorFor(c.FocusedNetworkID())
}

// LastErrorFor returns one network's last error, or "".
func (c *Controller) LastErrorFor(networkID string) string {
	return c.lastErrors[networkID]
}

// CurrentNick returns the focused network's current nick, or "".
func (c *Controller) CurrentNick() string {
	return c.currentNicks[c.FocusedNetworkID()]
}

// SelfAway reports whether the focused network has us marked away.
func (c *Controller) SelfAway() bool {
	networkID := c.FocusedNetworkID()
	return networkID != "" && c.reducer.SelfAway(networkID)
}

// Topic returns the selected conversation's topic, or a direct-message caption.
func (c *Controller) Topic() string {
	if c.selected == nil {
		return ""
	}
	conversation := c.reducer.Find(*c.selected)
	if conversation == nil {
		if c.IsChannel() {
			return ""
		}
		return "Direct message"
	}
	if channel := conversation.Channel(); channel != nil {
		return channel.Topic
	}
	return "Direct message with " + conversation.Target
}

// IsChannel reports whether the selected target is a channel under the
// network's advertised CHANTYPES.
func (c *Controller) IsChannel() bool {
	if c.selected == nil {
		return false
	}
	features := c.reducer.ServerFeatures(c.selected.NetworkID)
	return features.IsChannel(c.SelectedTarget())
}

// PeopleCount returns the selected channel's member count, or 0.
func (c *Controller) PeopleCount() int {
	if c.selected == nil {
		return 0
	}
	return len(c.reducer.OrderedMembers(*c.selected))
}

// TypingNicks returns the display nicks currently typing in the selected
// conversation.
func (c *Controller) TypingNicks() []string {
	if c.selected == nil {
		return nil
	}
	return c.reducer.TypingNicks(*c.selected, c.now())
}

// HasAwayPresence reports whether the selected network negotiated
// away-notify.
func (c *Controller) HasAwayPresence() bool {
	return c.selected != nil &&
		c.capabilities[c.selected.NetworkID].Contains(irc.CapabilityAwayNotify)
}

// HasMemberStatus reports whether the selected network negotiated both
// draft/metadata-2 and batch.
func (c *Controller) HasMemberStatus() bool {
	if c.selected == nil {
		return false
	}
	caps := c.capabilities[c.selected.NetworkID]
	return caps.Contains(irc.CapabilityMemberMetadata) && caps.Contains(irc.CapabilityBatch)
}

// HasTyping reports whether the selected network negotiated message-tags.
func (c *Controller) HasTyping() bool {
	return c.selected != nil &&
		c.capabilities[c.selected.NetworkID].Contains(irc.CapabilityMessageTags)
}

// UnreadCountFor returns the summed unread count across one network's
// conversations.
func (c *Controller) UnreadCountFor(networkID string) int {
	if networkID == "" {
		return 0
	}
	total := 0
	for key, conversation := range c.reducer.Conversations() {
		if key.NetworkID == networkID {
			total += conversation.Unread
		}
	}
	return total
}

// MentionFor reports whether any conversation on one network has an unread
// mention.
func (c *Controller) MentionFor(networkID string) bool {
	if networkID == "" {
		return false
	}
	for key, conversation := range c.reducer.Conversations() {
		if key.NetworkID == networkID && conversation.Mentions > 0 {
			return true
		}
	}
	return false
}

// SetWindowActive records focus. Gaining focus consumes the selected
// conversation's unread; the "new messages" mark survives. It mirrors
// IrcController::setWindowActive (src/irc/irccontroller.cpp:800-817).
func (c *Controller) SetWindowActive(active bool) {
	c.reducer.SetWindowActive(active)
	if !active || c.selected == nil {
		return
	}
	if !c.reducer.MarkRead(*c.selected) {
		return
	}
	c.Publish(irc.ViewNotify{Conversations: true})
}

// SetMuted records the muted flag for one conversation. Persistence is the
// Phase 8 MuteStore seam.
func (c *Controller) SetMuted(networkID, target string, muted bool) {
	if networkID == "" || target == "" {
		return
	}
	c.reducer.SetMuted(c.reducer.ConversationKey(networkID, target), muted)
	c.Publish(irc.ViewNotify{Conversations: true})
}

// --- Lifecycle ------------------------------------------------------------

// Start activates one network's session and reports whether it ended up live.
// A missing network sets its last error. It mirrors IrcController::start
// (src/irc/irccontroller.cpp:1051-1065).
func (c *Controller) Start(networkID string) bool {
	if c.manager.Find(networkID) == nil {
		c.setLastError(networkID, "That network is not configured")
		c.notifyStatusChanged()
		return false
	}
	c.setLastError(networkID, "")
	started := c.manager.Activate(networkID)
	c.refreshConnectionStatus()
	return started
}

// AddSession creates and registers a session for config, wires this controller
// as its handler, and records the configured nick. A nil clock falls back to
// the controller's clock.
func (c *Controller) AddSession(config session.SessionConfig, transport session.Transport, clock session.Clock) (*session.Session, error) {
	if clock == nil {
		clock = c.clock
	}
	s, err := c.manager.Create(config, transport, clock)
	if err != nil {
		return nil, err
	}
	c.currentNicks[config.NetworkID] = config.Nick
	s.SetHandler(c)
	return s, nil
}

// DiscardSession stops and unregisters one network's session. It reports false
// when there is no such session. It mirrors IrcController::discardSession
// (src/irc/irccontroller.cpp:492-514).
func (c *Controller) DiscardSession(networkID string) bool {
	if c.manager.Find(networkID) == nil {
		return false
	}
	c.reducer.Apply(irc.SelfAwayEvent{NetworkID: networkID, Away: false}, c.now())
	delete(c.currentNicks, networkID)
	delete(c.capabilities, networkID)
	c.statusConsole.Clear(networkID)
	c.manager.Discard(networkID)
	c.notifyCapabilitiesChanged()
	c.notifyStatusChanged()
	return true
}

// ForgetNetworkState drops every controller and reducer fact about a network
// while keeping its session. It mirrors IrcController::forgetNetworkState
// (src/irc/irccontroller.cpp:517-557).
func (c *Controller) ForgetNetworkState(networkID string) {
	if networkID == "" {
		return
	}
	c.reducer.ForgetNetwork(networkID)
	delete(c.currentNicks, networkID)
	delete(c.capabilities, networkID)
	delete(c.lastErrors, networkID)
	if c.selected != nil && c.selected.NetworkID == networkID {
		c.ClearConversationSelection()
	}
	c.reloadModels()
	c.notifyCapabilitiesChanged()
	c.notifyStatusChanged()
}

// SetNetworkOrder records the network display order and republishes the
// sidebar.
func (c *Controller) SetNetworkOrder(order []string) {
	if stringSlicesEqual(c.networkOrder, order) {
		return
	}
	c.networkOrder = append([]string(nil), order...)
	c.Publish(irc.ViewNotify{Conversations: true})
}

// --- Message handling -----------------------------------------------------

func (c *Controller) handleMessage(networkID string, message irc.Message) {
	switch message.Command {
	case "005":
		if len(message.Params) > 2 {
			features := c.reducer.ServerFeatures(networkID)
			last := len(message.Params) - 1
			for index := 1; index < last; index++ {
				features.ApplyToken(irc.WireText([]byte(message.Params[index])))
			}
			c.reducer.SetServerFeatures(networkID, features)
		}
		return
	case "376", "422":
		// A burst of 005 tokens is applied above without touching the models;
		// MOTD end only latches the case mapping. Restoring open directs is
		// Phase 9.
		features := c.reducer.ServerFeatures(networkID)
		if !features.CaseMappingKnown() {
			features.MarkCaseMappingKnown()
			c.reducer.SetServerFeatures(networkID, features)
		}
	}

	features := c.reducer.ServerFeatures(networkID)
	currentNick := c.currentNicks[networkID]
	for _, event := range irc.Translate(networkID, currentNick, features, message, c.now()) {
		if nick, ok := event.(irc.NickEvent); ok {
			if features.CaseMapping().Equals(nick.OldNick, currentNick) {
				c.currentNicks[networkID] = nick.NewNick
			}
		}
		c.Apply(event)
	}
}

func (c *Controller) handleCapabilities(networkID string, capabilities irc.CapabilitySet) {
	previous := c.capabilities[networkID]
	c.capabilities[networkID] = capabilities
	dropped := func(capability irc.Capability) bool {
		return previous.Contains(capability) && !capabilities.Contains(capability)
	}
	awayDropped := dropped(irc.CapabilityAwayNotify)
	metadataDropped := dropped(irc.CapabilityMemberMetadata) || dropped(irc.CapabilityBatch)
	if awayDropped || metadataDropped {
		c.reducer.ClearPresenceFacts(networkID, awayDropped, metadataDropped)
		if metadataDropped {
			c.peerMetadataEpoch++
		}
		c.reloadModels()
	}
	if dropped(irc.CapabilityMessageTags) {
		c.reducer.ClearTypingFacts(networkID)
	}
	c.notifyCapabilitiesChanged()
}

// noteKeptReplay drains the replay/remembered-query hand-offs the reducer
// produces. Their consumers are the Phase 11 PlaybackCoordinator and the
// Phase 9 open-direct store, so Phase 2 only keeps the lists bounded.
func (c *Controller) noteKeptReplay() {
	c.reducer.TakeKeptReplay()
	c.reducer.TakeRememberedQueries()
}

// --- Open-direct and mute seams (deferred) --------------------------------

// rememberOpenDirect records a direct message worth restoring. Phase 9 owns the
// IrcOpenDirectStore equivalent; Phase 2 keeps nothing.
func (c *Controller) rememberOpenDirect(networkID, target string) {}

// forgetOpenDirect drops a remembered direct message. Phase 9 owns the store.
func (c *Controller) forgetOpenDirect(networkID, target string) {}

// --- Status callbacks -----------------------------------------------------

func (c *Controller) refreshConnectionStatus() {
	if s := c.manager.Find(c.FocusedNetworkID()); s != nil {
		c.connectionStatus = stateText(s.State())
	}
	c.notifyStatusChanged()
}

func (c *Controller) setLastError(networkID, message string) {
	if message == "" {
		delete(c.lastErrors, networkID)
		return
	}
	c.lastErrors[networkID] = message
}

func (c *Controller) notifySelectionChanged() {
	if c.OnSelectionChanged != nil {
		c.OnSelectionChanged()
	}
}

func (c *Controller) notifyStatusChanged() {
	if c.OnStatusChanged != nil {
		c.OnStatusChanged()
	}
}

func (c *Controller) notifyCapabilitiesChanged() {
	if c.OnCapabilitiesChanged != nil {
		c.OnCapabilitiesChanged()
	}
}

// --- Helpers --------------------------------------------------------------

// stateText maps a session state to the footer text. It mirrors the anonymous
// stateText in src/irc/irccontroller.cpp:177-198.
func stateText(state session.SessionState) string {
	switch state {
	case session.StateIdle, session.StateFailed:
		return "Offline"
	case session.StateConnecting, session.StateStsUpgrading, session.StateCapLs,
		session.StateCapReq, session.StateSasl, session.StateRegistering:
		return "Connecting"
	case session.StateRegistered:
		return "Connected"
	case session.StateClosing:
		return "Disconnecting"
	case session.StateReconnecting:
		return "Reconnecting"
	}
	return "Offline"
}

// accountsMoved reports whether an event can move an account fact. It mirrors
// the accountsMoved ladder in IrcController::apply.
func accountsMoved(kind irc.EventKind, event irc.Event) bool {
	switch kind {
	case irc.EventAccount, irc.EventWelcome, irc.EventPart, irc.EventQuit,
		irc.EventKick, irc.EventNick:
		return true
	case irc.EventJoin:
		join, ok := event.(irc.JoinEvent)
		return ok && join.Account != nil
	}
	return false
}

// servicesAccountMatches compares two account values with empty and "*"
// treated as the same "no account". It mirrors servicesAccountMatches in
// src/irc/irccontroller.cpp:200-206.
func servicesAccountMatches(stored, incoming string) bool {
	return canonicalAccount(stored) == canonicalAccount(incoming)
}

func canonicalAccount(value string) string {
	if value == "" || value == "*" {
		return ""
	}
	return value
}

// firstConversation picks the (network, normalized target) minimum so the
// auto-selection matches the reducer's std::map begin().
func firstConversation(reducer *irc.EventReducer) (irc.ConversationKey, *irc.ConversationState) {
	var bestKey irc.ConversationKey
	var best *irc.ConversationState
	first := true
	for key, conversation := range reducer.Conversations() {
		if first || conversationKeyLess(key, bestKey) {
			bestKey, best, first = key, conversation, false
		}
	}
	return bestKey, best
}

func conversationKeyLess(left, right irc.ConversationKey) bool {
	if left.NetworkID != right.NetworkID {
		return left.NetworkID < right.NetworkID
	}
	return left.NormalizedTarget < right.NormalizedTarget
}

func stringSlicesEqual(left, right []string) bool {
	if len(left) != len(right) {
		return false
	}
	for index := range left {
		if left[index] != right[index] {
			return false
		}
	}
	return true
}
