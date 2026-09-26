package session

import (
	"fmt"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"
	"unicode"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// This file is the Go port of IrcSession in src/irc/ircsession.{h,cpp}: the
// connection state machine, CAP/SASL/registration driver, ping watchdog,
// reconnect backoff, labeled requests, the history-batch ledger, and the
// outbound command writers.
//
// Concurrency model
//
// The C++ session is single-threaded: Qt delivers transport signals, QTimer
// firings, and public slot calls on one event loop, and IrcSession emits its
// own signals re-entrantly. The Go transport may call the Sink from a reader
// goroutine, and the timer seam fires on its own goroutine, so all session
// state is guarded by one mutex.
//
// To preserve the C++ ordering of signals and to never re-enter the mutex,
// every entry point runs its body under locked(), which queues handler
// notifications and transport calls that can synchronously call back into the
// session (transport.Shutdown -> Sink.Disconnected, transport.Write ->
// Sink.Error, transport.Connect -> Sink.Connected). The queue is drained in
// order after the mutex is released. A queued transport call that re-enters
// the session runs inline, exactly like the synchronous Qt signal, and may
// queue and drain its own work.
//
// One divergence remains: notifications are delivered after the current entry
// point finishes mutating state rather than re-entrantly in the middle of it.
// Handler order is preserved, and handlers never run while the lock is held.

// SessionState is the connection state machine. It mirrors IrcSession::State
// in src/irc/ircsession.h:99-111.
type SessionState int

const (
	StateIdle SessionState = iota
	StateConnecting
	StateStsUpgrading
	StateCapLs
	StateCapReq
	StateSasl
	StateRegistering
	StateRegistered
	StateClosing
	StateReconnecting
	StateFailed
)

// String renders the C++ enumerator name for logs and tests.
func (s SessionState) String() string {
	switch s {
	case StateIdle:
		return "Idle"
	case StateConnecting:
		return "Connecting"
	case StateStsUpgrading:
		return "StsUpgrading"
	case StateCapLs:
		return "CapLs"
	case StateCapReq:
		return "CapReq"
	case StateSasl:
		return "Sasl"
	case StateRegistering:
		return "Registering"
	case StateRegistered:
		return "Registered"
	case StateClosing:
		return "Closing"
	case StateReconnecting:
		return "Reconnecting"
	case StateFailed:
		return "Failed"
	}
	return "Unknown"
}

// ErrorKind classifies a session failure. It mirrors IrcSession::ErrorKind in
// src/irc/ircsession.h:114-120. The order is significant because
// reportedRetryErrors packs one dedup bit per kind.
type ErrorKind int

const (
	ErrorProtocol ErrorKind = iota
	ErrorAuthentication
	ErrorTLS
	ErrorNetwork
	ErrorRegistration
)

// String renders the C++ enumerator name.
func (k ErrorKind) String() string {
	switch k {
	case ErrorProtocol:
		return "Protocol"
	case ErrorAuthentication:
		return "Authentication"
	case ErrorTLS:
		return "TLS"
	case ErrorNetwork:
		return "Network"
	case ErrorRegistration:
		return "Registration"
	}
	return "Unknown"
}

// PendingInvite is the last unhandled INVITE. It mirrors IrcPendingInvite.
type PendingInvite struct {
	Nick    string
	Channel string
}

// IgnoreFilter drops an inbound message before translation, history capture,
// or status classification. networkID is the session's network id (the C++
// callback passed the current nick; the Go contract uses the network id).
type IgnoreFilter func(message irc.Message, networkID string) bool

// Handler receives session events. It is the Go shape of the IrcSession
// signals. Handlers run outside the session mutex and must not block: the real
// transport delivers bytes from a reader goroutine, and blocking it stalls the
// socket.
type Handler interface {
	StateChanged(state SessionState)
	ErrorOccurred(networkID string, kind ErrorKind, message string)
	Registered(networkID string)
	ReconnectScheduled(networkID string, delayMs, attempt int)
	MessageReceived(networkID string, message irc.Message)
	HistoryBatchReceived(networkID string, batch irc.HistoryBatch)
	StatusEntry(entry irc.StatusEntry)
	CapabilitiesChanged(networkID string, capabilities irc.CapabilitySet)
	RequestLabelFinished(networkID, requestLabel string)
	AutojoinChannelsChanged(networkID string, channels []string, keys map[string]string)
}

// ReachabilitySource reports when the host's network becomes reachable again,
// so a scheduled reconnect can start early. It mirrors IrcReachabilitySource.
// Set it with SetReachabilitySource; nil disables it.
type ReachabilitySource interface {
	Reachable() <-chan struct{}
}

const (
	kHistoryLimit         = 100
	kHistoryBufferCeiling = 256
	kMaxOpenBatches       = 16
	kMaxIgnoredBatches    = 32

	kPingWatchdogToken       = "omairc-watchdog"
	kCtcpPingPayloadMaxBytes = 32
	kSaslScramSha256         = "SCRAM-SHA-256"

	int32Max = int(1<<31 - 1)
)

const kCtcpReplyInterval = 5000 * time.Millisecond

type pingWatchdogState int

const (
	watchdogOff pingWatchdogState = iota
	watchdogWatching
	watchdogProbing
)

type registrationNickState int

const (
	registrationNickConfigured registrationNickState = iota
	registrationNickUnderscore
	registrationNickDigit
)

type saslScramState int

const (
	saslScramIdle saslScramState = iota
	saslScramAwaitPrompt
	saslScramAwaitServerFirst
	saslScramAwaitServerFinal
	saslScramVerified
)

// openBatch is the Go shape of IrcSession::OpenBatch
// (src/irc/ircsession.h:328-336).
type openBatch struct {
	batchType    string
	parent       string
	replayRoot   string
	kind         irc.HistoryKind
	hasKind      bool
	collected    irc.HistoryBatch
	generation   int
	requestLabel string
}

// sessionAction is one deferred unit of work. It either delivers a handler
// notification or performs a transport call that may re-enter the session.
type sessionAction func(Handler)

// Session is the per-network state machine. Build one with NewSession and
// install a Handler before Start. All methods are safe for concurrent use.
type Session struct {
	mu sync.Mutex

	config    SessionConfig
	transport Transport
	clock     Clock

	portNumber uint16
	tlsEnabled bool

	autojoinChannels []string
	autojoinKeys     map[string]string
	pendingJoinKeys  map[string]string
	nickname         string

	sts         *STSStore
	stsDuration *int64
	pendingSTS  *STSAdvertisement

	features     irc.ServerFeatures
	channelTypes string
	historyLimit int

	capabilities          *irc.CapabilityNegotiation
	metadataCapability    irc.MetadataCapability
	publishedCapabilities irc.CapabilitySet

	typing typingPublisher

	framer irc.Framer

	openBatches       map[string]*openBatch
	ignoredBatches    map[string]struct{}
	historyGeneration map[string]int
	historyAsked      map[string]struct{}
	historyPending    map[string]int

	pendingLabels    map[string]time.Time
	nextRequestLabel uint64

	ctcpReplyClock map[string]time.Time

	pingWatchdog        pingWatchdogState
	reconnectAttempt    int
	reportedRetryErrors uint32

	registrationSent    bool
	registrationNick    registrationNickState
	saslRequested       bool
	saslPending         bool
	saslSucceeded       bool
	saslMechanism       string
	saslExchangeStarted bool
	saslScramExchange   bool
	saslScramStep       saslScramState
	saslIncoming        saslChallengeBuffer
	scram               *SASLScram

	capabilityNegotiationEnded bool
	capabilityListSeen         bool

	expectedDisconnect       bool
	reconnectAfterDisconnect bool

	sessionState SessionState

	ignoreFilter  IgnoreFilter
	pendingInvite *PendingInvite

	handler Handler

	reachability     ReachabilitySource
	reachabilityStop chan struct{}

	reconnectTimer  Timer
	capabilityTimer Timer
	pingTimer       Timer
	labelTimer      Timer

	// deferred holds work to run after the mutex is released. Guarded by mu.
	deferred []sessionAction
}

// Session implements the transport Sink: SetSink(s) makes it the receiver of
// the lifecycle and bytes events that drive the state machine.
var _ Sink = (*Session)(nil)

// NewSession builds a session. transport must be non-nil; clock may be nil, in
// which case RealClock is used. It mirrors the IrcSession constructor
// (src/irc/ircsession.cpp:293-407): the transport's sink is installed here and
// the capability negotiation is primed with whether SASL credentials exist.
func NewSession(config SessionConfig, transport Transport, clock Clock) *Session {
	if clock == nil {
		clock = RealClock{}
	}
	s := &Session{
		config:            config,
		transport:         transport,
		clock:             clock,
		portNumber:        config.Port,
		tlsEnabled:        config.TLSEnabled,
		autojoinChannels:  append([]string(nil), config.AutojoinChannels...),
		autojoinKeys:      cloneStringMap(config.AutojoinKeys),
		pendingJoinKeys:   map[string]string{},
		nickname:          config.Nick,
		sts:               NewSTSStore(clock),
		historyLimit:      kHistoryLimit,
		features:          irc.NewServerFeatures(),
		capabilities:      irc.NewCapabilityNegotiation(config.saslSecret() != ""),
		typing:            newTypingPublisher(),
		openBatches:       map[string]*openBatch{},
		ignoredBatches:    map[string]struct{}{},
		historyGeneration: map[string]int{},
		historyAsked:      map[string]struct{}{},
		historyPending:    map[string]int{},
		pendingLabels:     map[string]time.Time{},
		ctcpReplyClock:    map[string]time.Time{},
		scram:             NewSASLScram(),
		sessionState:      StateIdle,
	}
	if transport != nil {
		transport.SetSink(s)
	}
	return s
}

// locked runs f under the session mutex, then drains the deferred queue in
// order after releasing it. See the file comment for why transport calls and
// handler notifications are deferred.
func (s *Session) locked(f func()) {
	s.mu.Lock()
	f()
	deferred := s.deferred
	s.deferred = nil
	handler := s.handler
	s.mu.Unlock()
	for _, action := range deferred {
		action(handler)
	}
}

// emit queues one handler notification.
func (s *Session) emit(notify func(Handler)) {
	s.deferred = append(s.deferred, sessionAction(func(handler Handler) {
		if handler != nil {
			notify(handler)
		}
	}))
}

// post queues one transport call that may synchronously re-enter the session.
func (s *Session) post(action func()) {
	s.deferred = append(s.deferred, sessionAction(func(Handler) { action() }))
}

// SetHandler installs the event receiver. It is safe to call before or after
// Start.
func (s *Session) SetHandler(handler Handler) {
	s.mu.Lock()
	s.handler = handler
	s.mu.Unlock()
}

// SetIgnoreFilter installs the drop predicate. A nil filter keeps everything.
func (s *Session) SetIgnoreFilter(filter IgnoreFilter) {
	s.mu.Lock()
	s.ignoreFilter = filter
	s.mu.Unlock()
}

// SetReachabilitySource installs the optional reachability source. A reachable
// signal starts a scheduled reconnect without waiting out the backoff. Pass
// nil to remove the source. The channel is consumed on a goroutine, so the
// signal is delivered asynchronously.
func (s *Session) SetReachabilitySource(source ReachabilitySource) {
	s.mu.Lock()
	if s.reachabilityStop != nil {
		close(s.reachabilityStop)
		s.reachabilityStop = nil
	}
	s.reachability = source
	if source != nil {
		stop := make(chan struct{})
		s.reachabilityStop = stop
		reachable := source.Reachable()
		go func() {
			for {
				select {
				case <-stop:
					return
				case _, ok := <-reachable:
					if !ok {
						return
					}
					s.locked(s.beginReconnectAttemptLocked)
				}
			}
		}()
	}
	s.mu.Unlock()
}

// NetworkID returns the network id.
func (s *Session) NetworkID() string { return s.config.NetworkID }

// Name returns the resolved display name.
func (s *Session) Name() string { return s.config.resolvedName() }

// Host returns the configured host.
func (s *Session) Host() string { return s.config.Host }

// Port returns the active port, which may differ from the configured port
// after an STS upgrade or a cached STS policy.
func (s *Session) Port() uint16 {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.portNumber
}

// TLSEnabled returns the active TLS state.
func (s *Session) TLSEnabled() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.tlsEnabled
}

// Nick returns the current nick, which may change after a 433 fallback or a
// self NICK.
func (s *Session) Nick() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.nickname
}

// ChannelTypes returns the advertised CHANTYPES, or "" before it arrived.
func (s *Session) ChannelTypes() string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.channelTypes
}

// State returns the current state.
func (s *Session) State() SessionState {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.sessionState
}

// ReconnectAttempt returns how many reconnect attempts have been scheduled.
func (s *Session) ReconnectAttempt() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.reconnectAttempt
}

// Capabilities returns the capabilities the server acknowledged.
func (s *Session) Capabilities() irc.CapabilitySet {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.capabilities.Enabled()
}

// MetadataCapability returns the draft/metadata-2 limits last advertised.
func (s *Session) MetadataCapability() irc.MetadataCapability {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.metadataCapability
}

// HistoryPending reports whether any channel still has a CHATHISTORY request
// outstanding.
func (s *Session) HistoryPending() bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	return len(s.historyPending) > 0
}

// AutojoinChannels returns a copy of the current autojoin channel list.
func (s *Session) AutojoinChannels() []string {
	s.mu.Lock()
	defer s.mu.Unlock()
	return append([]string(nil), s.autojoinChannels...)
}

// PendingInvite returns the last unhandled INVITE, if any.
func (s *Session) PendingInvite() (PendingInvite, bool) {
	var invite PendingInvite
	var ok bool
	s.locked(func() {
		if s.pendingInvite != nil {
			invite = *s.pendingInvite
			ok = true
		}
	})
	return invite, ok
}

// OpenBatchCount returns how many batches are currently open.
func (s *Session) OpenBatchCount() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return len(s.openBatches)
}

// PendingRequestLabelCount returns how many labeled requests are outstanding.
func (s *Session) PendingRequestLabelCount() int {
	s.mu.Lock()
	defer s.mu.Unlock()
	return len(s.pendingLabels)
}

// HasPendingRequestLabel reports whether a labeled request is outstanding.
func (s *Session) HasPendingRequestLabel(label string) bool {
	s.mu.Lock()
	defer s.mu.Unlock()
	if label == "" {
		return false
	}
	_, ok := s.pendingLabels[label]
	return ok
}

// Start validates the config and begins connecting. A start is ignored unless
// the session is Idle or Failed. It mirrors IrcSession::start
// (src/irc/ircsession.cpp:514-539).
func (s *Session) Start() { s.locked(s.startLocked) }

func (s *Session) startLocked() {
	if s.sessionState != StateIdle && s.sessionState != StateFailed {
		return
	}
	if s.config.NetworkID == "" || s.config.Host == "" || s.config.Port == 0 ||
		s.config.Nick == "" || s.config.Username == "" || s.config.Realname == "" {
		s.failLocked(ErrorRegistration, "IRC session configuration is incomplete", false)
		return
	}

	s.expectedDisconnect = false
	s.reconnectAfterDisconnect = false
	s.stsDuration = nil
	s.reconnectAttempt = 0
	s.reportedRetryErrors = 0
	s.portNumber = s.config.Port
	s.tlsEnabled = s.config.TLSEnabled
	s.applyCachedStsLocked()
	s.resetForConnectionLocked()
	s.setStateLocked(StateConnecting)
	s.transport.Connect(s.config.Host, s.portNumber, s.tlsEnabled)
}

// Stop shuts the session down. It mirrors IrcSession::stop
// (src/irc/ircsession.cpp:541-566).
func (s *Session) Stop() { s.locked(s.stopLocked) }

func (s *Session) stopLocked() {
	s.rescheduleStsExpiryLocked()
	s.expectedDisconnect = true
	s.reconnectAfterDisconnect = false
	s.cancelTimer(&s.reconnectTimer)
	s.cancelTimer(&s.capabilityTimer)
	s.cancelPingWatchdogLocked()
	s.clearPendingRequestLabelsLocked(true)
	s.reconnectAttempt = 0
	s.reportedRetryErrors = 0

	if s.sessionState == StateIdle {
		return
	}
	if s.sessionState == StateReconnecting || s.sessionState == StateFailed {
		s.setStateLocked(StateIdle)
		return
	}
	s.setStateLocked(StateClosing)
	s.post(func() {
		s.transport.Shutdown()
		s.locked(func() {
			switch s.transport.State() {
			case ConnectionIdle, ConnectionDisconnected, ConnectionFailed:
				s.setStateLocked(StateIdle)
			}
		})
	})
}

// SendChannelMode is intentionally absent. Channel-mode dispatch (the C++
// IrcChannelModeRequest and IrcSession::sendChannelMode) depends on
// irc.ChannelModeRequest, which is not part of the Phase 1 core. Channel-mode
// commands land in Phase 7 with the slash-command catalog
// (/mode, /op, /deop, /voice, /devoice, /ban).

// SendPrivmsg writes a PRIVMSG, splitting an overlong body across frames. It
// mirrors IrcSession::sendPrivmsg (src/irc/ircsession.cpp:568-576).
func (s *Session) SendPrivmsg(target, body string) bool {
	var sent bool
	s.locked(func() { sent = s.sendPrivmsgLocked(target, body) })
	return sent
}

func (s *Session) sendPrivmsgLocked(target, body string) bool {
	if !isValidPrivmsgTarget(target) || body == "" {
		return false
	}
	sent := s.sendTrailingBodyLocked("PRIVMSG "+target+" :", body, "", "")
	if sent {
		s.typing.noteMessageSent(target)
	}
	return sent
}

// SendNotice writes a NOTICE. It mirrors IrcSession::sendNotice.
func (s *Session) SendNotice(target, body string) bool {
	var sent bool
	s.locked(func() { sent = s.sendNoticeLocked(target, body) })
	return sent
}

func (s *Session) sendNoticeLocked(target, body string) bool {
	if target == "" || body == "" {
		return false
	}
	return s.sendTrailingBodyLocked("NOTICE "+target+" :", body, "", "")
}

// SendAction writes a CTCP ACTION. It mirrors IrcSession::sendAction.
func (s *Session) SendAction(target, body string) bool {
	var sent bool
	s.locked(func() { sent = s.sendActionLocked(target, body) })
	return sent
}

func (s *Session) sendActionLocked(target, body string) bool {
	if !isValidPrivmsgTarget(target) || body == "" {
		return false
	}
	sent := s.sendTrailingBodyLocked("PRIVMSG "+target+" :\x01ACTION ", body, "\x01", "")
	if sent {
		s.typing.noteMessageSent(target)
	}
	return sent
}

// SendCtcp writes a CTCP query, optionally under a request label. It mirrors
// IrcSession::sendCtcp (src/irc/ircsession.cpp:612-622).
func (s *Session) SendCtcp(target, command, argument, requestLabel string) bool {
	var sent bool
	s.locked(func() { sent = s.sendCtcpLocked(target, command, argument, requestLabel) })
	return sent
}

func (s *Session) sendCtcpLocked(target, command, argument, requestLabel string) bool {
	verb := strings.ToUpper(strings.TrimSpace(command))
	if !isValidPrivmsgTarget(target) || verb == "" || strings.Contains(verb, " ") {
		return false
	}
	payload := irc.CtcpPayload(irc.CtcpRequest{Command: verb, Argument: argument})
	return s.sendTrailingBodyLocked("PRIVMSG "+target+" :", payload, "", requestLabel)
}

// SendTyping writes a +typing TAGMSG when the message-tags capability is
// enabled and the per-target pace allows it. It mirrors IrcSession::sendTyping
// (src/irc/ircsession.cpp:624-640).
func (s *Session) SendTyping(target string, phase irc.TypingPhase) bool {
	var sent bool
	s.locked(func() { sent = s.sendTypingLocked(target, phase) })
	return sent
}

func (s *Session) sendTypingLocked(target string, phase irc.TypingPhase) bool {
	if s.sessionState != StateRegistered {
		return false
	}
	if !s.capabilities.Enabled().Contains(irc.CapabilityMessageTags) {
		return false
	}
	now := s.clock.Now()
	if !s.typing.shouldSend(target, phase, now) {
		return false
	}
	line := irc.TypingTagmsg(target, phase)
	if len(line) == 0 {
		return false
	}
	frame := cloneBytes(line)
	s.post(func() { s.transport.Write(frame) })
	s.typing.recordSent(target, phase, now)
	return true
}

// Join writes a JOIN and remembers a keyed join for autojoin persistence. It
// mirrors IrcSession::join (src/irc/ircsession.cpp:642-655).
func (s *Session) Join(target irc.JoinTarget) bool {
	var sent bool
	s.locked(func() { sent = s.joinLocked(target) })
	return sent
}

func (s *Session) joinLocked(target irc.JoinTarget) bool {
	if s.sessionState != StateRegistered {
		return false
	}
	channel := target.Channel()
	key, hasKey := target.Key()
	keyValue := ""
	if hasKey {
		keyValue = key
	}
	line, err := irc.Join(channel, keyValue)
	if err != nil {
		return false
	}
	if hasKey {
		s.pendingJoinKeys[s.foldChannelLocked(channel)] = key
	}
	s.sendLineLocked([]byte(line))
	return true
}

// Part writes a PART. It mirrors IrcSession::part.
func (s *Session) Part(channel string) bool {
	var sent bool
	s.locked(func() {
		if channel != "" {
			sent = s.sendCommandLocked("PART "+channel, "")
		}
	})
	return sent
}

// Kick writes a KICK with an optional reason. It mirrors IrcSession::kick.
func (s *Session) Kick(channel, nick, reason string) bool {
	var sent bool
	s.locked(func() {
		if channel == "" || nick == "" {
			return
		}
		command := "KICK " + channel + " " + nick
		if reason != "" {
			command = "KICK " + channel + " " + nick + " :" + reason
		}
		sent = s.sendCommandLocked(command, "")
	})
	return sent
}

// Invite writes an INVITE. It mirrors IrcSession::invite.
func (s *Session) Invite(nick, channel string) bool {
	var sent bool
	s.locked(func() {
		if nick != "" && channel != "" {
			sent = s.sendCommandLocked("INVITE "+nick+" "+channel, "")
		}
	})
	return sent
}

// SetTopic writes a TOPIC. It mirrors IrcSession::setTopic.
func (s *Session) SetTopic(channel, topic string) bool {
	var sent bool
	s.locked(func() {
		if channel != "" && topic != "" {
			sent = s.sendCommandLocked("TOPIC "+channel+" :"+topic, "")
		}
	})
	return sent
}

// SetAway writes AWAY, or clears it when the reason is empty. It mirrors
// IrcSession::setAway (src/irc/ircsession.cpp:685-694).
func (s *Session) SetAway(reason string) bool {
	var sent bool
	s.locked(func() { sent = s.setAwayLocked(reason) })
	return sent
}

func (s *Session) setAwayLocked(reason string) bool {
	trimmed := strings.TrimSpace(reason)
	if trimmed == "" {
		return s.sendCommandLocked("AWAY", "")
	}
	return s.sendCommandLocked("AWAY :"+trimmed, "")
}

// MarkAway writes AWAY without a reason, which marks the user away with the
// server's default message. It differs from SetAway(""), which clears away.
// It mirrors IrcSession::markAway (src/irc/ircsession.cpp:696-702).
func (s *Session) MarkAway(reason string) bool {
	var sent bool
	s.locked(func() {
		trimmed := strings.TrimSpace(reason)
		if trimmed == "" {
			sent = s.sendCommandLocked("AWAY :", "")
			return
		}
		sent = s.sendCommandLocked("AWAY :"+trimmed, "")
	})
	return sent
}

// ClearAway clears away state. It mirrors IrcSession::clearAway.
func (s *Session) ClearAway() bool {
	var sent bool
	s.locked(func() { sent = s.setAwayLocked("") })
	return sent
}

// SetOwnMetadata writes one metadata value and returns the value placed on the
// wire. An empty value clears the key and returns ("", true) on success; a
// false ok means nothing was sent. It mirrors IrcSession::setOwnMetadata
// (src/irc/ircsession.cpp:704-741).
func (s *Session) SetOwnMetadata(key, value string) (string, bool) {
	var wire string
	var ok bool
	s.locked(func() { wire, ok = s.setOwnMetadataLocked(key, value) })
	return wire, ok
}

func (s *Session) setOwnMetadataLocked(key, value string) (string, bool) {
	if s.sessionState != StateRegistered {
		return "", false
	}
	enabled := s.capabilities.Enabled()
	if !enabled.Contains(irc.CapabilityMemberMetadata) || !enabled.Contains(irc.CapabilityBatch) {
		return "", false
	}
	if key == "" || !irc.IsKnownKey(key) {
		return "", false
	}
	stored := irc.CanonicalKey(key)
	if stored == "" {
		return "", false
	}
	if value == "" {
		if !s.sendCommandLocked("METADATA * SET "+stored, "") {
			return "", false
		}
		return "", true
	}
	prefix := "METADATA * SET " + stored + " :"
	wireBudget := irc.MaxClassicFrameBytes - len(prefix) - 2
	if wireBudget < 0 {
		wireBudget = 0
	}
	maxBytes := irc.EffectiveMaxValueBytes(s.metadataCapability.MaxValueBytes)
	if wireBudget < maxBytes {
		maxBytes = wireBudget
	}
	if maxBytes <= 0 {
		return "", false
	}
	clamped := irc.ClampedTo(value, maxBytes)
	if clamped == "" {
		return "", false
	}
	if !s.sendCommandLocked("METADATA * SET "+stored+" :"+clamped, "") {
		return "", false
	}
	return clamped, true
}

// ClearOwnMetadata clears one metadata key. It mirrors
// IrcSession::clearOwnMetadata.
func (s *Session) ClearOwnMetadata(key string) bool {
	var ok bool
	s.locked(func() { _, ok = s.setOwnMetadataLocked(key, "") })
	return ok
}

// ChangeNick writes a NICK. It mirrors IrcSession::changeNick.
func (s *Session) ChangeNick(nick string) bool {
	var sent bool
	s.locked(func() {
		if nick != "" {
			sent = s.sendCommandLocked("NICK "+nick, "")
		}
	})
	return sent
}

// Quit writes QUIT when registered, then stops the session. It mirrors
// IrcSession::quit (src/irc/ircsession.cpp:754-766).
func (s *Session) Quit(reason string) bool {
	var sent bool
	s.locked(func() {
		if s.sessionState == StateIdle {
			return
		}
		if s.sessionState == StateRegistered {
			command := "QUIT"
			if reason != "" {
				command = "QUIT :" + reason
			}
			if !s.sendCommandLocked(command, "") {
				return
			}
		}
		s.stopLocked()
		sent = true
	})
	return sent
}

// Whois writes WHOIS <nick> <nick>, optionally under a request label. It
// mirrors IrcSession::whois (src/irc/ircsession.cpp:811-817).
func (s *Session) Whois(nick, requestLabel string) bool {
	var sent bool
	s.locked(func() {
		trimmed := strings.TrimSpace(nick)
		if trimmed == "" {
			return
		}
		sent = s.sendCommandLocked("WHOIS "+trimmed+" "+trimmed, requestLabel)
	})
	return sent
}

// StartLabeledRequest begins a labeled request and returns its label, or ""
// when labeled-response is not enabled. It mirrors
// IrcSession::startLabeledRequest.
func (s *Session) StartLabeledRequest() string {
	var label string
	s.locked(func() {
		if !s.capabilities.Enabled().Contains(irc.CapabilityLabeledResponse) {
			return
		}
		label = s.beginLabeledRequestLocked()
	})
	return label
}

// CancelRequestLabel drops a pending label without notifying. It mirrors
// IrcSession::cancelRequestLabel.
func (s *Session) CancelRequestLabel(label string) {
	s.locked(func() { s.dropRequestLabelLocked(label) })
}

// SendMonitor writes a MONITOR command. Modifier is one of '+', '-', 'C', 'L',
// or 'S'. It mirrors IrcSession::sendMonitor (src/irc/ircsession.cpp:768-806).
func (s *Session) SendMonitor(modifier rune, nicks []string) bool {
	var sent bool
	s.locked(func() { sent = s.sendMonitorLocked(modifier, nicks) })
	return sent
}

func (s *Session) sendMonitorLocked(modifier rune, nicks []string) bool {
	mark := unicode.ToUpper(modifier)
	switch mark {
	case 'C', 'L', 'S':
		if len(nicks) != 0 {
			return false
		}
		return s.sendCommandLocked("MONITOR "+string(mark), "")
	case '+', '-':
	default:
		return false
	}
	if len(nicks) == 0 {
		return false
	}

	prefix := "MONITOR " + string(mark) + " "
	var batch []string
	for _, nick := range nicks {
		if nick == "" {
			return false
		}
		next := append(append([]string(nil), batch...), nick)
		line := prefix + strings.Join(next, ",")
		if len(line)+2 > irc.MaxClassicFrameBytes {
			if len(batch) == 0 {
				return false
			}
			if !s.sendCommandLocked(prefix+strings.Join(batch, ","), "") {
				return false
			}
			batch = []string{nick}
			if len(prefix+nick)+2 > irc.MaxClassicFrameBytes {
				return false
			}
		} else {
			batch = next
		}
	}
	return len(batch) > 0 && s.sendCommandLocked(prefix+strings.Join(batch, ","), "")
}

// List writes LIST, or LIST <mask>. It mirrors IrcSession::list.
func (s *Session) List(mask string) bool {
	var sent bool
	s.locked(func() {
		trimmed := strings.TrimSpace(mask)
		if strings.ContainsAny(trimmed, "\r\n") {
			return
		}
		if trimmed == "" {
			sent = s.sendCommandLocked("LIST", "")
			return
		}
		sent = s.sendCommandLocked("LIST "+trimmed, "")
	})
	return sent
}

// SendRaw writes one already-built command line. It mirrors
// IrcSession::sendRaw.
func (s *Session) SendRaw(line string) bool {
	var sent bool
	s.locked(func() { sent = s.sendCommandLocked(line, "") })
	return sent
}

// --- Transport Sink -------------------------------------------------------

// Connected fires after the socket opens. For plaintext it begins capability
// negotiation. It mirrors the IrcTransport::connected lambda
// (src/irc/ircsession.cpp:360-365).
func (s *Session) Connected() { s.locked(s.connectedLocked) }

func (s *Session) connectedLocked() {
	if !s.tlsEnabled && (s.sessionState == StateConnecting || s.sessionState == StateStsUpgrading) {
		s.beginCapabilityNegotiationLocked()
	}
}

// Encrypted fires after the TLS handshake. It begins capability negotiation.
// It mirrors the IrcTransport::encrypted lambda (src/irc/ircsession.cpp:366-370).
func (s *Session) Encrypted() { s.locked(s.encryptedLocked) }

func (s *Session) encryptedLocked() {
	if s.tlsEnabled && (s.sessionState == StateConnecting || s.sessionState == StateStsUpgrading) {
		s.beginCapabilityNegotiationLocked()
	}
}

// Disconnected fires when the connection ends. It drives the STS upgrade
// reconnect, the scheduled reconnect, or a failure. It mirrors the
// IrcTransport::disconnected lambda (src/irc/ircsession.cpp:380-399).
func (s *Session) Disconnected() { s.locked(s.disconnectedLocked) }

func (s *Session) disconnectedLocked() {
	if s.shouldFinishStsUpgradeLocked() {
		s.resetForConnectionLocked()
		s.transport.Connect(s.config.Host, s.portNumber, s.tlsEnabled)
		return
	}
	s.rescheduleStsExpiryLocked()
	if s.reconnectAfterDisconnect {
		s.reconnectAfterDisconnect = false
		s.scheduleReconnectLocked()
		return
	}
	if s.expectedDisconnect {
		if s.sessionState == StateClosing {
			s.setStateLocked(StateIdle)
		}
		return
	}
	s.failLocked(ErrorNetwork, "Connection closed by the server", true)
}

// BytesReceived feeds raw inbound bytes to the framer. It mirrors the
// IrcTransport::bytesReceived lambda (src/irc/ircsession.cpp:371-372).
func (s *Session) BytesReceived(bytes []byte) {
	s.locked(func() { s.handleBytesLocked(bytes) })
}

// Error reports a transport failure. It mirrors the
// IrcTransport::errorOccurred lambda (src/irc/ircsession.cpp:373-379).
func (s *Session) Error(message string) {
	s.locked(func() { s.errorOccurredLocked(message) })
}

func (s *Session) errorOccurredLocked(message string) {
	if s.expectedDisconnect {
		return
	}
	lowered := strings.ToLower(message)
	tlsFailure := strings.Contains(lowered, "tls") || strings.Contains(lowered, "certificate")
	kind := ErrorNetwork
	if tlsFailure {
		kind = ErrorTLS
	}
	s.failLocked(kind, message, true)
}

// --- Bytes and frames -----------------------------------------------------

func (s *Session) handleBytesLocked(bytes []byte) {
	if s.pingWatchdog == watchdogWatching {
		s.armPingWatchdogLocked()
	}

	result := s.framer.Feed(bytes)
	for _, fault := range result.Faults {
		s.emitProtocolError("frame", fault.Err, fault.Preview, fault.ByteCount)
	}

	for _, frame := range result.Frames {
		if s.sessionState == StateStsUpgrading {
			break
		}
		parsed, err := irc.Parse(frame)
		if err != nil {
			s.emitProtocolError("message", err, frame, len(frame))
			continue
		}
		s.handleMessageLocked(parsed)
	}
}

func (s *Session) emitProtocolError(kind string, err error, preview string, byteCount int) {
	message := describeMalformed(kind, err, preview, byteCount, s.channelTypes)
	networkID := s.config.NetworkID
	s.emit(func(handler Handler) {
		handler.ErrorOccurred(networkID, ErrorProtocol, message)
	})
}

// --- Message dispatch -----------------------------------------------------

func (s *Session) handleMessageLocked(message irc.Message) {
	if s.ignoreFilter != nil && s.ignoreFilter(message, s.config.NetworkID) {
		return
	}

	if message.Command == "INVITE" && len(message.Params) >= 2 {
		nick := irc.PrefixNick(message)
		channel := parameter(message, len(message.Params)-1)
		if nick != "" && channel != "" {
			s.pendingInvite = &PendingInvite{Nick: nick, Channel: channel}
		}
	}

	requestLabel := s.correlationLabelLocked(message)
	if irc.StatusKeepsIncoming(message, s.nickname, s.channelTypes) {
		entries := irc.IncomingAll(s.config.NetworkID, message, s.channelTypes, s.clock.Now())
		for index := range entries {
			if requestLabel != "" {
				entries[index].SetRequestLabel(requestLabel)
			}
			entry := entries[index]
			s.emit(func(handler Handler) { handler.StatusEntry(entry) })
		}
	}
	s.applyISupportLocked(message)
	if message.Command == "376" || message.Command == "422" {
		s.features.MarkCaseMappingKnown()
	}

	if message.Command == "BATCH" {
		s.handleBatchLocked(message)
		return
	}
	if s.captureInBatchLocked(message) {
		return
	}

	if message.Command == "ACK" {
		s.finishRequestLabelLocked(tagValue(message, "label"))
		return
	}

	if message.Command == "PING" {
		if len(message.Params) == 0 {
			networkID := s.config.NetworkID
			s.emit(func(handler Handler) {
				handler.ErrorOccurred(networkID, ErrorProtocol, "PING message has no token")
			})
			return
		}
		token := message.Params[len(message.Params)-1]
		s.sendLineLocked([]byte("PONG :" + token + "\r\n"))
		return
	}

	if message.Command == "PONG" && s.pongMatchesWatchdogLocked(message) {
		s.armPingWatchdogLocked()
		return
	}

	if message.Command == "PRIVMSG" && len(message.Params) >= 2 &&
		s.nicksEqualLocked(parameter(message, 0), s.nickname) {
		if request, ok := irc.ParseCtcpRequest(parameter(message, 1)); ok && request.Command != "ACTION" {
			sender := irc.PrefixNick(message)
			if sender == "" ||
				irc.IsServiceIdentity(sender, ctcpReplyHost(message), s.channelTypes) {
				return
			}
			var payload string
			switch request.Command {
			case "PING":
				if len(request.Argument) > kCtcpPingPayloadMaxBytes {
					return
				}
				payload = irc.CtcpPayload(irc.CtcpRequest{Command: "PING", Argument: request.Argument})
			case "TIME":
				payload = irc.CtcpPayload(irc.CtcpRequest{
					Command:  "TIME",
					Argument: s.clock.Now().Format(time.RFC1123Z),
				})
			case "VERSION":
				payload = irc.CtcpPayload(irc.CtcpRequest{
					Command:  "VERSION",
					Argument: irc.CtcpVersionReplyText(),
				})
			default:
				return
			}
			if !s.allowCtcpReplyLocked(sender) {
				return
			}
			s.sendNoticeLocked(sender, payload)
			return
		}
	}

	if message.Command == "CAP" {
		s.handleCapLocked(message)
		return
	}
	if message.Command == "AUTHENTICATE" {
		s.handleAuthenticateLocked(message)
		return
	}
	if message.Command == "001" {
		s.handleWelcomeLocked(message)
		return
	}
	if message.Command == "903" {
		// Welcome before the verifier already failed the exchange. A later
		// 903 must not move that session to Registering.
		if s.sessionState != StateSasl {
			return
		}
		if s.saslPending {
			if (s.saslScramExchange || s.saslMechanism == kSaslScramSha256) &&
				s.saslScramStep != saslScramVerified {
				s.failLocked(ErrorAuthentication, "SASL authentication failed", false)
				return
			}
			s.saslPending = false
			s.saslSucceeded = true
			s.endCapabilityNegotiationLocked()
			s.setStateLocked(StateRegistering)
		}
		return
	}
	if message.Command == "774" {
		s.handleMetadataSyncLaterLocked(message)
		return
	}
	if message.Command == "904" || message.Command == "905" {
		s.failLocked(ErrorAuthentication, "SASL authentication failed", false)
		return
	}
	if message.Command == "421" && parameterIndex(message, "CAP") >= 0 &&
		s.sessionState == StateCapLs {
		s.capabilityNegotiationEnded = true
		s.cancelTimer(&s.capabilityTimer)
		s.sendRegistrationLocked()
		return
	}
	if message.Command == "464" {
		s.failLocked(ErrorAuthentication, "Server password or authentication was rejected", false)
		return
	}
	if isRegistrationRefusalNumeric(message.Command) {
		if s.sessionState != StateRegistered {
			if message.Command == "433" && s.tryRegistrationNickFallbackLocked() {
				return
			}
			s.failLocked(ErrorRegistration,
				"IRC registration was refused ("+irc.WireText([]byte(message.Command))+")", false)
			return
		}
	}
	if message.Command == "ERROR" {
		s.emitMessageReceived(message)
		text := "IRC server reported an error"
		if len(message.Params) > 0 {
			text = irc.WireText([]byte(message.Params[len(message.Params)-1]))
		}
		s.failLocked(ErrorNetwork, text, true)
		return
	}
	if message.Command == "NICK" {
		oldNick := claimedNick(message)
		newNick := parameter(message, 0)
		if oldNick != "" && newNick != "" && s.nicksEqualLocked(oldNick, s.nickname) {
			s.nickname = newNick
		}
	}

	if message.Command == "FAIL" {
		s.handleChatHistoryFailLocked(message)
	}
	if message.Command == "366" && len(message.Params) >= 2 {
		s.probeChannelAwayLocked(parameter(message, 1))
	}

	s.emitMessageReceived(message)

	switch {
	case message.Command == "JOIN" && s.selfPrefixedLocked(message):
		channel := parameter(message, 0)
		if channel != "" {
			s.bumpHistoryGenerationLocked(channel)
			s.requestChannelHistoryLocked(channel)
			s.recordAutojoinLocked(channel, true)
			if s.pendingInvite != nil &&
				s.foldChannelLocked(channel) == s.foldChannelLocked(s.pendingInvite.Channel) {
				s.pendingInvite = nil
			}
		}
	case message.Command == "PART" && s.selfPrefixedLocked(message):
		channel := parameter(message, 0)
		if channel != "" {
			s.bumpHistoryGenerationLocked(channel)
			s.forgetChannelHistoryLocked(channel)
			s.recordAutojoinLocked(channel, false)
		}
	case message.Command == "KICK" && s.selfIsLocked(parameter(message, 1)):
		channel := parameter(message, 0)
		if channel != "" {
			s.bumpHistoryGenerationLocked(channel)
			s.forgetChannelHistoryLocked(channel)
			s.recordAutojoinLocked(channel, false)
		}
	case message.Command == "475" && len(message.Params) >= 2:
		s.dropStoredAutojoinKeyLocked(parameter(message, 1))
	}

	carriedLabel := tagValue(message, "label")
	if carriedLabel != "" {
		s.finishRequestLabelLocked(carriedLabel)
	}
}

// --- Capability negotiation ----------------------------------------------

func (s *Session) setStateLocked(state SessionState) {
	if s.sessionState == state {
		return
	}
	if s.sessionState == StateRegistered && state != StateRegistered {
		s.pendingInvite = nil
	}
	s.sessionState = state
	s.emit(func(handler Handler) { handler.StateChanged(state) })
}

func (s *Session) beginCapabilityNegotiationLocked() {
	s.setStateLocked(StateCapLs)
	s.sendLineLocked([]byte("CAP LS 302\r\n"))
}

func (s *Session) requestCapabilitiesLocked() {
	request := s.capabilities.TakeRequest()
	if request.RequestsSasl {
		s.saslRequested = true
		s.saslPending = true
		// Committed when AUTHENTICATE is sent. A later CAP DEL/NEW must not
		// retarget an in-progress SCRAM-SHA-256 exchange to PLAIN.
		if !s.saslExchangeStarted {
			s.saslMechanism = request.SaslMechanism
		}
	}

	if len(request.Lines) > 0 {
		if !s.registrationSent {
			s.setStateLocked(StateCapReq)
		}
		for _, line := range request.Lines {
			s.sendLineLocked([]byte("CAP REQ :" + line + "\r\n"))
		}
		s.armTimer(&s.capabilityTimer, s.config.CapabilityTimeoutMs, s.onCapabilityTimeoutLocked)
	}

	s.sendRegistrationLocked()
	s.endCapabilityNegotiationLocked()
}

func (s *Session) onCapabilityTimeoutLocked() {
	abandoned := s.capabilities.AbandonOutstanding()
	if abandoned.Contains(irc.CapabilitySasl) {
		s.saslPending = false
	}
	s.publishCapabilitiesLocked()
	s.endCapabilityNegotiationLocked()
}

func (s *Session) endCapabilityNegotiationLocked() {
	if s.capabilityNegotiationEnded || s.sessionState == StateRegistered {
		return
	}
	if !s.registrationSent || !s.capabilities.Settled() || s.saslPending {
		return
	}
	s.capabilityNegotiationEnded = true
	s.cancelTimer(&s.capabilityTimer)
	s.sendLineLocked([]byte("CAP END\r\n"))
}

func (s *Session) publishCapabilitiesLocked() {
	enabled := s.capabilities.Enabled()
	if enabled == s.publishedCapabilities {
		return
	}
	s.publishedCapabilities = enabled
	networkID := s.config.NetworkID
	s.emit(func(handler Handler) { handler.CapabilitiesChanged(networkID, enabled) })
}

func (s *Session) subscribeToMemberMetadataLocked() {
	enabled := s.capabilities.Enabled()
	if !enabled.Contains(irc.CapabilityMemberMetadata) || !enabled.Contains(irc.CapabilityBatch) {
		return
	}
	keys := irc.SubscriptionKeys(s.metadataCapability.MaxSubs)
	if len(keys) == 0 {
		return
	}
	s.sendLineLocked([]byte("METADATA * SUB " + strings.Join(keys, " ") + "\r\n"))
}

func (s *Session) probeChannelAwayLocked(channel string) {
	if channel == "" || !s.capabilities.Enabled().Contains(irc.CapabilityAwayNotify) {
		return
	}
	s.sendCommandLocked("WHO "+channel, "")
}

func (s *Session) handleMetadataSyncLaterLocked(message irc.Message) {
	target := parameter(message, 1)
	if target == "" {
		return
	}
	retryAfterSeconds := parseIntOrZero(parameter(message, 2))
	if retryAfterSeconds < 0 {
		retryAfterSeconds = 0
	}
	if retryAfterSeconds > 60 {
		retryAfterSeconds = 60
	}
	s.clock.AfterFunc(time.Duration(retryAfterSeconds)*time.Second, func() {
		s.locked(func() { s.sendCommandLocked("METADATA "+target+" SYNC", "") })
	})
}

// --- Registration ---------------------------------------------------------

func (s *Session) sendRegistrationLocked() {
	if s.registrationSent {
		return
	}

	sendPass := s.config.Password != "" &&
		(!s.saslRequested || s.config.NickServPassword != "")
	pass := ""
	if sendPass {
		pass, _ = irc.Pass(s.config.Password)
	}
	nick, nickErr := irc.Nick(s.config.Nick)
	user, userErr := irc.User(s.config.Username, s.config.Realname)
	if (sendPass && pass == "") || nickErr != nil || userErr != nil {
		s.failLocked(ErrorRegistration,
			"IRC registration fields contain invalid characters", false)
		return
	}

	if pass != "" {
		s.sendLineLocked([]byte(pass))
	}
	s.sendLineLocked([]byte(nick))
	s.sendLineLocked([]byte(user))
	s.registrationSent = true
	s.setStateLocked(StateRegistering)
}

func (s *Session) tryRegistrationNickFallbackLocked() bool {
	if !s.registrationSent {
		return false
	}

	var fallback string
	next := registrationNickDigit
	switch s.registrationNick {
	case registrationNickConfigured:
		fallback = s.config.Nick + "_"
		next = registrationNickUnderscore
	case registrationNickUnderscore:
		fallback = s.config.Nick + "2"
		next = registrationNickDigit
	case registrationNickDigit:
		return false
	}

	line, err := irc.Nick(fallback)
	if err != nil {
		return false
	}
	s.registrationNick = next
	s.nickname = fallback
	s.sendLineLocked([]byte(line))
	return true
}

// --- Welcome and ISUPPORT ------------------------------------------------

func (s *Session) applyISupportLocked(message irc.Message) {
	if message.Command != "005" || len(message.Params) <= 2 {
		return
	}
	const channelTypesPrefix = "CHANTYPES="
	const historyLimitPrefix = "CHATHISTORY="
	last := len(message.Params) - 1
	for index := 1; index < last; index++ {
		token := irc.WireText([]byte(message.Params[index]))
		s.features.ApplyToken(token)
		switch {
		case strings.HasPrefix(token, channelTypesPrefix):
			s.channelTypes = token[len(channelTypesPrefix):]
		case strings.HasPrefix(token, historyLimitPrefix):
			maximum, err := strconv.Atoi(token[len(historyLimitPrefix):])
			if err == nil {
				// The token is the server's per-command maximum. Zero means it
				// does not impose one.
				if maximum > 0 {
					s.historyLimit = min(kHistoryLimit, maximum)
				} else {
					s.historyLimit = kHistoryLimit
				}
			}
		}
	}
}

func (s *Session) handleWelcomeLocked(message irc.Message) {
	if s.sessionState == StateRegistered || s.sessionState == StateFailed {
		return
	}

	// 001 is not SASL success. Fail only while SCRAM is still outstanding. A
	// capability timeout that already sent CAP END without AUTHENTICATE has
	// ended negotiation, so that welcome registers.
	if s.saslMechanism == kSaslScramSha256 && !s.saslSucceeded &&
		(s.saslPending || s.sessionState == StateSasl || s.saslExchangeStarted) {
		s.failLocked(ErrorAuthentication, "SASL authentication failed", false)
		return
	}

	assigned := parameter(message, 0)
	if assigned != "" {
		s.nickname = assigned
	}

	s.reconnectAttempt = 0
	s.reportedRetryErrors = 0
	s.cancelTimer(&s.capabilityTimer)
	s.capabilities.AbandonOutstanding()
	s.setStateLocked(StateRegistered)
	networkID := s.config.NetworkID
	s.emit(func(handler Handler) { handler.Registered(networkID) })
	s.armPingWatchdogLocked()
	s.subscribeToMemberMetadataLocked()

	if s.config.NickServPassword != "" && !s.saslSucceeded &&
		s.saslMechanism != kSaslScramSha256 {
		if !s.tlsEnabled {
			entry := irc.Lifecycle(networkID, irc.LogSeverityAlert, "identify",
				"NickServ identify will be sent in clear text", s.clock.Now())
			s.emit(func(handler Handler) { handler.StatusEntry(entry) })
		}
		s.sendPrivmsgLocked("NickServ", "IDENTIFY "+s.config.NickServPassword)
	}

	for _, channel := range s.autojoinChannels {
		var secret *string
		if key := lookupAutojoinKey(s.autojoinKeys, s.features.CaseMapping(), channel); key != "" {
			secret = &key
		}
		target, ok := irc.MakeJoinTarget(channel, secret, irc.NewServerFeatures())
		if !ok || !s.joinLocked(target) {
			s.emit(func(handler Handler) {
				handler.ErrorOccurred(networkID, ErrorProtocol, "Invalid autojoin channel")
			})
		}
	}
}

// --- STS ------------------------------------------------------------------

func (s *Session) applyCachedStsLocked() {
	cached, ok := s.sts.Lookup(s.config.Host)
	if !ok {
		return
	}
	s.portNumber = cached.Port
	s.tlsEnabled = true
}

func (s *Session) handleStsAdvertisementLocked(advertisement *STSAdvertisement) {
	if advertisement == nil {
		return
	}
	if !s.tlsEnabled {
		if advertisement.Port == nil || s.sessionState == StateStsUpgrading {
			return
		}
		s.beginStsUpgradeLocked(*advertisement.Port)
		return
	}
	if advertisement.DurationSeconds == nil {
		return
	}
	if *advertisement.DurationSeconds == 0 {
		s.sts.Clear(s.config.Host)
		s.stsDuration = nil
		return
	}
	s.sts.Store(s.config.Host, s.portNumber, *advertisement.DurationSeconds)
	s.stsDuration = advertisement.DurationSeconds
}

func (s *Session) beginStsUpgradeLocked(port uint16) {
	s.portNumber = port
	s.tlsEnabled = true
	s.setStateLocked(StateStsUpgrading)
	entry := irc.Lifecycle(s.config.NetworkID, irc.LogSeverityInfo, "sts",
		fmt.Sprintf("Upgraded to TLS on port %d", port), s.clock.Now())
	s.emit(func(handler Handler) { handler.StatusEntry(entry) })
	s.post(func() { s.transport.Shutdown() })
}

func (s *Session) shouldFinishStsUpgradeLocked() bool {
	return s.sessionState == StateStsUpgrading &&
		!s.expectedDisconnect && !s.reconnectAfterDisconnect
}

func (s *Session) rescheduleStsExpiryLocked() {
	if !s.tlsEnabled || s.stsDuration == nil {
		return
	}
	s.sts.Store(s.config.Host, s.portNumber, *s.stsDuration)
}

// --- CAP / AUTHENTICATE ---------------------------------------------------

func (s *Session) handleCapLocked(message irc.Message) {
	lsIndex := parameterIndex(message, "LS")
	if lsIndex >= 0 {
		s.capabilityListSeen = true
		tokens := capabilityTokens(message, lsIndex)
		s.capabilities.Advertise(tokens)
		if advertisement, ok := ParseSTSAdvertisement(tokens); ok {
			s.pendingSTS = &advertisement
		}
		if metadata, ok := irc.ParseMetadataCapability(tokens); ok {
			s.metadataCapability = metadata
		}
		continuation := len(message.Params) > lsIndex+1 && parameter(message, lsIndex+1) == "*"
		if continuation {
			return
		}
		s.handleStsAdvertisementLocked(s.pendingSTS)
		if s.sessionState == StateStsUpgrading {
			return
		}
		s.requestCapabilitiesLocked()
		return
	}

	// Every other CAP subcommand answers a negotiation we started. Acting on
	// one before CAP LS would register the connection with nothing agreed.
	if !s.capabilityListSeen {
		return
	}

	newIndex := parameterIndex(message, "NEW")
	if newIndex >= 0 {
		tokens := capabilityTokens(message, newIndex)
		s.capabilities.Advertise(tokens)
		if advertisement, ok := ParseSTSAdvertisement(tokens); ok {
			s.handleStsAdvertisementLocked(&advertisement)
		}
		if metadata, ok := irc.ParseMetadataCapability(tokens); ok {
			s.metadataCapability = metadata
		}
		if s.sessionState == StateStsUpgrading {
			return
		}
		s.requestCapabilitiesLocked()
		return
	}

	delIndex := parameterIndex(message, "DEL")
	if delIndex >= 0 {
		s.capabilities.Withdraw(capabilityTokens(message, delIndex))
		s.publishCapabilitiesLocked()
		if !s.capabilities.Enabled().Contains(irc.CapabilityMemberMetadata) {
			s.metadataCapability = irc.MetadataCapability{}
		}
		if !s.replayEnabledLocked(irc.HistoryChat) {
			s.abandonHistoryRequestsLocked()
		}
		s.requestCapabilitiesLocked()
		return
	}

	ackIndex := parameterIndex(message, "ACK")
	if ackIndex >= 0 {
		tokens := capabilityTokens(message, ackIndex)
		granted := s.capabilities.Acknowledge(tokens)
		if metadata, ok := irc.ParseMetadataCapability(tokens); ok {
			// A bare ACK keeps limits learned from CAP LS / NEW.
			if metadata.MaxSubs != nil || metadata.MaxValueBytes != nil {
				s.metadataCapability = metadata
			}
		}
		s.publishCapabilitiesLocked()
		if granted.Contains(irc.CapabilitySasl) && !s.saslExchangeStarted {
			s.saslExchangeStarted = true
			s.setStateLocked(StateSasl)
			if s.saslMechanism == kSaslScramSha256 {
				s.saslScramExchange = true
				s.saslScramStep = saslScramAwaitPrompt
				s.saslIncoming = saslChallengeBuffer{}
				s.sendLineLocked([]byte("AUTHENTICATE SCRAM-SHA-256\r\n"))
			} else {
				s.sendLineLocked([]byte("AUTHENTICATE PLAIN\r\n"))
			}
			return
		}
		if s.sessionState == StateRegistered {
			s.subscribeToMemberMetadataLocked()
		}
		s.endCapabilityNegotiationLocked()
		return
	}

	nakIndex := parameterIndex(message, "NAK")
	if nakIndex >= 0 {
		refused := s.capabilities.Reject(capabilityTokens(message, nakIndex))
		if refused.Contains(irc.CapabilitySasl) {
			s.saslPending = false
			s.failLocked(ErrorAuthentication, "Server rejected the SASL capability", false)
			return
		}
		s.requestCapabilitiesLocked()
	}
}

func (s *Session) handleAuthenticateLocked(message irc.Message) {
	if s.sessionState != StateSasl || len(message.Params) == 0 {
		return
	}
	front := message.Params[0]
	if front == "*" {
		s.failLocked(ErrorAuthentication, "Server aborted SASL authentication", false)
		return
	}
	if s.saslScramExchange || s.saslMechanism == kSaslScramSha256 {
		s.handleScramAuthenticateLocked([]byte(front))
		return
	}
	if front != "+" {
		return
	}

	account := s.config.SASLAccountName()
	secret := s.config.saslSecret()
	plain := make([]byte, 0, len(account)*2+len(secret)+2)
	plain = append(plain, account...)
	plain = append(plain, 0)
	plain = append(plain, account...)
	plain = append(plain, 0)
	plain = append(plain, secret...)
	s.sendSaslResponseLocked(plain)
}

func (s *Session) handleScramAuthenticateLocked(payload []byte) {
	switch s.saslScramStep {
	case saslScramAwaitPrompt:
		if string(payload) != "+" {
			s.failLocked(ErrorAuthentication, "SASL authentication failed", false)
			return
		}
		started := s.scram.Start(s.config.SASLAccountName(), s.config.saslSecret(), nil)
		if !started.OK() {
			s.failLocked(ErrorAuthentication, "SASL authentication failed", false)
			return
		}
		s.saslScramStep = saslScramAwaitServerFirst
		s.sendSaslResponseLocked(started.Message)
		return
	case saslScramAwaitServerFirst, saslScramAwaitServerFinal:
		// fall through to chunk reassembly
	default:
		return
	}

	message, complete, ok := s.saslIncoming.Take(payload)
	if !ok {
		s.failLocked(ErrorAuthentication, "SASL authentication failed", false)
		return
	}
	if !complete {
		return
	}

	if s.saslScramStep == saslScramAwaitServerFirst {
		clientFinal := s.scram.TakeServerFirst(message)
		if !clientFinal.OK() {
			s.failLocked(ErrorAuthentication, "SASL authentication failed", false)
			return
		}
		s.saslScramStep = saslScramAwaitServerFinal
		s.sendSaslResponseLocked(clientFinal.Message)
		return
	}

	verified := s.scram.TakeServerFinal(message)
	if !verified.OK() {
		s.failLocked(ErrorAuthentication, "SASL authentication failed", false)
		return
	}
	// The server-final challenge is non-empty. SASL 3.1 still requires an
	// empty client response before the server sends 903.
	s.saslScramStep = saslScramVerified
	s.sendSaslResponseLocked(nil)
}

func (s *Session) sendSaslResponseLocked(raw []byte) {
	for _, frame := range encodeSASLFrames(raw) {
		s.sendLineLocked(frame)
	}
}

// --- History batches ------------------------------------------------------

func (s *Session) handleBatchLocked(message irc.Message) {
	if len(message.Params) == 0 {
		return
	}
	token := parameter(message, 0)
	if len(token) < 2 {
		return
	}
	reference := token[1:]
	if reference == "" {
		return
	}
	if token[0] == '+' {
		parent := tagValue(message, "batch")
		batchType := parameter(message, 1)
		carriedLabel := tagValue(message, "label")
		labeledResponse := strings.EqualFold(batchType, "labeled-response")
		if _, exists := s.openBatches[reference]; exists {
			return
		}
		kind, hasKind := replayKindFor(batchType)
		if hasKind && !s.replayEnabledLocked(kind) {
			s.ignoreBatchLocked(reference)
			return
		}
		// A history batch that answers a request we are still waiting on, or a
		// labeled-response batch whose opening @label is still pending, is never
		// crowded out. Everything else shares the open-batch budget.
		solicited := hasKind && kind == irc.HistoryChat &&
			s.answersPendingHistoryLocked(parameter(message, 2))
		pendingLabeledResponse := labeledResponse && s.hasPendingRequestLabelLocked(carriedLabel)
		overOpenCap := !solicited && !pendingLabeledResponse &&
			len(s.openBatches) >= kMaxOpenBatches
		_, ignored := s.ignoredBatches[reference]
		_, parentIgnored := s.ignoredBatches[parent]
		if ignored || (parent != "" && parentIgnored) ||
			(overOpenCap && s.isHistoryBatchLocked(batchType, parent)) {
			s.ignoreBatchLocked(reference)
			if hasKind && kind == irc.HistoryChat {
				s.clearHistoryPendingLocked(parameter(message, 2))
			}
			return
		}
		if overOpenCap {
			return
		}

		frame := &openBatch{batchType: batchType, parent: parent}
		if labeledResponse {
			frame.requestLabel = carriedLabel
		}
		parentRoot := ""
		if parentBatch, ok := s.openBatches[frame.parent]; ok {
			parentRoot = parentBatch.replayRoot
		}
		if parentRoot != "" {
			frame.replayRoot = parentRoot
		} else if hasKind {
			frame.replayRoot = reference
		}
		if frame.replayRoot == reference {
			frame.hasKind = hasKind
			if hasKind {
				frame.kind = kind
			}
			frame.collected.Target = parameter(message, 2)
			frame.generation = s.historyGenerationLocked(frame.collected.Target)
		}
		s.openBatches[reference] = frame
		return
	}
	if token[0] == '-' {
		s.closeBatchLocked(reference)
	}
}

func (s *Session) closeBatchLocked(reference string) {
	if _, ignored := s.ignoredBatches[reference]; ignored {
		delete(s.ignoredBatches, reference)
		return
	}
	found, ok := s.openBatches[reference]
	if !ok {
		return
	}
	frame := *found
	delete(s.openBatches, reference)

	var children []string
	for childReference, batch := range s.openBatches {
		if batch.replayRoot == reference {
			children = append(children, childReference)
			delete(s.openBatches, childReference)
		}
	}
	for _, child := range children {
		s.ignoreBatchLocked(child)
	}

	if frame.requestLabel != "" {
		s.finishRequestLabelLocked(frame.requestLabel)
	}
	if frame.replayRoot == reference && frame.collected.Target != "" {
		currentMembership := frame.generation == s.historyGenerationLocked(frame.collected.Target)
		if currentMembership && frame.hasKind && frame.kind == irc.HistoryChat {
			delete(s.historyPending, s.foldChannelLocked(frame.collected.Target))
		}
		// Self-join bumps history generation while a znc.in/playback batch can
		// still be open. That batch is not a CHATHISTORY answer, so deliver it
		// and let the reducer hold or splice.
		if !(frame.hasKind && frame.kind == irc.HistoryBouncerPlayback) && !currentMembership {
			return
		}
		batch := frame.collected
		if frame.hasKind && frame.kind == irc.HistoryBouncerPlayback {
			batch.Kind = irc.HistoryBouncerPlayback
		}
		lines := make([]irc.Message, len(batch.Lines))
		copy(lines, batch.Lines)
		batch.Lines = lines
		networkID := s.config.NetworkID
		s.emit(func(handler Handler) { handler.HistoryBatchReceived(networkID, batch) })
	}
}

func (s *Session) captureInBatchLocked(message irc.Message) bool {
	batch := tagValue(message, "batch")
	if batch == "" {
		return false
	}
	if _, ignored := s.ignoredBatches[batch]; ignored {
		return true
	}
	found, ok := s.openBatches[batch]
	if !ok {
		return false
	}
	if found.replayRoot == "" {
		return false
	}
	root, ok := s.openBatches[found.replayRoot]
	if !ok {
		return false
	}
	// Playback and CHATHISTORY LATEST are oldest-first. A full buffer keeps the
	// newest lines. Overflow stays swallowed so it cannot surface as live
	// traffic and move the exclusive PLAY bound past lines never kept.
	lines := root.collected.Lines
	if len(lines) >= kHistoryBufferCeiling {
		lines = lines[1:]
	}
	lines = append(lines, message)
	root.collected.Lines = lines
	return true
}

func replayKindFor(batchType string) (irc.HistoryKind, bool) {
	switch strings.ToLower(batchType) {
	case "chathistory", "draft/chathistory":
		return irc.HistoryChat, true
	case "znc.in/playback":
		return irc.HistoryBouncerPlayback, true
	}
	return irc.HistoryChat, false
}

// replayEnabledLocked mirrors IrcSession::replayEnabled
// (src/irc/ircsession.cpp:1562-1570): bouncer playback needs only `batch`,
// while a CHATHISTORY answer also needs `chathistory`.
func (s *Session) replayEnabledLocked(kind irc.HistoryKind) bool {
	enabled := s.capabilities.Enabled()
	if !enabled.Contains(irc.CapabilityBatch) {
		return false
	}
	return kind != irc.HistoryChat || enabled.Contains(irc.CapabilityChatHistory)
}

func (s *Session) ignoreBatchLocked(reference string) {
	if reference == "" {
		return
	}
	delete(s.openBatches, reference)
	if _, ignored := s.ignoredBatches[reference]; ignored {
		return
	}
	// At the ceiling we stop remembering references rather than start
	// discarding tagged lines we cannot account for.
	if len(s.ignoredBatches) >= kMaxIgnoredBatches {
		return
	}
	s.ignoredBatches[reference] = struct{}{}
}

func (s *Session) requestChannelHistoryLocked(channel string) {
	if channel == "" || !s.replayEnabledLocked(irc.HistoryChat) {
		return
	}
	key := s.foldChannelLocked(channel)
	if _, asked := s.historyAsked[key]; asked {
		return
	}
	s.historyAsked[key] = struct{}{}
	s.historyPending[key] = s.historyGenerationLocked(channel)
	s.sendCommandLocked(fmt.Sprintf("CHATHISTORY LATEST %s * %d", channel, s.historyLimit), "")
}

func (s *Session) forgetChannelHistoryLocked(channel string) {
	folded := s.foldChannelLocked(channel)
	delete(s.historyAsked, folded)
	delete(s.historyPending, folded)
	s.dropHistoryBatchesLocked(channel)
}

func (s *Session) dropHistoryBatchesLocked(channel string) {
	folded := s.foldChannelLocked(channel)
	roots := map[string]struct{}{}
	for reference, batch := range s.openBatches {
		if batch.replayRoot == reference && s.foldChannelLocked(batch.collected.Target) == folded {
			roots[reference] = struct{}{}
		}
	}
	if len(roots) == 0 {
		return
	}
	var gone []string
	for reference, batch := range s.openBatches {
		_, isRoot := roots[reference]
		_, parentIsRoot := roots[batch.replayRoot]
		if isRoot || parentIsRoot {
			gone = append(gone, reference)
			delete(s.openBatches, reference)
		}
	}
	for _, reference := range gone {
		s.ignoreBatchLocked(reference)
	}
}

func (s *Session) bumpHistoryGenerationLocked(channel string) {
	folded := s.foldChannelLocked(channel)
	s.historyGeneration[folded] = s.historyGenerationLocked(channel) + 1
}

func (s *Session) historyGenerationLocked(channel string) int {
	return s.historyGeneration[s.foldChannelLocked(channel)]
}

func (s *Session) foldChannelLocked(channel string) string {
	return irc.WireText([]byte(s.features.CaseMapping().Normalize(channel)))
}

func (s *Session) clearHistoryPendingLocked(channel string) {
	delete(s.historyPending, s.foldChannelLocked(channel))
}

func (s *Session) answersPendingHistoryLocked(channel string) bool {
	if channel == "" {
		return false
	}
	folded := s.foldChannelLocked(channel)
	pending, ok := s.historyPending[folded]
	return ok && pending == s.historyGenerationLocked(channel) &&
		!s.hasOpenCurrentHistoryBatchLocked(channel)
}

func (s *Session) hasOpenCurrentHistoryBatchLocked(channel string) bool {
	folded := s.foldChannelLocked(channel)
	generation := s.historyGenerationLocked(channel)
	for reference, batch := range s.openBatches {
		if batch.replayRoot != reference {
			continue
		}
		// Only a batch we asked for can be the answer we are waiting on.
		if !(batch.hasKind && batch.kind == irc.HistoryChat) {
			continue
		}
		if s.foldChannelLocked(batch.collected.Target) != folded {
			continue
		}
		if batch.generation == generation {
			return true
		}
	}
	return false
}

func (s *Session) abandonHistoryRequestsLocked() {
	targets := map[string]struct{}{}
	for reference, batch := range s.openBatches {
		if batch.replayRoot == reference && batch.collected.Target != "" {
			targets[batch.collected.Target] = struct{}{}
		}
	}
	for target := range targets {
		s.dropHistoryBatchesLocked(target)
	}
	s.historyPending = map[string]int{}
}

func (s *Session) isHistoryBatchLocked(batchType, parent string) bool {
	if _, ok := replayKindFor(batchType); ok {
		return true
	}
	if parent == "" {
		return false
	}
	if _, ignored := s.ignoredBatches[parent]; ignored {
		return true
	}
	batch, ok := s.openBatches[parent]
	return ok && batch.replayRoot != ""
}

func (s *Session) handleChatHistoryFailLocked(message irc.Message) {
	if !strings.EqualFold(parameter(message, 0), "CHATHISTORY") {
		return
	}
	if len(message.Params) < 4 {
		return
	}
	// FAIL CHATHISTORY <code> <subcommand> [<target>] [<context>] :description.
	// The target's position moves with the code, so look for the request this
	// answers instead of indexing. The trailing description is skipped so a
	// channel named in prose cannot cancel a different request.
	for index := 2; index+1 < len(message.Params); index++ {
		channel := parameter(message, index)
		if !s.answersPendingHistoryLocked(channel) {
			continue
		}
		delete(s.historyPending, s.foldChannelLocked(channel))
		return
	}
}

// --- Autojoin -------------------------------------------------------------

func (s *Session) recordAutojoinLocked(channel string, joined bool) {
	if channel == "" {
		return
	}

	foundIndex := -1
	for index, existing := range s.autojoinChannels {
		if s.features.CaseMapping().Equals(existing, channel) {
			foundIndex = index
			break
		}
	}

	if joined {
		listed := channel
		nameChanged := false
		if foundIndex == -1 {
			s.autojoinChannels = append(s.autojoinChannels, channel)
			nameChanged = true
		} else {
			listed = s.autojoinChannels[foundIndex]
		}

		keyChanged := false
		pending := s.pendingJoinKeys[s.foldChannelLocked(channel)]
		delete(s.pendingJoinKeys, s.foldChannelLocked(channel))
		if pending != "" {
			existing := lookupAutojoinKey(s.autojoinKeys, s.features.CaseMapping(), listed)
			if existing != pending {
				eraseAutojoinKey(s.autojoinKeys, s.features.CaseMapping(), listed)
				s.autojoinKeys[listed] = pending
				keyChanged = true
			}
		}
		if !nameChanged && !keyChanged {
			return
		}
	} else {
		hadName := foundIndex != -1
		hadKey := eraseAutojoinKey(s.autojoinKeys, s.features.CaseMapping(), channel)
		delete(s.pendingJoinKeys, s.foldChannelLocked(channel))
		if !hadName && !hadKey {
			return
		}
		if hadName {
			s.autojoinChannels = append(s.autojoinChannels[:foundIndex], s.autojoinChannels[foundIndex+1:]...)
		}
	}
	s.emitAutojoinChangedLocked()
}

func (s *Session) dropStoredAutojoinKeyLocked(channel string) {
	if channel == "" {
		return
	}
	delete(s.pendingJoinKeys, s.foldChannelLocked(channel))
	if !eraseAutojoinKey(s.autojoinKeys, s.features.CaseMapping(), channel) {
		return
	}
	s.emitAutojoinChangedLocked()
}

func (s *Session) emitAutojoinChangedLocked() {
	networkID := s.config.NetworkID
	channels := append([]string(nil), s.autojoinChannels...)
	keys := cloneStringMap(s.autojoinKeys)
	s.emit(func(handler Handler) {
		handler.AutojoinChannelsChanged(networkID, channels, keys)
	})
}

func lookupAutojoinKey(keys map[string]string, mapping irc.CaseMapping, channel string) string {
	if len(keys) == 0 {
		return ""
	}
	ordered := make([]string, 0, len(keys))
	for key := range keys {
		ordered = append(ordered, key)
	}
	sort.Strings(ordered)
	for _, key := range ordered {
		if mapping.Equals(key, channel) {
			return keys[key]
		}
	}
	return ""
}

func eraseAutojoinKey(keys map[string]string, mapping irc.CaseMapping, channel string) bool {
	if len(keys) == 0 {
		return false
	}
	ordered := make([]string, 0, len(keys))
	for key := range keys {
		ordered = append(ordered, key)
	}
	sort.Strings(ordered)
	for _, key := range ordered {
		if mapping.Equals(key, channel) {
			delete(keys, key)
			return true
		}
	}
	return false
}

// --- Identity helpers -----------------------------------------------------

func (s *Session) selfPrefixedLocked(message irc.Message) bool {
	return s.selfIsLocked(claimedNick(message))
}

func (s *Session) selfIsLocked(nick string) bool {
	return nick != "" && s.nicksEqualLocked(nick, s.nickname)
}

func (s *Session) nicksEqualLocked(left, right string) bool {
	return s.features.CaseMapping().Equals(left, right)
}

func (s *Session) correlationLabelLocked(message irc.Message) string {
	label := tagValue(message, "label")
	if label != "" {
		return label
	}
	batch := tagValue(message, "batch")
	if batch == "" {
		return ""
	}
	frame, ok := s.openBatches[batch]
	if !ok {
		return ""
	}
	return frame.requestLabel
}

func (s *Session) allowCtcpReplyLocked(nick string) bool {
	key := strings.ToLower(nick)
	if last, ok := s.ctcpReplyClock[key]; ok &&
		s.clock.Now().Sub(last) < kCtcpReplyInterval {
		return false
	}
	s.ctcpReplyClock[key] = s.clock.Now()
	return true
}

// --- Labeled requests -----------------------------------------------------

func (s *Session) beginLabeledRequestLocked() string {
	s.nextRequestLabel++
	label := fmt.Sprintf("lr%d", s.nextRequestLabel)
	timeoutMs := s.config.LabeledResponseTimeoutMs
	if timeoutMs < 0 {
		timeoutMs = 0
	}
	s.pendingLabels[label] = s.clock.Now().Add(time.Duration(timeoutMs) * time.Millisecond)
	s.armLabelTimerLocked()
	return label
}

func (s *Session) dropRequestLabelLocked(label string) {
	if label == "" {
		return
	}
	if _, ok := s.pendingLabels[label]; !ok {
		return
	}
	delete(s.pendingLabels, label)
	s.armLabelTimerLocked()
}

func (s *Session) finishRequestLabelLocked(label string) {
	if label == "" {
		return
	}
	if _, ok := s.pendingLabels[label]; !ok {
		return
	}
	delete(s.pendingLabels, label)
	s.armLabelTimerLocked()
	networkID := s.config.NetworkID
	s.emit(func(handler Handler) { handler.RequestLabelFinished(networkID, label) })
}

func (s *Session) hasPendingRequestLabelLocked(label string) bool {
	if label == "" {
		return false
	}
	_, ok := s.pendingLabels[label]
	return ok
}

func (s *Session) clearPendingRequestLabelsLocked(notify bool) {
	labels := make([]string, 0, len(s.pendingLabels))
	for label := range s.pendingLabels {
		labels = append(labels, label)
	}
	sort.Strings(labels)
	s.pendingLabels = map[string]time.Time{}
	s.cancelTimer(&s.labelTimer)
	if !notify {
		return
	}
	networkID := s.config.NetworkID
	for _, label := range labels {
		label := label
		s.emit(func(handler Handler) { handler.RequestLabelFinished(networkID, label) })
	}
}

func (s *Session) armLabelTimerLocked() {
	if len(s.pendingLabels) == 0 {
		s.cancelTimer(&s.labelTimer)
		return
	}
	now := s.clock.Now()
	var earliest time.Time
	first := true
	for _, deadline := range s.pendingLabels {
		if first || deadline.Before(earliest) {
			earliest = deadline
			first = false
		}
	}
	delay := int(earliest.Sub(now) / time.Millisecond)
	s.armTimer(&s.labelTimer, delay, s.onLabelTimerFiredLocked)
}

func (s *Session) onLabelTimerFiredLocked() {
	now := s.clock.Now()
	var expired []string
	for label, deadline := range s.pendingLabels {
		if !deadline.After(now) {
			expired = append(expired, label)
		}
	}
	sort.Strings(expired)
	for _, label := range expired {
		delete(s.pendingLabels, label)
	}
	networkID := s.config.NetworkID
	for _, label := range expired {
		label := label
		s.emit(func(handler Handler) { handler.RequestLabelFinished(networkID, label) })
	}
	s.armLabelTimerLocked()
}

// --- Ping watchdog --------------------------------------------------------

func (s *Session) armPingWatchdogLocked() {
	if s.sessionState != StateRegistered {
		return
	}
	s.pingWatchdog = watchdogWatching
	s.armTimer(&s.pingTimer, s.config.PingTimeoutMs, s.onPingWatchdogFiredLocked)
}

func (s *Session) cancelPingWatchdogLocked() {
	s.pingWatchdog = watchdogOff
	s.cancelTimer(&s.pingTimer)
}

func (s *Session) onPingWatchdogFiredLocked() {
	if s.sessionState != StateRegistered {
		return
	}
	switch s.pingWatchdog {
	case watchdogWatching:
		s.pingWatchdog = watchdogProbing
		s.sendLineLocked([]byte("PING :" + kPingWatchdogToken + "\r\n"))
		s.armTimer(&s.pingTimer, s.config.PingTimeoutMs, s.onPingWatchdogFiredLocked)
	case watchdogProbing:
		s.failLocked(ErrorNetwork, "Ping timeout", true)
	}
}

func (s *Session) pongMatchesWatchdogLocked(message irc.Message) bool {
	if s.pingWatchdog != watchdogProbing || len(message.Params) == 0 {
		return false
	}
	return message.Params[len(message.Params)-1] == kPingWatchdogToken
}

// --- Failure and reconnect ------------------------------------------------

func (s *Session) failLocked(kind ErrorKind, message string, reconnect bool) {
	s.cancelPingWatchdogLocked()
	s.clearPendingRequestLabelsLocked(true)

	if reconnect {
		bit := uint32(1) << uint(kind)
		if s.reportedRetryErrors&bit == 0 {
			s.reportedRetryErrors |= bit
			networkID := s.config.NetworkID
			s.emit(func(handler Handler) { handler.ErrorOccurred(networkID, kind, message) })
		}
		switch s.transport.State() {
		case ConnectionConnecting, ConnectionConnected, ConnectionEncrypted, ConnectionClosing:
			s.reconnectAfterDisconnect = true
			s.post(func() { s.transport.Shutdown() })
			return
		}
		s.scheduleReconnectLocked()
		return
	}

	networkID := s.config.NetworkID
	s.emit(func(handler Handler) { handler.ErrorOccurred(networkID, kind, message) })
	s.expectedDisconnect = true
	s.cancelTimer(&s.reconnectTimer)
	s.setStateLocked(StateFailed)
	s.post(func() { s.transport.Shutdown() })
}

func (s *Session) scheduleReconnectLocked() {
	if !s.config.ReconnectEnabled {
		s.setStateLocked(StateFailed)
		return
	}
	if s.sessionState == StateReconnecting {
		return
	}

	s.reconnectAttempt++
	delay := s.reconnectDelayLocked()
	s.setStateLocked(StateReconnecting)
	s.armTimer(&s.reconnectTimer, delay, s.beginReconnectAttemptLocked)
	networkID := s.config.NetworkID
	attempt := s.reconnectAttempt
	s.emit(func(handler Handler) { handler.ReconnectScheduled(networkID, delay, attempt) })
}

func (s *Session) beginReconnectAttemptLocked() {
	if s.sessionState != StateReconnecting {
		return
	}
	s.cancelTimer(&s.reconnectTimer)
	s.applyCachedStsLocked()
	s.resetForConnectionLocked()
	s.setStateLocked(StateConnecting)
	s.transport.Connect(s.config.Host, s.portNumber, s.tlsEnabled)
}

func (s *Session) reconnectDelayLocked() int {
	base := max(0, s.config.ReconnectBaseDelayMs)
	maximum := max(base, s.config.ReconnectMaximumDelayMs)
	delay := int64(base)
	for step := 1; step < s.reconnectAttempt && delay < int64(maximum); step++ {
		doubled := delay * 2
		if doubled > int64(maximum) {
			doubled = int64(maximum)
		}
		delay = doubled
	}
	if delay > int64(int32Max) {
		delay = int64(int32Max)
	}
	return int(delay)
}

func (s *Session) resetForConnectionLocked() {
	s.framer = irc.Framer{}
	s.openBatches = map[string]*openBatch{}
	s.ignoredBatches = map[string]struct{}{}
	s.historyAsked = map[string]struct{}{}
	s.historyPending = map[string]int{}
	s.historyGeneration = map[string]int{}
	s.historyLimit = kHistoryLimit
	s.features = irc.NewServerFeatures()
	s.nickname = s.config.Nick
	s.registrationSent = false
	s.registrationNick = registrationNickConfigured
	s.saslRequested = false
	s.saslPending = false
	s.saslSucceeded = false
	s.saslMechanism = ""
	s.saslExchangeStarted = false
	s.saslScramExchange = false
	s.saslScramStep = saslScramIdle
	s.saslIncoming = saslChallengeBuffer{}
	s.capabilityNegotiationEnded = false
	s.capabilityListSeen = false
	s.pendingSTS = nil
	s.pendingInvite = nil
	s.pendingJoinKeys = map[string]string{}
	s.channelTypes = ""
	s.cancelTimer(&s.capabilityTimer)
	s.cancelPingWatchdogLocked()
	s.clearPendingRequestLabelsLocked(true)
	s.capabilities.Reset(s.config.saslSecret() != "")
	s.metadataCapability = irc.MetadataCapability{}
	s.typing.reset()
	s.publishCapabilitiesLocked()
}

// --- Command writers ------------------------------------------------------

func (s *Session) sendLineLocked(line []byte) {
	if len(line) == 0 {
		return
	}
	if irc.StatusKeepsOutgoing(line) {
		entry := irc.Outgoing(s.config.NetworkID, line, s.channelTypes, s.clock.Now())
		s.emit(func(handler Handler) { handler.StatusEntry(entry) })
	}
	frame := cloneBytes(line)
	s.post(func() { s.transport.Write(frame) })
}

func (s *Session) sendCommandLocked(command, requestLabel string) bool {
	if s.sessionState != StateRegistered {
		return false
	}
	wire := command
	if requestLabel != "" {
		if !isTagSafeLabel(requestLabel) {
			return false
		}
		wire = "@label=" + requestLabel + " " + command
	}
	line, err := irc.Line(wire)
	if err != nil {
		return false
	}
	s.sendLineLocked([]byte(line))
	return true
}

func (s *Session) sendTrailingBodyLocked(prefix, body, suffix, requestLabel string) bool {
	chunks := irc.SplitTrailingParam(prefix, body, suffix)
	if len(chunks) == 0 {
		return false
	}
	if requestLabel != "" && len(chunks) != 1 {
		return false
	}
	for _, chunk := range chunks {
		if !s.sendCommandLocked(prefix+chunk+suffix, requestLabel) {
			return false
		}
	}
	return true
}

func (s *Session) emitMessageReceived(message irc.Message) {
	networkID := s.config.NetworkID
	s.emit(func(handler Handler) { handler.MessageReceived(networkID, message) })
}

// --- Timers ---------------------------------------------------------------

// armTimer restarts the one-shot timer in slot with a delay of delayMs. The
// callback runs under the session mutex and is ignored if a newer timer has
// replaced this one.
func (s *Session) armTimer(slot *Timer, delayMs int, fire func()) {
	if *slot != nil {
		(*slot).Stop()
	}
	var handle Timer
	handle = s.clock.AfterFunc(msDuration(delayMs), func() {
		s.locked(func() {
			if *slot != handle {
				return
			}
			*slot = nil
			fire()
		})
	})
	*slot = handle
}

func (s *Session) cancelTimer(slot *Timer) {
	if *slot != nil {
		(*slot).Stop()
		*slot = nil
	}
}

// --- Package helpers ------------------------------------------------------

func msDuration(milliseconds int) time.Duration {
	if milliseconds < 0 {
		milliseconds = 0
	}
	return time.Duration(milliseconds) * time.Millisecond
}

func cloneStringMap(source map[string]string) map[string]string {
	cloned := make(map[string]string, len(source))
	for key, value := range source {
		cloned[key] = value
	}
	return cloned
}

func parameter(message irc.Message, index int) string {
	if index < 0 || index >= len(message.Params) {
		return ""
	}
	return irc.WireText([]byte(message.Params[index]))
}

func tagValue(message irc.Message, name string) string {
	for _, tag := range message.Tags {
		if tag.Name == name && tag.Value != nil {
			return irc.WireText([]byte(*tag.Value))
		}
	}
	return ""
}

func parameterIndex(message irc.Message, value string) int {
	for index := range message.Params {
		if strings.EqualFold(parameter(message, index), value) {
			return index
		}
	}
	return -1
}

func capabilityTokens(message irc.Message, subcommandIndex int) []string {
	if len(message.Params) <= subcommandIndex+1 {
		return nil
	}
	raw := irc.WireText([]byte(message.Params[len(message.Params)-1]))
	var tokens []string
	for _, field := range strings.Split(raw, " ") {
		if field != "" {
			tokens = append(tokens, field)
		}
	}
	return tokens
}

func claimedNick(message irc.Message) string {
	if message.Prefix == nil {
		return ""
	}
	if message.Prefix.Nick != "" {
		return irc.WireText([]byte(message.Prefix.Nick))
	}
	return irc.WireText([]byte(message.Prefix.Raw))
}

func ctcpReplyHost(message irc.Message) string {
	if message.Prefix == nil {
		return ""
	}
	if message.Prefix.Host != "" {
		return irc.WireText([]byte(message.Prefix.Host))
	}
	if message.Prefix.Nick == "" {
		return irc.WireText([]byte(message.Prefix.Raw))
	}
	return ""
}

func isRegistrationRefusalNumeric(command string) bool {
	switch command {
	case "432", "433", "436", "437", "451", "462", "465":
		return true
	}
	return false
}

func isTagSafeLabel(label string) bool {
	if label == "" || len(label) > 64 {
		return false
	}
	for index := 0; index < len(label); index++ {
		character := label[index]
		ok := (character >= 'A' && character <= 'Z') ||
			(character >= 'a' && character <= 'z') ||
			(character >= '0' && character <= '9') ||
			character == '-' || character == '_'
		if !ok {
			return false
		}
	}
	return true
}

func isValidPrivmsgTarget(target string) bool {
	if target == "" || target[0] == ':' {
		return false
	}
	for _, character := range target {
		if unicode.IsSpace(character) || unicode.IsControl(character) {
			return false
		}
	}
	return true
}

func parseIntOrZero(text string) int {
	value, err := strconv.Atoi(strings.TrimSpace(text))
	if err != nil {
		return 0
	}
	return value
}

// previewWire renders a bounded, fail-closed preview of a malformed frame. It
// mirrors the anonymous previewWire in src/irc/ircsession.cpp:60-92: printable
// bytes are kept, controls become '?' in the display form and ' ' in the spaced
// form, and the stripped form (no spaces) is offered to the secret redactor
// first so a tab-separated PASS is still caught.
func previewWire(preview string, byteCount int, channelTypes string) string {
	display := make([]byte, 0, len(preview))
	spaced := make([]byte, 0, len(preview))
	stripped := make([]byte, 0, len(preview))
	for index := 0; index < len(preview); index++ {
		character := preview[index]
		if character == '\t' || (character >= 0x20 && character < 0x7F) {
			display = append(display, character)
			spaced = append(spaced, character)
			stripped = append(stripped, character)
			continue
		}
		display = append(display, '?')
		spaced = append(spaced, ' ')
	}

	result := irc.WireText(display)
	strippedText := irc.WireText(stripped)
	if safe, ok := irc.RedactPreviewLine(strippedText, channelTypes); ok {
		result = safe
	} else {
		spacedText := irc.WireText(spaced)
		if safeSpaced, ok := irc.RedactPreviewLine(spacedText, channelTypes); ok {
			result = safeSpaced
		}
	}
	if byteCount > len(preview) {
		result += "\u2026"
	}
	return result
}

func describeMalformed(kind string, err error, preview string, byteCount int, channelTypes string) string {
	text := fmt.Sprintf("Malformed IRC %s: %s (%d bytes)", kind, err.Error(), byteCount)
	shown := previewWire(preview, byteCount, channelTypes)
	if shown == "" {
		return text
	}
	return text + ". Preview: " + shown
}

// --- Typing publisher -----------------------------------------------------

// typingTargetClock is one target's outbound typing pace. It mirrors
// IrcTypingPublisher::TargetClock (src/irc/irctypingpublisher.h:22-28).
type typingTargetClock struct {
	lastSentAt   time.Time
	hasLastSent  bool
	lastPhase    irc.TypingPhase
	suppressDone bool
}

// typingPublisher implements the outbound typing gate and pacing of
// IrcTypingPublisher (src/irc/irctypingpublisher.cpp), inlined into the
// session so no external wiring is needed.
type typingPublisher struct {
	targets map[string]*typingTargetClock
}

func newTypingPublisher() typingPublisher {
	return typingPublisher{targets: map[string]*typingTargetClock{}}
}

func (p *typingPublisher) shouldSend(target string, phase irc.TypingPhase, now time.Time) bool {
	if phase == irc.TypingPaused {
		return false
	}
	clock, ok := p.targets[target]
	if !ok {
		return phase == irc.TypingActive
	}
	if phase == irc.TypingDone && clock.suppressDone {
		return false
	}
	if phase == irc.TypingDone && clock.lastPhase != irc.TypingActive {
		return false
	}
	if clock.hasLastSent &&
		now.Sub(clock.lastSentAt) < time.Duration(irc.TypingSendIntervalMs)*time.Millisecond {
		return false
	}
	return true
}

func (p *typingPublisher) recordSent(target string, phase irc.TypingPhase, now time.Time) {
	clock, ok := p.targets[target]
	if !ok {
		clock = &typingTargetClock{}
		p.targets[target] = clock
	}
	clock.lastSentAt = now
	clock.hasLastSent = true
	clock.lastPhase = phase
	if phase != irc.TypingDone {
		clock.suppressDone = false
	}
}

func (p *typingPublisher) noteMessageSent(target string) {
	clock, ok := p.targets[target]
	if !ok {
		clock = &typingTargetClock{}
		p.targets[target] = clock
	}
	clock.suppressDone = true
}

func (p *typingPublisher) reset() {
	p.targets = map[string]*typingTargetClock{}
}
