package controller

import (
	"strings"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

// PlaybackCoordinator owns the per-connection playback state the controller
// used to keep: the registration snapshot, the ZNC PLAY requests already sent
// this connection, and the autojoin and joined-channel books. It is the Go port
// of IrcPlaybackCoordinator (src/irc/ircplaybackcoordinator.cpp).
//
// The controller forwards session events and passes the lookups it still owns
// (ZNC playback capability, MOTD-seen, open directs) as arguments; this type
// decides which PLAY requests go out and how a bouncer batch is trimmed.
type PlaybackCoordinator struct {
	times   *storage.PlaybackTimeStore
	reducer *irc.EventReducer

	// playbackSnapshot is the stamps saved at numeric 001, before this
	// connection's traffic. The first PLAY for each target reads it. Later
	// live lines update the store for the next attach and do not rewrite it.
	playbackSnapshot map[string][]storage.PlaybackTargetTime
	// zncPlaybackSent is the PLAY lines already written this connection.
	zncPlaybackSent map[string]zncPlaybackSent
	zncAutojoin     map[string][]string
	// zncJoinedChannels are the channels this connection has seen a self-JOIN
	// for; each needs its own PLAY retry once a playback batch is kept.
	zncJoinedChannels map[string][]string

	// sendZnc writes one ZNC playback PLAY line. It defaults to
	// session.SendRaw; tests substitute a recorder. It stays unexported so the
	// public API does not widen.
	sendZnc func(s *session.Session, target, from string) bool
}

// zncPlaybackSent is the PLAY book for one connection. all is `PLAY * 0` on a
// first attach with no saved stamps, which opens the clock for every target.
// queries is `PLAY * 0` after per-target requests on later attaches, which
// discovers offline query buffers. targets is each per-target PLAY, including
// a channel PLAY, so lines after that request may move the clock. A channel
// PLAY still does not stop a self-JOIN retry: the module drops a channel that
// is not on, and the retry stops only once a playback batch was kept.
type zncPlaybackSent struct {
	all     bool
	queries bool
	targets map[string]bool
}

// NewPlaybackCoordinator returns an empty coordinator over the persisted time
// store and the shared reducer.
func NewPlaybackCoordinator(times *storage.PlaybackTimeStore, reducer *irc.EventReducer) *PlaybackCoordinator {
	return &PlaybackCoordinator{
		times:             times,
		reducer:           reducer,
		playbackSnapshot:  make(map[string][]storage.PlaybackTargetTime),
		zncPlaybackSent:   make(map[string]zncPlaybackSent),
		zncAutojoin:       make(map[string][]string),
		zncJoinedChannels: make(map[string][]string),
		sendZnc: func(s *session.Session, target, from string) bool {
			return s.SendRaw("ZNC *playback PLAY " + target + " " + from)
		},
	}
}

// OnRegistered takes the registration snapshot from the persisted clocks and
// records this connection's autojoin. It mirrors onRegistered.
func (p *PlaybackCoordinator) OnRegistered(networkID string, autojoinChannels []string) {
	p.playbackSnapshot[networkID] = p.times.Targets(networkID)
	p.zncAutojoin[networkID] = append([]string(nil), autojoinChannels...)
	delete(p.zncJoinedChannels, networkID)
}

// OnLeftRegistration clears everything connection-scoped, because a reconnect
// must replay as if the connection never happened. It mirrors
// onLeftRegistration.
func (p *PlaybackCoordinator) OnLeftRegistration(networkID string) {
	delete(p.zncPlaybackSent, networkID)
	delete(p.playbackSnapshot, networkID)
	delete(p.zncAutojoin, networkID)
	delete(p.zncJoinedChannels, networkID)
}

// DropSnapshot clears only the registration snapshot: forgetting a network
// drops the stamps the next attach would resume from, but the ZNC books stay
// because the profile survives. It mirrors dropSnapshot.
func (p *PlaybackCoordinator) DropSnapshot(networkID string) {
	delete(p.playbackSnapshot, networkID)
}

// Rekey renames one snapshot target, merging the old stamp into newTarget. It
// mirrors rekey.
func (p *PlaybackCoordinator) Rekey(networkID, oldTarget, newTarget string) {
	rows, ok := p.playbackSnapshot[networkID]
	if !ok || oldTarget == "" || newTarget == "" {
		return
	}
	features := p.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	oldIndex := -1
	for index := range rows {
		if mapping.Equals(rows[index].Target, oldTarget) {
			oldIndex = index
			break
		}
	}
	if oldIndex < 0 {
		return
	}
	when := rows[oldIndex].When
	if mapping.Equals(oldTarget, newTarget) {
		rows[oldIndex].Target = newTarget
		p.playbackSnapshot[networkID] = rows
		return
	}
	rows = append(rows[:oldIndex], rows[oldIndex+1:]...)
	for index := range rows {
		if !mapping.Equals(rows[index].Target, newTarget) {
			continue
		}
		if !when.IsZero() && (rows[index].When.IsZero() || when.After(rows[index].When)) {
			rows[index].When = when
		}
		p.playbackSnapshot[networkID] = rows
		return
	}
	rows = append(rows, storage.PlaybackTargetTime{Target: newTarget, When: when})
	p.playbackSnapshot[networkID] = rows
}

// NoteKeptReplay consumes the reducer's kept replay lines and stamps the
// persisted clocks. zncPlaybackCap(networkID) reports whether the network
// advertised znc.in/playback. It mirrors noteKeptReplay.
func (p *PlaybackCoordinator) NoteKeptReplay(zncPlaybackCap func(networkID string) bool) {
	// PLAY's lower bound is exclusive. Only a replay line that landed, or
	// matched one already there, may move it. A held, dropped, or never-joined
	// channel batch stays eligible for the next PLAY.
	for _, line := range p.reducer.TakeKeptReplay() {
		// A line spliced before this target's first PLAY must not move the
		// exclusive bound that request is about to send, and must not be
		// stored for the next attach either.
		if !p.mayNotePlaybackTime(line.NetworkID, line.Target, zncPlaybackCap(line.NetworkID)) {
			continue
		}
		features := p.reducer.ServerFeatures(line.NetworkID)
		p.times.Note(line.NetworkID, line.Target, line.ServerTime, features.CaseMapping())
	}
}

// NotePlaybackClock records the newest server-time for the message's target so
// the next attach resumes from it. It mirrors notePlaybackClock.
func (p *PlaybackCoordinator) NotePlaybackClock(networkID string, message irc.Message, currentNick string, zncPlaybackCap bool) {
	if networkID == "" {
		return
	}
	if !strings.EqualFold(message.Command, "PRIVMSG") || len(message.Params) < 2 {
		return
	}
	when, ok := irc.ServerTimeOf(message)
	if !ok {
		return
	}
	if ctcp, ok := irc.ParseCtcpRequest(parameterText(message, 1)); ok && ctcp.Command != "ACTION" {
		return
	}

	features := p.reducer.ServerFeatures(networkID)
	wireTarget := parameterText(message, 0)
	if _, ok := irc.ConversationFor(networkID, wireTarget, message, currentNick, features); !ok {
		return
	}

	sender := irc.PrefixNick(message)
	channel := features.IsChannel(wireTarget)
	self := features.CaseMapping().Equals(sender, currentNick)
	displayTarget := sender
	if channel || self {
		displayTarget = wireTarget
	}
	if displayTarget == "" {
		return
	}
	if !channel && (!irc.NickIsRoutable(displayTarget) ||
		irc.TargetLooksLikeService(displayTarget, features)) {
		return
	}
	// A self echo whose query or channel does not exist yet is InboundSelf,
	// so the reducer drops it. Each target's PLAY bound is exclusive, so
	// noting that line would skip bouncer history that was never kept. An
	// admitted line, or one that matches an existing row, may still move the
	// clock.
	if self && p.reducer.Find(p.reducer.ConversationKey(networkID, displayTarget)) == nil {
		return
	}
	// The first PLAY for this target reads the registration snapshot. A line
	// before that request is not stored, or the next attach skips the same
	// buffer. PLAY * 0 opens every target; a per-target PLAY opens only that
	// one.
	if !p.mayNotePlaybackTime(networkID, displayTarget, zncPlaybackCap) {
		return
	}
	p.times.Note(networkID, displayTarget, when, features.CaseMapping())
}

// TrimBouncerBatch drops wildcard PLAY lines older than the registration stamp
// from a bouncer batch. The controller applies the event afterwards. It mirrors
// trimBouncerBatch.
func (p *PlaybackCoordinator) TrimBouncerBatch(networkID string, event *irc.HistoryEvent) {
	sent, ok := p.zncPlaybackSent[networkID]
	if !ok || !sent.queries {
		return
	}
	bound, ok := p.playbackSnapshotTime(networkID, event.Target)
	if !ok {
		return
	}
	boundMs := bound.UTC().UnixMilli()
	kept := event.Lines[:0]
	for _, line := range event.Lines {
		if line.ServerTime == nil || line.ServerTime.IsZero() {
			kept = append(kept, line)
			continue
		}
		lineMs := line.ServerTime.UTC().UnixMilli()
		if lineMs > boundMs {
			kept = append(kept, line)
			continue
		}
		if lineMs < boundMs {
			continue
		}
		if line.MsgID.Value == "" {
			continue
		}
		kept = append(kept, line)
	}
	event.Lines = kept
}

// NoteJoinedChannel records a self-JOIN so a later request retries the
// channel's own PLAY until a playback batch is kept. It mirrors
// noteJoinedChannel.
func (p *PlaybackCoordinator) NoteJoinedChannel(networkID, channel string) {
	if networkID == "" || channel == "" {
		return
	}
	features := p.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	channels := p.zncJoinedChannels[networkID]
	for _, existing := range channels {
		if mapping.Equals(existing, channel) {
			return
		}
	}
	p.zncJoinedChannels[networkID] = append(channels, channel)
}

// Request sends the PLAY lines for a freshly registered, MOTD-seen network. It
// mirrors request.
func (p *PlaybackCoordinator) Request(s *session.Session, zncPlaybackCap, motdSeen bool,
	restoredDirects []string, persistableDirect func(target string) bool) {
	if s == nil || s.State() != session.StateRegistered {
		return
	}
	networkID := s.NetworkID()
	// Open queries are restored at MOTD end. PLAY before that uses a stamp
	// that can skip self-only lines whose direct does not exist yet.
	if !motdSeen {
		return
	}
	if !zncPlaybackCap {
		return
	}
	sent, hasSent := p.zncPlaybackSent[networkID]
	if hasSent && sent.all {
		return
	}

	features := p.reducer.ServerFeatures(networkID)
	// Registration snapshot, taken at numeric 001. Traffic on this connection
	// must not replace the stamp this first PLAY sends.
	stored := p.playbackSnapshot[networkID]

	// Nothing stored yet: one PLAY * 0 fetches every buffer, including queries.
	// Autojoin must not replace that with per-channel PLAY. The module drops a
	// channel PLAY until that channel is on, and PLAY * 0 does not mark the
	// channel covered.
	if len(stored) == 0 {
		alreadySpecific := hasSent && len(sent.targets) > 0
		if !alreadySpecific {
			if !p.sendZnc(s, "*", "0") {
				return
			}
			p.markAll(networkID)
		}
	} else {
		for _, row := range stored {
			normalized := p.reducer.ConversationKey(networkID, row.Target).NormalizedTarget
			if p.zncPlaybackCovers(networkID, normalized) {
				continue
			}
			if !p.sendZnc(s, row.Target, storage.PlaybackPlayStamp(row.When, true)) {
				return
			}
			p.markTarget(networkID, normalized)
		}
		// A restored direct with no stamp is absent from the store, so the loop
		// above skips it once any other target has one. Ask from the beginning.
		// A nick the user never opened is not listed.
		for _, nick := range restoredDirects {
			if !persistableDirect(nick) {
				continue
			}
			if _, ok := p.playbackSnapshotTime(networkID, nick); ok {
				continue
			}
			key := p.reducer.ConversationKey(networkID, nick)
			if p.reducer.Find(key) == nil {
				continue
			}
			if p.zncPlaybackCovers(networkID, key.NormalizedTarget) {
				continue
			}
			if !p.sendZnc(s, nick, "0") {
				return
			}
			p.markTarget(networkID, key.NormalizedTarget)
		}
		// Snapshot from registration, before this connection's JOIN echoes. A
		// channel joined before the MOTD is not autojoin for this request.
		for _, channel := range p.zncAutojoin[networkID] {
			if channel == "" || !features.IsChannel(channel) {
				continue
			}
			if _, ok := p.playbackSnapshotTime(networkID, channel); ok {
				continue
			}
			normalized := p.reducer.ConversationKey(networkID, channel).NormalizedTarget
			if p.zncPlaybackCovers(networkID, normalized) {
				continue
			}
			if !p.sendZnc(s, channel, "0") {
				return
			}
			p.markTarget(networkID, normalized)
		}
		// Offline query buffers are absent from the snapshot, restored directs,
		// and autojoin. With znc.in/playback enabled the module suppresses
		// automatic delivery, so PLAY * 0 discovers them. Known targets already
		// got per-target PLAY with their resume bounds.
		if !p.zncPlaybackSent[networkID].queries {
			if !p.sendZnc(s, "*", "0") {
				return
			}
			p.markQueries(networkID)
		}
	}
	// A channel joined before this request still needs its own PLAY when no
	// playback batch for it has been kept. PLAY * 0 does not cover that retry:
	// a channel that was not on yet never answered.
	for _, channel := range p.zncJoinedChannels[networkID] {
		p.RequestChannelPlayback(s, channel, zncPlaybackCap, motdSeen)
	}
}

// RequestChannelPlayback retries one joined channel's PLAY. It mirrors
// requestChannelPlayback.
func (p *PlaybackCoordinator) RequestChannelPlayback(s *session.Session, channel string, zncPlaybackCap, motdSeen bool) {
	if s == nil || channel == "" || s.State() != session.StateRegistered {
		return
	}
	networkID := s.NetworkID()
	if !motdSeen {
		return
	}
	if !zncPlaybackCap {
		return
	}
	features := p.reducer.ServerFeatures(networkID)
	if !features.IsChannel(channel) {
		return
	}
	// Covered only after a playback batch was spliced or deduped. A PLAY
	// written at MOTD does not count, and neither does PLAY * 0: the module
	// emits nothing for a channel that is not on.
	if p.reducer.PlaybackBatchKept(networkID, channel) {
		return
	}
	// A self-JOIN retry still uses the registration snapshot, not a live line
	// from earlier in this connection.
	when, ok := p.playbackSnapshotTime(networkID, channel)
	if !p.sendZnc(s, channel, storage.PlaybackPlayStamp(when, ok)) {
		return
	}
	p.markTarget(networkID, p.reducer.ConversationKey(networkID, channel).NormalizedTarget)
}

// markAll records that PLAY * 0 opened every target for the network.
func (p *PlaybackCoordinator) markAll(networkID string) {
	sent := p.zncPlaybackSent[networkID]
	sent.all = true
	p.zncPlaybackSent[networkID] = sent
}

// markQueries records that the post-target PLAY * 0 discovery line went out.
func (p *PlaybackCoordinator) markQueries(networkID string) {
	sent := p.zncPlaybackSent[networkID]
	sent.queries = true
	p.zncPlaybackSent[networkID] = sent
}

// markTarget records one per-target PLAY by normalized target.
func (p *PlaybackCoordinator) markTarget(networkID, normalizedTarget string) {
	sent := p.zncPlaybackSent[networkID]
	if sent.targets == nil {
		sent.targets = make(map[string]bool)
	}
	sent.targets[normalizedTarget] = true
	p.zncPlaybackSent[networkID] = sent
}

// zncPlaybackCovers reports whether a PLAY line already sent this connection
// covers normalizedTarget. It mirrors zncPlaybackCovers.
func (p *PlaybackCoordinator) zncPlaybackCovers(networkID, normalizedTarget string) bool {
	sent, ok := p.zncPlaybackSent[networkID]
	if !ok {
		return false
	}
	return sent.all || sent.queries || sent.targets[normalizedTarget]
}

// playbackSnapshotTime returns the registration stamp for target, or reports
// false when the network has no snapshot or the target is not in it. It mirrors
// playbackSnapshotTime.
func (p *PlaybackCoordinator) playbackSnapshotTime(networkID, target string) (time.Time, bool) {
	rows, ok := p.playbackSnapshot[networkID]
	if !ok || target == "" {
		return time.Time{}, false
	}
	features := p.reducer.ServerFeatures(networkID)
	mapping := features.CaseMapping()
	for _, row := range rows {
		if mapping.Equals(row.Target, target) {
			return row.When, true
		}
	}
	return time.Time{}, false
}

// mayNotePlaybackTime reports whether a live line may move the stored clock for
// networkID/target. It mirrors mayNotePlaybackTime.
func (p *PlaybackCoordinator) mayNotePlaybackTime(networkID, target string, zncPlaybackCap bool) bool {
	if networkID == "" || target == "" {
		return false
	}
	// No playback request is coming. A line the user actually saw is the stamp
	// a later playback-capable attach should resume from.
	if !zncPlaybackCap {
		return true
	}
	return p.zncPlaybackCovers(networkID,
		p.reducer.ConversationKey(networkID, target).NormalizedTarget)
}
