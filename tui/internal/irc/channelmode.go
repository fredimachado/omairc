package irc

// This file is the Go port of IrcChannelModeRequest in
// src/irc/ircchannelmode.{h,cpp}: parsing a /mode argument into a query or a
// change, validated against the server's channel types.

// ChannelModeRequest is a parsed /mode argument. Query is true for a bare
// "MODE <channel>" query; otherwise Modes and Parameters describe a change.
// It mirrors IrcChannelModeRequest.
type ChannelModeRequest struct {
	Channel    string
	Modes      string
	Parameters []string
	Query      bool
}

// ParseChannelModeRequest parses a /mode argument against features. It mirrors
// IrcChannelModeRequest::parse: it splits on single spaces, rejects control
// characters, requires the first token to be an advertised channel, and yields
// a query for a lone channel or a change otherwise. ok is false when the
// argument is empty, contains a forbidden character, or does not start with a
// channel.
func ParseChannelModeRequest(argument string, features ServerFeatures) (ChannelModeRequest, bool) {
	tokens := splitSpaceSkipEmpty(argument)
	if len(tokens) == 0 {
		return ChannelModeRequest{}, false
	}
	for _, token := range tokens {
		if containsForbidden(token) {
			return ChannelModeRequest{}, false
		}
	}

	channel := tokens[0]
	if !features.IsChannel(channel) {
		return ChannelModeRequest{}, false
	}
	if len(tokens) == 1 {
		return ChannelModeRequest{Channel: channel, Query: true}, true
	}

	parameters := make([]string, len(tokens)-2)
	copy(parameters, tokens[2:])
	return ChannelModeRequest{
		Channel:    channel,
		Modes:      tokens[1],
		Parameters: parameters,
		Query:      false,
	}, true
}
