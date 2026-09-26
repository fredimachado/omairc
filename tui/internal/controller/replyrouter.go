package controller

import (
	"strconv"
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// This file is the Go port of IrcReplyRouter in src/irc/ircreplyrouter.{h,cpp}.
// It owns WHOIS, CTCP, labeled-reply, and own-metadata watch state and the
// routing of their replies. Arming a watch and routing the reply live in one
// place; echoing a transcript line or a status outcome stays a callback into
// the controller through ReplyHost, which still publishes the models.

// ReplyHost is the surface ReplyRouter drives. It mirrors IrcReplyRouter::Host.
type ReplyHost interface {
	ApplyWhoisTranscript(event irc.WhoisTranscriptEvent)
	RecordStatus(entry irc.StatusEntry)
	SelectedKey() (irc.ConversationKey, bool)
	SelfNick(networkID string) string
	PersistAvatarURL(networkID, url string)
	Capabilities(networkID string) irc.CapabilitySet
	QueryNetworkID(surface irc.ComposerSurface) string
	SessionFor(surface irc.ComposerSurface) *session.Session
	SelectedSession() *session.Session
	SelectedTarget() string
	SelectedIsCloseableDirect() bool
	HasNetworks() bool
	Now() time.Time
}

// replyDestination is a conversation destination or a Status-only one. It is
// the Go shape of the C++ std::variant<IrcWhoisStatusOnly, IrcConversationKey>.
type replyDestination struct {
	key    irc.ConversationKey
	hasKey bool
}

// replyWhoisWatchKey identifies one in-flight WHOIS by network and normalized
// nick. It mirrors IrcWhoisWatchKey.
type replyWhoisWatchKey struct {
	networkID      string
	normalizedNick string
}

// replyCtcpWatchKey identifies one in-flight CTCP query by network, normalized
// nick, and uppercased command. It mirrors IrcCtcpWatchKey.
type replyCtcpWatchKey struct {
	networkID      string
	normalizedNick string
	command        string
}

// replyLabeledWatchKey identifies one labeled request. It mirrors
// IrcLabeledWatchKey.
type replyLabeledWatchKey struct {
	networkID    string
	requestLabel string
}

// replyLabeledWatchKind distinguishes a labeled WHOIS from a labeled CTCP. It
// mirrors IrcLabeledWatchKind.
type replyLabeledWatchKind int

const (
	replyLabeledWhois replyLabeledWatchKind = iota
	replyLabeledCtcp
)

// replyLabeledWatch is one in-flight labeled request. It mirrors
// IrcLabeledWatch.
type replyLabeledWatch struct {
	kind            replyLabeledWatchKind
	destination     replyDestination
	metadataEmitted bool
}

// replyWhoisWatch is one in-flight plain WHOIS. It mirrors IrcWhoisWatch.
type replyWhoisWatch struct {
	destination       replyDestination
	failedIsAmbiguous bool
	metadataEmitted   bool
}

// replyCtcpWatch is one in-flight plain CTCP query. It mirrors IrcCtcpWatch.
type replyCtcpWatch struct {
	destination replyDestination
}

// replyOwnMetadataKind distinguishes a set watch from a clear watch. It
// mirrors IrcOwnMetadataWatch::Kind.
type replyOwnMetadataKind int

const (
	replyOwnMetadataSet replyOwnMetadataKind = iota
	replyOwnMetadataClear
)

// replyOwnMetadataWatch is one in-flight own-metadata set or clear. It mirrors
// IrcOwnMetadataWatch.
type replyOwnMetadataWatch struct {
	destination replyDestination
	kind        replyOwnMetadataKind
	value       string
}

// ReplyRouter owns the watch maps and routes their replies. It mirrors
// IrcReplyRouter.
type ReplyRouter struct {
	reducer *irc.EventReducer
	host    ReplyHost

	whoisWatches       map[replyWhoisWatchKey]replyWhoisWatch
	ctcpWatches        map[replyCtcpWatchKey]replyCtcpWatch
	labeledWatches     map[replyLabeledWatchKey]replyLabeledWatch
	ownMetadataWatches map[string]map[string]replyOwnMetadataWatch

	// ownMetadataDispatchDepth defers an own-metadata reply that arrives while
	// a set/clear is still in flight. The demo loopback answers synchronously,
	// so the 761/766 can land before the watch is armed; the reply is queued
	// and replayed once the watch exists, matching the async ordering a real
	// server provides.
	ownMetadataDispatchDepth  int
	pendingOwnMetadataReplies []replyOwnMetadataPending
}

// replyOwnMetadataPending is one deferred 761/766 reply.
type replyOwnMetadataPending struct {
	networkID string
	nick      string
	key       string
	value     string
}

// NewReplyRouter builds a router over reducer, publishing through host. It
// mirrors the IrcReplyRouter constructor.
func NewReplyRouter(reducer *irc.EventReducer, host ReplyHost) *ReplyRouter {
	return &ReplyRouter{
		reducer:            reducer,
		host:               host,
		whoisWatches:       make(map[replyWhoisWatchKey]replyWhoisWatch),
		ctcpWatches:        make(map[replyCtcpWatchKey]replyCtcpWatch),
		labeledWatches:     make(map[replyLabeledWatchKey]replyLabeledWatch),
		ownMetadataWatches: make(map[string]map[string]replyOwnMetadataWatch),
	}
}

// Forget drops every watch for one network. It is what discard,
// forget-network, and the welcome event call. It mirrors IrcReplyRouter::forget.
func (r *ReplyRouter) Forget(networkID string) {
	if networkID == "" {
		return
	}
	for key := range r.whoisWatches {
		if key.networkID == networkID {
			delete(r.whoisWatches, key)
		}
	}
	r.replyForgetLabeledWatches(networkID, replyLabeledWhois)
	for key := range r.ctcpWatches {
		if key.networkID == networkID {
			delete(r.ctcpWatches, key)
		}
	}
	r.replyForgetLabeledWatches(networkID, replyLabeledCtcp)
	delete(r.ownMetadataWatches, networkID)
}

// NoteNickDelivery marks a plain WHOIS watch whose nick was delivered through a
// nick change as failed-is-ambiguous, so the resulting 401 does not count as a
// definitive no-such-nick. It mirrors IrcReplyRouter::noteNickDelivery.
func (r *ReplyRouter) NoteNickDelivery(networkID, target string) {
	features := r.reducer.ServerFeatures(networkID)
	if features.IsChannel(target) {
		return
	}
	key, ok := r.replyWhoisWatchKey(networkID, target)
	if !ok {
		return
	}
	watch, found := r.whoisWatches[key]
	if !found {
		return
	}
	watch.failedIsAmbiguous = true
	r.whoisWatches[key] = watch
}

// RouteStatusEntry routes WHOIS, CTCP, labeled, and own-metadata entries. The
// INVITE inbox branch stays on the controller. It mirrors
// IrcReplyRouter::routeStatusEntry.
func (r *ReplyRouter) RouteStatusEntry(entry irc.StatusEntry) {
	r.replyRouteOwnMetadataError(entry)
	networkID := entry.NetworkID()
	if entry.RequestLabel() != "" {
		key := replyLabeledWatchKey{networkID: networkID, requestLabel: entry.RequestLabel()}
		watch, found := r.labeledWatches[key]
		if found {
			if watch.kind == replyLabeledWhois {
				if line := entry.WhoisLine(); line != nil {
					r.replyRouteLabeledWhois(networkID, entry.RequestLabel(), *line)
				} else {
					r.replyRouteLabeledStandardReply(entry)
				}
			} else if watch.kind == replyLabeledCtcp {
				if line := entry.CtcpReplyLine(); line != nil {
					r.replyRouteLabeledCtcp(networkID, entry.RequestLabel(), *line, entry.Text())
				} else {
					r.replyRouteLabeledStandardReply(entry)
				}
			}
		}
	} else {
		if line := entry.WhoisLine(); line != nil {
			r.replyRouteWhoisLine(networkID, *line)
		}
		if line := entry.CtcpReplyLine(); line != nil {
			r.replyRouteCtcpReply(networkID, *line, entry.Text())
		}
	}
}

// RouteOwnMetadataFail routes a METADATA FAIL that answers a pending own
// set/clear, echoing the outcome and dropping the watch. It mirrors
// IrcReplyRouter::routeOwnMetadataFail.
func (r *ReplyRouter) RouteOwnMetadataFail(networkID string, message irc.Message) {
	if !strings.EqualFold(parameterText(message, 0), "METADATA") {
		return
	}
	networkWatches, found := r.ownMetadataWatches[networkID]
	if !found {
		return
	}

	code := strings.ToUpper(parameterText(message, 1))
	switch code {
	case "KEY_NO_PERMISSION", "VALUE_INVALID", "RATE_LIMITED",
		"KEY_NOT_SET", "LIMIT_REACHED", "KEY_INVALID":
	default:
		return
	}

	var failKeys []string
	lastContext := 2
	if len(message.Params) > 2 {
		lastContext = len(message.Params) - 1
	}
	for index := 2; index < lastContext; index++ {
		token := parameterText(message, index)
		if !irc.IsKnownKey(token) {
			continue
		}
		canonical := irc.CanonicalKey(token)
		if !replyContainsString(failKeys, canonical) {
			failKeys = append(failKeys, canonical)
		}
	}

	description := code
	if len(message.Params) > 0 {
		description = parameterText(message, len(message.Params)-1)
	}
	reason := code
	if description != "" {
		reason = code + " " + description
	}

	finishWatch := func(canonical string, watch replyOwnMetadataWatch) {
		if watch.kind == replyOwnMetadataClear && code == "KEY_NOT_SET" {
			r.replyEchoOwnMetadataOutcome(networkID, watch.destination,
				replyOwnMetadataClearedMessage(canonical))
			if canonical == irc.AvatarKey() {
				r.host.PersistAvatarURL(networkID, "")
			}
			return
		}
		outcome := replyOwnMetadataSetFailMessage(canonical, reason)
		if watch.kind == replyOwnMetadataClear {
			outcome = replyOwnMetadataClearFailMessage(canonical, reason)
		}
		r.replyEchoOwnMetadataOutcome(networkID, watch.destination, outcome)
	}

	if len(failKeys) == 0 {
		return
	}
	for _, canonical := range failKeys {
		watch, ok := networkWatches[canonical]
		if !ok {
			continue
		}
		delete(networkWatches, canonical)
		finishWatch(canonical, watch)
	}
	if len(networkWatches) == 0 {
		delete(r.ownMetadataWatches, networkID)
	}
}

// RequestLabelFinished drops a labeled watch when the session reports the
// request done. It mirrors IrcReplyRouter::requestLabelFinished.
func (r *ReplyRouter) RequestLabelFinished(networkID, requestLabel string) {
	if networkID == "" || requestLabel == "" {
		return
	}
	delete(r.labeledWatches, replyLabeledWatchKey{
		networkID:    networkID,
		requestLabel: requestLabel,
	})
}

// DispatchWhois runs /whois. It mirrors IrcReplyRouter::dispatchWhois.
func (r *ReplyRouter) DispatchWhois(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	nick := firstToken(command.Argument)
	var sess *session.Session
	if nick == "" {
		if r.host.SelectedIsCloseableDirect() {
			nick = r.host.SelectedTarget()
			sess = r.host.SelectedSession()
		} else if _, selected := r.host.SelectedKey(); selected {
			return irc.OutcomeWrongScope
		} else {
			return irc.OutcomeRefused
		}
	} else {
		networkID := r.host.QueryNetworkID(surface)
		if networkID == "" {
			if surface == irc.SurfaceConversation {
				return irc.OutcomeWrongScope
			}
			return irc.OutcomeRefused
		}
		sess = r.host.SessionFor(surface)
	}
	if nick == "" {
		return irc.OutcomeRefused
	}
	if sess == nil || sess.State() != session.StateRegistered {
		return irc.OutcomeNotConnected
	}
	destination := replyDestination{}
	if surface == irc.SurfaceConversation {
		selected, ok := r.host.SelectedKey()
		if !ok {
			return irc.OutcomeWrongScope
		}
		destination = replyDestination{key: selected, hasKey: true}
	}
	if r.replySendWhois(sess, nick, destination) {
		return irc.OutcomeSent
	}
	return irc.OutcomeRefused
}

// replyCtcpQueryName maps a CTCP verb to its command name. It mirrors
// IrcReplyRouter::ctcpQueryName.
func replyCtcpQueryName(verb irc.Verb) string {
	switch verb {
	case irc.VerbPing:
		return "PING"
	case irc.VerbTime:
		return "TIME"
	case irc.VerbVersion:
		return "VERSION"
	default:
		return ""
	}
}

// DispatchCtcp runs /ping, /time, or /version. It mirrors
// IrcReplyRouter::dispatchCtcp.
func (r *ReplyRouter) DispatchCtcp(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	query := replyCtcpQueryName(command.Verb)
	nick := firstToken(command.Argument)
	if restAfterFirstToken(command.Argument) != "" {
		return irc.OutcomeRefused
	}

	var sess *session.Session
	if nick == "" {
		if r.host.SelectedIsCloseableDirect() {
			nick = r.host.SelectedTarget()
			sess = r.host.SelectedSession()
		} else if _, selected := r.host.SelectedKey(); selected {
			return irc.OutcomeWrongScope
		} else {
			return irc.OutcomeRefused
		}
	} else {
		networkID := r.host.QueryNetworkID(surface)
		if networkID == "" {
			if surface == irc.SurfaceConversation {
				return irc.OutcomeWrongScope
			}
			return irc.OutcomeRefused
		}
		features := r.reducer.ServerFeatures(networkID)
		if features.IsChannel(nick) {
			return irc.OutcomeRefused
		}
		sess = r.host.SessionFor(surface)
	}
	if nick == "" {
		return irc.OutcomeRefused
	}
	if sess == nil || sess.State() != session.StateRegistered {
		return irc.OutcomeNotConnected
	}

	destination := replyDestination{}
	if surface == irc.SurfaceConversation {
		selected, ok := r.host.SelectedKey()
		if !ok {
			return irc.OutcomeWrongScope
		}
		destination = replyDestination{key: selected, hasKey: true}
	}

	argument := ""
	if command.Verb == irc.VerbPing {
		argument = strconv.FormatInt(r.host.Now().UnixMilli(), 10)
	}
	if r.replySendCtcpQuery(sess, nick, query, argument, destination) {
		return irc.OutcomeSent
	}
	return irc.OutcomeRefused
}

// replyWhoisWatchKey builds the watch key for one nick. It mirrors
// IrcReplyRouter::whoisWatchKey.
func (r *ReplyRouter) replyWhoisWatchKey(networkID, nick string) (replyWhoisWatchKey, bool) {
	trimmed := strings.TrimSpace(nick)
	if networkID == "" || trimmed == "" {
		return replyWhoisWatchKey{}, false
	}
	return replyWhoisWatchKey{
		networkID:      networkID,
		normalizedNick: r.reducer.ConversationKey(networkID, trimmed).NormalizedTarget,
	}, true
}

// replySendWhois arms the labeled or plain watch and writes the WHOIS. It
// mirrors IrcReplyRouter::sendWhois.
func (r *ReplyRouter) replySendWhois(sess *session.Session, nick string, destination replyDestination) bool {
	key, ok := r.replyWhoisWatchKey(sess.NetworkID(), nick)
	if !ok {
		return false
	}
	if destination.hasKey && destination.key.NetworkID != sess.NetworkID() {
		return false
	}

	requestLabel := sess.StartLabeledRequest()
	if requestLabel != "" {
		r.labeledWatches[replyLabeledWatchKey{networkID: sess.NetworkID(), requestLabel: requestLabel}] =
			replyLabeledWatch{kind: replyLabeledWhois, destination: destination}
		if !sess.Whois(nick, requestLabel) {
			delete(r.labeledWatches, replyLabeledWatchKey{networkID: sess.NetworkID(), requestLabel: requestLabel})
			sess.CancelRequestLabel(requestLabel)
			return false
		}
		return true
	}
	r.whoisWatches[key] = replyWhoisWatch{destination: destination}
	if !sess.Whois(nick, "") {
		delete(r.whoisWatches, key)
		return false
	}
	return true
}

// replyRouteWhoisLine routes a plain WHOIS line. It mirrors
// IrcReplyRouter::routeWhoisLine.
func (r *ReplyRouter) replyRouteWhoisLine(networkID string, line irc.WhoisLine) {
	key, ok := r.replyWhoisWatchKey(networkID, line.Nick())
	if !ok {
		return
	}
	watch, found := r.whoisWatches[key]
	if !found {
		return
	}
	if line.Progress() == irc.WhoisFailed && watch.failedIsAmbiguous {
		return
	}

	destination := watch.destination
	if destination.hasKey {
		r.host.ApplyWhoisTranscript(irc.WhoisTranscriptEvent{
			Destination:   destination.key,
			FormattedBody: line.Text(),
		})
	}

	if line.Progress() == irc.WhoisDetail && !watch.metadataEmitted {
		watch.metadataEmitted = true
		r.whoisWatches[key] = watch
		for _, text := range r.replyWhoisMetadataLines(networkID, line.Nick()) {
			r.host.RecordStatus(irc.Lifecycle(networkID, irc.LogSeverityInfo, "whois", text, r.host.Now()))
			if destination.hasKey {
				r.host.ApplyWhoisTranscript(irc.WhoisTranscriptEvent{
					Destination:   destination.key,
					FormattedBody: text,
				})
			}
		}
	}

	if line.Terminal() {
		delete(r.whoisWatches, key)
	}
}

// replyRouteLabeledWhois routes a WHOIS line carried by a labeled request. It
// mirrors IrcReplyRouter::routeLabeledWhois.
func (r *ReplyRouter) replyRouteLabeledWhois(networkID, requestLabel string, line irc.WhoisLine) {
	key := replyLabeledWatchKey{networkID: networkID, requestLabel: requestLabel}
	watch, found := r.labeledWatches[key]
	if !found || watch.kind != replyLabeledWhois {
		return
	}

	destination := watch.destination
	if destination.hasKey {
		r.host.ApplyWhoisTranscript(irc.WhoisTranscriptEvent{
			Destination:   destination.key,
			FormattedBody: line.Text(),
		})
	}

	if line.Progress() == irc.WhoisDetail && !watch.metadataEmitted {
		watch.metadataEmitted = true
		r.labeledWatches[key] = watch
		for _, text := range r.replyWhoisMetadataLines(networkID, line.Nick()) {
			r.host.RecordStatus(irc.Lifecycle(networkID, irc.LogSeverityInfo, "whois", text, r.host.Now()))
			if destination.hasKey {
				r.host.ApplyWhoisTranscript(irc.WhoisTranscriptEvent{
					Destination:   destination.key,
					FormattedBody: text,
				})
			}
		}
	}

	if line.Terminal() {
		delete(r.labeledWatches, key)
	}
}

// replyRouteLabeledCtcp routes a CTCP reply carried by a labeled request. It
// mirrors IrcReplyRouter::routeLabeledCtcp.
func (r *ReplyRouter) replyRouteLabeledCtcp(networkID, requestLabel string, line irc.CtcpReplyLine, text string) {
	key := replyLabeledWatchKey{networkID: networkID, requestLabel: requestLabel}
	watch, found := r.labeledWatches[key]
	if !found || watch.kind != replyLabeledCtcp {
		return
	}

	destination := watch.destination
	delete(r.labeledWatches, key)
	if ctcpKey, ok := r.replyCtcpWatchKey(networkID, line.Nick(), line.Command()); ok {
		delete(r.ctcpWatches, ctcpKey)
	}

	if !destination.hasKey || text == "" {
		return
	}
	r.host.ApplyWhoisTranscript(irc.WhoisTranscriptEvent{
		Destination:   destination.key,
		FormattedBody: text,
	})
}

// replyRouteLabeledStandardReply copies a labeled FAIL/WARN/NOTE or 4xx/5xx
// standard reply into the destination transcript and drops the watch on Alert.
// It mirrors IrcReplyRouter::routeLabeledStandardReply.
func (r *ReplyRouter) replyRouteLabeledStandardReply(entry irc.StatusEntry) {
	key := replyLabeledWatchKey{networkID: entry.NetworkID(), requestLabel: entry.RequestLabel()}
	watch, found := r.labeledWatches[key]
	if !found {
		return
	}

	if replyCopiesLabeledStandardReply(entry) && watch.destination.hasKey && entry.Text() != "" {
		r.host.ApplyWhoisTranscript(irc.WhoisTranscriptEvent{
			Destination:   watch.destination.key,
			FormattedBody: entry.Text(),
		})
	}

	if entry.Severity() == irc.LogSeverityAlert {
		delete(r.labeledWatches, key)
	}
}

// replyForgetLabeledWatches drops every labeled watch of one kind for one
// network. It mirrors IrcReplyRouter::forgetLabeledWatches.
func (r *ReplyRouter) replyForgetLabeledWatches(networkID string, kind replyLabeledWatchKind) {
	if networkID == "" {
		return
	}
	for key, watch := range r.labeledWatches {
		if key.networkID == networkID && watch.kind == kind {
			delete(r.labeledWatches, key)
		}
	}
}

// replyWhoisMetadataLines renders the metadata facts WHOIS taught the reducer.
// It mirrors IrcReplyRouter::whoisMetadataLines.
func (r *ReplyRouter) replyWhoisMetadataLines(networkID, nick string) []string {
	facts := r.reducer.NickPresence(networkID, nick)
	var lines []string
	addValue := func(key, pattern string) {
		value := facts.Metadata(key)
		if value == "" {
			return
		}
		lines = append(lines, replyFormatPair(pattern, nick, value))
	}
	addValue(irc.DisplayNameKey(), "%1 is also known as %2")
	addValue(irc.PronounsKey(), "%1 pronouns %2")
	addValue(irc.StatusKey(), "%1 status %2")
	if facts.IsBot() {
		software := facts.Metadata(irc.BotKey())
		if software == "" {
			lines = append(lines, nick+" is a bot")
		} else {
			lines = append(lines, nick+" is a bot ("+software+")")
		}
	}
	addValue(irc.HomepageKey(), "%1 homepage %2")
	addValue(irc.ColorKey(), "%1 color %2")
	addValue(irc.AvatarKey(), "%1 avatar %2")
	return lines
}

// replyCtcpWatchKey builds the watch key for one CTCP query. It mirrors
// IrcReplyRouter::ctcpWatchKey.
func (r *ReplyRouter) replyCtcpWatchKey(networkID, nick, command string) (replyCtcpWatchKey, bool) {
	trimmed := strings.TrimSpace(nick)
	verb := strings.ToUpper(strings.TrimSpace(command))
	if networkID == "" || trimmed == "" || verb == "" {
		return replyCtcpWatchKey{}, false
	}
	return replyCtcpWatchKey{
		networkID:      networkID,
		normalizedNick: r.reducer.ConversationKey(networkID, trimmed).NormalizedTarget,
		command:        verb,
	}, true
}

// replySendCtcpQuery arms the ctcp and optional labeled watch and writes the
// query. It mirrors IrcReplyRouter::sendCtcpQuery.
func (r *ReplyRouter) replySendCtcpQuery(sess *session.Session, nick, command, argument string, destination replyDestination) bool {
	key, ok := r.replyCtcpWatchKey(sess.NetworkID(), nick, command)
	if !ok {
		return false
	}
	if destination.hasKey && destination.key.NetworkID != sess.NetworkID() {
		return false
	}

	requestLabel := sess.StartLabeledRequest()
	if requestLabel != "" {
		r.labeledWatches[replyLabeledWatchKey{networkID: sess.NetworkID(), requestLabel: requestLabel}] =
			replyLabeledWatch{kind: replyLabeledCtcp, destination: destination}
	}
	r.ctcpWatches[key] = replyCtcpWatch{destination: destination}
	if !sess.SendCtcp(nick, command, argument, requestLabel) {
		if requestLabel != "" {
			delete(r.labeledWatches, replyLabeledWatchKey{networkID: sess.NetworkID(), requestLabel: requestLabel})
			sess.CancelRequestLabel(requestLabel)
		}
		delete(r.ctcpWatches, key)
		return false
	}
	return true
}

// replyRouteCtcpReply routes a plain CTCP reply. It mirrors
// IrcReplyRouter::routeCtcpReply.
func (r *ReplyRouter) replyRouteCtcpReply(networkID string, line irc.CtcpReplyLine, text string) {
	key, ok := r.replyCtcpWatchKey(networkID, line.Nick(), line.Command())
	if !ok {
		return
	}
	watch, found := r.ctcpWatches[key]
	if !found {
		return
	}

	destination := watch.destination
	delete(r.ctcpWatches, key)

	if !destination.hasKey || text == "" {
		return
	}
	r.host.ApplyWhoisTranscript(irc.WhoisTranscriptEvent{
		Destination:   destination.key,
		FormattedBody: text,
	})
}

// DispatchStatus runs /status. It mirrors IrcReplyRouter::dispatchStatus.
func (r *ReplyRouter) DispatchStatus(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	sess := r.host.SessionFor(surface)
	if sess == nil {
		if surface == irc.SurfaceConversation && !replyHasSelected(r.host) && r.host.HasNetworks() {
			return irc.OutcomeRefused
		}
		return irc.OutcomeNotConnected
	}
	if sess.State() != session.StateRegistered {
		return irc.OutcomeNotConnected
	}

	networkID := sess.NetworkID()
	capabilities := r.host.Capabilities(networkID)
	if !capabilities.Contains(irc.CapabilityMemberMetadata) || !capabilities.Contains(irc.CapabilityBatch) {
		return r.replyEchoMetadataCommandFeedback(surface, networkID,
			replyOwnMetadataNoCapMessage(irc.StatusKey()))
	}

	if command.Argument == "" {
		current := r.reducer.NickPresence(networkID, sess.Nick()).Status()
		if current == "" {
			return r.replyEchoMetadataCommandFeedback(surface, networkID,
				replyOwnMetadataInspectEmptyMessage(irc.StatusKey()))
		}
		return r.replyEchoMetadataCommandFeedback(surface, networkID,
			replyOwnMetadataInspectValueMessage(irc.StatusKey(), current))
	}

	if replyIsOwnMetadataClearAlias(command.Argument) {
		return r.replyDispatchOwnMetadataClear(sess, irc.StatusKey())
	}

	valueBudget := irc.EffectiveMaxValueBytes(sess.MetadataCapability().MaxValueBytes)
	if valueBudget <= 0 {
		return r.replyEchoMetadataCommandFeedback(surface, networkID,
			replyOwnMetadataNoValueMessage(irc.StatusKey()))
	}
	clamped := irc.ClampedTo(command.Argument, valueBudget)
	if clamped == "" {
		return irc.OutcomeRefused
	}
	return r.replyDispatchOwnMetadataSet(sess, irc.StatusKey(), clamped)
}

// DispatchAvatar runs /avatar. It mirrors IrcReplyRouter::dispatchAvatar.
func (r *ReplyRouter) DispatchAvatar(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	sess := r.host.SessionFor(surface)
	if sess == nil {
		if surface == irc.SurfaceConversation && !replyHasSelected(r.host) && r.host.HasNetworks() {
			return irc.OutcomeRefused
		}
		return irc.OutcomeNotConnected
	}
	if sess.State() != session.StateRegistered {
		return irc.OutcomeNotConnected
	}

	networkID := sess.NetworkID()
	capabilities := r.host.Capabilities(networkID)
	if !capabilities.Contains(irc.CapabilityMemberMetadata) || !capabilities.Contains(irc.CapabilityBatch) {
		return r.replyEchoMetadataCommandFeedback(surface, networkID,
			replyOwnMetadataNoCapMessage(irc.AvatarKey()))
	}

	if command.Argument == "" {
		current := r.reducer.NickPresence(networkID, sess.Nick()).Avatar()
		if current == "" {
			return r.replyEchoMetadataCommandFeedback(surface, networkID,
				replyOwnMetadataInspectEmptyMessage(irc.AvatarKey()))
		}
		return r.replyEchoMetadataCommandFeedback(surface, networkID,
			replyOwnMetadataInspectValueMessage(irc.AvatarKey(), current))
	}

	if replyIsOwnMetadataClearAlias(command.Argument) {
		return r.replyDispatchOwnMetadataClear(sess, irc.AvatarKey())
	}

	resolved := avatarMetadataValue(command.Argument)
	if resolved == "" {
		return r.replyEchoMetadataCommandFeedback(surface, networkID,
			"Avatar must be an HTTPS URL or an email address.")
	}

	valueBudget := irc.EffectiveMaxValueBytes(sess.MetadataCapability().MaxValueBytes)
	if valueBudget <= 0 {
		return r.replyEchoMetadataCommandFeedback(surface, networkID,
			replyOwnMetadataNoValueMessage(irc.AvatarKey()))
	}
	clamped := irc.ClampedTo(resolved, valueBudget)
	if clamped == "" {
		return irc.OutcomeRefused
	}
	return r.replyDispatchOwnMetadataSet(sess, irc.AvatarKey(), clamped)
}

// replyDispatchOwnMetadataClear clears one key and arms the clear watch. It
// mirrors IrcReplyRouter::dispatchOwnMetadataClear.
func (r *ReplyRouter) replyDispatchOwnMetadataClear(sess *session.Session, metadataKey string) irc.CommandOutcome {
	r.replyBeginOwnMetadataDispatch()
	if !sess.ClearOwnMetadata(metadataKey) {
		r.replyEndOwnMetadataDispatch()
		return irc.OutcomeRefused
	}
	r.replyArmOwnMetadataWatch(sess.NetworkID(), metadataKey, replyOwnMetadataClear, "")
	r.replyEndOwnMetadataDispatch()
	return irc.OutcomeSent
}

// replyDispatchOwnMetadataSet sets one key and arms the set watch. It mirrors
// IrcReplyRouter::dispatchOwnMetadataSet.
func (r *ReplyRouter) replyDispatchOwnMetadataSet(sess *session.Session, metadataKey, value string) irc.CommandOutcome {
	if value == "" {
		return irc.OutcomeRefused
	}
	r.replyBeginOwnMetadataDispatch()
	sent, ok := sess.SetOwnMetadata(metadataKey, value)
	if !ok || sent == "" {
		r.replyEndOwnMetadataDispatch()
		return irc.OutcomeRefused
	}
	r.replyArmOwnMetadataWatch(sess.NetworkID(), metadataKey, replyOwnMetadataSet, sent)
	r.replyEndOwnMetadataDispatch()
	return irc.OutcomeSent
}

// replyBeginOwnMetadataDispatch opens a set/clear that may be answered before
// its watch is armed.
func (r *ReplyRouter) replyBeginOwnMetadataDispatch() { r.ownMetadataDispatchDepth++ }

// replyEndOwnMetadataDispatch closes the in-flight set/clear and replays any
// reply that arrived while it was open, now that the watch exists.
func (r *ReplyRouter) replyEndOwnMetadataDispatch() {
	if r.ownMetadataDispatchDepth > 0 {
		r.ownMetadataDispatchDepth--
	}
	if r.ownMetadataDispatchDepth > 0 {
		return
	}
	pending := r.pendingOwnMetadataReplies
	r.pendingOwnMetadataReplies = nil
	for _, item := range pending {
		r.RouteOwnMetadataReply(item.networkID, item.nick, item.key, item.value)
	}
}

// replyEchoMetadataCommandFeedback echoes a local /status or /avatar message to
// the selected conversation, or records it on Status. It mirrors
// IrcReplyRouter::echoMetadataCommandFeedback.
func (r *ReplyRouter) replyEchoMetadataCommandFeedback(surface irc.ComposerSurface, networkID, text string) irc.CommandOutcome {
	if selected, ok := r.host.SelectedKey(); ok && selected.NetworkID == networkID {
		r.host.ApplyWhoisTranscript(irc.WhoisTranscriptEvent{
			Destination:   selected,
			FormattedBody: text,
		})
		return irc.OutcomeSent
	}
	if surface == irc.SurfaceConversation {
		return irc.OutcomeWrongScope
	}
	r.host.RecordStatus(irc.Outcome(networkID, text, r.host.Now()))
	return irc.OutcomeSent
}

// replyArmOwnMetadataWatch records the pending own-metadata set or clear,
// targeting the selected conversation when it is on the same network. It
// mirrors IrcReplyRouter::armOwnMetadataWatch.
func (r *ReplyRouter) replyArmOwnMetadataWatch(networkID, metadataKey string, kind replyOwnMetadataKind, value string) {
	destination := replyDestination{}
	if selected, ok := r.host.SelectedKey(); ok && selected.NetworkID == networkID {
		destination = replyDestination{key: selected, hasKey: true}
	}
	canonical := irc.CanonicalKey(metadataKey)
	networkWatches := r.ownMetadataWatches[networkID]
	if networkWatches == nil {
		networkWatches = make(map[string]replyOwnMetadataWatch)
		r.ownMetadataWatches[networkID] = networkWatches
	}
	networkWatches[canonical] = replyOwnMetadataWatch{
		destination: destination,
		kind:        kind,
		value:       value,
	}
}

// replyEchoOwnMetadataOutcome echoes an own-metadata outcome to its captured
// destination, the still-selected conversation, or Status. It mirrors
// IrcReplyRouter::echoOwnMetadataOutcome.
func (r *ReplyRouter) replyEchoOwnMetadataOutcome(networkID string, destination replyDestination, text string) {
	if destination.hasKey {
		r.host.ApplyWhoisTranscript(irc.WhoisTranscriptEvent{
			Destination:   destination.key,
			FormattedBody: text,
		})
		return
	}
	if selected, ok := r.host.SelectedKey(); ok && selected.NetworkID == networkID {
		r.host.ApplyWhoisTranscript(irc.WhoisTranscriptEvent{
			Destination:   selected,
			FormattedBody: text,
		})
		return
	}
	r.host.RecordStatus(irc.Outcome(networkID, text, r.host.Now()))
}

// RouteOwnMetadataReply correlates a confirmed own-metadata line with its
// pending watch. It mirrors IrcReplyRouter::routeOwnMetadataReply.
func (r *ReplyRouter) RouteOwnMetadataReply(networkID, nick, key, value string) {
	if r.ownMetadataDispatchDepth > 0 {
		r.pendingOwnMetadataReplies = append(r.pendingOwnMetadataReplies,
			replyOwnMetadataPending{networkID: networkID, nick: nick, key: key, value: value})
		return
	}
	canonical := irc.CanonicalKey(key)
	networkWatches, found := r.ownMetadataWatches[networkID]
	if !found {
		return
	}
	watch, ok := networkWatches[canonical]
	if !ok {
		return
	}

	self := r.host.SelfNick(networkID)
	if self == "" {
		return
	}
	features := r.reducer.ServerFeatures(networkID)
	if !features.CaseMapping().Equals(nick, self) {
		return
	}

	if watch.kind == replyOwnMetadataClear {
		if value != "" {
			return
		}
		r.replyEchoOwnMetadataOutcome(networkID, watch.destination,
			replyOwnMetadataClearedMessage(canonical))
		if canonical == irc.AvatarKey() {
			r.host.PersistAvatarURL(networkID, "")
		}
	} else {
		if value == "" || watch.value != value {
			return
		}
		r.replyEchoOwnMetadataOutcome(networkID, watch.destination,
			replyOwnMetadataSetMessage(canonical, value))
		if canonical == irc.AvatarKey() {
			r.host.PersistAvatarURL(networkID, value)
		}
	}

	delete(networkWatches, canonical)
	if len(networkWatches) == 0 {
		delete(r.ownMetadataWatches, networkID)
	}
}

// replyRouteOwnMetadataError routes the 764/767/769 numerics that answer a
// pending own-metadata set or clear. It mirrors
// IrcReplyRouter::routeOwnMetadataError.
func (r *ReplyRouter) replyRouteOwnMetadataError(entry irc.StatusEntry) {
	networkID := entry.NetworkID()
	networkWatches, found := r.ownMetadataWatches[networkID]
	if !found {
		return
	}
	label := entry.Label()
	if label != "764" && label != "767" && label != "769" {
		return
	}
	keyToken := firstToken(entry.Text())
	if keyToken == "" {
		return
	}
	canonical := irc.CanonicalKey(keyToken)
	watch, ok := networkWatches[canonical]
	if !ok {
		return
	}
	reason := label
	if entry.Text() != "" {
		reason = entry.Text()
	}
	message := replyOwnMetadataSetFailMessage(canonical, reason)
	if watch.kind == replyOwnMetadataClear {
		message = replyOwnMetadataClearFailMessage(canonical, reason)
	}
	r.replyEchoOwnMetadataOutcome(networkID, watch.destination, message)
	delete(networkWatches, canonical)
	if len(networkWatches) == 0 {
		delete(r.ownMetadataWatches, networkID)
	}
}

// replyIsOwnMetadataClearAlias reports whether argument is exactly `clear`. It
// mirrors isOwnMetadataClearAlias.
func replyIsOwnMetadataClearAlias(argument string) bool {
	return strings.EqualFold(firstToken(argument), "clear") && restAfterFirstToken(argument) == ""
}

// replyOwnMetadataClearedMessage renders the clear success text. It mirrors
// ownMetadataClearedMessage.
func replyOwnMetadataClearedMessage(metadataKey string) string {
	if strings.EqualFold(metadataKey, irc.StatusKey()) {
		return "Standing status cleared."
	}
	if strings.EqualFold(metadataKey, irc.AvatarKey()) {
		return "Avatar cleared."
	}
	return ""
}

// replyOwnMetadataSetMessage renders the set success text. It mirrors
// ownMetadataSetMessage.
func replyOwnMetadataSetMessage(metadataKey, value string) string {
	if strings.EqualFold(metadataKey, irc.StatusKey()) {
		return "Standing status set to " + value + "."
	}
	if strings.EqualFold(metadataKey, irc.AvatarKey()) {
		return "Avatar set to " + value + "."
	}
	return ""
}

// replyOwnMetadataClearFailMessage renders the clear failure text. It mirrors
// ownMetadataClearFailMessage.
func replyOwnMetadataClearFailMessage(metadataKey, reason string) string {
	if strings.EqualFold(metadataKey, irc.StatusKey()) {
		return "Could not clear standing status: " + reason
	}
	if strings.EqualFold(metadataKey, irc.AvatarKey()) {
		return "Could not clear avatar: " + reason
	}
	return ""
}

// replyOwnMetadataSetFailMessage renders the set failure text. It mirrors
// ownMetadataSetFailMessage.
func replyOwnMetadataSetFailMessage(metadataKey, reason string) string {
	if strings.EqualFold(metadataKey, irc.StatusKey()) {
		return "Could not set standing status: " + reason
	}
	if strings.EqualFold(metadataKey, irc.AvatarKey()) {
		return "Could not set avatar: " + reason
	}
	return ""
}

// replyOwnMetadataNoCapMessage renders the missing-capability text. It mirrors
// ownMetadataNoCapMessage.
func replyOwnMetadataNoCapMessage(metadataKey string) string {
	if strings.EqualFold(metadataKey, irc.StatusKey()) {
		return "This network does not support standing status."
	}
	if strings.EqualFold(metadataKey, irc.AvatarKey()) {
		return "This network does not support avatars."
	}
	return ""
}

// replyOwnMetadataNoValueMessage renders the no-value-budget text. It mirrors
// ownMetadataNoValueMessage.
func replyOwnMetadataNoValueMessage(metadataKey string) string {
	if strings.EqualFold(metadataKey, irc.StatusKey()) {
		return "This network does not allow standing status text."
	}
	if strings.EqualFold(metadataKey, irc.AvatarKey()) {
		return "This network does not allow avatar URLs."
	}
	return ""
}

// replyOwnMetadataInspectEmptyMessage renders the inspect-with-no-value text.
// It mirrors ownMetadataInspectEmptyMessage.
func replyOwnMetadataInspectEmptyMessage(metadataKey string) string {
	if strings.EqualFold(metadataKey, irc.StatusKey()) {
		return "No standing status. Use /status <text> or /status clear."
	}
	if strings.EqualFold(metadataKey, irc.AvatarKey()) {
		return "No standing avatar. Use /avatar <url|email> or /avatar clear."
	}
	return ""
}

// replyOwnMetadataInspectValueMessage renders the inspect-with-a-value text. It
// mirrors ownMetadataInspectValueMessage.
func replyOwnMetadataInspectValueMessage(metadataKey, value string) string {
	if strings.EqualFold(metadataKey, irc.StatusKey()) {
		return "Standing status: " + value
	}
	if strings.EqualFold(metadataKey, irc.AvatarKey()) {
		return "Standing avatar: " + value
	}
	return ""
}

// replyCopiesLabeledStandardReply reports whether a labeled standard reply is
// worth copying into the transcript: a FAIL/WARN/NOTE, or a 3-digit 4xx/5xx. It
// mirrors copiesLabeledStandardReply.
func replyCopiesLabeledStandardReply(entry irc.StatusEntry) bool {
	label := entry.Label()
	if label == "FAIL" || label == "WARN" || label == "NOTE" {
		return true
	}
	if len(label) != 3 {
		return false
	}
	for index := 0; index < 3; index++ {
		if label[index] < '0' || label[index] > '9' {
			return false
		}
	}
	return label[0] == '4' || label[0] == '5'
}

// replyHasSelected reports whether a conversation is selected. It mirrors the
// optional-to-bool test the C++ router uses.
func replyHasSelected(host ReplyHost) bool {
	_, ok := host.SelectedKey()
	return ok
}

// replyContainsString reports whether values holds wanted.
func replyContainsString(values []string, wanted string) bool {
	for _, value := range values {
		if value == wanted {
			return true
		}
	}
	return false
}

// replyFormatPair substitutes %1 and %2 in pattern with first and second,
// scanning only the pattern so a substituted value is never re-expanded. It
// matches QString::arg for the two-placeholder patterns the router uses.
func replyFormatPair(pattern, first, second string) string {
	var builder strings.Builder
	for index := 0; index < len(pattern); index++ {
		if pattern[index] == '%' && index+1 < len(pattern) {
			switch pattern[index+1] {
			case '1':
				builder.WriteString(first)
				index++
				continue
			case '2':
				builder.WriteString(second)
				index++
				continue
			}
		}
		builder.WriteByte(pattern[index])
	}
	return builder.String()
}
