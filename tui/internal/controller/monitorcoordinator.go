package controller

import (
	"strconv"
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// This file is the Go port of IrcMonitorCoordinator in
// src/irc/ircmonitorcoordinator.{h,cpp}: the persisted MONITOR nick list and
// the session-scoped presence tracking behind /monitor, /unmonitor, and
// /monitored. The coordinator reads the persisted MonitorStore and reports
// presence through the host; notifications (signal plus inbox) stay on the
// controller.
//
// Presence is session-scoped: it clears when registration ends, on discard,
// and on registration re-entry. The persisted nick list belongs to the store
// and forgets only through MonitorStore.Forget.

// MonitorHost is the surface IrcMonitorCoordinator drives. It mirrors
// IrcMonitorCoordinator::Host.
type MonitorHost interface {
	// RecordStatus appends one Status console entry.
	RecordStatus(entry irc.StatusEntry)
	// NotifyMonitor reports one presence change for a watched nick. newlyOnline
	// is true only for an Offline -> Online transition.
	NotifyMonitor(networkID, display, body string, newlyOnline bool)
	// QueryNetworkID resolves the network for a composer surface.
	QueryNetworkID(surface irc.ComposerSurface) string
	// SessionFor resolves the session for a composer surface.
	SessionFor(surface irc.ComposerSurface) *session.Session
	// SessionForNetwork resolves the session for one network id.
	SessionForNetwork(networkID string) *session.Session
	// SelectedKey returns the selected conversation, if any.
	SelectedKey() (irc.ConversationKey, bool)
	// Now is the clock the Status entries are stamped with.
	Now() time.Time
}

// monitorPresence is one watched nick's last observed state. It mirrors
// IrcMonitorCoordinator::Presence.
type monitorPresence int

const (
	monitorPresenceUnknown monitorPresence = iota
	monitorPresenceOnline
	monitorPresenceOffline
)

// MonitorCoordinator owns MONITOR subscription state and per-nick presence.
type MonitorCoordinator struct {
	reducer    *irc.EventReducer
	monitors   *MonitorStore
	mutes      *MuteStore
	host       MonitorHost
	subscribed map[string]struct{}
	presence   map[string]map[string]monitorPresence
}

// NewMonitorCoordinator builds a coordinator over the shared reducer and
// stores.
func NewMonitorCoordinator(
	reducer *irc.EventReducer,
	monitors *MonitorStore,
	mutes *MuteStore,
	host MonitorHost,
) *MonitorCoordinator {
	return &MonitorCoordinator{
		reducer:    reducer,
		monitors:   monitors,
		mutes:      mutes,
		host:       host,
		subscribed: make(map[string]struct{}),
		presence:   make(map[string]map[string]monitorPresence),
	}
}

// ForgetPresence clears the session-scoped subscription and presence state for
// one network. It mirrors forgetPresence.
func (m *MonitorCoordinator) ForgetPresence(networkID string) {
	delete(m.subscribed, networkID)
	delete(m.presence, networkID)
}

// monitorDisplayNick returns the stored spelling of nick under the network
// case mapping, or nick itself when it is not stored. It mirrors
// monitorDisplayNick.
func (m *MonitorCoordinator) monitorDisplayNick(networkID, nick string) string {
	features := m.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	for _, stored := range m.monitors.Nicks(networkID) {
		if mapping.Equals(stored, nick) {
			return stored
		}
	}
	return nick
}

// monitorNotifyMuted reports whether online notification for nick is muted,
// either through the mute store or a muted conversation. It mirrors
// monitorNotifyMuted.
func (m *MonitorCoordinator) monitorNotifyMuted(networkID, nick string) bool {
	features := m.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	if m.mutes.Contains(networkID, nick, mapping) {
		return true
	}
	conversation := m.reducer.Find(m.reducer.ConversationKey(networkID, nick))
	return conversation != nil && conversation.Muted
}

// SubscribeMonitors sends the persisted nick list for one network once it is
// registered and MONITOR is advertised. Repeated calls for the same network
// are ignored until ForgetPresence. It mirrors subscribeMonitors.
func (m *MonitorCoordinator) SubscribeMonitors(networkID string) {
	if _, ok := m.subscribed[networkID]; ok {
		return
	}
	features := m.reducer.ServerFeatures(networkID)
	if !features.MonitorAdvertised() {
		return
	}
	s := m.host.SessionForNetwork(networkID)
	if s == nil || s.State() != session.StateRegistered {
		return
	}

	m.subscribed[networkID] = struct{}{}
	delete(m.presence, networkID)

	nicks := m.monitors.Listed(networkID, features.CaseMapping())
	if limit, ok := features.MonitorLimit(); ok && len(nicks) > limit {
		nicks = nicks[:limit]
	}
	if len(nicks) == 0 {
		return
	}
	s.SendMonitor('+', nicks)
}

// HandleMonitorPresence folds one 730/731 numeric into presence state and
// records the transition. It mirrors handleMonitorPresence.
func (m *MonitorCoordinator) HandleMonitorPresence(networkID string, message irc.Message, online bool) {
	features := m.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	states := m.presence[networkID]
	if states == nil {
		states = make(map[string]monitorPresence)
		m.presence[networkID] = states
	}
	for _, nick := range monitorTargetNicks(message) {
		if !m.monitors.Contains(networkID, nick, mapping) {
			continue
		}
		key := monitorFoldedNick(mapping, nick)
		previous := states[key]
		next := monitorPresenceOffline
		if online {
			next = monitorPresenceOnline
		}
		states[key] = next
		if previous == monitorPresenceUnknown || previous == next {
			continue
		}
		display := m.monitorDisplayNick(networkID, nick)
		body := "is offline"
		if online {
			body = "is online"
		}
		m.host.RecordStatus(irc.Outcome(networkID, display+" "+body, m.host.Now()))
		if m.monitorNotifyMuted(networkID, nick) {
			continue
		}
		m.host.NotifyMonitor(networkID, display, body, online && previous != monitorPresenceOnline)
	}
}

// HandleMonitorListFull handles 734: the server refused the subscription
// because the list is full, so every named target is dropped. It mirrors
// handleMonitorListFull.
func (m *MonitorCoordinator) HandleMonitorListFull(networkID string, message irc.Message) {
	limit := parameterText(message, 1)
	targets := parameterText(message, 2)
	features := m.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	for _, target := range strings.Split(targets, ",") {
		trimmed := monitorTargetNick(target)
		if trimmed == "" {
			continue
		}
		m.monitors.Remove(networkID, trimmed, mapping)
		if states := m.presence[networkID]; states != nil {
			delete(states, monitorFoldedNick(mapping, trimmed))
		}
	}
	m.host.RecordStatus(irc.Outcome(networkID, monitorListFullText(limit, targets), m.host.Now()))
}

// DispatchMonitor runs /monitored, /monitor, or /unmonitor. It mirrors
// dispatchMonitor.
func (m *MonitorCoordinator) DispatchMonitor(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	networkID := m.host.QueryNetworkID(surface)
	if networkID == "" {
		if surface == irc.SurfaceConversation {
			if _, ok := m.host.SelectedKey(); !ok {
				return irc.OutcomeWrongScope
			}
		}
		return irc.OutcomeRefused
	}
	s := m.host.SessionFor(surface)
	if s == nil || s.State() != session.StateRegistered {
		return irc.OutcomeNotConnected
	}

	features := m.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	advertised := features.MonitorAdvertised()

	if command.Verb == irc.VerbMonitored {
		if command.Argument != "" {
			return irc.OutcomeRefused
		}
		nicks := m.monitors.Listed(networkID, mapping)
		text := "Not watching anyone"
		if len(nicks) > 0 {
			states := m.presence[networkID]
			parts := make([]string, 0, len(nicks))
			for _, nick := range nicks {
				presence := states[monitorFoldedNick(mapping, nick)]
				state := "unknown"
				switch presence {
				case monitorPresenceOnline:
					state = "online"
				case monitorPresenceOffline:
					state = "offline"
				}
				parts = append(parts, nick+" ("+state+")")
			}
			text = "Watching: " + strings.Join(parts, ", ")
		}
		m.host.RecordStatus(irc.Outcome(networkID, text, m.host.Now()))
		return irc.OutcomeSent
	}

	nick := firstToken(command.Argument)
	if restAfterFirstToken(command.Argument) != "" || !ignoreNickIsUsable(nick, features) {
		return irc.OutcomeRefused
	}

	if !advertised {
		m.host.RecordStatus(irc.Outcome(networkID, "This network does not support MONITOR.", m.host.Now()))
		return irc.OutcomeSent
	}

	var text string
	if command.Verb == irc.VerbMonitor {
		if m.monitors.Contains(networkID, nick, mapping) {
			text = "Already watching " + nick
		} else if limit, ok := features.MonitorLimit(); ok &&
			len(m.monitors.Listed(networkID, mapping)) >= limit {
			m.host.RecordStatus(irc.Outcome(networkID, monitorListFullText(strconv.Itoa(limit), nick), m.host.Now()))
			return irc.OutcomeSent
		} else if m.monitors.Add(networkID, nick, mapping) {
			if !s.SendMonitor('+', []string{nick}) {
				m.monitors.Remove(networkID, nick, mapping)
				return irc.OutcomeRefused
			}
			text = "Watching " + nick
		} else {
			text = "Already watching " + nick
		}
	} else {
		removed := m.monitors.Remove(networkID, nick, mapping)
		if removed {
			if states := m.presence[networkID]; states != nil {
				delete(states, monitorFoldedNick(mapping, nick))
			}
			s.SendMonitor('-', []string{nick})
			text = "No longer watching " + nick
		} else {
			text = "Not watching " + nick
		}
	}
	m.host.RecordStatus(irc.Outcome(networkID, text, m.host.Now()))
	return irc.OutcomeSent
}

// monitorTargetNicks returns the comma-separated nicks of a 730/731/734
// message's trailing parameter, with any "!user@host" suffix stripped. It
// mirrors monitorTargetNicks.
func monitorTargetNicks(message irc.Message) []string {
	if len(message.Params) < 2 {
		return nil
	}
	trailing := parameterText(message, len(message.Params)-1)
	var nicks []string
	for _, target := range strings.Split(trailing, ",") {
		nick := monitorTargetNick(target)
		if nick != "" {
			nicks = append(nicks, nick)
		}
	}
	return nicks
}

// monitorTargetNick strips everything from the first '!' onward and trims. It
// mirrors monitorTargetNick.
func monitorTargetNick(target string) string {
	if bang := strings.IndexByte(target, '!'); bang >= 0 {
		target = target[:bang]
	}
	return strings.TrimSpace(target)
}

// monitorFoldedNick normalizes nick under the network case mapping. It mirrors
// the anonymous foldedNick helper.
func monitorFoldedNick(mapping irc.CaseMapping, nick string) string {
	return mapping.Normalize(nick)
}

// monitorListFullText renders the "Monitor list is full" Status text. It
// mirrors monitorListFullText.
func monitorListFullText(limit, targets string) string {
	text := "Monitor list is full"
	if limit != "" {
		text += " (" + limit + ")"
	}
	if targets != "" {
		text += ": " + strings.ReplaceAll(targets, ",", ", ")
	}
	return text + "."
}
