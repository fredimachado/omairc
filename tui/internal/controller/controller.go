package controller

import (
	"sort"
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
	"github.com/fredimachado/omairc/tui/internal/storage"
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
	inbox         *irc.Inbox

	currentNicks map[string]string
	lastErrors   map[string]string
	capabilities map[string]irc.CapabilitySet

	selected         *irc.ConversationKey
	selectedTarget   string
	consoleNetworkID string
	// consoleOpen is the raw Status-console model flag, mirroring
	// IrcStatusConsole::isOpen. It is not the surface the transcript shows:
	// ConsoleOpen() derives that the way OmaircWindow.qml's consoleVisible does,
	// so a cleared selection falls back to Status. Only the two callers that
	// mirror Qt's m_console.isOpen() reads (FocusedNetworkID and the /join
	// dispatch surface) want the raw flag; they ask StatusConsoleOpen.
	consoleOpen  bool
	networkOrder []string
	// networkCollapsed holds the sidebar collapse state. Collapse/reorder
	// state lives on the controller (mirroring the Qt IrcConnection home)
	// because the sidebar reads NetworkOrder()/Conversations(), not
	// connection.Networks().
	networkCollapsed map[string]bool
	connectionStatus string

	// Phase 7 slash subsystems and their stores. commands, replies, monitor,
	// and autoaway own their decisions; the controller owns the side effects.
	ignores         *IgnoreStore
	mutes           *MuteStore
	highlights      *HighlightStore
	monitors        *MonitorStore
	avatars         *AvatarStore
	prefs           *Preferences
	membershipNoise irc.MembershipNoise

	// Phase 11 persistence. New starts ephemeral so unit tests never touch the
	// user's config; the shell opts in with SetEphemeral(false) and then loads
	// the persisted stores and preferences.
	ephemeral           bool
	openDirects         *storage.OpenDirectStore
	closed              *storage.ClosedConversationStore
	playbackTimes       *storage.PlaybackTimeStore
	scrollPlaces        *storage.ScrollPlaceStore
	playback            *PlaybackCoordinator
	openDirectsMotdSeen map[string]bool

	commands     *CommandDispatcher
	replies      *ReplyRouter
	monitor      *MonitorCoordinator
	autoaway     *AutoawayRuntime
	channelLists *ChannelListRequest
	channelList  *ChannelListModel

	channelListOpen bool
	unawaySent      map[string]bool
	typingTarget    string

	conversationEpoch int
	peerMetadataEpoch int
	peerAccountEpoch  int
	// coalesceMemberRow skips one member-row rebuild when the next event in
	// the same IRC line repaints that nick. A WHO reply is away, then nick
	// facts, and the row should rebuild once with both.
	coalesceMemberRow bool

	conversations []ConversationSnapshot
	messages      []MessageSnapshot
	members       []MemberSnapshot

	// OnMentionArrived is called for each mention the reducer hands up. It is
	// nil-able; Phase 9 wires the inbox and notifications.
	OnMentionArrived func(author, body, networkID, target, msgid string)
	// OnFileLink is called from the upload goroutine when a file upload
	// finishes. It must not touch the controller. The shell turns it into a
	// message on the Update goroutine. Nil-able.
	OnFileLink     func(url, message string)
	fileUploadBusy bool
	// OnInboxChanged fires when the session waiting list changes: an arrival
	// was appended, or a row was consumed/dismissed/purged. It is the UI's
	// re-render wake-up. It is nil-able.
	OnInboxChanged func()
	// OnAvatarURLChanged fires when the standing avatar URL for a network
	// moves. Nil-able; the shell wires it to the profile store.
	OnAvatarURLChanged func(networkID, url string)
	// OnAutojoinChanged fires when a network's autojoin list changes.
	// Nil-able; the shell wires it to the profile store.
	OnAutojoinChanged func(networkID string, channels []string, keys map[string]string)
	// OnMonitorArrived is called for each MONITOR presence change the monitor
	// coordinator reports. It is nil-able; Phase 9 owns the inbox append and
	// the desktop notification.
	OnMonitorArrived func(networkID, display, body string, newlyOnline bool)
	// OnSelectionChanged fires when the selected conversation, the focused
	// network, or the Status surface changes.
	OnSelectionChanged func()
	// OnStatusChanged fires when a connection state, last error, or self-away
	// fact that the identity footer reads changes.
	OnStatusChanged func()
	// OnCapabilitiesChanged fires when the negotiated capability set changes or
	// the selection moves to a network with a different set.
	OnCapabilitiesChanged func()
	transcriptFocused     bool
	transcriptFollowEnd   bool

	// OnViewChanged fires whenever Publish dirtied a view surface, so the shell
	// re-renders. Publish runs on both the Update goroutine and session
	// goroutines; the shell must coalesce this wake-up rather than call Send
	// synchronously, the same rule as the selection and status callbacks. It
	// carries no payload: the shell re-reads the snapshots. Nil-able.
	OnViewChanged func()
}

// New returns an empty controller with a real clock, an empty reducer, and an
// empty default-capacity Status console.
func New() *Controller {
	c := &Controller{
		clock:            session.RealClock{},
		manager:          session.NewManager(),
		reducer:          irc.NewEventReducer(),
		statusConsole:    NewStatusConsole(0),
		inbox:            irc.NewInbox(),
		currentNicks:     make(map[string]string),
		lastErrors:       make(map[string]string),
		capabilities:     make(map[string]irc.CapabilitySet),
		networkCollapsed: make(map[string]bool),
		connectionStatus: "Offline",
		ephemeral:        true,
	}
	// IrcEventReducer starts with the window treated as active
	// (src/irc/irceventreducer.h:395). NewEventReducer leaves the Go field at
	// its zero value, so the controller restores the Qt default here.
	c.reducer.SetWindowActive(true)
	c.transcriptFocused = true
	c.transcriptFollowEnd = true
	c.openDirectsMotdSeen = make(map[string]bool)
	c.openDirects = storage.NewOpenDirectStore()
	c.openDirects.SetEphemeral(true)
	c.closed = storage.NewClosedConversationStore()
	c.closed.SetEphemeral(true)
	c.reducer.SetClosedPersistence(c.persistClosed, c.containsClosed)
	c.playbackTimes = storage.NewPlaybackTimeStore()
	c.playbackTimes.SetEphemeral(true)
	c.scrollPlaces = storage.NewScrollPlaceStore()
	c.scrollPlaces.SetEphemeral(true)
	c.playback = NewPlaybackCoordinator(c.playbackTimes, c.reducer)
	// New starts ephemeral, like the Qt constructor before the shell opts in
	// (src/irc/irccontroller.cpp:400-411): no transcript log until asked.
	c.reducer.SetConversationLog(nil)
	c.initSlashSubsystems()
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
	c.rebindClocks()
}

func (c *Controller) now() time.Time {
	if c.clock == nil {
		return time.Time{}
	}
	return c.clock.Now()
}

// displayLocation is the zone every displayed clock is read in: the clock's own
// location, which is the system local zone for the real clock. The reducer keeps
// a parsed server-time tag in UTC (translator.go's ircServerTimeOf), so the zone
// has to be applied before a row's HH:mm is rendered or compared. Qt localizes
// the same way, with QDateTime::toLocalTime in MessageListModel::displayTime and
// with new Date() in OmaircWindow.qml's currentTranscriptMinute.
func (c *Controller) displayLocation() *time.Location {
	if now := c.now(); !now.IsZero() {
		return now.Location()
	}
	return time.Local
}

// --- Phase 11 persistence -------------------------------------------------

// Group and key names match src/irc/irccontroller.cpp and ircconnection.cpp so
// the Qt client and the terminal client share one INI section.
const (
	preferencesGroup             = "preferences"
	reopenDirectMessagesKey      = "reopenDirectMessages"
	loadPeerAvatarsKey           = "loadPeerAvatars"
	openConversationsAtUnreadKey = "openConversationsAtUnread"
	membershipNoiseKey           = "membershipNoise"
	networkOrderKey              = "networkOrder"
	collapsedNetworksKey         = "collapsedNetworks"
)

// SetEphemeral turns disk persistence on/off. New() starts ephemeral so unit
// tests never touch the user's config; the shell opts in. It mirrors
// IrcController::setEphemeral (src/irc/irccontroller.cpp:408-415).
func (c *Controller) SetEphemeral(ephemeral bool) {
	c.ephemeral = ephemeral
	if ephemeral {
		c.reducer.SetConversationLog(nil)
	}
	c.openDirects.SetEphemeral(ephemeral)
	c.closed.SetEphemeral(ephemeral)
	c.playbackTimes.SetEphemeral(ephemeral)
	c.scrollPlaces.SetEphemeral(ephemeral)
	c.autoaway.SetEphemeral(ephemeral)
}

// SetConversationLog wires the reducer's transcript log.
func (c *Controller) SetConversationLog(log irc.ConversationLog) {
	c.reducer.SetConversationLog(log)
}

// SetOpenDirectStore installs the persisted open-direct store, mapping nil to
// an ephemeral in-memory store.
func (c *Controller) SetOpenDirectStore(store *storage.OpenDirectStore) {
	if store == nil {
		store = storage.NewOpenDirectStore()
		store.SetEphemeral(true)
	}
	c.openDirects = store
}

// SetPlaybackTimeStore installs the persisted playback-time store, mapping nil
// to an ephemeral in-memory store. The playback coordinator reads the same
// pointer, so it is re-pointed here.
func (c *Controller) SetPlaybackTimeStore(store *storage.PlaybackTimeStore) {
	if store == nil {
		store = storage.NewPlaybackTimeStore()
		store.SetEphemeral(true)
	}
	c.playbackTimes = store
	if c.playback != nil {
		c.playback.times = store
	}
}

// LoadStoredPreferences reads the preferences group into c.prefs and restores
// the sidebar order and collapse set. It is a no-op when ephemeral. It mirrors
// IrcController::loadStoredPreferences (src/irc/irccontroller.cpp:417-425).
func (c *Controller) LoadStoredPreferences() {
	if c.ephemeral {
		return
	}
	settings := storage.OpenSettings("")
	c.prefs.SetEnabled(irc.PrefDirects, settings.Bool(preferencesGroup, reopenDirectMessagesKey, true))
	c.prefs.SetEnabled(irc.PrefAvatars, settings.Bool(preferencesGroup, loadPeerAvatarsKey, true))
	c.prefs.SetEnabled(irc.PrefUnread, settings.Bool(preferencesGroup, openConversationsAtUnreadKey, true))
	if noise, ok := irc.ParseMembershipNoise(settings.Value(preferencesGroup, membershipNoiseKey)); ok {
		c.membershipNoise = noise
	}
	c.reducer.SetMembershipNoise(c.membershipNoise)
	c.loadStoredNetworkOrder(settings)
}

// savePreferenceBool persists one preferences-group bool with the fail-closed
// probe from preferenceIniRefusesWrite: a malformed or unwritable ini is left
// alone instead of being rebuilt from the in-process cache.
func savePreferenceBool(key string, enabled bool) {
	settings := storage.OpenSettings("")
	if settings.WriteBlocked() != storage.StatusWritten {
		return
	}
	settings.SetBool(preferencesGroup, key, enabled)
	settings.Sync()
}

func savePreferenceValue(key, value string) {
	settings := storage.OpenSettings("")
	if settings.WriteBlocked() != storage.StatusWritten {
		return
	}
	settings.SetValue(preferencesGroup, key, value)
	settings.Sync()
}

// savePreferenceList persists one preferences-group string list. It shares the
// fail-closed probe and only writes when the value changed, mirroring
// IrcConnection's savePreferenceList (src/irc/ircconnection.cpp:69-87).
func savePreferenceList(key string, value []string) {
	settings := storage.OpenSettings("")
	if settings.WriteBlocked() != storage.StatusWritten {
		return
	}
	if existing := settings.StringList(preferencesGroup, key); stringSlicesEqual(existing, value) {
		return
	}
	settings.SetStringList(preferencesGroup, key, value)
	settings.Sync()
}

// ReopenDirects reports whether direct messages reopen at startup.
func (c *Controller) ReopenDirects() bool { return c.prefs.Enabled(irc.PrefDirects) }

// ShowAvatars reports whether peer avatars are shown.
func (c *Controller) ShowAvatars() bool { return c.prefs.Enabled(irc.PrefAvatars) }

// OpenAtUnread reports whether conversations open at their unread start.
func (c *Controller) OpenAtUnread() bool { return c.prefs.Enabled(irc.PrefUnread) }

// SetReopenDirects stores the reopen-directs toggle, persists it unless
// ephemeral, and restores the open directs of every registered network when it
// turns back on. It mirrors IrcController::setReopenDirectMessages
// (src/irc/irccontroller.cpp:753-767).
func (c *Controller) SetReopenDirects(enabled bool) {
	if c.ReopenDirects() == enabled {
		return
	}
	c.prefs.SetEnabled(irc.PrefDirects, enabled)
	if !c.ephemeral {
		savePreferenceBool(reopenDirectMessagesKey, enabled)
	}
	if !enabled {
		return
	}
	for _, networkID := range c.manager.NetworkIDs() {
		if s := c.manager.Find(networkID); s != nil && s.State() == session.StateRegistered {
			c.restoreOpenDirects(networkID)
		}
	}
}

// SetShowAvatars stores the avatar toggle and persists it unless ephemeral.
func (c *Controller) SetShowAvatars(enabled bool) {
	if c.ShowAvatars() == enabled {
		return
	}
	c.prefs.SetEnabled(irc.PrefAvatars, enabled)
	if !c.ephemeral {
		savePreferenceBool(loadPeerAvatarsKey, enabled)
	}
}

// SetOpenAtUnread stores the open-at-unread toggle and persists it unless
// ephemeral.
func (c *Controller) SetOpenAtUnread(enabled bool) {
	if c.OpenAtUnread() == enabled {
		return
	}
	c.prefs.SetEnabled(irc.PrefUnread, enabled)
	if !c.ephemeral {
		savePreferenceBool(openConversationsAtUnreadKey, enabled)
	}
}

// PrefEnabled maps one /pref toggle to its getter. It mirrors the prefEnabled
// lambda in IrcController (src/irc/irccontroller.cpp:330-340).
func (c *Controller) PrefEnabled(name irc.PrefName) bool {
	switch name {
	case irc.PrefDirects:
		return c.ReopenDirects()
	case irc.PrefAvatars:
		return c.ShowAvatars()
	case irc.PrefUnread:
		return c.OpenAtUnread()
	case irc.PrefJoins:
		return false
	}
	return false
}

// MembershipNoise reports how join, part, quit, and nick lines are shown.
func (c *Controller) MembershipNoise() irc.MembershipNoise {
	return c.membershipNoise
}

// SetMembershipNoise stores the membership-noise setting and applies it to
// later lines. Lines already on screen stay as they are.
func (c *Controller) SetMembershipNoise(noise irc.MembershipNoise) {
	if c.membershipNoise == noise {
		return
	}
	c.membershipNoise = noise
	c.reducer.SetMembershipNoise(noise)
	if !c.ephemeral {
		savePreferenceValue(membershipNoiseKey, irc.MembershipNoiseToken(noise))
	}
}

// PrefApply maps one /pref toggle to its setter. It mirrors the prefApply
// lambda in IrcController (src/irc/irccontroller.cpp:341-351).
func (c *Controller) PrefApply(name irc.PrefName, enabled bool) {
	switch name {
	case irc.PrefDirects:
		c.SetReopenDirects(enabled)
	case irc.PrefAvatars:
		c.SetShowAvatars(enabled)
	case irc.PrefUnread:
		c.SetOpenAtUnread(enabled)
	case irc.PrefJoins:
	}
}

// loadStoredNetworkOrder restores networkOrder and networkCollapsed from the
// preferences group. Saved ids without a session are dropped; the remaining
// registered networks keep NetworkIDs() order but are re-sorted by the
// profileLess fallback (case-insensitive resolved name, then trimmed nick, then
// network id) using the session's own name and nick. Sort by network id only
// would lose the Qt profile ordering, and the controller cannot import
// internal/connection because connection imports controller.
func (c *Controller) loadStoredNetworkOrder(settings *storage.Settings) {
	registered := c.manager.NetworkIDs()
	seen := make(map[string]bool, len(registered))
	order := make([]string, 0, len(registered))
	for _, id := range settings.StringList(preferencesGroup, networkOrderKey) {
		if id == "" || seen[id] || c.manager.Find(id) == nil {
			continue
		}
		seen[id] = true
		order = append(order, id)
	}
	leftovers := make([]string, 0, len(registered))
	for _, id := range registered {
		if !seen[id] {
			leftovers = append(leftovers, id)
		}
	}
	sort.SliceStable(leftovers, func(i, j int) bool {
		return c.profileLessNetwork(leftovers[i], leftovers[j])
	})
	c.networkOrder = append(order, leftovers...)

	collapsed := make(map[string]bool)
	for _, id := range settings.StringList(preferencesGroup, collapsedNetworksKey) {
		if id != "" && c.manager.Find(id) != nil {
			collapsed[id] = true
		}
	}
	c.networkCollapsed = collapsed
}

// profileLessNetwork ports profileLess from src/irc/ircconnection.cpp:21-33
// over the controller's sessions: case-insensitive resolved name, then trimmed
// nick, then network id.
func (c *Controller) profileLessNetwork(left, right string) bool {
	leftSession := c.manager.Find(left)
	rightSession := c.manager.Find(right)
	if leftSession == nil || rightSession == nil {
		return left < right
	}
	leftName := strings.ToLower(leftSession.Name())
	rightName := strings.ToLower(rightSession.Name())
	if leftName != rightName {
		return leftName < rightName
	}
	leftNick := strings.ToLower(strings.TrimSpace(leftSession.Nick()))
	rightNick := strings.ToLower(strings.TrimSpace(rightSession.Nick()))
	if leftNick != rightNick {
		return leftNick < rightNick
	}
	return left < right
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

// NetworkOrder returns the network ids in display order: the order recorded
// by SetNetworkOrder first (only ids that still have a session), then any
// remaining registered networks in NetworkIDs() order.
func (c *Controller) NetworkOrder() []string {
	registered := c.manager.NetworkIDs()
	if len(c.networkOrder) == 0 {
		return registered
	}
	seen := make(map[string]bool, len(registered))
	order := make([]string, 0, len(registered))
	for _, id := range c.networkOrder {
		if seen[id] || c.manager.Find(id) == nil {
			continue
		}
		seen[id] = true
		order = append(order, id)
	}
	for _, id := range registered {
		if seen[id] {
			continue
		}
		seen[id] = true
		order = append(order, id)
	}
	return order
}

// --- Session handler ------------------------------------------------------

// StateChanged refreshes the focused connection status. A non-Registered
// transition also tells the playback coordinator that the connection ended, so
// a reconnect replays as if it never happened. It mirrors the stateChanged
// lambda in IrcController::addSession (src/irc/irccontroller.cpp:467-473).
func (c *Controller) StateChanged(networkID string, state session.SessionState) {
	if networkID != "" && state != session.StateRegistered {
		c.playback.OnLeftRegistration(networkID)
	}
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
	s := c.manager.Find(networkID)
	if s != nil {
		c.playback.OnRegistered(networkID, s.AutojoinChannels())
	}
	nick := c.currentNicks[networkID]
	if s != nil {
		nick = s.Nick()
		c.currentNicks[networkID] = nick
	}
	c.reducer.SetServerFeatures(networkID, irc.NewServerFeatures())
	c.Apply(irc.WelcomeEvent{NetworkID: networkID, CurrentNick: nick})
	if s != nil {
		c.autoaway.OnSessionRegistered(s)
	}
	// A fresh registration forgets the previous connection's MOTD latch so the
	// next 376/422 restores open directs again.
	delete(c.openDirectsMotdSeen, networkID)
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

// HistoryBatchReceived replays one history batch into the reducer. A bouncer
// playback batch is trimmed against the registration stamp first. A TARGETS
// answer discovers directs instead of splicing lines. It mirrors
// IrcController::handleHistoryBatch (src/irc/irccontroller.cpp:2373-2405).
func (c *Controller) HistoryBatchReceived(networkID string, batch irc.HistoryBatch) {
	if batch.Kind == irc.HistoryTargets {
		s := c.manager.Find(networkID)
		features := c.reducer.ServerFeatures(networkID)
		mapping := features.CaseMapping()
		created := c.playback.NoteDiscoveredTargets(
			s,
			batch.Targets,
			c.currentNicks[networkID],
			c.openDirects.Listed(networkID, mapping),
			c.openDirects.DismissedListed(networkID, mapping),
			func(target string) bool {
				return c.persistableDirectTarget(networkID, target)
			},
		)
		if s != nil {
			c.playback.NoteTargetsPage(s, batch.Targets, batch.HistoryEnded, s.HistoryLimit())
		}
		if created > 0 {
			c.reloadModels()
		}
		return
	}
	if batch.AfterRequest && len(batch.Lines) == 0 {
		c.dropEmptyDiscoveredDirect(networkID, batch.Target)
	}
	features := c.reducer.ServerFeatures(networkID)
	event, ok := irc.TranslateHistory(networkID, c.currentNicks[networkID], features, batch, c.now())
	if !ok {
		return
	}
	if event.Kind == irc.HistoryBouncerPlayback {
		c.playback.TrimBouncerBatch(networkID, &event)
		if len(event.Lines) == 0 {
			return
		}
	}
	c.Apply(event)
}

// ChatHistoryRequestFinished clears browse flags when a BEFORE request ends
// without a delivered batch.
func (c *Controller) ChatHistoryRequestFinished(networkID, target string, failed bool, before bool) {
	if !failed || !before {
		return
	}
	c.reducer.ClearHistoryPageCapTail(c.reducer.ConversationKey(networkID, target))
}

// ChatHistoryFailed retries one failed TARGETS query and drops a discovered
// direct whose AFTER came back empty or failed. It mirrors
// IrcController::handleChatHistoryFailed.
func (c *Controller) ChatHistoryFailed(networkID, subcommand, target string) {
	if strings.EqualFold(subcommand, "TARGETS") {
		if !c.playback.RetryTargets(networkID) {
			return
		}
		if s := c.manager.Find(networkID); s != nil {
			c.requestChatHistoryCatchUp(s)
		}
		return
	}
	if strings.EqualFold(subcommand, "AFTER") {
		c.dropEmptyDiscoveredDirect(networkID, target)
	}
}

func (c *Controller) dropEmptyDiscoveredDirect(networkID, target string) {
	if !c.playback.WasDiscovered(networkID, target) {
		return
	}
	features := c.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	for _, nick := range c.openDirects.Listed(networkID, mapping) {
		if mapping.Equals(nick, target) {
			return
		}
	}
	key := c.reducer.ConversationKey(networkID, target)
	conversation := c.reducer.Find(key)
	if conversation == nil || len(conversation.Messages) > 0 {
		return
	}
	c.reducer.DropDirectMessage(key)
	c.reloadModels()
}

// StatusEntry records one classified Status line and routes its correlated
// WHOIS, CTCP, and metadata replies.
func (c *Controller) StatusEntry(entry irc.StatusEntry) {
	c.statusConsole.Append(entry)
	c.replies.RouteStatusEntry(entry)
	if entry.Label() == "INVITE" {
		s := c.manager.Find(entry.NetworkID())
		if s == nil {
			return
		}
		pending, ok := s.PendingInvite()
		if !ok {
			return
		}
		c.appendInbox(irc.InboxItem{
			Kind:      irc.InboxInvite,
			Timestamp: c.now(),
			NetworkID: entry.NetworkID(),
			Actor:     pending.Nick,
			Target:    pending.Channel,
			Preview:   entry.Text(),
		})
	}
}

// CapabilitiesChanged stores the set and clears presence/typing facts that a
// dropped capability can no longer maintain. It mirrors IrcController::
// handleCapabilities (src/irc/irccontroller.cpp:931-967).
func (c *Controller) CapabilitiesChanged(networkID string, capabilities irc.CapabilitySet) {
	c.handleCapabilities(networkID, capabilities)
}

// RequestLabelFinished drops the labeled watch for a finished request.
func (c *Controller) RequestLabelFinished(networkID, requestLabel string) {
	c.replies.RequestLabelFinished(networkID, requestLabel)
}

// AutojoinChannelsChanged forwards a session's autojoin edit to the shell's
// profile store. Nil-able, so it stays a no-op without a wired callback.
func (c *Controller) AutojoinChannelsChanged(networkID string, channels []string, keys map[string]string) {
	if c.OnAutojoinChanged != nil {
		c.OnAutojoinChanged(networkID, channels, keys)
	}
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
		stored := c.reducer.NickPresence(account.NetworkID, account.Nick)
		// A first `*` is a logout we have not stored yet. A repeat of the
		// same account, including a second logout, still changes nothing.
		if stored.AccountKnown && servicesAccountMatches(stored.Account, account.Account) {
			return
		}
	}
	kind := irc.KindOf(event)
	typingOnly := kind == irc.EventTyping
	selfAwayOnly := kind == irc.EventSelfAway

	// A welcome re-enters registration: reply watches, auto-away state, and
	// the unaway latch all reset first. It mirrors the welcome branch in
	// IrcController::apply (src/irc/irccontroller.cpp:2027-2032).
	if welcome, ok := event.(irc.WelcomeEvent); ok {
		delete(c.unawaySent, welcome.NetworkID)
		c.autoaway.ForgetNetwork(welcome.NetworkID)
		c.replies.Forget(welcome.NetworkID)
	} else if selfAway, ok := event.(irc.SelfAwayEvent); ok {
		delete(c.unawaySent, selfAway.NetworkID)
	}

	reopenNetwork, reopenTarget, reopen := c.otherReopenedDirect(event)
	c.reducer.Apply(event, c.now())
	c.noteKeptReplay()

	if metadata, ok := event.(irc.MemberMetadataEvent); ok {
		c.peerMetadataEpoch++
		c.replies.RouteOwnMetadataReply(metadata.NetworkID, metadata.Nick,
			metadata.Key, metadata.Value)
	} else if _, ok := event.(irc.NickFactsEvent); ok {
		c.peerMetadataEpoch++
	}
	if accountsMoved(kind, event) {
		c.peerAccountEpoch++
	}

	// A NICK moves the persisted open-direct and playback clocks with it. A
	// self-authored line in an existing direct is proof the user engaged the
	// query, so it is remembered for restore. It mirrors IrcController::apply
	// (src/irc/irccontroller.cpp:2056-2091).
	if nick, ok := nickEventOf(event); ok {
		features := c.reducer.ServerFeatures(nick.NetworkID)
		mapping := features.CaseMapping()
		c.openDirects.Rekey(nick.NetworkID, nick.OldNick, nick.NewNick, mapping)
		c.openDirects.RekeyDismissed(nick.NetworkID, nick.OldNick, nick.NewNick, mapping)
		c.closed.Rekey(nick.NetworkID, nick.OldNick, nick.NewNick, mapping)
		c.playbackTimes.Rekey(nick.NetworkID, nick.OldNick, nick.NewNick, mapping)
		c.scrollPlaces.Rekey(nick.NetworkID, nick.OldNick, nick.NewNick, mapping)
		c.playback.Rekey(nick.NetworkID, nick.OldNick, nick.NewNick)
	} else if message, ok := messageEventOf(event); ok {
		c.noteSelfAuthoredDirect(message.Conversation.NetworkID, message.Author, message.Target)
	} else if notice, ok := noticeEventOf(event); ok {
		c.noteSelfAuthoredDirect(notice.Conversation.NetworkID, notice.Author, notice.Target)
	} else if action, ok := actionEventOf(event); ok {
		c.noteSelfAuthoredDirect(action.Conversation.NetworkID, action.Author, action.Target)
	}
	if reopen {
		key := c.reducer.ConversationKey(reopenNetwork, reopenTarget)
		if conversation := c.reducer.Find(key); conversation != nil && !conversation.IsChannel() {
			c.rememberOpenDirect(reopenNetwork, reopenTarget)
		}
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
		// Claiming a conversation means the transcript leaves Status, so the
		// console model closes here exactly as it does in selectConversation.
		// The shell opens Status when a profile is applied (before any
		// conversation exists), and a real server's auto-join is what first
		// reaches this branch; without the close the sidebar and member panel
		// would follow the joined channel while the transcript stayed on Status.
		c.consoleOpen = false
		features := c.reducer.ServerFeatures(key.NetworkID)
		if c.reducer.Find(key) != nil && !features.IsChannel(key.NormalizedTarget) {
			c.requestReadMarkerGet(c.manager.Find(key.NetworkID), key.NetworkID, conversation.Target)
		}
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
	c.syncReadMarkerForSelection()

	if arrival, ok := c.reducer.TakeInboxArrival(); ok {
		c.appendInbox(irc.InboxItem{
			Kind:      arrival.Kind,
			Timestamp: c.now(),
			NetworkID: arrival.NetworkID,
			Actor:     arrival.Actor,
			Target:    arrival.Target,
			Preview:   arrival.Body,
			MsgID:     arrival.MsgID,
		})
	}
}

// memberRowNick is the normalized nick a member-row notify would repaint.
// Events that reset the panel, or do not touch it, return false.
func memberRowNick(reducer *irc.EventReducer, event irc.Event) (string, bool) {
	switch value := event.(type) {
	case irc.AwayEvent:
		return reducer.ConversationKey(value.NetworkID, value.Nick).NormalizedTarget, true
	case irc.AccountEvent:
		return reducer.ConversationKey(value.NetworkID, value.Nick).NormalizedTarget, true
	case irc.MemberMetadataEvent:
		return reducer.ConversationKey(value.NetworkID, value.Nick).NormalizedTarget, true
	case irc.NickFactsEvent:
		return reducer.ConversationKey(value.NetworkID, value.Nick).NormalizedTarget, true
	default:
		return "", false
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
	if notify.Members != irc.MemberSurfaceNone &&
		!(c.coalesceMemberRow && notify.Members == irc.MemberSurfaceRow) {
		c.rebuildMembers()
	}
	if notify.Selection {
		c.notifySelectionChanged()
	}
	// A typing event repaints the sidebar's typing role. Qt emits
	// typingChanged() and calls ConversationListModel::invalidateTyping() only
	// when the conversation list was not already reloaded. The Go snapshot
	// materializes that role, so rebuild it here; the typing-changed
	// notification itself is the Phase 8 seam.
	if notify.Typing && !notify.Conversations {
		c.rebuildConversations()
		c.conversationEpoch++
	}
	// Wake the shell for any surface this publish dirtied. The selection,
	// status, and capability callbacks above cover their own surfaces, but a
	// chat line or a membership change has no other wake-up: without this the
	// runtime never repaints the transcript or the sidebar for them.
	if notify != (irc.ViewNotify{}) && c.OnViewChanged != nil {
		c.OnViewChanged()
	}
}

// --- Session inbox --------------------------------------------------------

// appendInbox stores one arrival stamped with the controller clock, then
// wakes the UI. It mirrors IrcController::appendInbox/syncInbox.
func (c *Controller) appendInbox(item irc.InboxItem) {
	features := c.reducer.ServerFeatures(item.NetworkID)
	c.inbox.Append(item, features.CaseMapping())
	c.notifyInboxChanged()
}

func (c *Controller) notifyInboxChanged() {
	if c.OnInboxChanged != nil {
		c.OnInboxChanged()
	}
}

// InboxCount returns the number of waiting session-inbox rows.
func (c *Controller) InboxCount() int {
	return c.inbox.Count()
}

// InboxItems returns the waiting rows newest first.
func (c *Controller) InboxItems() []InboxSnapshot {
	items := c.inbox.Items()
	rows := make([]InboxSnapshot, 0, len(items))
	for _, item := range items {
		rows = append(rows, InboxSnapshot{
			Kind:      irc.KindName(item.Kind),
			NetworkID: item.NetworkID,
			Actor:     item.Actor,
			Target:    item.Target,
			Preview:   item.Preview,
			MsgID:     item.MsgID.Value,
			Label:     item.Label(),
		})
	}
	return rows
}

// DismissInboxItem drops one waiting row. An out-of-range index is a no-op. It
// mirrors IrcController::dismissInboxItem (src/irc/irccontroller.cpp:995-1004).
func (c *Controller) DismissInboxItem(index int) {
	if index < 0 || index >= c.inbox.Count() {
		return
	}
	c.inbox.ConsumeAt(index)
	c.notifyInboxChanged()
}

// ActivateInboxItem consumes one waiting row and reveals its target, joining
// an invited channel. An out-of-range index is a no-op. It mirrors
// IrcController::activateInboxItem (src/irc/irccontroller.cpp:1006-1039).
func (c *Controller) ActivateInboxItem(index int) {
	if index < 0 || index >= c.inbox.Count() {
		return
	}
	item := c.inbox.At(index)
	c.inbox.ConsumeAt(index)
	c.notifyInboxChanged()

	switch item.Kind {
	case irc.InboxMention, irc.InboxHighlight, irc.InboxDirect:
		c.RevealConversation(item.NetworkID, item.Target)
	case irc.InboxInvite:
		s := c.manager.Find(item.NetworkID)
		if s == nil {
			break
		}
		features := c.reducer.ServerFeatures(item.NetworkID)
		target, ok := irc.MakeJoinTarget(item.Target, nil, features)
		if !ok {
			break
		}
		key := c.reducer.ConversationKey(item.NetworkID, item.Target)
		wasClosed := c.reducer.Closed(key)
		c.reducer.ClearClosed(key)
		wasCancelled := c.commands.TakeCancelledSelfJoin(key)
		if s.Join(target) {
			c.openJoinedChannel(item.NetworkID, item.Target)
		} else {
			if wasClosed {
				c.reducer.NoteClosed(key)
			}
			if wasCancelled {
				c.commands.NoteCancelled(key)
			}
		}
	case irc.InboxMonitorOnline:
		c.RevealConversation(item.NetworkID, item.Actor)
	case irc.InboxKick:
		c.RevealConversation(item.NetworkID, item.Target)
	}
}

// RevealConversation opens or creates a conversation and selects it, clearing
// any monitor row for a direct message. It mirrors
// IrcController::revealConversation (src/irc/irccontroller.cpp:1143-1161).
func (c *Controller) RevealConversation(networkID, target string) {
	if networkID == "" || target == "" {
		return
	}
	key := c.reducer.ConversationKey(networkID, target)
	if c.reducer.Find(key) == nil {
		if c.reducer.EnsureConversation(key, target, irc.CauseUserOpen) == nil {
			return
		}
		c.rememberOpenDirect(networkID, target)
		c.Publish(irc.ViewNotify{Conversations: true})
	}
	features := c.reducer.ServerFeatures(networkID)
	if !features.IsChannel(key.NormalizedTarget) {
		c.inbox.ConsumeMonitor(networkID, target, features.CaseMapping())
		c.notifyInboxChanged()
	}
	c.SelectConversation(networkID, target)
}

// PlainIrcText strips mIRC colors and formatting control codes, mirroring
// IrcTextFormatter::plainIrcText.
func (c *Controller) PlainIrcText(text string) string {
	return irc.PlainIrcText(text)
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

// selectionNotify reloads the sidebar, transcript, and member panel alongside
// the selection, matching the model selects in IrcController::selectConversation.
// ConversationListModel::select calls reload(), which repaints every row's
// roles, so the unread and mention counts a selection just consumed clear
// immediately rather than waiting for the next conversation-dirtying event.
func selectionNotify() irc.ViewNotify {
	return irc.ViewNotify{
		Conversations: true,
		Messages:      true,
		Members:       irc.MemberSurfaceReset,
		Selection:     true,
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
// status follow: the Status console when its model is open, else the selected
// conversation's network, else the last Status network. It reads the raw console
// flag, mirroring IrcController::focusedNetworkId's m_console.isOpen()
// (src/irc/irccontroller.cpp:606-612).
func (c *Controller) FocusedNetworkID() string {
	if c.StatusConsoleOpen() {
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
	// Moving to a different conversation withdraws a hint the previous target
	// was still publishing. It mirrors IrcController::selectConversation
	// (src/irc/irccontroller.cpp:1079-1086).
	changed := c.selected == nil ||
		c.selected.NetworkID != key.NetworkID ||
		c.selected.NormalizedTarget != key.NormalizedTarget
	if changed && c.typingTarget != "" {
		if previous := c.selectedSession(); previous != nil {
			previous.SendTyping(c.typingTarget, irc.TypingDone)
		}
		c.typingTarget = ""
	}
	c.selected = &key
	c.selectedTarget = target
	features := c.reducer.ServerFeatures(networkID)
	c.inbox.ConsumeConversation(networkID, target, features.CaseMapping())
	if !features.IsChannel(key.NormalizedTarget) {
		c.inbox.ConsumeMonitor(networkID, target, features.CaseMapping())
	}
	c.notifyInboxChanged()
	c.reducer.MarkSelected(key)
	c.Publish(selectionNotify())
	c.refreshConnectionStatus()
	c.notifyCapabilitiesChanged()
	if conversation := c.reducer.Find(key); conversation != nil && !features.IsChannel(key.NormalizedTarget) {
		c.requestReadMarkerGet(c.manager.Find(networkID), networkID, target)
	}
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

// ConsoleOpen reports whether the Status surface is on screen. It mirrors
// OmaircWindow.qml's consoleVisible:
//
//	networkConsole ? (networkConsole.open || irc.selectedTarget.length === 0) : false
//
// The raw console flag alone is not enough. A cleared selection leaves nothing
// else to show, so Status is the surface again; and selecting a conversation
// closes the console (see SelectConversation and the first-conversation fallback
// in Apply), so the transcript follows the conversation. Every view predicate
// reads this derived value; only the callers that mirror Qt's m_console.isOpen()
// reads want the raw flag, via StatusConsoleOpen.
func (c *Controller) ConsoleOpen() bool {
	return c.consoleOpen || c.SelectedTarget() == ""
}

// outboundFrameBytes is the focused network's LINELEN, or 512 when no
// network is focused. It mirrors IrcController::focusedFrameBytes.
func (c *Controller) outboundFrameBytes() int {
	if c == nil {
		return irc.MaxClassicFrameBytes
	}
	id := c.FocusedNetworkID()
	if id == "" {
		return irc.MaxClassicFrameBytes
	}
	return c.reducer.ServerFeatures(id).LineLength()
}

// ComposerByteBudget is the UTF-8 byte cap for the composer on the current
// surface. Status is a raw line. A conversation is the PRIVMSG body that fits
// in one frame of the focused network's LINELEN. It mirrors
// IrcController::composerByteBudget.
func (c *Controller) ComposerByteBudget() int {
	if c == nil {
		return irc.ComposerByteBudget("")
	}
	return c.ComposerByteBudgetFor("")
}

// ComposerByteBudgetFor is ComposerByteBudget, tightened when draft is a
// `/me` action. It mirrors IrcController::composerByteBudgetFor.
func (c *Controller) ComposerByteBudgetFor(draft string) int {
	frame := c.outboundFrameBytes()
	if c == nil || c.ConsoleOpen() {
		return irc.ComposerByteBudget("", frame)
	}
	return irc.ComposerByteBudgetForDraft(c.SelectedTarget(), draft, frame)
}

// ClampUtf8Prefix keeps a UTF-8 prefix inside maxBytes. It mirrors
// IrcController::clampUtf8Prefix.
func (c *Controller) ClampUtf8Prefix(text string, maxBytes int) string {
	return irc.ClampUtf8Prefix(text, maxBytes)
}

// StatusConsoleOpen reports the raw Status-console model flag, mirroring
// IrcStatusConsole::isOpen. It stays true after a cleared selection, exactly as
// in the Qt client, so the focused network still follows the console and a
// command typed on the console surface is still dispatched as a Status command.
func (c *Controller) StatusConsoleOpen() bool { return c.consoleOpen }

// ClearConversationSelection forgets the selection, leaving the Status surface
// as the focused network. It mirrors IrcController::clearConversationSelection
// (src/irc/irccontroller.cpp:1252-1271).
func (c *Controller) ClearConversationSelection() {
	// Clearing the selection withdraws a hint the previous target was still
	// publishing. It mirrors IrcController::clearConversationSelection
	// (src/irc/irccontroller.cpp:1255-1260).
	if c.typingTarget != "" {
		if previous := c.selectedSession(); previous != nil {
			previous.SendTyping(c.typingTarget, irc.TypingDone)
		}
		c.typingTarget = ""
	}
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

// CloseDirectMessage drops the selected direct message, or a channel you have
// left, and selects its neighbor. A channel you are in stays. It returns
// false when nothing closeable is selected.
func (c *Controller) CloseDirectMessage() bool {
	networkID := c.FocusedNetworkID()
	if !c.closeSelectedConversation() {
		return false
	}
	if c.LastErrorFor(networkID) == "" {
		return true
	}
	c.setLastError(networkID, "")
	c.notifyStatusChanged()
	return true
}

// ChannelJoined reports whether the selected conversation is a channel the
// user is currently in.
func (c *Controller) ChannelJoined() bool {
	if c.selected == nil {
		return false
	}
	conversation := c.reducer.Find(*c.selected)
	if conversation == nil {
		return false
	}
	channel := conversation.Channel()
	return channel != nil && channel.Joined
}

// CanCloseSelection reports whether Close applies: a direct message, or a
// channel the user has left.
func (c *Controller) CanCloseSelection() bool {
	if c.selected == nil {
		return false
	}
	conversation := c.reducer.Find(*c.selected)
	if conversation == nil || !conversation.IsChannel() {
		return conversation != nil
	}
	channel := conversation.Channel()
	return channel != nil && !channel.Joined
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

// ConversationTopic returns the plain channel topic for one conversation. A
// direct message has none; the "Direct message with" caption is not a topic.
// It mirrors IrcController::conversationTopic.
func (c *Controller) ConversationTopic(networkID, target string) string {
	if c == nil || networkID == "" || target == "" {
		return ""
	}
	conversation := c.reducer.Find(c.reducer.ConversationKey(networkID, target))
	if conversation == nil {
		return ""
	}
	channel := conversation.Channel()
	if channel == nil {
		return ""
	}
	return irc.PlainIrcText(channel.Topic)
}

// PeerRealname returns a nick's meaningful GECOS, or "" when it is unknown
// or a placeholder. It mirrors IrcController::peerRealname.
func (c *Controller) PeerRealname(networkID, nick string) string {
	if c == nil || networkID == "" || nick == "" {
		return ""
	}
	stored := c.reducer.NickPresence(networkID, nick).Realname
	if !irc.MeaningfulRealname(stored, nick) {
		return ""
	}
	return stored
}

// JumpScore ranks one jump row. It mirrors IrcController::jumpScore.
func JumpScore(query, name, detail string) int {
	return irc.JumpScore(query, name, detail)
}

// JumpResultLimit is how many Ctrl+K rows the overlay keeps.
func JumpResultLimit() int { return irc.JumpResultLimit }

// SelectedPeerHeader returns the query header for the selected direct message.
// A channel, the console, and an empty selection report false. Presence is
// "online", "away", or "offline". Realname is the meaningful gecos. Labels
// are the short account, operator, and bot list.
func (c *Controller) SelectedPeerHeader() (PeerHeader, bool) {
	if c.consoleOpen || c.selected == nil || c.IsChannel() {
		return PeerHeader{}, false
	}
	nick := c.SelectedTarget()
	if conversation := c.reducer.Find(*c.selected); conversation != nil && conversation.Target != "" {
		nick = conversation.Target
	}
	if nick == "" {
		return PeerHeader{}, false
	}
	normalized := c.reducer.ConversationKey(c.selected.NetworkID, nick).NormalizedTarget
	presence := "offline"
	switch c.reducer.PeerPresence(c.selected.NetworkID, normalized) {
	case irc.PeerOnline:
		presence = "online"
	case irc.PeerAway:
		presence = "away"
	}
	return PeerHeader{
		Nick:     nick,
		Presence: presence,
		Realname: c.reducer.MeaningfulRealname(c.selected.NetworkID, nick),
		Labels:   c.reducer.PeerFactLabels(c.selected.NetworkID, nick),
	}, true
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

// MarkAllRead marks every conversation read. Sidebar badges follow the cleared
// unread and mention counts. It mirrors IrcController::markAllRead.
func (c *Controller) MarkAllRead() {
	if !c.reducer.MarkAllRead() {
		return
	}
	c.Publish(irc.ViewNotify{Conversations: true})
}

// SetWindowActive records focus. Gaining focus consumes the selected
// conversation's unread; the "new messages" mark survives. It mirrors
// IrcController::setWindowActive (src/irc/irccontroller.cpp:800-817).
func (c *Controller) SetWindowActive(active bool) {
	c.reducer.SetWindowActive(active)
	c.transcriptFocused = active
	if !active || c.selected == nil {
		return
	}
	if c.reducer.MarkRead(*c.selected) {
		c.Publish(irc.ViewNotify{Conversations: true})
	}
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

// Disconnect stops the live session for networkID while keeping it registered,
// so the model can start it again with Apply. It mirrors
// IrcConnection::disconnectSelected -> IrcSession::quit: a missing session or an
// already-idle one reports false. Returns true when a session was stopped.
func (c *Controller) Disconnect(networkID string) bool {
	s := c.manager.Find(networkID)
	if s == nil {
		return false
	}
	if !s.Quit("") {
		return false
	}
	c.refreshConnectionStatus()
	return true
}

// SessionIsLive reports whether networkID is registered and its session is not
// Idle or Failed.
func (c *Controller) SessionIsLive(networkID string) bool {
	return c.manager.IsLive(networkID)
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
	networkID := config.NetworkID
	s.SetChatHistoryResume(func(target string) (time.Time, bool) {
		if when, ok := c.playback.ResumeTime(networkID, target); ok {
			return when, true
		}
		// Registered is delivered after the read that carried 001 is released.
		// A self-JOIN in that same read runs first. The store still holds the
		// previous connection's stamps, which is the snapshot that delivery
		// is about to take. Once the snapshot exists, a missing target stays
		// missing so a live line cannot move this connection's AFTER bound.
		if c.playback.snapshotTaken(networkID) {
			return time.Time{}, false
		}
		features := c.reducer.ServerFeatures(networkID)
		return c.playbackTimes.Noted(networkID, target, features.CaseMapping())
	})
	s.SetHandler(c)
	c.hydrateMutes(config.NetworkID)
	c.syncHighlightWords(config.NetworkID)
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
	c.autoaway.ForgetNetwork(networkID)
	delete(c.openDirectsMotdSeen, networkID)
	c.monitor.ForgetPresence(networkID)
	c.channelLists.Forget(networkID)
	if current := c.ChannelListSnapshot(); current.NetworkID == networkID {
		c.channelList.Clear()
		c.channelListOpen = false
	}
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
	delete(c.unawaySent, networkID)
	c.commands.ForgetNetwork(networkID)
	c.replies.Forget(networkID)
	c.monitor.ForgetPresence(networkID)
	c.autoaway.ForgetNetwork(networkID)
	delete(c.openDirectsMotdSeen, networkID)
	c.openDirects.Forget(networkID)
	c.closed.Forget(networkID)
	c.playbackTimes.Forget(networkID)
	c.scrollPlaces.Forget(networkID)
	c.playback.DropSnapshot(networkID)
	c.channelLists.Forget(networkID)
	c.inbox.PurgeNetwork(networkID)
	c.notifyInboxChanged()
	if current := c.ChannelListSnapshot(); current.NetworkID == networkID {
		c.channelList.Clear()
		c.channelListOpen = false
	}
	if c.selected != nil && c.selected.NetworkID == networkID {
		c.ClearConversationSelection()
	}
	c.reloadModels()
	c.notifyCapabilitiesChanged()
	c.notifyStatusChanged()
}

// SetNetworkOrder records the network display order, republishes the sidebar,
// and persists the order unless ephemeral. An unchanged order is a no-op. It
// mirrors IrcController::setNetworkOrder (src/irc/irccontroller.cpp:559-564)
// plus IrcConnection::persistNetworkOrder.
func (c *Controller) SetNetworkOrder(order []string) {
	if stringSlicesEqual(c.networkOrder, order) {
		return
	}
	c.networkOrder = append([]string(nil), order...)
	c.persistNetworkOrder()
	c.Publish(irc.ViewNotify{Conversations: true})
}

// persistNetworkOrder saves the recorded sidebar order. An ephemeral
// controller never touches disk.
func (c *Controller) persistNetworkOrder() {
	if c.ephemeral {
		return
	}
	savePreferenceList(networkOrderKey, c.networkOrder)
}

// persistCollapsedNetworks saves the collapsed ids in display order.
func (c *Controller) persistCollapsedNetworks() {
	if c.ephemeral {
		return
	}
	savePreferenceList(collapsedNetworksKey, c.collapsedNetworkIDs())
}

// collapsedNetworkIDs returns the ids currently collapsed, in display order.
func (c *Controller) collapsedNetworkIDs() []string {
	order := c.NetworkOrder()
	ids := make([]string, 0, len(order))
	for _, id := range order {
		if c.networkCollapsed[id] {
			ids = append(ids, id)
		}
	}
	return ids
}

// IsNetworkCollapsed reports whether the sidebar collapses one network. An
// empty id is never collapsed.
func (c *Controller) IsNetworkCollapsed(networkID string) bool {
	return networkID != "" && c.networkCollapsed[networkID]
}

// SetNetworkCollapsed records whether the sidebar collapses one network and
// republishes the sidebar when the flag moved. An empty id is a no-op.
func (c *Controller) SetNetworkCollapsed(networkID string, collapsed bool) {
	if networkID == "" {
		return
	}
	if c.networkCollapsed[networkID] == collapsed {
		return
	}
	if collapsed {
		c.networkCollapsed[networkID] = true
	} else {
		delete(c.networkCollapsed, networkID)
	}
	c.persistCollapsedNetworks()
	c.Publish(irc.ViewNotify{Conversations: true})
}

// SetAllNetworksCollapsed collapses or expands every network in the current
// display order, publishing the sidebar once. It is a no-op when every network
// already has the requested value.
func (c *Controller) SetAllNetworksCollapsed(collapsed bool) {
	order := c.NetworkOrder()
	changed := false
	for _, id := range order {
		if c.networkCollapsed[id] != collapsed {
			changed = true
			break
		}
	}
	if !changed {
		return
	}
	for _, id := range order {
		if collapsed {
			c.networkCollapsed[id] = true
		} else {
			delete(c.networkCollapsed, id)
		}
	}
	c.persistCollapsedNetworks()
	c.Publish(irc.ViewNotify{Conversations: true})
}

// MoveNetwork moves one network by delta places in the effective display order
// and reports whether the order changed. It returns false for a zero delta, an
// unknown network, or a move past either end of the list. Focus stays with the
// network id; the controller keeps no focus.
func (c *Controller) MoveNetwork(networkID string, delta int) bool {
	order := c.NetworkOrder()
	idx := -1
	for index, id := range order {
		if id == networkID {
			idx = index
			break
		}
	}
	if idx < 0 || delta == 0 {
		return false
	}
	target := idx + delta
	if target < 0 || target >= len(order) {
		return false
	}
	moved := make([]string, 0, len(order))
	moved = append(moved, order[:idx]...)
	moved = append(moved, order[idx+1:]...)
	newOrder := make([]string, 0, len(order))
	newOrder = append(newOrder, moved[:target]...)
	newOrder = append(newOrder, networkID)
	newOrder = append(newOrder, moved[target:]...)
	c.SetNetworkOrder(newOrder)
	return true
}

// --- Message handling -----------------------------------------------------

func (c *Controller) handleMessage(networkID string, message irc.Message) {
	// The playback clock reads every inbound PRIVMSG before routing, so a line
	// that only lands on another client's transcript still resumes there.
	c.notePlaybackClock(networkID, message)
	if message.Command == "FAIL" {
		c.replies.RouteOwnMetadataFail(networkID, message)
	}
	switch message.Command {
	case "005":
		if len(message.Params) > 2 {
			features := c.reducer.ServerFeatures(networkID)
			last := len(message.Params) - 1
			for index := 1; index < last; index++ {
				features.ApplyToken(irc.WireText([]byte(message.Params[index])))
			}
			c.reducer.SetServerFeatures(networkID, features)
			c.monitor.SubscribeMonitors(networkID)
			c.notifyViewChanged()
		}
		return
	case "730":
		c.monitor.HandleMonitorPresence(networkID, message, true)
		return
	case "731":
		c.monitor.HandleMonitorPresence(networkID, message, false)
		return
	case "734":
		c.monitor.HandleMonitorListFull(networkID, message)
		return
	}
	if c.handleChannelListMessage(networkID, message) {
		return
	}
	if message.Command == "376" || message.Command == "422" {
		// A burst of 005 tokens is applied above without touching the models;
		// MOTD end latches the case mapping and restores open directs.
		features := c.reducer.ServerFeatures(networkID)
		if !features.CaseMappingKnown() {
			features.MarkCaseMappingKnown()
			c.reducer.SetServerFeatures(networkID, features)
		}
		c.noteOpenDirectsMotd(networkID)
	}

	features := c.reducer.ServerFeatures(networkID)
	currentNick := c.currentNicks[networkID]
	events := irc.Translate(networkID, currentNick, features, message, c.now())
	for index, event := range events {
		if nick, ok := event.(irc.NickEvent); ok {
			if features.CaseMapping().Equals(nick.OldNick, currentNick) {
				c.currentNicks[networkID] = nick.NewNick
			}
		}
		// A WHO line is away, then nick facts, for one person. The member
		// snapshot reads both from the reducer, so it rebuilds once.
		// Other pairs stay separate: a repeated account event can return
		// before it publishes, and skipping the first row would drop it.
		c.coalesceMemberRow = false
		if index+1 < len(events) {
			_, away := event.(irc.AwayEvent)
			_, facts := events[index+1].(irc.NickFactsEvent)
			if away && facts {
				nick, nickOK := memberRowNick(c.reducer, event)
				next, nextOK := memberRowNick(c.reducer, events[index+1])
				c.coalesceMemberRow = nickOK && nextOK && nick == next
			}
		}
		c.Apply(event)
		c.coalesceMemberRow = false
		if join, ok := event.(irc.JoinEvent); ok {
			mapping := features.CaseMapping()
			selfJoin := mapping.Equals(join.Nick, currentNick)
			joinKey := c.reducer.ConversationKey(join.NetworkID, join.Channel)
			if selfJoin && c.commands.TakeCancelledSelfJoin(joinKey) {
				if s := c.manager.Find(join.NetworkID); s != nil {
					s.Part(join.Channel)
				}
				// Close drops the row. Leave keeps it, just not joined.
				if c.reducer.Closed(joinKey) {
					c.dismissChannel(join.NetworkID, join.Channel)
				} else {
					c.markChannelLeft(join.NetworkID, join.Channel)
				}
				continue
			}
			if selfJoin && features.IsChannel(join.Channel) {
				c.inbox.ConsumeInvite(join.NetworkID, join.Channel, mapping)
				c.notifyInboxChanged()
				if s := c.manager.Find(join.NetworkID); s != nil {
					if pending, ok := s.PendingInvite(); ok &&
						mapping.Equals(join.Channel, pending.Channel) {
						c.SelectConversation(join.NetworkID, join.Channel)
					}
					// A channel joined before the playback request still
					// needs its own PLAY; a batch already kept ends the retry.
					c.playback.NoteJoinedChannel(join.NetworkID, join.Channel)
					c.requestChannelPlayback(s, join.Channel)
				}
			}
		}
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
// produces: keeping playback clocks for landed replay lines, and remembering
// queries the user replied to. It mirrors IrcController::noteKeptReplay
// (src/irc/irccontroller.cpp:2288-2302).
func (c *Controller) noteKeptReplay() {
	c.playback.NoteKeptReplay(func(networkID string) bool {
		return c.capabilities[networkID].Contains(irc.CapabilityZncPlayback)
	})
	// A kept self line means the user replied in that query. Remember it only
	// when the direct exists; a dropped self-only batch must not.
	for _, query := range c.reducer.TakeRememberedQueries() {
		if c.reducer.Find(c.reducer.ConversationKey(query.NetworkID, query.Target)) == nil {
			continue
		}
		c.rememberOpenDirect(query.NetworkID, query.Target)
	}
}

// notePlaybackClock forwards one inbound line to the playback coordinator. It
// mirrors IrcController::notePlaybackClock.
func (c *Controller) notePlaybackClock(networkID string, message irc.Message) {
	c.playback.NotePlaybackClock(networkID, message,
		c.currentNicks[networkID],
		c.capabilities[networkID].Contains(irc.CapabilityZncPlayback))
}

// --- Open directs and playback --------------------------------------------

// persistableDirectTarget reports whether target is a direct message worth
// persisting: non-empty, not a channel, and not a network service. It mirrors
// IrcController::persistableDirectTarget (src/irc/irccontroller.cpp:1561-1569).
func (c *Controller) persistableDirectTarget(networkID, target string) bool {
	if networkID == "" || target == "" {
		return false
	}
	features := c.reducer.ServerFeatures(networkID)
	if features.IsChannel(target) {
		return false
	}
	return !irc.TargetLooksLikeService(target, features)
}

// persistClosed writes one closed channel into the store that survives welcome.
func (c *Controller) persistClosed(key irc.ConversationKey, closed bool) {
	if c.closed == nil || key.NetworkID == "" || key.NormalizedTarget == "" {
		return
	}
	features := c.reducer.ServerFeatures(key.NetworkID)
	mapping := features.CaseMapping()
	if closed {
		c.closed.Add(key.NetworkID, key.NormalizedTarget, mapping)
		return
	}
	c.closed.Remove(key.NetworkID, key.NormalizedTarget, mapping)
}

// containsClosed reports a channel close that welcome already dropped from memory.
func (c *Controller) containsClosed(key irc.ConversationKey) bool {
	if c.closed == nil || key.NetworkID == "" || key.NormalizedTarget == "" {
		return false
	}
	features := c.reducer.ServerFeatures(key.NetworkID)
	mapping := features.CaseMapping()
	return c.closed.Contains(key.NetworkID, key.NormalizedTarget, mapping)
}

// otherReopenedDirect reports a live message, notice, or action from someone
// else that is about to recreate a query the user closed. The check runs
// before the reducer inserts the row and clears nothing on its own.
func (c *Controller) otherReopenedDirect(event irc.Event) (networkID, target string, ok bool) {
	var key irc.ConversationKey
	var author, stored string
	if message, matched := messageEventOf(event); matched {
		key, author, stored = message.Conversation, message.Author, message.Target
	} else if notice, matched := noticeEventOf(event); matched {
		key, author, stored = notice.Conversation, notice.Author, notice.Target
	} else if action, matched := actionEventOf(event); matched {
		key, author, stored = action.Conversation, action.Author, action.Target
	} else {
		return "", "", false
	}
	features := c.reducer.ServerFeatures(key.NetworkID)
	if features.IsChannel(key.NormalizedTarget) {
		return "", "", false
	}
	mapping := features.CaseMapping()
	if mapping.Equals(author, c.currentNicks[key.NetworkID]) {
		return "", "", false
	}
	if c.reducer.Find(key) != nil {
		return "", "", false
	}
	if stored == "" {
		stored = key.NormalizedTarget
	}
	if c.closed == nil {
		return "", "", false
	}
	if !c.closed.Contains(key.NetworkID, stored, mapping) &&
		!c.closed.Contains(key.NetworkID, key.NormalizedTarget, mapping) {
		return "", "", false
	}
	return key.NetworkID, stored, true
}

// rememberOpenDirect records a direct message worth restoring. It mirrors
// IrcController::rememberOpenDirect (src/irc/irccontroller.cpp:1571-1585).
func (c *Controller) rememberOpenDirect(networkID, target string) {
	if !c.persistableDirectTarget(networkID, target) {
		return
	}
	conversation := c.reducer.Find(c.reducer.ConversationKey(networkID, target))
	if conversation != nil && conversation.IsChannel() {
		return
	}
	stored := target
	if conversation != nil && conversation.Target != "" {
		stored = conversation.Target
	}
	features := c.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	c.openDirects.Undismiss(networkID, stored, mapping)
	c.openDirects.Add(networkID, stored, mapping)
	if c.closed != nil {
		c.closed.Remove(networkID, stored, mapping)
	}
}

// forgetOpenDirect drops a remembered direct message. It mirrors
// IrcController::forgetOpenDirect (src/irc/irccontroller.cpp:1587-1593).
func (c *Controller) forgetOpenDirect(networkID, target string) {
	if networkID == "" || target == "" {
		return
	}
	features := c.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	c.openDirects.Remove(networkID, target, mapping)
	c.openDirects.Dismiss(networkID, target, mapping)
	if c.closed != nil {
		c.closed.Add(networkID, target, mapping)
	}
}

// noteSelfAuthoredDirect remembers target when the event author is our own
// current nick and the conversation is an existing non-channel. It mirrors the
// message/notice/action branches of IrcController::apply
// (src/irc/irccontroller.cpp:2064-2091).
func (c *Controller) noteSelfAuthoredDirect(networkID, author, target string) {
	features := c.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	if !mapping.Equals(author, c.currentNicks[networkID]) {
		return
	}
	conversation := c.reducer.Find(c.reducer.ConversationKey(networkID, target))
	if conversation != nil && !conversation.IsChannel() {
		c.rememberOpenDirect(networkID, target)
	}
}

// noteOpenDirectsMotd restores the network's open directs once, at MOTD end,
// then releases held query playback, asks the bouncer for buffers, and catches
// up with CHATHISTORY from the last place reached. It mirrors
// IrcController::noteOpenDirectsMotd (src/irc/irccontroller.cpp:1676-1696).
func (c *Controller) noteOpenDirectsMotd(networkID string) {
	if networkID == "" || c.openDirectsMotdSeen[networkID] {
		return
	}
	c.openDirectsMotdSeen[networkID] = true
	c.restoreOpenDirects(networkID)
	// Held self-only query batches splice into directs this restore just
	// opened. Release after restore, and do not drop them first.
	spliced := c.reducer.ReleasePendingQueryPlayback(networkID)
	c.noteKeptReplay()
	if spliced {
		c.Publish(irc.ViewNotify{Conversations: true, Messages: true})
	}
	if s := c.manager.Find(networkID); s != nil {
		c.requestZncPlayback(s)
		c.requestChatHistoryCatchUp(s)
	}
}

// restoreOpenDirects reopens the persisted direct messages that are not already
// loaded. It mirrors IrcController::restoreOpenDirects
// (src/irc/irccontroller.cpp:1614-1642).
func (c *Controller) restoreOpenDirects(networkID string) {
	if !c.ReopenDirects() || networkID == "" {
		return
	}
	prune := c.openDirectsMotdSeen[networkID]
	features := c.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	created := false
	for _, target := range c.openDirects.Listed(networkID, mapping) {
		if !c.persistableDirectTarget(networkID, target) {
			if prune {
				c.openDirects.Remove(networkID, target, mapping)
			}
			continue
		}
		key := c.reducer.ConversationKey(networkID, target)
		if c.reducer.Find(key) != nil {
			continue
		}
		if c.reducer.EnsureConversation(key, target, irc.CauseRestore) != nil {
			created = true
		} else if prune {
			c.openDirects.Remove(networkID, target, mapping)
		}
	}
	if !created {
		return
	}
	c.reloadModels()
}

// requestZncPlayback asks the session's bouncer for the buffers it may hold. It
// mirrors IrcController::requestZncPlayback (src/irc/irccontroller.cpp:2311-2328).
func (c *Controller) requestZncPlayback(s *session.Session) {
	if s == nil {
		return
	}
	networkID := s.NetworkID()
	features := c.reducer.ServerFeatures(networkID)
	restoredDirects := c.openDirects.Listed(networkID, features.CaseMapping())
	c.playback.Request(s,
		c.capabilities[networkID].Contains(irc.CapabilityZncPlayback),
		c.openDirectsMotdSeen[networkID],
		restoredDirects,
		func(target string) bool {
			return c.persistableDirectTarget(networkID, target)
		})
}

// requestChatHistoryCatchUp fills lines that arrived while away when the
// server offers chathistory. It mirrors IrcController::requestChatHistoryCatchUp.
func (c *Controller) requestChatHistoryCatchUp(s *session.Session) {
	if s == nil {
		return
	}
	networkID := s.NetworkID()
	caps := c.capabilities[networkID]
	chatHistory := caps.Contains(irc.CapabilityChatHistory) && caps.Contains(irc.CapabilityBatch)
	features := c.reducer.ServerFeatures(networkID)
	c.playback.RequestCatchUp(
		s,
		chatHistory,
		c.openDirectsMotdSeen[networkID],
		c.openDirects.Listed(networkID, features.CaseMapping()),
		func(target string) bool {
			return c.persistableDirectTarget(networkID, target)
		},
		c.now(),
	)
}

// requestChannelPlayback retries one joined channel's PLAY. It mirrors
// IrcController::requestZncChannelPlayback (src/irc/irccontroller.cpp:2330-2338).
func (c *Controller) requestChannelPlayback(s *session.Session, channel string) {
	if s == nil {
		return
	}
	networkID := s.NetworkID()
	c.playback.RequestChannelPlayback(s, channel,
		c.capabilities[networkID].Contains(irc.CapabilityZncPlayback),
		c.openDirectsMotdSeen[networkID])
}

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

func (c *Controller) notifyViewChanged() {
	if c.OnViewChanged != nil {
		c.OnViewChanged()
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

// nickEventOf unwraps a NickEvent value or pointer. It mirrors the reducer's
// dereferenceEvent so Controller.Apply handles either shape.
func nickEventOf(event irc.Event) (irc.NickEvent, bool) {
	switch value := event.(type) {
	case irc.NickEvent:
		return value, true
	case *irc.NickEvent:
		if value != nil {
			return *value, true
		}
	}
	return irc.NickEvent{}, false
}

// messageEventOf unwraps a MessageEvent value or pointer.
func messageEventOf(event irc.Event) (irc.MessageEvent, bool) {
	switch value := event.(type) {
	case irc.MessageEvent:
		return value, true
	case *irc.MessageEvent:
		if value != nil {
			return *value, true
		}
	}
	return irc.MessageEvent{}, false
}

// noticeEventOf unwraps a NoticeEvent value or pointer.
func noticeEventOf(event irc.Event) (irc.NoticeEvent, bool) {
	switch value := event.(type) {
	case irc.NoticeEvent:
		return value, true
	case *irc.NoticeEvent:
		if value != nil {
			return *value, true
		}
	}
	return irc.NoticeEvent{}, false
}

// actionEventOf unwraps an ActionEvent value or pointer.
func actionEventOf(event irc.Event) (irc.ActionEvent, bool) {
	switch value := event.(type) {
	case irc.ActionEvent:
		return value, true
	case *irc.ActionEvent:
		if value != nil {
			return *value, true
		}
	}
	return irc.ActionEvent{}, false
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
