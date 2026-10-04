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
	target := conversation.Target
	if target == "" {
		target = key.NormalizedTarget
	}
	c.reducer.MarkHistoryPageCapTail(key)
	if !s.RequestOlderHistory(target, msgid, timestamp) {
		if !s.HistoryInflightFor(target) {
			c.reducer.ClearHistoryPageCapTail(key)
		}
		return false
	}
	return true
}

// OlderTranscriptHistoryInflight reports whether a CHATHISTORY page is already
// in flight for the selected conversation.
func (c *Controller) OlderTranscriptHistoryInflight() bool {
	key, ok := c.reducer.Selected()
	if !ok {
		return false
	}
	conversation := c.reducer.Find(key)
	if conversation == nil {
		return false
	}
	s := c.manager.Find(key.NetworkID)
	if s == nil {
		return false
	}
	target := conversation.Target
	if target == "" {
		target = key.NormalizedTarget
	}
	return s.HistoryInflightFor(target)
}

// ClearHistoryPageCapTailForConversationID drops tail-first trimming on one
// conversation when the reader leaves it while a BEFORE page is armed.
func (c *Controller) ClearHistoryPageCapTailForConversationID(conversationID string) {
	if conversationID == "" {
		return
	}
	key, ok := irc.ParseConversationID(conversationID)
	if !ok {
		return
	}
	c.reducer.ClearHistoryPageCapTail(key)
}

// ClearHistoryPageCapTail drops tail-first trimming when the reader returns to
// the live end of the transcript.
func (c *Controller) ClearHistoryPageCapTail() {
	key, ok := c.reducer.Selected()
	if !ok {
		return
	}
	c.reducer.ClearHistoryPageCapTail(key)
}

// TranscriptSpliceEpoch is the selected conversation's splice generation.
func (c *Controller) TranscriptSpliceEpoch() int {
	key, ok := c.reducer.Selected()
	if !ok {
		return 0
	}
	conversation := c.reducer.Find(key)
	if conversation == nil {
		return 0
	}
	return conversation.SpliceEpoch
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
		if !message.MsgID.IsEmpty() {
			return message.MsgID.Value, message.Timestamp, true
		}
	}
	for _, message := range messages {
		if !message.Timestamp.IsZero() {
			return "", message.Timestamp, true
		}
	}
	return "", time.Time{}, false
}
