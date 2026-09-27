package controller

import (
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// This file is the Go port of IrcCommandDispatcher in
// src/irc/irccommanddispatcher.{h,cpp}: every slash-verb decision, with the
// side effects it cannot own performed through CommandHost. Join, part, mode,
// away, pref, ignore/mute/highlight, and query decisions stay here; the
// controller performs them and owns the stores this reads.

// QuietWire selects the outbound command used by a quiet send. It mirrors
// IrcCommandDispatcher::QuietWire.
type QuietWire int

const (
	// QuietWirePrivmsg sends PRIVMSG.
	QuietWirePrivmsg QuietWire = iota
	// QuietWireNotice sends NOTICE.
	QuietWireNotice
)

// QuietTarget selects which targets a quiet send accepts. It mirrors
// IrcCommandDispatcher::QuietTarget: /msg is nick-only, /notice accepts any
// target.
type QuietTarget int

const (
	// QuietTargetNick forbids channels.
	QuietTargetNick QuietTarget = iota
	// QuietTargetAny accepts any target.
	QuietTargetAny
)

// commandQuietSend is the wire/target pair for one quiet-send verb. It mirrors
// IrcCommandDispatcher::QuietSend.
type commandQuietSend struct {
	wire   QuietWire
	target QuietTarget
}

// commandQuietSendFor resolves the quiet-send shape for verb. It mirrors
// IrcCommandDispatcher::quietSendFor: nil for anything but Msg and Notice.
func commandQuietSendFor(verb irc.Verb) (commandQuietSend, bool) {
	switch verb {
	case irc.VerbMsg:
		return commandQuietSend{wire: QuietWirePrivmsg, target: QuietTargetNick}, true
	case irc.VerbNotice:
		return commandQuietSend{wire: QuietWireNotice, target: QuietTargetAny}, true
	}
	return commandQuietSend{}, false
}

// CommandHost is the surface IrcCommandDispatcher drives. It mirrors
// IrcCommandDispatcher::Host: session and selection lookup, model and console
// side effects, and the preference setters.
type CommandHost interface {
	QueryNetworkID(surface irc.ComposerSurface) string
	SessionFor(surface irc.ComposerSurface) *session.Session
	SessionForNetwork(networkID string) *session.Session
	SelectedSession() *session.Session
	SelectedTarget() string
	SelectedIsChannel() bool
	SelectedIsCloseableDirect() bool
	SelectedKey() (irc.ConversationKey, bool)
	HasNetworks() bool
	StatusNetworkID() string

	EchoLocal(kind irc.MessageKind, body string)
	OpenJoinedChannel(networkID, channel string)
	DismissChannel(networkID, channel string) bool
	DropSelectedDirectAndReselect()
	ClearSurface(surface irc.ComposerSurface) irc.CommandOutcome
	RememberOpenDirect(networkID, target string)
	NoteNickDelivery(networkID, target string)
	NoteLocalActivity()
	UnawayAfterChat(s *session.Session)
	NoteManualAway(networkID string)
	NoteAwayCleared(networkID string)
	ClearTypingTarget()
	SendSelectedMessage(body string) irc.CommandOutcome
	EchoIfPresent(s *session.Session, target, body string, wire QuietWire)

	ApplyMute(networkID, target string, muted bool) bool
	SyncHighlightWords(networkID string)
	ReloadConversations()
	SelectConversation(networkID, target string)
	ApplyWhoisTranscript(event irc.WhoisTranscriptEvent)
	RecordStatus(entry irc.StatusEntry)

	DispatchList(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome
	DispatchAutoaway(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome
	DispatchWhois(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome
	DispatchCtcp(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome
	DispatchStatus(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome
	DispatchAvatar(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome
	DispatchMonitor(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome

	PrefEnabled(name irc.PrefName) bool
	PrefApply(name irc.PrefName, enabled bool)

	Now() time.Time
}

// CommandDispatcher owns slash-command routing. It mirrors IrcCommandDispatcher.
type CommandDispatcher struct {
	reducer    *irc.EventReducer
	ignores    *IgnoreStore
	mutes      *MuteStore
	highlights *HighlightStore
	host       CommandHost
	cancelled  map[irc.ConversationKey]struct{}
}

// NewCommandDispatcher builds a dispatcher over the reducer, the three
// case-mapped stores, and the host callbacks.
func NewCommandDispatcher(
	reducer *irc.EventReducer,
	ignores *IgnoreStore,
	mutes *MuteStore,
	highlights *HighlightStore,
	host CommandHost,
) *CommandDispatcher {
	return &CommandDispatcher{
		reducer:    reducer,
		ignores:    ignores,
		mutes:      mutes,
		highlights: highlights,
		host:       host,
		cancelled:  map[irc.ConversationKey]struct{}{},
	}
}

// Dispatch routes one parsed command typed on surface. It mirrors
// IrcCommandDispatcher::dispatch.
func (d *CommandDispatcher) Dispatch(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	if command.Verb == irc.VerbEmpty {
		return irc.OutcomeSent
	}
	if command.Verb == irc.VerbUnknown {
		return irc.OutcomeUnsupported
	}
	if !command.AllowedOn(surface) {
		return irc.OutcomeWrongScope
	}

	if command.Verb == irc.VerbSay {
		return d.host.SendSelectedMessage(command.Argument)
	}

	if command.Verb == irc.VerbAction {
		active := d.host.SelectedSession()
		if _, ok := d.host.SelectedKey(); active == nil || !ok {
			return irc.OutcomeWrongScope
		}
		target := d.host.SelectedTarget()
		sent := active.SendAction(target, command.Argument)
		if sent {
			d.host.RememberOpenDirect(active.NetworkID(), target)
			d.host.NoteNickDelivery(active.NetworkID(), target)
			d.host.EchoLocal(irc.KindAction, command.Argument)
			d.host.ClearTypingTarget()
			d.host.NoteLocalActivity()
			d.host.UnawayAfterChat(active)
		}
		if sent {
			return irc.OutcomeSent
		}
		return irc.OutcomeRefused
	}

	if command.Verb == irc.VerbQuery {
		return d.dispatchQuery(command, surface)
	}

	if _, ok := commandQuietSendFor(command.Verb); ok {
		return d.dispatchQuietSend(command, surface)
	}

	if command.Verb == irc.VerbMode {
		return d.dispatchMode(command, surface)
	}

	switch command.Verb {
	case irc.VerbOp, irc.VerbDeop, irc.VerbVoice, irc.VerbDevoice, irc.VerbBan:
		return d.dispatchChannelModeWrapper(command, surface)
	}

	switch command.Verb {
	case irc.VerbNs, irc.VerbCs, irc.VerbZnc:
		return d.dispatchServiceMsg(command, surface)
	}

	if command.Verb == irc.VerbRaw {
		return d.dispatchRaw(command, surface)
	}

	if command.Verb == irc.VerbWhois {
		return d.host.DispatchWhois(command, surface)
	}

	switch command.Verb {
	case irc.VerbPing, irc.VerbTime, irc.VerbVersion:
		return d.host.DispatchCtcp(command, surface)
	}

	if command.Verb == irc.VerbClear {
		return d.host.ClearSurface(surface)
	}

	if command.Verb == irc.VerbClose {
		if !d.host.SelectedIsCloseableDirect() {
			return irc.OutcomeWrongScope
		}
		d.host.DropSelectedDirectAndReselect()
		return irc.OutcomeSent
	}

	if command.Verb == irc.VerbTopic {
		return d.setSelectedTopic(command.Argument)
	}

	switch command.Verb {
	case irc.VerbIgnore, irc.VerbUnignore, irc.VerbIgnored:
		return d.dispatchIgnore(command, surface)
	}

	switch command.Verb {
	case irc.VerbMonitor, irc.VerbUnmonitor, irc.VerbMonitored:
		return d.host.DispatchMonitor(command, surface)
	}

	switch command.Verb {
	case irc.VerbMute, irc.VerbUnmute, irc.VerbMuted:
		return d.dispatchMute(command, surface)
	}

	switch command.Verb {
	case irc.VerbHighlight, irc.VerbUnhighlight, irc.VerbHighlights:
		return d.dispatchHighlight(command, surface)
	}

	if command.Verb == irc.VerbHelp {
		return d.dispatchHelp(surface)
	}

	if command.Verb == irc.VerbAutoaway {
		return d.host.DispatchAutoaway(command, surface)
	}

	if command.Verb == irc.VerbPref {
		return d.dispatchPref(command, surface)
	}

	if command.Verb == irc.VerbList {
		return d.host.DispatchList(command, surface)
	}

	if command.Verb == irc.VerbStatus {
		return d.host.DispatchStatus(command, surface)
	}

	if command.Verb == irc.VerbAvatar {
		return d.host.DispatchAvatar(command, surface)
	}

	active := d.host.SessionFor(surface)
	if active == nil {
		if surface == irc.SurfaceConversation {
			if _, ok := d.host.SelectedKey(); !ok && d.host.HasNetworks() {
				return irc.OutcomeRefused
			}
		}
		return irc.OutcomeNotConnected
	}
	if active.State() != session.StateRegistered && command.Verb != irc.VerbQuit {
		return irc.OutcomeNotConnected
	}

	sent := false
	switch command.Verb {
	case irc.VerbJoin:
		return d.dispatchJoin(active, command)
	case irc.VerbPart:
		var outcome irc.CommandOutcome
		var done bool
		sent, outcome, done = d.dispatchPart(active, command, surface)
		if done {
			return outcome
		}
	case irc.VerbKick:
		var outcome irc.CommandOutcome
		var done bool
		sent, outcome, done = d.dispatchKick(active, command, surface)
		if done {
			return outcome
		}
	case irc.VerbInvite:
		var outcome irc.CommandOutcome
		var done bool
		sent, outcome, done = d.dispatchInvite(active, command, surface)
		if done {
			return outcome
		}
	case irc.VerbNick:
		sent = command.Argument != "" && active.ChangeNick(command.Argument)
	case irc.VerbQuit:
		sent = active.Quit(command.Argument)
	case irc.VerbAway:
		sent = active.SetAway(command.Argument)
		if sent {
			if strings.TrimSpace(command.Argument) == "" {
				d.host.NoteAwayCleared(active.NetworkID())
			} else {
				d.host.NoteManualAway(active.NetworkID())
			}
		}
	case irc.VerbBack:
		sent = active.ClearAway()
		if sent {
			d.host.NoteAwayCleared(active.NetworkID())
		}
	default:
		return irc.OutcomeUnsupported
	}
	if sent {
		return irc.OutcomeSent
	}
	return irc.OutcomeRefused
}

// dispatchJoin writes every parsed join target, erasing each successfully
// written channel's pending cancellation, then reveals the last one. It is the
// Join arm of dispatch.
func (d *CommandDispatcher) dispatchJoin(active *session.Session, command irc.Command) irc.CommandOutcome {
	features := d.reducer.ServerFeatures(active.NetworkID())
	var targets []irc.JoinTarget
	if command.Argument == "" {
		pending, ok := active.PendingInvite()
		if !ok {
			return irc.OutcomeRefused
		}
		target, ok := irc.MakeJoinTarget(pending.Channel, nil, features)
		if !ok {
			return irc.OutcomeRefused
		}
		targets = []irc.JoinTarget{target}
	} else {
		targets = irc.ParseJoinTargets(command.Argument, features)
		if targets == nil {
			return irc.OutcomeRefused
		}
	}
	sent := true
	for _, target := range targets {
		wrote := active.Join(target)
		if wrote {
			delete(d.cancelled, d.reducer.ConversationKey(active.NetworkID(), target.Channel()))
		}
		sent = wrote && sent
	}
	if sent {
		d.host.OpenJoinedChannel(active.NetworkID(), targets[len(targets)-1].Channel())
	}
	if sent {
		return irc.OutcomeSent
	}
	return irc.OutcomeRefused
}

// dispatchPart resolves the Part arm of dispatch. done is true when the arm
// returns an outcome directly (a WrongScope/Refused branch); otherwise sent
// carries the wire result.
func (d *CommandDispatcher) dispatchPart(
	active *session.Session,
	command irc.Command,
	surface irc.ComposerSurface,
) (bool, irc.CommandOutcome, bool) {
	channel := firstToken(command.Argument)
	if channel == "" {
		key, ok := d.host.SelectedKey()
		if !ok || !d.host.SelectedIsChannel() {
			return false, irc.OutcomeWrongScope, true
		}
		if key.NetworkID != d.host.QueryNetworkID(surface) {
			return false, irc.OutcomeRefused, true
		}
		channel = d.host.SelectedTarget()
		if channel == "" {
			return false, irc.OutcomeRefused, true
		}
	}
	key := d.reducer.ConversationKey(active.NetworkID(), channel)
	conversation := d.reducer.Find(key)
	joined := conversation != nil && conversation.Channel() != nil && conversation.Channel().Joined
	if d.host.DismissChannel(active.NetworkID(), channel) {
		if joined {
			active.Part(channel)
		} else {
			d.NoteCancelled(key)
		}
		return true, irc.OutcomeSent, false
	}
	return active.Part(channel), irc.OutcomeSent, false
}

// dispatchKick resolves the Kick arm of dispatch. done is true when the arm
// returns an outcome directly; otherwise sent carries the wire result.
func (d *CommandDispatcher) dispatchKick(
	active *session.Session,
	command irc.Command,
	surface irc.ComposerSurface,
) (bool, irc.CommandOutcome, bool) {
	first := firstToken(command.Argument)
	if first == "" {
		return false, irc.OutcomeRefused, true
	}
	networkID := d.host.QueryNetworkID(surface)
	features := d.reducer.ServerFeatures(networkID)
	if features.IsChannel(first) {
		afterChannel := restAfterFirstToken(command.Argument)
		nick := firstToken(afterChannel)
		if nick == "" {
			return false, irc.OutcomeRefused, true
		}
		return active.Kick(first, nick, restAfterFirstToken(afterChannel)), irc.OutcomeSent, false
	}
	key, ok := d.host.SelectedKey()
	if !ok || !d.host.SelectedIsChannel() {
		return false, irc.OutcomeWrongScope, true
	}
	if key.NetworkID != networkID {
		return false, irc.OutcomeRefused, true
	}
	channel := d.host.SelectedTarget()
	sent := channel != "" && active.Kick(channel, first, restAfterFirstToken(command.Argument))
	return sent, irc.OutcomeSent, false
}

// dispatchInvite resolves the Invite arm of dispatch. done is true when the arm
// returns an outcome directly; otherwise sent carries the wire result.
func (d *CommandDispatcher) dispatchInvite(
	active *session.Session,
	command irc.Command,
	surface irc.ComposerSurface,
) (bool, irc.CommandOutcome, bool) {
	nick := firstToken(command.Argument)
	if nick == "" {
		return false, irc.OutcomeWrongScope, true
	}
	networkID := d.host.QueryNetworkID(surface)
	features := d.reducer.ServerFeatures(networkID)
	if features.IsChannel(nick) {
		return false, irc.OutcomeRefused, true
	}
	rest := restAfterFirstToken(command.Argument)
	channel := ""
	if rest == "" {
		if surface == irc.SurfaceStatus {
			return false, irc.OutcomeWrongScope, true
		}
		key, ok := d.host.SelectedKey()
		if !ok || !d.host.SelectedIsChannel() {
			return false, irc.OutcomeWrongScope, true
		}
		if key.NetworkID != networkID {
			return false, irc.OutcomeRefused, true
		}
		channel = d.host.SelectedTarget()
	} else {
		channel = firstToken(rest)
		if !features.IsChannel(channel) || restAfterFirstToken(rest) != "" {
			return false, irc.OutcomeRefused, true
		}
	}
	sent := channel != "" && active.Invite(nick, channel)
	return sent, irc.OutcomeSent, false
}

// dispatchQuery opens or selects a direct-message conversation, optionally
// sending text into it. It mirrors IrcCommandDispatcher::dispatchQuery.
func (d *CommandDispatcher) dispatchQuery(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	nick := firstToken(command.Argument)
	rest := restAfterFirstToken(command.Argument)
	if nick == "" {
		return irc.OutcomeRefused
	}

	networkID := d.host.QueryNetworkID(surface)
	if networkID == "" {
		if surface == irc.SurfaceConversation {
			return irc.OutcomeWrongScope
		}
		return irc.OutcomeRefused
	}

	features := d.reducer.ServerFeatures(networkID)
	if features.IsChannel(nick) {
		return irc.OutcomeRefused
	}

	if rest != "" {
		active := d.host.SessionForNetwork(networkID)
		if active == nil || active.State() != session.StateRegistered {
			return irc.OutcomeNotConnected
		}
	}

	key := d.reducer.ConversationKey(networkID, nick)
	if d.reducer.EnsureConversation(key, nick, irc.CauseUserOpen) == nil {
		return irc.OutcomeRefused
	}
	d.host.RememberOpenDirect(networkID, nick)
	d.host.ReloadConversations()
	d.host.SelectConversation(networkID, nick)
	if rest == "" {
		return irc.OutcomeSent
	}
	return d.host.SendSelectedMessage(rest)
}

// dispatchQuietSend writes a /msg or /notice outside the transcript. It mirrors
// IrcCommandDispatcher::dispatchQuietSend.
func (d *CommandDispatcher) dispatchQuietSend(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	spec, ok := commandQuietSendFor(command.Verb)
	if !ok {
		return irc.OutcomeUnsupported
	}

	target := firstToken(command.Argument)
	body := restAfterFirstToken(command.Argument)
	if target == "" || body == "" {
		return irc.OutcomeRefused
	}

	networkID := d.host.QueryNetworkID(surface)
	if networkID == "" {
		if surface == irc.SurfaceConversation {
			return irc.OutcomeWrongScope
		}
		return irc.OutcomeRefused
	}

	if spec.target == QuietTargetNick {
		features := d.reducer.ServerFeatures(networkID)
		if features.IsChannel(target) {
			return irc.OutcomeRefused
		}
	}

	active := d.host.SessionForNetwork(networkID)
	if active == nil || active.State() != session.StateRegistered {
		return irc.OutcomeNotConnected
	}

	sent := false
	if spec.wire == QuietWirePrivmsg {
		sent = active.SendPrivmsg(target, body)
	} else {
		sent = active.SendNotice(target, body)
	}
	if !sent {
		return irc.OutcomeRefused
	}

	d.host.NoteNickDelivery(networkID, target)
	key := d.reducer.ConversationKey(networkID, target)
	d.reducer.EnsureConversation(key, target, irc.CauseQuietSend)
	d.host.EchoIfPresent(active, target, body, spec.wire)
	if spec.wire == QuietWirePrivmsg {
		d.host.NoteLocalActivity()
		d.host.UnawayAfterChat(active)
	}
	return irc.OutcomeSent
}

// dispatchMode parses and writes a MODE query or change. It mirrors
// IrcCommandDispatcher::dispatchMode.
func (d *CommandDispatcher) dispatchMode(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	if command.Argument == "" {
		return irc.OutcomeRefused
	}

	networkID := d.host.QueryNetworkID(surface)
	if networkID == "" {
		if surface == irc.SurfaceConversation {
			return irc.OutcomeWrongScope
		}
		return irc.OutcomeRefused
	}

	request, ok := irc.ParseChannelModeRequest(command.Argument, d.reducer.ServerFeatures(networkID))
	if !ok {
		return irc.OutcomeRefused
	}

	active := d.host.SessionForNetwork(networkID)
	if active == nil || active.State() != session.StateRegistered {
		return irc.OutcomeNotConnected
	}
	if active.SendChannelMode(request) {
		return irc.OutcomeSent
	}
	return irc.OutcomeRefused
}

// dispatchChannelModeWrapper turns /op //deop //voice //devoice //ban into a
// MODE change on the selected channel. It mirrors
// IrcCommandDispatcher::dispatchChannelModeWrapper.
func (d *CommandDispatcher) dispatchChannelModeWrapper(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	key, ok := d.host.SelectedKey()
	if !ok || !d.host.SelectedIsChannel() {
		return irc.OutcomeWrongScope
	}
	if key.NetworkID != d.host.QueryNetworkID(surface) {
		return irc.OutcomeRefused
	}

	token := firstToken(command.Argument)
	if token == "" || restAfterFirstToken(command.Argument) != "" {
		return irc.OutcomeRefused
	}

	modes := ""
	switch command.Verb {
	case irc.VerbOp:
		modes = "+o"
	case irc.VerbDeop:
		modes = "-o"
	case irc.VerbVoice:
		modes = "+v"
	case irc.VerbDevoice:
		modes = "-v"
	case irc.VerbBan:
		modes = "+b"
	default:
		return irc.OutcomeUnsupported
	}

	parameter := token
	if command.Verb == irc.VerbBan && !strings.Contains(token, "!") && !strings.Contains(token, "@") {
		parameter = token + "!*@*"
	}

	mode := irc.Command{
		Verb:     irc.VerbMode,
		Argument: d.host.SelectedTarget() + " " + modes + " " + parameter,
	}
	return d.dispatchMode(mode, surface)
}

// dispatchServiceMsg redirects /ns //cs //znc into a /msg to the service nick.
// It mirrors IrcCommandDispatcher::dispatchServiceMsg.
func (d *CommandDispatcher) dispatchServiceMsg(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	nick := ""
	switch command.Verb {
	case irc.VerbNs:
		nick = "NickServ"
	case irc.VerbCs:
		nick = "ChanServ"
	case irc.VerbZnc:
		nick = "*status"
	default:
		return irc.OutcomeUnsupported
	}
	msg := command
	msg.Verb = irc.VerbMsg
	if command.Argument == "" {
		msg.Argument = nick
	} else {
		msg.Argument = nick + " " + command.Argument
	}
	return d.dispatchQuietSend(msg, surface)
}

// dispatchRaw writes one literal line. It mirrors
// IrcCommandDispatcher::dispatchRaw.
func (d *CommandDispatcher) dispatchRaw(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	if command.Argument == "" {
		return irc.OutcomeRefused
	}

	networkID := d.host.QueryNetworkID(surface)
	if networkID == "" {
		if surface == irc.SurfaceConversation {
			return irc.OutcomeWrongScope
		}
		return irc.OutcomeRefused
	}

	active := d.host.SessionForNetwork(networkID)
	if active == nil || active.State() != session.StateRegistered {
		return irc.OutcomeNotConnected
	}
	if active.SendRaw(command.Argument) {
		return irc.OutcomeSent
	}
	return irc.OutcomeRefused
}

// dispatchIgnore applies /ignore //unignore //ignored over the ignore store. It
// mirrors IrcCommandDispatcher::dispatchIgnore.
func (d *CommandDispatcher) dispatchIgnore(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	networkID := d.host.QueryNetworkID(surface)
	if networkID == "" {
		if surface == irc.SurfaceConversation {
			if _, ok := d.host.SelectedKey(); !ok {
				return irc.OutcomeWrongScope
			}
		}
		return irc.OutcomeRefused
	}
	active := d.host.SessionFor(surface)
	if active == nil || active.State() == session.StateIdle || active.State() == session.StateFailed {
		return irc.OutcomeNotConnected
	}

	features := d.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	text := ""
	if command.Verb == irc.VerbIgnored {
		if command.Argument != "" {
			return irc.OutcomeRefused
		}
		nicks := d.ignores.Listed(networkID, mapping)
		if len(nicks) == 0 {
			text = "Not ignoring anyone"
		} else {
			text = "Ignoring: " + strings.Join(nicks, ", ")
		}
	} else {
		nick := firstToken(command.Argument)
		if restAfterFirstToken(command.Argument) != "" || !ignoreNickIsUsable(nick, features) {
			return irc.OutcomeRefused
		}
		if command.Verb == irc.VerbIgnore {
			if d.ignores.Add(networkID, nick, mapping) {
				text = "Ignoring " + nick
			} else {
				text = "Already ignoring " + nick
			}
		} else {
			if d.ignores.Remove(networkID, nick, mapping) {
				text = "No longer ignoring " + nick
			} else {
				text = "Not ignoring " + nick
			}
		}
	}
	d.host.RecordStatus(irc.Outcome(networkID, text, d.host.Now()))
	return irc.OutcomeSent
}

// dispatchMute applies /mute //unmute //muted over the host's mute applier. It
// mirrors IrcCommandDispatcher::dispatchMute.
func (d *CommandDispatcher) dispatchMute(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	networkID := d.host.QueryNetworkID(surface)
	if networkID == "" {
		if surface == irc.SurfaceConversation {
			if _, ok := d.host.SelectedKey(); !ok {
				return irc.OutcomeWrongScope
			}
		}
		return irc.OutcomeRefused
	}
	active := d.host.SessionFor(surface)
	if active == nil || active.State() == session.StateIdle || active.State() == session.StateFailed {
		return irc.OutcomeNotConnected
	}

	features := d.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	text := ""
	if command.Verb == irc.VerbMuted {
		if command.Argument != "" {
			return irc.OutcomeRefused
		}
		targets := d.mutes.Listed(networkID, mapping)
		if len(targets) == 0 {
			text = "Not muting anything"
		} else {
			text = "Muted: " + strings.Join(targets, ", ")
		}
	} else {
		target := firstToken(command.Argument)
		if restAfterFirstToken(command.Argument) != "" {
			return irc.OutcomeRefused
		}
		if target == "" {
			if surface == irc.SurfaceStatus {
				return irc.OutcomeWrongScope
			}
			if _, ok := d.host.SelectedKey(); !ok {
				return irc.OutcomeWrongScope
			}
			target = d.host.SelectedTarget()
		}
		if !muteTargetIsUsable(target, features) {
			return irc.OutcomeRefused
		}
		if command.Verb == irc.VerbMute {
			if d.host.ApplyMute(networkID, target, true) {
				text = "Muted " + target
			} else {
				text = "Already muted " + target
			}
		} else {
			if d.host.ApplyMute(networkID, target, false) {
				text = "No longer muted " + target
			} else {
				text = "Not muted " + target
			}
		}
	}
	d.host.RecordStatus(irc.Outcome(networkID, text, d.host.Now()))
	return irc.OutcomeSent
}

// dispatchHighlight applies /highlight //unhighlight //highlights over the
// highlight store. It mirrors IrcCommandDispatcher::dispatchHighlight.
func (d *CommandDispatcher) dispatchHighlight(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	networkID := d.host.QueryNetworkID(surface)
	if networkID == "" {
		if surface == irc.SurfaceConversation {
			if _, ok := d.host.SelectedKey(); !ok {
				return irc.OutcomeWrongScope
			}
		}
		return irc.OutcomeRefused
	}
	active := d.host.SessionFor(surface)
	if active == nil || active.State() == session.StateIdle || active.State() == session.StateFailed {
		return irc.OutcomeNotConnected
	}

	features := d.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	text := ""
	if command.Verb == irc.VerbHighlights {
		if command.Argument != "" {
			return irc.OutcomeRefused
		}
		words := d.highlights.Listed(networkID, mapping)
		if len(words) == 0 {
			text = "No highlight words"
		} else {
			text = "Highlights: " + strings.Join(words, ", ")
		}
	} else {
		word := firstToken(command.Argument)
		if word == "" || restAfterFirstToken(command.Argument) != "" {
			return irc.OutcomeRefused
		}
		if command.Verb == irc.VerbHighlight {
			if d.highlights.Add(networkID, word, mapping) {
				text = "Highlighting " + word
			} else {
				text = "Already highlighting " + word
			}
		} else {
			if d.highlights.Remove(networkID, word, mapping) {
				text = "No longer highlighting " + word
			} else {
				text = "Not highlighting " + word
			}
		}
		d.host.SyncHighlightWords(networkID)
	}
	d.host.RecordStatus(irc.Outcome(networkID, text, d.host.Now()))
	return irc.OutcomeSent
}

// dispatchHelp prints the verb catalog on the surface it was typed. It mirrors
// IrcCommandDispatcher::dispatchHelp.
func (d *CommandDispatcher) dispatchHelp(surface irc.ComposerSurface) irc.CommandOutcome {
	networkID := d.host.QueryNetworkID(surface)
	if networkID == "" {
		networkID = d.host.StatusNetworkID()
	}
	if networkID == "" {
		return irc.OutcomeRefused
	}

	rows := irc.VerbTable()
	names := make([]string, 0, len(rows))
	for _, row := range rows {
		names = append(names, "/"+row.Name)
	}
	text := "Commands: " + strings.Join(names, ", ") + ". Empty /join joins the latest invite."
	if surface == irc.SurfaceConversation {
		key, ok := d.host.SelectedKey()
		if !ok {
			return irc.OutcomeWrongScope
		}
		d.host.ApplyWhoisTranscript(irc.WhoisTranscriptEvent{Destination: key, FormattedBody: text})
		return irc.OutcomeSent
	}
	d.host.RecordStatus(irc.Outcome(networkID, text, d.host.Now()))
	return irc.OutcomeSent
}

// echoPrefFeedback prints /pref output: into the selected conversation, or into
// the Status console. It mirrors IrcCommandDispatcher::echoPrefFeedback.
func (d *CommandDispatcher) echoPrefFeedback(surface irc.ComposerSurface, text string) irc.CommandOutcome {
	if surface == irc.SurfaceConversation {
		if key, ok := d.host.SelectedKey(); ok {
			d.host.ApplyWhoisTranscript(irc.WhoisTranscriptEvent{Destination: key, FormattedBody: text})
		}
		return irc.OutcomeSent
	}
	if networkID := d.host.StatusNetworkID(); networkID != "" {
		d.host.RecordStatus(irc.Outcome(networkID, text, d.host.Now()))
	}
	return irc.OutcomeSent
}

// dispatchPref reads or sets the /pref toggles. It mirrors
// IrcCommandDispatcher::dispatchPref.
func (d *CommandDispatcher) dispatchPref(command irc.Command, surface irc.ComposerSurface) irc.CommandOutcome {
	request := irc.ParsePrefArgument(command.Argument)
	if request.Kind == irc.PrefUsageKind {
		usage := irc.PrefUsage()
		if spec := irc.FindVerb(irc.VerbPref); spec != nil {
			usage = spec.Usage
		}
		return d.echoPrefFeedback(surface, usage)
	}

	if request.Kind == irc.PrefSet {
		d.host.PrefApply(request.Name, request.Enabled)
	}

	if request.Kind == irc.PrefQueryAll {
		return d.echoPrefFeedback(surface, irc.FormatPrefList(
			d.host.PrefEnabled(irc.PrefDirects),
			d.host.PrefEnabled(irc.PrefAvatars),
			d.host.PrefEnabled(irc.PrefUnread)))
	}
	if request.Kind == irc.PrefQueryOne {
		return d.echoPrefFeedback(surface,
			irc.FormatPrefQuery(request.Name, d.host.PrefEnabled(request.Name)))
	}
	return d.echoPrefFeedback(surface,
		irc.FormatPrefState(request.Name, d.host.PrefEnabled(request.Name)))
}

// setSelectedTopic writes a channel topic. It mirrors
// IrcCommandDispatcher::setSelectedTopic.
func (d *CommandDispatcher) setSelectedTopic(topic string) irc.CommandOutcome {
	if _, ok := d.host.SelectedKey(); !ok || !d.host.SelectedIsChannel() {
		return irc.OutcomeWrongScope
	}
	if topic == "" {
		return irc.OutcomeSent
	}
	active := d.host.SelectedSession()
	if active == nil || active.State() != session.StateRegistered {
		return irc.OutcomeNotConnected
	}
	if active.SetTopic(d.host.SelectedTarget(), topic) {
		return irc.OutcomeSent
	}
	return irc.OutcomeRefused
}

// NoteCancelled records a pending self-join cancellation, shared with the
// inbound self-join branch in the controller's handleMessage.
func (d *CommandDispatcher) NoteCancelled(key irc.ConversationKey) {
	if d.cancelled == nil {
		d.cancelled = map[irc.ConversationKey]struct{}{}
	}
	d.cancelled[key] = struct{}{}
}

// TakeCancelledSelfJoin consumes a pending self-join cancellation and reports
// whether one was recorded.
func (d *CommandDispatcher) TakeCancelledSelfJoin(key irc.ConversationKey) bool {
	if _, ok := d.cancelled[key]; !ok {
		return false
	}
	delete(d.cancelled, key)
	return true
}

// ForgetNetwork drops every pending self-join cancellation for networkID.
func (d *CommandDispatcher) ForgetNetwork(networkID string) {
	for key := range d.cancelled {
		if key.NetworkID == networkID {
			delete(d.cancelled, key)
		}
	}
}
