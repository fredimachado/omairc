package controller

import "github.com/fredimachado/omairc/tui/internal/irc"

// This file holds the selection-relative presence/typing reads the transcript
// chrome needs. They live here rather than on controller.go so the controller
// core stays about state transitions, matching the Qt split where the window
// reads nickIsTyping/typingNicks off IrcController.

// NickIsTyping reports whether nick is one of the selected conversation's
// typing peers, compared under the selected network's case mapping. It mirrors
// IrcController::nickIsTyping (src/irc/irccontroller.cpp:820).
func (c *Controller) NickIsTyping(nick string) bool {
	if c.selected == nil || nick == "" {
		return false
	}
	features := c.reducer.ServerFeatures(c.selected.NetworkID)
	mapping := features.CaseMapping()
	for _, typing := range c.TypingNicks() {
		if mapping.Equals(typing, nick) {
			return true
		}
	}
	return false
}

// TranscriptTypingIndicator returns the transcript footer's typing hint: the
// first typing nick, whether the footer groups under the previous live chat
// row, and whether the footer shows at all. It mirrors the typingRow footer in
// src/OmaircWindow.qml:2948-2986 plus typingFollowsPeerChat
// (src/OmaircWindow.qml:414-420).
func (c *Controller) TranscriptTypingIndicator() (nick string, grouped bool, show bool) {
	if c.selected == nil {
		return "", false, false
	}
	if nicks := c.TypingNicks(); len(nicks) > 0 {
		nick = nicks[0]
	}
	show = c.HasTyping() && !c.IsChannel() && !c.ConsoleOpen() && nick != ""
	grouped = c.typingFollowsPeerChat(nick)
	return nick, grouped, show
}

// typingFollowsPeerChat decides whether the typing footer groups under the
// previous transcript row. The footer has no timestamp of its own, so it is
// treated as the next live chat row from the peer arriving now, using the same
// local HH:mm MessageListModel::displayTime would assign that row. It mirrors
// typingFollowsPeerChat and continuesMessageGroup (src/OmaircWindow.qml:382-421).
func (c *Controller) typingFollowsPeerChat(nick string) bool {
	if c.selected == nil || nick == "" {
		return false
	}
	conversation := c.reducer.Find(*c.selected)
	if conversation == nil {
		return false
	}
	messages := conversation.Messages
	if len(messages) == 0 {
		return false
	}
	// The footer would be row len(messages), so the previous row is the last
	// message the reducer holds.
	last := messages[len(messages)-1]
	if last.Kind != irc.KindMessage || last.Origin != irc.OriginLive {
		return false
	}
	features := c.reducer.ServerFeatures(c.selected.NetworkID)
	if !features.CaseMapping().Equals(last.Author, nick) {
		return false
	}
	return last.Timestamp.Format("15:04") == c.now().Format("15:04")
}
