package controller

import (
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// RequestOlderTranscriptHistory asks the focused network for one older
// CHATHISTORY page before the oldest loaded transcript row. It returns false
// when paging cannot send a request.
func (c *Controller) RequestOlderTranscriptHistory() bool {
	key, ok := c.reducer.Selected()
	if !ok {
		return false
	}
	conversation := c.reducer.Find(key)
	if conversation == nil || len(conversation.Messages) == 0 {
		return false
	}
	msgid, timestamp, ok := oldestHistoryCursor(conversation.Messages)
	if !ok {
		return false
	}
	s := c.manager.Find(key.NetworkID)
	if s == nil {
		return false
	}
	c.reducer.MarkHistoryPageCapTail(key)
	target := conversation.Target
	if target == "" {
		target = key.NormalizedTarget
	}
	if !s.RequestOlderHistory(target, msgid, timestamp) {
		conversation.HistoryPageCapTail = false
		return false
	}
	return true
}

// ChatHistoryEnabled reports whether the focused network negotiated batch and
// chathistory.
func (c *Controller) ChatHistoryEnabled() bool {
	s := c.manager.Find(c.FocusedNetworkID())
	if s == nil {
		return false
	}
	caps := s.Capabilities()
	return caps.Contains(irc.CapabilityBatch) && caps.Contains(irc.CapabilityChatHistory)
}

func oldestHistoryCursor(messages []irc.ReducedMessage) (msgid string, timestamp time.Time, ok bool) {
	for _, message := range messages {
		switch message.Kind {
		case irc.KindMessage, irc.KindAction:
			if !message.MsgID.IsEmpty() {
				return message.MsgID.Value, message.Timestamp, true
			}
			if !message.Timestamp.IsZero() {
				return "", message.Timestamp, true
			}
		}
	}
	return "", time.Time{}, false
}
