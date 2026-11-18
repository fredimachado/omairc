package controller

import (
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// getPhase is the one GET phase per conversation: not sent (absent from the
// map), awaiting the reply, or released once the reply or a FAIL arrived. Only
// noteGetSent constructs getAwaiting, so awaiting a GET that was never sent
// cannot be stored. It mirrors client/src/controller/readmarker.rs GetPhase.
type getPhase interface{ getPhase() }

type getNotSent struct{}

type getAwaiting struct{}

type getReleased struct{}

func (getNotSent) getPhase()  {}
func (getAwaiting) getPhase() {}
func (getReleased) getPhase() {}

// readMarkerState tracks the GET phase of each direct message this controller
// session. Released keeps the "do not GET again" role until the conversation or
// network is forgotten.
type readMarkerState struct {
	gets map[irc.ConversationKey]getPhase
}

func newReadMarkerState() readMarkerState {
	return readMarkerState{gets: make(map[irc.ConversationKey]getPhase)}
}

// readMarkerViewport is what the shell reports. The transcript-position work
// changes where FollowEnd comes from, never this type.
type readMarkerViewport struct {
	Focused, FollowEnd bool
}

// open reports whether the viewport is caught up and focused.
func (v readMarkerViewport) open() bool { return v.Focused && v.FollowEnd }

func (s *readMarkerState) forgetNetwork(networkID string) {
	for key := range s.gets {
		if key.NetworkID == networkID {
			delete(s.gets, key)
		}
	}
}

func (s *readMarkerState) forgetConversation(key irc.ConversationKey) {
	delete(s.gets, key)
}

func (s *readMarkerState) phase(key irc.ConversationKey) getPhase {
	if phase, ok := s.gets[key]; ok {
		return phase
	}
	return getNotSent{}
}

// noteGetSent is the only constructor of getAwaiting; call it after the
// session accepted the GET.
func (s *readMarkerState) noteGetSent(key irc.ConversationKey) {
	s.gets[key] = getAwaiting{}
}

// releaseGet moves an awaiting GET to released and reports whether it was
// awaiting.
func (s *readMarkerState) releaseGet(key irc.ConversationKey) bool {
	if _, ok := s.phase(key).(getAwaiting); !ok {
		return false
	}
	s.gets[key] = getReleased{}
	return true
}

// releaseNetwork releases every awaiting GET on the network (FAIL without a
// target) and reports whether any was awaiting.
func (s *readMarkerState) releaseNetwork(networkID string) bool {
	released := false
	for key, phase := range s.gets {
		if _, ok := phase.(getAwaiting); ok && key.NetworkID == networkID {
			s.gets[key] = getReleased{}
			released = true
		}
	}
	return released
}

// awaitingKeys lists the awaiting GETs on one network.
func (s *readMarkerState) awaitingKeys(networkID string) []irc.ConversationKey {
	var keys []irc.ConversationKey
	for key, phase := range s.gets {
		if _, ok := phase.(getAwaiting); ok && key.NetworkID == networkID {
			keys = append(keys, key)
		}
	}
	return keys
}

// NoteTranscriptViewport records whether the shell is focused and the selected
// transcript is pinned to the bottom. It mirrors the Qt window's active plus
// transcript-at-end gate used before publishing a read marker.
func (c *Controller) NoteTranscriptViewport(focused bool, followEnd bool) {
	c.viewport = readMarkerViewport{Focused: focused, FollowEnd: followEnd}
	c.publishReadMarker()
}

// ReadMarkerReceived applies a server read marker for one target. An empty
// target clears every awaiting get on the network (FAIL without a target).
func (c *Controller) ReadMarkerReceived(networkID, target string, marker *time.Time) {
	c.lock()
	defer c.unlock()
	if networkID == "" {
		return
	}
	if target == "" {
		if c.readMarkers.releaseNetwork(networkID) {
			c.publishReadMarker()
		}
		return
	}
	features := c.reducer.ServerFeatures(networkID)
	key := c.reducer.ConversationKey(networkID, target)
	awaited := c.readMarkers.releaseGet(key)
	// Welcome may record the hold under the selected key before 005 arrives,
	// when ConversationKey still uses the default casemapping.
	if !awaited && c.selected != nil && c.selected.NetworkID == networkID {
		if features.CaseMapping().Equals(c.selected.NormalizedTarget, target) {
			awaited = c.readMarkers.releaseGet(*c.selected)
		}
	}
	if !awaited {
		for _, pending := range c.readMarkers.awaitingKeys(networkID) {
			if features.CaseMapping().Equals(pending.NormalizedTarget, target) {
				awaited = c.readMarkers.releaseGet(pending)
				break
			}
		}
	}
	if features.IsChannel(target) {
		if c.reducer.Find(key) == nil {
			return
		}
	} else if marker == nil {
		if awaited {
			c.publishReadMarker()
		}
		return
	}
	if c.reducer.ApplyServerReadMarker(key, marker) {
		c.Publish(irc.ViewNotify{Conversations: true})
	}
	if awaited {
		c.publishReadMarker()
	}
}

// publishReadMarker is the only caller that asks a session to send a read
// marker SET. Inputs: the selection, the GET phase, the viewport the shell
// reported, and the reducer's latest message time. The session's SET slot
// decides in-flight dedupe. bin/check-conventions enforces the single caller.
func (c *Controller) publishReadMarker() {
	if c.selected == nil || c.consoleOpen || !c.viewport.open() || !c.reducer.WindowActive() {
		return
	}
	if _, awaiting := c.readMarkers.phase(*c.selected).(getAwaiting); awaiting {
		return
	}
	session := c.manager.Find(c.selected.NetworkID)
	if session == nil {
		return
	}
	conversation := c.reducer.Find(*c.selected)
	if conversation == nil {
		return
	}
	when, ok := irc.LatestServerMessageTime(conversation)
	if !ok {
		return
	}
	if conversation.ReadMarker != nil && !irc.ReadMarkerTimeAfter(when, *conversation.ReadMarker) {
		return
	}
	target := conversation.Target
	if target == "" {
		target = c.selectedTarget
	}
	session.QueueReadMarkerSet(target, when)
}

// requestReadMarkerGetWithKey asks for one MARKREAD get, recording the hold under
// key (the selection's conversation identity) so pre-005 casemapping cannot miss it.
func (c *Controller) requestReadMarkerGetWithKey(key irc.ConversationKey, target string) {
	if target == "" {
		return
	}
	features := c.reducer.ServerFeatures(key.NetworkID)
	if features.IsChannel(target) {
		return
	}
	if _, notSent := c.readMarkers.phase(key).(getNotSent); !notSent {
		return
	}
	session := c.manager.Find(key.NetworkID)
	if session == nil {
		return
	}
	if session.ReadMarkerCommand() == "" {
		return
	}
	if !session.RequestReadMarkerGet(target) {
		return
	}
	c.readMarkers.noteGetSent(key)
}

func (c *Controller) readMarkerForgetNetwork(networkID string) {
	c.readMarkers.forgetNetwork(networkID)
}

func (c *Controller) readMarkerClearAwaitingNetwork(networkID string) {
	if c.readMarkers.releaseNetwork(networkID) {
		c.publishReadMarker()
	}
}

func (c *Controller) requestReadMarkerGetForSelectedDirect(networkID string) {
	if c.selected == nil || c.selected.NetworkID != networkID {
		return
	}
	features := c.reducer.ServerFeatures(networkID)
	if features.IsChannel(c.selected.NormalizedTarget) {
		return
	}
	conversation := c.reducer.Find(*c.selected)
	if conversation == nil {
		return
	}
	target := conversation.Target
	if target == "" {
		target = c.selectedTarget
	}
	c.requestReadMarkerGetWithKey(*c.selected, target)
}
