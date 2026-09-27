package controller

import (
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// This file is the Go port of IrcAutoawayRuntime in
// src/irc/ircautoawayruntime.{h,cpp}: the idle and grace timers, the tripped
// state, the per-network auto/manual away sets, and the /autoaway dispatch.
// Away decisions stay here; the controller only records Status lines and keeps
// its own unaway bookkeeping.
//
// Persistence (QSettings) is deferred to phase 11. LoadStored initializes a
// zero config and arms the idle timer, SetEphemeral is stored, and a
// persist-worthy change calls autoawaySave, which is a no-op for now. Every
// timer goes through the injected session.Clock.

// AutoawayHost is the surface IrcAutoawayRuntime drives. It mirrors
// IrcAutoawayRuntime::Host.
type AutoawayHost interface {
	// FindSession resolves one network's session, or nil.
	FindSession(networkID string) *session.Session
	// NetworkIDs lists every configured network id.
	NetworkIDs() []string
	// SelfAway reports whether the user is already away on one network.
	SelfAway(networkID string) bool
	// MarkUnawaySent records that the unaway line was sent for one network.
	MarkUnawaySent(networkID string)
	// RecordStatus appends one Status console entry.
	RecordStatus(networkID, text string)
	// QueryNetworkID resolves the network for a composer surface.
	QueryNetworkID(surface irc.ComposerSurface) string
	// ConsoleNetworkID returns the network id the Status console belongs to.
	ConsoleNetworkID() string
	// SelectedKey returns the selected conversation, if any.
	SelectedKey() (irc.ConversationKey, bool)
	// ApplyEvent folds one synthetic event into the reducer.
	ApplyEvent(event irc.Event)
	// Now is the clock the trimmed status text is stamped with.
	Now() time.Time
}

// AutoawayRuntime owns the auto-away timers and per-network away sets.
type AutoawayRuntime struct {
	host  AutoawayHost
	clock session.Clock

	ephemeral bool
	config    irc.AutoawayConfig

	idleTimer     session.Timer
	graceTimer    session.Timer
	idleInterval  time.Duration
	graceInterval time.Duration
	graceArmed    bool
	tripped       bool

	autoAwayNetworks   map[string]struct{}
	manualAwayNetworks map[string]struct{}
}

// NewAutoawayRuntime builds an auto-away runtime. A nil clock falls back to the
// real clock.
func NewAutoawayRuntime(host AutoawayHost, clock session.Clock) *AutoawayRuntime {
	if clock == nil {
		clock = session.NewRealClock()
	}
	return &AutoawayRuntime{
		host:               host,
		clock:              clock,
		autoAwayNetworks:   make(map[string]struct{}),
		manualAwayNetworks: make(map[string]struct{}),
	}
}

// SetEphemeral records whether this run must not persist preferences. The flag
// is stored now; autoawaySave already honors it when phase 11 fills in the
// QSettings half.
func (a *AutoawayRuntime) SetEphemeral(ephemeral bool) {
	a.ephemeral = ephemeral
}

// LoadStored loads the persisted configuration and arms the idle timer. The
// QSettings read is deferred, so the config starts zeroed.
func (a *AutoawayRuntime) LoadStored() {
	a.config = irc.AutoawayConfig{}
	a.autoawayArmIdle()
}

// DispatchAutoaway runs /autoaway. It mirrors dispatchAutoaway.
func (a *AutoawayRuntime) DispatchAutoaway(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	request := irc.ParseAutoawayArgument(command.Argument)
	if request.Kind == irc.AutoawayUsage {
		return a.autoawayEchoUsage(surface)
	}

	persist := false
	var text string
	previousReason := a.autoawayReason()
	switch request.Kind {
	case irc.AutoawayQuery:
		text = irc.FormatAutoawayQuery(a.config)
	case irc.AutoawayDisable:
		a.config.Enabled = false
		persist = true
		a.autoawayStopTimers()
		a.tripped = false
		a.autoawayClearNetworks(false)
		text = irc.FormatAutoawayConfirmation(a.config)
	case irc.AutoawayEnableOn:
		if a.config.TimeoutSeconds < irc.AutoawayMinTimeoutSeconds {
			return a.autoawayEchoUsage(surface)
		}
		a.config.Enabled = true
		persist = true
		if !a.tripped {
			a.autoawayArmIdle()
		}
		text = irc.FormatAutoawayConfirmation(a.config)
	case irc.AutoawaySetTimeout:
		a.config.Enabled = true
		a.config.TimeoutSeconds = request.TimeoutSeconds
		if request.Text != "" {
			a.config.OneShotReason = request.Text
		}
		persist = true
		if !a.tripped {
			a.autoawayArmIdle()
		}
		text = irc.FormatAutoawayConfirmation(a.config)
	case irc.AutoawaySetDefaultReason:
		a.config.DefaultReason = request.Text
		persist = true
		text = irc.FormatAutoawayConfirmation(a.config)
	case irc.AutoawayClearDefaultReason:
		a.config.DefaultReason = ""
		persist = true
		text = irc.FormatAutoawayConfirmation(a.config)
	case irc.AutoawayUsage:
		return a.autoawayEchoUsage(surface)
	}
	if persist {
		a.autoawaySave()
	}
	if a.tripped && a.config.Enabled && a.autoawayReason() != previousReason {
		a.autoawayRefreshReason()
	}
	return a.autoawayEchoFeedback(surface, text)
}

// NoteLocalActivity treats one input event as activity: it clears a tripped
// auto-away and re-arms the idle timer. It mirrors noteLocalActivity.
func (a *AutoawayRuntime) NoteLocalActivity() {
	wasTripped := a.tripped
	a.tripped = false
	if len(a.autoAwayNetworks) > 0 {
		a.autoawayClearNetworks(true)
	} else if wasTripped {
		a.config.OneShotReason = ""
	}
	if a.config.Enabled {
		a.autoawayArmIdle()
	}
}

// NoteManualAway records that the user ran /away on one network, so auto-away
// must not touch it. It mirrors noteManualAway.
func (a *AutoawayRuntime) NoteManualAway(networkID string) {
	delete(a.autoAwayNetworks, networkID)
	a.manualAwayNetworks[networkID] = struct{}{}
}

// NoteAwayCleared records that the user ran /back on one network. It mirrors
// noteAwayCleared.
func (a *AutoawayRuntime) NoteAwayCleared(networkID string) {
	_, wasAuto := a.autoAwayNetworks[networkID]
	delete(a.autoAwayNetworks, networkID)
	delete(a.manualAwayNetworks, networkID)
	if wasAuto && len(a.autoAwayNetworks) == 0 {
		a.config.OneShotReason = ""
	}
}

// OnSessionRegistered marks a freshly registered session away when auto-away
// is tripped. It mirrors onSessionRegistered.
func (a *AutoawayRuntime) OnSessionRegistered(s *session.Session) {
	if a.tripped && a.config.Enabled {
		a.autoawayMarkSession(s)
	}
}

// ForgetNetwork drops one network from both away sets. It mirrors
// forgetNetwork.
func (a *AutoawayRuntime) ForgetNetwork(networkID string) {
	delete(a.autoAwayNetworks, networkID)
	delete(a.manualAwayNetworks, networkID)
}

// FireIdleForTest runs the idle callback without waiting out the idle timer.
func (a *AutoawayRuntime) FireIdleForTest() { a.autoawayOnIdle() }

// FireGraceForTest runs the grace callback without waiting out the grace
// timer.
func (a *AutoawayRuntime) FireGraceForTest() { a.autoawayOnGrace() }

// IdleActiveForTest reports whether the idle timer is still scheduled.
func (a *AutoawayRuntime) IdleActiveForTest() bool { return a.idleTimer != nil }

// GraceActiveForTest reports whether the grace timer is still scheduled.
func (a *AutoawayRuntime) GraceActiveForTest() bool { return a.graceTimer != nil }

// IdleIntervalForTest returns the last idle interval, or 0 when never armed.
func (a *AutoawayRuntime) IdleIntervalForTest() time.Duration { return a.idleInterval }

// GraceIntervalForTest returns the last grace interval, or 0 when never armed.
func (a *AutoawayRuntime) GraceIntervalForTest() time.Duration { return a.graceInterval }

// autoawaySave persists the configuration. QSettings is deferred, so this is
// the no-op seam phase 11 fills in; the ephemeral flag is already honored.
func (a *AutoawayRuntime) autoawaySave() {
	if a.ephemeral {
		return
	}
}

// autoawayWithinRange reports whether the configured timeout is in the
// accepted range and auto-away is enabled.
func (a *AutoawayRuntime) autoawayWithinRange() bool {
	return a.config.Enabled &&
		a.config.TimeoutSeconds >= irc.AutoawayMinTimeoutSeconds &&
		a.config.TimeoutSeconds <= irc.AutoawayMaxTimeoutSeconds
}

// autoawayArmIdle restarts the idle timer when auto-away is armed. It mirrors
// armAutoawayIdle.
func (a *AutoawayRuntime) autoawayArmIdle() {
	a.autoawayStopTimers()
	if !a.autoawayWithinRange() {
		return
	}
	a.idleInterval = time.Duration(a.config.TimeoutSeconds) * time.Second
	a.idleTimer = a.clock.AfterFunc(a.idleInterval, a.autoawayOnIdle)
}

// autoawayStopTimers stops both timers and disarms the grace step. It mirrors
// stopAutoawayTimers.
func (a *AutoawayRuntime) autoawayStopTimers() {
	a.autoawayStopIdle()
	a.autoawayStopGrace()
	a.graceArmed = false
}

func (a *AutoawayRuntime) autoawayStopIdle() {
	if a.idleTimer != nil {
		a.idleTimer.Stop()
		a.idleTimer = nil
	}
}

func (a *AutoawayRuntime) autoawayStopGrace() {
	if a.graceTimer != nil {
		a.graceTimer.Stop()
		a.graceTimer = nil
	}
}

// autoawayOnIdle opens the grace window once the idle timer fires. It mirrors
// onAutoawayIdle.
func (a *AutoawayRuntime) autoawayOnIdle() {
	if !a.config.Enabled || a.config.TimeoutSeconds < irc.AutoawayMinTimeoutSeconds {
		return
	}
	a.autoawayStopIdle()
	a.graceArmed = true
	graceMs := irc.AutoawayGraceSeconds(a.config.TimeoutSeconds) * 1000
	if graceMs <= 0 {
		a.autoawayOnGrace()
		return
	}
	a.graceInterval = time.Duration(graceMs) * time.Millisecond
	a.graceTimer = a.clock.AfterFunc(a.graceInterval, a.autoawayOnGrace)
}

// autoawayOnGrace trips auto-away when the grace window elapses. It mirrors
// onAutoawayGrace.
func (a *AutoawayRuntime) autoawayOnGrace() {
	if !a.graceArmed {
		return
	}
	a.graceArmed = false
	a.autoawayStopGrace()
	a.autoawayTrip()
}

// autoawayReason returns the one-shot reason when set, else the default.
func (a *AutoawayRuntime) autoawayReason() string {
	if a.config.OneShotReason != "" {
		return a.config.OneShotReason
	}
	return a.config.DefaultReason
}

// autoawayRecordStatus records one non-empty Status line. It mirrors
// recordAutoawayStatus.
func (a *AutoawayRuntime) autoawayRecordStatus(networkID, text string) {
	if networkID == "" || text == "" {
		return
	}
	a.host.RecordStatus(networkID, text)
}

// autoawayMarkSession marks one registered session away and records it in the
// auto-away set. It mirrors markSessionAutoAway.
func (a *AutoawayRuntime) autoawayMarkSession(s *session.Session) bool {
	if s == nil || s.State() != session.StateRegistered {
		return false
	}
	if !s.MarkAway(a.autoawayReason()) {
		return false
	}
	a.autoAwayNetworks[s.NetworkID()] = struct{}{}
	return true
}

// autoawayRefreshReason re-sends AWAY with the current reason to every
// auto-away network. It mirrors refreshAutoAwayReason.
func (a *AutoawayRuntime) autoawayRefreshReason() {
	if !a.tripped || !a.config.Enabled {
		return
	}
	reason := a.autoawayReason()
	for networkID := range a.autoAwayNetworks {
		s := a.host.FindSession(networkID)
		if s == nil || s.State() != session.StateRegistered {
			continue
		}
		s.MarkAway(reason)
	}
}

// autoawayTrip turns auto-away on for every eligible registered network. It
// mirrors tripAutoaway.
func (a *AutoawayRuntime) autoawayTrip() {
	if !a.config.Enabled {
		return
	}
	a.tripped = true
	status := irc.FormatAutoawayTrippedStatus(a.autoawayReason())
	for _, networkID := range a.host.NetworkIDs() {
		if _, ok := a.autoAwayNetworks[networkID]; ok {
			continue
		}
		if _, ok := a.manualAwayNetworks[networkID]; ok {
			continue
		}
		if a.host.SelfAway(networkID) {
			continue
		}
		if a.autoawayMarkSession(a.host.FindSession(networkID)) {
			a.autoawayRecordStatus(networkID, status)
		}
	}
	a.autoawayStopTimers()
}

// autoawayClearNetworks clears the tripped state and brings every auto-away
// network back. It mirrors clearAutoAwayNetworks.
func (a *AutoawayRuntime) autoawayClearNetworks(logCleared bool) {
	a.tripped = false
	networks := make([]string, 0, len(a.autoAwayNetworks))
	for networkID := range a.autoAwayNetworks {
		networks = append(networks, networkID)
	}
	a.autoAwayNetworks = make(map[string]struct{})
	status := ""
	if logCleared {
		status = irc.FormatAutoawayClearedStatus()
	}
	for _, networkID := range networks {
		s := a.host.FindSession(networkID)
		if s == nil {
			continue
		}
		if s.State() == session.StateRegistered && s.ClearAway() {
			a.host.MarkUnawaySent(networkID)
			if logCleared {
				a.autoawayRecordStatus(networkID, status)
			}
		}
	}
	a.config.OneShotReason = ""
}

// autoawayEchoFeedback routes /autoaway feedback to the selected conversation
// or the Status console. It mirrors echoAutoawayFeedback.
func (a *AutoawayRuntime) autoawayEchoFeedback(surface irc.ComposerSurface, text string) irc.CommandOutcome {
	networkID := a.host.QueryNetworkID(surface)
	if networkID == "" {
		networkID = a.host.ConsoleNetworkID()
	}
	if networkID == "" {
		return irc.OutcomeRefused
	}

	if surface == irc.SurfaceConversation {
		selected, ok := a.host.SelectedKey()
		if !ok {
			return irc.OutcomeWrongScope
		}
		a.host.ApplyEvent(irc.WhoisTranscriptEvent{Destination: selected, FormattedBody: text})
		return irc.OutcomeSent
	}
	a.host.RecordStatus(networkID, text)
	return irc.OutcomeSent
}

// autoawayEchoUsage echoes the verb table's usage line. It mirrors
// echoAutoawayUsage.
func (a *AutoawayRuntime) autoawayEchoUsage(surface irc.ComposerSurface) irc.CommandOutcome {
	usage := "/autoaway [off|on|duration [reason]|reason [text]]"
	if spec := irc.FindVerb(irc.VerbAutoaway); spec != nil {
		usage = spec.Usage
	}
	return a.autoawayEchoFeedback(surface, usage)
}
