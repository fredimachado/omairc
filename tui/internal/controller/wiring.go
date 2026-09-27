package controller

import (
	"strconv"
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// This file wires the Phase 7 slash subsystems into the controller. The
// subsystems own their decisions (commanddispatcher.go, replyrouter.go,
// monitorcoordinator.go, autoawayruntime.go, channellist.go); the controller
// owns the side effects they cannot: session and selection lookup, model
// application, the Status console, and the stores. The adapters below are the
// Go equivalent of the C++ Host callback structs.

// --- Host adapters --------------------------------------------------------

// ctrlHost implements CommandHost, ReplyHost, and MonitorHost over one
// controller. AutoawayHost's RecordStatus(networkID, text) collides with the
// StatusEntry variant, so the autoaway runtime gets autoawayAdapter instead.
type ctrlHost struct{ c *Controller }

func (h ctrlHost) QueryNetworkID(surface irc.ComposerSurface) string {
	return h.c.queryNetworkID(surface)
}

func (h ctrlHost) SessionFor(surface irc.ComposerSurface) *session.Session {
	return h.c.sessionFor(surface)
}

func (h ctrlHost) SessionForNetwork(networkID string) *session.Session {
	return h.c.sessionForNetwork(networkID)
}

func (h ctrlHost) SelectedSession() *session.Session { return h.c.selectedSession() }

func (h ctrlHost) SelectedTarget() string { return h.c.SelectedTarget() }

func (h ctrlHost) SelectedIsChannel() bool { return h.c.IsChannel() }

func (h ctrlHost) SelectedIsCloseableDirect() bool { return h.c.selectedIsCloseableDirect() }

func (h ctrlHost) SelectedKey() (irc.ConversationKey, bool) { return h.c.selectedKey() }

func (h ctrlHost) HasNetworks() bool { return h.c.hasNetworks() }

func (h ctrlHost) StatusNetworkID() string { return h.c.consoleNetworkID }

func (h ctrlHost) EchoLocal(kind irc.MessageKind, body string) { h.c.echoLocal(kind, body) }

func (h ctrlHost) OpenJoinedChannel(networkID, channel string) {
	h.c.openJoinedChannel(networkID, channel)
}

func (h ctrlHost) DismissChannel(networkID, channel string) bool {
	return h.c.dismissChannel(networkID, channel)
}

func (h ctrlHost) DropSelectedDirectAndReselect() { h.c.dropSelectedDirectAndReselect() }

func (h ctrlHost) ClearSurface(surface irc.ComposerSurface) irc.CommandOutcome {
	return h.c.clearSurface(surface)
}

func (h ctrlHost) RememberOpenDirect(networkID, target string) {
	h.c.rememberOpenDirect(networkID, target)
}

func (h ctrlHost) NoteNickDelivery(networkID, target string) {
	h.c.replies.NoteNickDelivery(networkID, target)
}

func (h ctrlHost) NoteLocalActivity() { h.c.autoaway.NoteLocalActivity() }

func (h ctrlHost) UnawayAfterChat(s *session.Session) { h.c.unawayAfterChat(s) }

func (h ctrlHost) NoteManualAway(networkID string) { h.c.autoaway.NoteManualAway(networkID) }

func (h ctrlHost) NoteAwayCleared(networkID string) { h.c.autoaway.NoteAwayCleared(networkID) }

func (h ctrlHost) ClearTypingTarget() { h.c.typingTarget = "" }

func (h ctrlHost) SendSelectedMessage(body string) irc.CommandOutcome {
	return h.c.sendSelectedMessageOutcome(body)
}

func (h ctrlHost) EchoIfPresent(s *session.Session, target, body string, wire QuietWire) {
	h.c.echoIfPresentWire(s, target, body, wire)
}

func (h ctrlHost) ApplyMute(networkID, target string, muted bool) bool {
	return h.c.applyMute(networkID, target, muted)
}

func (h ctrlHost) SyncHighlightWords(networkID string) { h.c.syncHighlightWords(networkID) }

func (h ctrlHost) ReloadConversations() { h.c.Publish(irc.ViewNotify{Conversations: true}) }

func (h ctrlHost) SelectConversation(networkID, target string) {
	h.c.SelectConversation(networkID, target)
}

func (h ctrlHost) ApplyWhoisTranscript(event irc.WhoisTranscriptEvent) { h.c.Apply(event) }

func (h ctrlHost) RecordStatus(entry irc.StatusEntry) { h.c.statusConsole.Append(entry) }

func (h ctrlHost) DispatchList(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	return h.c.dispatchList(command, surface)
}

func (h ctrlHost) DispatchAutoaway(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	return h.c.autoaway.DispatchAutoaway(command, surface)
}

func (h ctrlHost) DispatchWhois(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	return h.c.replies.DispatchWhois(command, surface)
}

func (h ctrlHost) DispatchCtcp(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	return h.c.replies.DispatchCtcp(command, surface)
}

func (h ctrlHost) DispatchStatus(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	return h.c.replies.DispatchStatus(command, surface)
}

func (h ctrlHost) DispatchAvatar(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	return h.c.replies.DispatchAvatar(command, surface)
}

func (h ctrlHost) DispatchMonitor(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	return h.c.monitor.DispatchMonitor(command, surface)
}

func (h ctrlHost) PrefEnabled(name irc.PrefName) bool { return h.c.prefs.Enabled(name) }

func (h ctrlHost) PrefApply(name irc.PrefName, enabled bool) { h.c.prefs.SetEnabled(name, enabled) }

// ReplyHost extras.
func (h ctrlHost) SelfNick(networkID string) string { return h.c.currentNicks[networkID] }

func (h ctrlHost) PersistAvatarURL(networkID, url string) { h.c.avatars.SetURL(networkID, url) }

func (h ctrlHost) Capabilities(networkID string) irc.CapabilitySet {
	return h.c.capabilities[networkID]
}

// MonitorHost extras.
func (h ctrlHost) NotifyMonitor(networkID, display, body string, newlyOnline bool) {
	if h.c.OnMonitorArrived != nil {
		h.c.OnMonitorArrived(networkID, display, body, newlyOnline)
	}
}

// AutoawayHost-only surface.
func (h ctrlHost) FindSession(networkID string) *session.Session {
	return h.c.manager.Find(networkID)
}

func (h ctrlHost) NetworkIDs() []string { return h.c.NetworkIDs() }

func (h ctrlHost) SelfAway(networkID string) bool { return h.c.reducer.SelfAway(networkID) }

func (h ctrlHost) MarkUnawaySent(networkID string) { h.c.unawaySent[networkID] = true }

func (h ctrlHost) ConsoleNetworkID() string { return h.c.consoleNetworkID }

func (h ctrlHost) ApplyEvent(event irc.Event) { h.c.Apply(event) }

func (h ctrlHost) Now() time.Time { return h.c.now() }

// autoawayAdapter shadows ctrlHost.RecordStatus(entry) with the text variant
// AutoawayHost declares.
type autoawayAdapter struct{ ctrlHost }

func (h autoawayAdapter) RecordStatus(networkID, text string) {
	if networkID == "" || text == "" {
		return
	}
	h.c.statusConsole.Append(irc.Outcome(networkID, text, h.c.now()))
}

// initSlashSubsystems builds the Phase 7 subsystems. It runs once from New,
// after the clock is set.
func (c *Controller) initSlashSubsystems() {
	c.ignores = NewIgnoreStore()
	c.mutes = NewMuteStore()
	c.highlights = NewHighlightStore()
	c.monitors = NewMonitorStore()
	c.avatars = NewAvatarStore()
	c.prefs = NewPreferences()
	c.unawaySent = make(map[string]bool)

	host := ctrlHost{c: c}
	c.commands = NewCommandDispatcher(c.reducer, c.ignores, c.mutes, c.highlights, host)
	c.replies = NewReplyRouter(c.reducer, host)
	c.monitor = NewMonitorCoordinator(c.reducer, c.monitors, c.mutes, host)
	c.autoaway = NewAutoawayRuntime(autoawayAdapter{host}, c.clock)
	c.channelLists = NewChannelListRequest(c.clock)
	c.channelList = NewChannelListModel()
}

// rebindClocks recreates the subsystems that hold a clock, so SetClock lands
// after construction. The session manager owns per-session clocks, so only the
// channel-list timeout and the auto-away timers are rebound here.
func (c *Controller) rebindClocks() {
	if c.channelLists != nil {
		c.channelLists = NewChannelListRequest(c.clock)
	}
	if c.autoaway != nil {
		c.autoaway = NewAutoawayRuntime(autoawayAdapter{ctrlHost{c: c}}, c.clock)
	}
}

// --- Selection and session lookup -----------------------------------------

func (c *Controller) queryNetworkID(surface irc.ComposerSurface) string {
	if surface == irc.SurfaceStatus {
		return c.consoleNetworkID
	}
	if c.selected != nil {
		return c.selected.NetworkID
	}
	return ""
}

func (c *Controller) sessionFor(surface irc.ComposerSurface) *session.Session {
	return c.manager.Find(c.queryNetworkID(surface))
}

func (c *Controller) sessionForNetwork(networkID string) *session.Session {
	return c.manager.Find(networkID)
}

func (c *Controller) selectedSession() *session.Session {
	if c.selected == nil {
		return nil
	}
	return c.manager.Find(c.selected.NetworkID)
}

func (c *Controller) selectedKey() (irc.ConversationKey, bool) {
	if c.selected == nil {
		return irc.ConversationKey{}, false
	}
	return *c.selected, true
}

func (c *Controller) hasNetworks() bool { return len(c.manager.NetworkIDs()) > 0 }

func (c *Controller) selectedIsCloseableDirect() bool {
	if c.selected == nil {
		return false
	}
	conversation := c.reducer.Find(*c.selected)
	return conversation == nil || !conversation.IsChannel()
}

// --- Send and submit ------------------------------------------------------

// SendMessage parses one composer submission and dispatches it on the
// conversation surface. It reports whether the command was sent: a refused
// command keeps its text in the composer. It mirrors
// IrcController::sendMessage.
func (c *Controller) SendMessage(text string) bool {
	command := irc.ParseCommand(text)
	if command.Verb == irc.VerbEmpty {
		return false
	}
	return c.report(c.commands.Dispatch(command, irc.SurfaceConversation), command,
		irc.SurfaceConversation)
}

// ConsoleSubmit parses one Status-console submission and dispatches it on the
// Status surface. A non-empty command is always accepted (the composer clears)
// and a failure is logged on Status, mirroring IrcStatusConsole::submit.
func (c *Controller) ConsoleSubmit(text string) bool {
	command := irc.ParseCommand(text)
	if command.Verb == irc.VerbEmpty {
		return false
	}
	outcome := c.commands.Dispatch(command, irc.SurfaceStatus)
	if outcome != irc.OutcomeSent {
		if c.consoleNetworkID != "" {
			c.statusConsole.Append(irc.Outcome(c.consoleNetworkID,
				irc.CommandOutcomeText(outcome, command), c.now()))
		}
		c.notifyStatusChanged()
	}
	return true
}

// report records the outcome text on the focused network and reports whether
// the command was sent. It mirrors IrcController::report.
func (c *Controller) report(outcome irc.CommandOutcome, command irc.Command,
	surface irc.ComposerSurface) bool {
	networkID := c.errorNetworkID(surface)
	if outcome == irc.OutcomeSent {
		c.setLastError(networkID, "")
	} else {
		c.setLastError(networkID, irc.CommandOutcomeText(outcome, command))
	}
	c.notifyStatusChanged()
	return outcome == irc.OutcomeSent
}

func (c *Controller) errorNetworkID(surface irc.ComposerSurface) string {
	if surface == irc.SurfaceStatus {
		return c.consoleNetworkID
	}
	if c.selected != nil {
		return c.selected.NetworkID
	}
	return c.consoleNetworkID
}

func (c *Controller) sendSelectedMessageOutcome(body string) irc.CommandOutcome {
	s := c.selectedSession()
	if s == nil || c.selected == nil {
		return irc.OutcomeWrongScope
	}
	target := c.SelectedTarget()
	if !s.SendPrivmsg(target, body) {
		return irc.OutcomeRefused
	}
	c.rememberOpenDirect(s.NetworkID(), target)
	c.replies.NoteNickDelivery(s.NetworkID(), target)
	c.echoLocal(irc.KindMessage, body)
	c.typingTarget = ""
	c.autoaway.NoteLocalActivity()
	c.unawayAfterChat(s)
	return irc.OutcomeSent
}

// echoIfPresentWire echoes an existing conversation's own line for a quiet
// send. It mirrors IrcController::echoIfPresent.
func (c *Controller) echoIfPresentWire(s *session.Session, target, body string, wire QuietWire) {
	if s == nil {
		return
	}
	if wire == QuietWirePrivmsg && c.capabilities[s.NetworkID()].Contains(irc.CapabilityEchoMessage) {
		return
	}
	key := c.reducer.ConversationKey(s.NetworkID(), target)
	if c.reducer.Find(key) == nil {
		return
	}
	if wire == QuietWireNotice {
		c.Apply(irc.NoticeEvent{Conversation: key, Author: s.Nick(), Body: body,
			Timestamp: c.now(), Target: target})
		return
	}
	c.Apply(irc.MessageEvent{Conversation: key, Author: s.Nick(), Body: body,
		Timestamp: c.now(), Target: target})
}

func (c *Controller) unawayAfterChat(s *session.Session) {
	if s == nil {
		return
	}
	networkID := s.NetworkID()
	if !c.reducer.SelfAway(networkID) || c.unawaySent[networkID] {
		return
	}
	if s.ClearAway() {
		c.unawaySent[networkID] = true
		c.autoaway.NoteAwayCleared(networkID)
	}
}

// --- Conversation effects -------------------------------------------------

func (c *Controller) openJoinedChannel(networkID, channel string) {
	if networkID == "" || channel == "" {
		return
	}
	key := c.reducer.ConversationKey(networkID, channel)
	if c.reducer.EnsureConversation(key, channel, irc.CauseChannelState) == nil {
		return
	}
	c.Publish(irc.ViewNotify{Conversations: true})
	c.SelectConversation(networkID, channel)
}

func (c *Controller) dismissChannel(networkID, channel string) bool {
	if networkID == "" || channel == "" {
		return false
	}
	key := c.reducer.ConversationKey(networkID, channel)
	conversation := c.reducer.Find(key)
	if conversation == nil || !conversation.IsChannel() {
		return false
	}
	c.DropConversationAndReselect(key, false)
	return true
}

func (c *Controller) clearSurface(surface irc.ComposerSurface) irc.CommandOutcome {
	if surface == irc.SurfaceStatus {
		if c.consoleNetworkID == "" {
			return irc.OutcomeRefused
		}
		c.statusConsole.Clear(c.consoleNetworkID)
		return irc.OutcomeSent
	}
	if c.selected == nil {
		return irc.OutcomeWrongScope
	}
	c.reducer.ClearMessages(*c.selected)
	c.Publish(irc.ViewNotify{Messages: true})
	return irc.OutcomeSent
}

// applyMute records a /mute or /unmute in the store and the reducer. It
// mirrors IrcController::applyMute.
func (c *Controller) applyMute(networkID, target string, muted bool) bool {
	if networkID == "" || target == "" {
		return false
	}
	features := c.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	var changed bool
	if muted {
		changed = c.mutes.Add(networkID, target, mapping)
	} else {
		changed = c.mutes.Remove(networkID, target, mapping)
	}
	c.reducer.SetMuted(c.reducer.ConversationKey(networkID, target), muted)
	c.Publish(irc.ViewNotify{Conversations: true})
	return changed
}

// syncHighlightWords feeds the store's words into the reducer. It mirrors
// IrcController::syncHighlightWords.
func (c *Controller) syncHighlightWords(networkID string) {
	if networkID == "" {
		return
	}
	c.reducer.SetHighlightWords(networkID, c.highlights.Words(networkID))
}

// hydrateMutes folds the stored mute list into the reducer's muted flags. It
// mirrors IrcController::hydrateMutes.
func (c *Controller) hydrateMutes(networkID string) {
	for _, target := range c.mutes.Targets(networkID) {
		c.reducer.SetMuted(c.reducer.ConversationKey(networkID, target), true)
	}
}

// --- Channel list ---------------------------------------------------------

// ChannelListSnapshot is the /list overlay's view state.
type ChannelListSnapshot struct {
	Open      bool
	NetworkID string
	Mask      string
	Rows      []ChannelListRow
	Loading   bool
	Complete  bool
	Cached    bool
	Error     string
	Filter    string
}

// ChannelListSnapshot returns the channel-list overlay state for the shell.
func (c *Controller) ChannelListSnapshot() ChannelListSnapshot {
	if c.channelList == nil {
		return ChannelListSnapshot{}
	}
	state := c.channelList.State()
	return ChannelListSnapshot{
		Open:      c.channelListOpen,
		NetworkID: state.NetworkID,
		Mask:      state.Mask,
		Rows:      state.Rows,
		Loading:   state.Loading,
		Complete:  state.Complete,
		Cached:    state.Cached,
		Error:     state.Error,
		Filter:    state.Filter,
	}
}

// SetChannelListFilter sets the overlay's fuzzy filter.
func (c *Controller) SetChannelListFilter(filter string) {
	if c.channelList != nil {
		c.channelList.SetFilter(filter)
	}
}

// PlainChannelTopic strips mIRC colors and formatting from a /list topic for
// display. It mirrors ChannelListModel::plainTopic.
func (c *Controller) PlainChannelTopic(topic string) string {
	if c.channelList == nil {
		return topic
	}
	return c.channelList.PlainTopic(topic)
}

// DismissChannelList closes the overlay, keeping the cached rows.
func (c *Controller) DismissChannelList() { c.channelListOpen = false }

// JoinChannelListRow opens the overlay's row: it focuses an already-joined
// channel, else joins it. It reports whether an action was taken.
func (c *Controller) JoinChannelListRow(row int) bool {
	if c.channelList == nil {
		return false
	}
	snapshot := c.ChannelListSnapshot()
	if row < 0 || row >= len(snapshot.Rows) {
		return false
	}
	channel := snapshot.Rows[row].Channel
	networkID := snapshot.NetworkID
	if networkID == "" || channel == "" {
		return false
	}
	key := c.reducer.ConversationKey(networkID, channel)
	if conversation := c.reducer.Find(key); conversation != nil &&
		conversation.IsChannel() && conversation.Channel().Joined {
		c.SelectConversation(networkID, channel)
		return true
	}
	surface := irc.SurfaceConversation
	if c.consoleOpen {
		surface = irc.SurfaceStatus
	}
	return c.commands.Dispatch(irc.ParseCommand("/join "+channel), surface) == irc.OutcomeSent
}

// dispatchList handles /list. It mirrors IrcController::dispatchList.
func (c *Controller) dispatchList(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	networkID := c.queryNetworkID(surface)
	if networkID == "" {
		if surface == irc.SurfaceConversation && c.selected == nil {
			return irc.OutcomeWrongScope
		}
		return irc.OutcomeRefused
	}
	s := c.sessionForNetwork(networkID)
	if s == nil || s.State() != session.StateRegistered {
		return irc.OutcomeNotConnected
	}

	mask := strings.TrimSpace(command.Argument)
	found, ok := c.channelLists.State(networkID)
	sameMask := ok && SameChannelListMask(found.Mask, mask)
	current := c.ChannelListSnapshot()
	refresh := current.Open && current.NetworkID == networkID && sameMask

	request := c.channelLists.Request(networkID, mask, refresh)
	found, _ = c.channelLists.State(networkID)
	switch request {
	case ChannelListLoading:
		c.channelList.Show(networkID, found.Mask, found.Rows, false, true, false, found.Error)
		c.channelListOpen = true
		return irc.OutcomeSent
	case ChannelListCached:
		c.channelList.Show(networkID, found.Mask, found.Rows, true, false, true, "")
		c.channelListOpen = true
		return irc.OutcomeSent
	}

	if !c.beginChannelListLoad(s, networkID, mask) {
		return irc.OutcomeRefused
	}
	return irc.OutcomeSent
}

func (c *Controller) beginChannelListLoad(s *session.Session, networkID, mask string) bool {
	if s == nil {
		return false
	}
	c.channelLists.Request(networkID, mask, true)
	c.channelList.BeginLoad(networkID, mask)
	c.channelListOpen = true
	if !s.List(mask) {
		c.channelLists.CancelStart(networkID)
		c.channelList.Clear()
		c.channelListOpen = false
		return false
	}
	return true
}

func (c *Controller) finishChannelList(networkID string) {
	result := c.channelLists.Finish(networkID)
	if !result.Completed {
		return
	}
	cache, ok := c.channelLists.State(networkID)
	if !ok || cache.Loading {
		return
	}
	if result.HasPending {
		s := c.sessionForNetwork(networkID)
		if s != nil && s.State() == session.StateRegistered {
			if c.beginChannelListLoad(s, networkID, result.PendingMask) {
				return
			}
		}
	}
	if c.channelList != nil {
		current := c.ChannelListSnapshot()
		if current.NetworkID == networkID {
			c.channelList.Show(networkID, cache.Mask, cache.Rows, true, false, false, "")
		}
	}
}

// failChannelListFromNumeric fails an in-flight /list from 263, 416, or a
// 4xx/5xx numeric that mentions LIST. It mirrors
// IrcController::failChannelListFromNumeric.
func (c *Controller) failChannelListFromNumeric(networkID string, message irc.Message) bool {
	found, ok := c.channelLists.State(networkID)
	if !ok || !found.Loading {
		return false
	}
	if len(message.Command) != 3 {
		return false
	}
	tryAgain := message.Command == "263"
	tooMany := message.Command == "416"
	errorNumeric := message.Command[0] == '4' || message.Command[0] == '5'
	if !tryAgain && !tooMany && !errorNumeric {
		return false
	}
	mentionsList := false
	for index := 0; index < len(message.Params); index++ {
		if strings.EqualFold(parameterText(message, index), "LIST") {
			mentionsList = true
			break
		}
	}
	if tryAgain && !mentionsList {
		return false
	}
	if errorNumeric && !tooMany && !mentionsList {
		return false
	}
	text := ""
	if len(message.Params) > 0 {
		text = parameterText(message, len(message.Params)-1)
	}
	if text == "" {
		switch {
		case tryAgain:
			text = "Server load is too heavy. Try /list again."
		case tooMany:
			text = "Too many channel matches. Try a narrower /list."
		default:
			text = "Channel list failed. Try /list again."
		}
	}
	return c.failChannelList(networkID, text)
}

// failChannelList fails the request and the overlay together.
func (c *Controller) failChannelList(networkID, text string) bool {
	if !c.channelLists.Fail(networkID, text, false) {
		return false
	}
	if c.channelList != nil {
		current := c.ChannelListSnapshot()
		if current.NetworkID == networkID {
			c.channelList.Fail(text)
		}
	}
	return true
}

// handleChannelListMessage routes 321/322/323 and the LIST failure numerics.
// It reports whether the message was consumed. It mirrors the controller's
// channel-list branch in handleMessage.
func (c *Controller) handleChannelListMessage(networkID string, message irc.Message) bool {
	switch message.Command {
	case "322":
		if len(message.Params) >= 3 {
			users, err := strconv.Atoi(parameterText(message, 2))
			if err != nil {
				users = 0
			}
			topic := ""
			if len(message.Params) >= 4 {
				topic = parameterText(message, 3)
			}
			c.channelLists.Row(networkID, ChannelListRow{
				Channel: parameterText(message, 1),
				Users:   users,
				Topic:   topic,
			})
		}
		return true
	case "321":
		c.channelLists.Activity(networkID)
		return true
	case "323":
		c.finishChannelList(networkID)
		return true
	}
	return c.failChannelListFromNumeric(networkID, message)
}
