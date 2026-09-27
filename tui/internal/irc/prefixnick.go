package irc

// PrefixNick recovers the source nick from a message prefix, dropping a server
// name (a dotted host with no '!user@host'). It mirrors ircPrefixNick.
func PrefixNick(message Message) string {
	if message.Prefix == nil {
		return ""
	}
	if message.Prefix.Nick != "" {
		return WireText([]byte(message.Prefix.Nick))
	}
	raw := WireText([]byte(message.Prefix.Raw))
	if containsDot(raw) {
		return ""
	}
	return raw
}
