package controller

import (
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// readMarkerState tracks direct messages that already received a MARKREAD get
// this controller session, and which still await that get's reply before a set
// may publish. It mirrors client/src/controller/readmarker.rs ReadMarkerState.
type readMarkerState struct {
	getSent     map[irc.ConversationKey]struct{}
	awaitingGet map[irc.ConversationKey]struct{}
}

func newReadMarkerState() readMarkerState {
	return readMarkerState{
		getSent:     make(map[irc.ConversationKey]struct{}),
		awaitingGet: make(map[irc.ConversationKey]struct{}),
	}
}

func (s *readMarkerState) forgetNetwork(networkID string) {
	for key := range s.getSent {
		if key.NetworkID == networkID {
			delete(s.getSent, key)
		}
	}
	for key := range s.awaitingGet {
		if key.NetworkID == networkID {
			delete(s.awaitingGet, key)
		}
	}
}

func (s *readMarkerState) forgetConversation(key irc.ConversationKey) {
	delete(s.getSent, key)
	delete(s.awaitingGet, key)
}

func (s *readMarkerState) noteGetSent(key irc.ConversationKey) {
	s.getSent[key] = struct{}{}
	s.awaitingGet[key] = struct{}{}
}

func (s *readMarkerState) getSentContains(key irc.ConversationKey) bool {
	_, ok := s.getSent[key]
	return ok
}

func (s *readMarkerState) clearAwaitingGet(key irc.ConversationKey) bool {
	_, ok := s.awaitingGet[key]
	if ok {
		delete(s.awaitingGet, key)
	}
	return ok
}

func (s *readMarkerState) clearAwaitingNetwork(networkID string) bool {
	released := false
	for key := range s.awaitingGet {
		if key.NetworkID == networkID {
			delete(s.awaitingGet, key)
			released = true
		}
	}
	return released
}

func (s *readMarkerState) awaitsGet(key irc.ConversationKey) bool {
	_, ok := s.awaitingGet[key]
	return ok
}

// NoteTranscriptViewport records whether the shell is focused and the selected
// transcript is pinned to the bottom. It mirrors the Qt window's active plus
// transcript-at-end gate used before publishing a read marker.
func (c *Controller) NoteTranscriptViewport(focused bool, followEnd bool) {
	c.transcriptFocused = focused
	c.transcriptFollowEnd = followEnd
	c.syncReadMarkerForSelection()
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
		if c.readMarkers.clearAwaitingNetwork(networkID) {
			c.syncReadMarkerForSelection()
		}
		return
	}
	features := c.reducer.ServerFeatures(networkID)
	key := c.reducer.ConversationKey(networkID, target)
	awaited := c.readMarkers.clearAwaitingGet(key)
	// Welcome may record the hold under the selected key before 005 arrives,
	// when ConversationKey still uses the default casemapping.
	if !awaited && c.selected != nil && c.selected.NetworkID == networkID {
		if features.CaseMapping().Equals(c.selected.NormalizedTarget, target) {
			awaited = c.readMarkers.clearAwaitingGet(*c.selected)
		}
	}
	if !awaited {
		for pending := range c.readMarkers.awaitingGet {
			if pending.NetworkID != networkID {
				continue
			}
			if features.CaseMapping().Equals(pending.NormalizedTarget, target) {
				awaited = c.readMarkers.clearAwaitingGet(pending)
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
			c.syncReadMarkerForSelection()
		}
		return
	}
	if c.reducer.ApplyServerReadMarker(key, marker) {
		c.Publish(irc.ViewNotify{Conversations: true})
	}
	if awaited {
		c.syncReadMarkerForSelection()
	}
}

func (c *Controller) syncReadMarkerForSelection() {
	if c.selected == nil || c.consoleOpen || !c.transcriptFollowEnd || !c.transcriptFocused || !c.reducer.WindowActive() {
		return
	}
	if c.readMarkers.awaitsGet(*c.selected) {
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
	if c.readMarkers.getSentContains(key) {
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
	if c.readMarkers.clearAwaitingNetwork(networkID) {
		c.syncReadMarkerForSelection()
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
