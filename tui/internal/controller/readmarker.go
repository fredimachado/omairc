package controller

import (
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// NoteTranscriptViewport records whether the shell is focused and the selected
// transcript is pinned to the bottom. It mirrors the Qt window's active plus
// transcript-at-end gate used before publishing a read marker.
func (c *Controller) NoteTranscriptViewport(focused bool, followEnd bool) {
	c.transcriptFocused = focused
	c.transcriptFollowEnd = followEnd
	c.syncReadMarkerForSelection()
}

// ReadMarkerReceived applies a server read marker for one target.
func (c *Controller) ReadMarkerReceived(networkID, target string, marker *time.Time) {
	if networkID == "" || target == "" {
		return
	}
	features := c.reducer.ServerFeatures(networkID)
	key := c.reducer.ConversationKey(networkID, target)
	if features.IsChannel(target) {
		if c.reducer.Find(key) == nil {
			return
		}
	} else if marker == nil {
		return
	}
	if !c.reducer.ApplyServerReadMarker(key, marker) {
		return
	}
	c.Publish(irc.ViewNotify{Conversations: true})
}

func (c *Controller) syncReadMarkerForSelection() {
	if c.selected == nil || !c.transcriptFollowEnd || !c.transcriptFocused || !c.reducer.WindowActive() {
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
	if conversation.ReadMarker != nil && !when.After(*conversation.ReadMarker) {
		return
	}
	target := conversation.Target
	if target == "" {
		target = c.selectedTarget
	}
	session.QueueReadMarkerSet(target, when)
}

func (c *Controller) requestReadMarkerGet(session *session.Session, networkID, target string) {
	if session == nil || target == "" {
		return
	}
	features := c.reducer.ServerFeatures(networkID)
	if features.IsChannel(target) {
		return
	}
	session.RequestReadMarkerGet(target)
}
