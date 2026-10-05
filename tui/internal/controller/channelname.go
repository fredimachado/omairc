package controller

import "github.com/fredimachado/omairc/tui/internal/irc"

// ChannelNameAt returns the channel name at index in text for the focused
// network, using that network's CHANTYPES. An empty focused network matches
// nothing. It mirrors IrcController::channelNameAt.
func (c *Controller) ChannelNameAt(text string, index int) string {
	if c == nil {
		return ""
	}
	networkID := c.FocusedNetworkID()
	if networkID == "" {
		return ""
	}
	return irc.ChannelNameAt(text, index, c.reducer.ServerFeatures(networkID))
}

// ChannelNameSpans returns every channel name in text for the focused
// network. It mirrors IrcController::channelNameSpans.
func (c *Controller) ChannelNameSpans(text string) []irc.ChannelNameSpan {
	if c == nil || text == "" {
		return nil
	}
	networkID := c.FocusedNetworkID()
	if networkID == "" {
		return nil
	}
	return irc.ChannelNameSpans(text, c.reducer.ServerFeatures(networkID))
}

// HasConversation reports whether networkID already has a row for target.
// Comparison uses the network's case mapping. It mirrors
// IrcController::hasConversation.
func (c *Controller) HasConversation(networkID, target string) bool {
	if c == nil || networkID == "" || target == "" {
		return false
	}
	return c.reducer.Find(c.reducer.ConversationKey(networkID, target)) != nil
}
