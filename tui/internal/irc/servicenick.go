package irc

import "strings"

// IsServiceIdentity reports whether a message source is a network service
// rather than a person. It mirrors ircIsServiceIdentity: a nick ending in
// "serv" that is not a channel, or a host folding to "services" or
// "services.*". channelTypes is the advertised CHANTYPES; when empty, a
// target whose first character cannot start a nick is treated as a channel.
func IsServiceIdentity(nick, host, channelTypes string) bool {
	if nick != "" && !looksLikeChannel(nick, channelTypes) && endsWithFold(nick, "serv") {
		return true
	}
	if host == "" {
		return false
	}
	folded := strings.ToLower(host)
	return folded == "services" || strings.HasPrefix(folded, "services.")
}

// NickIsRoutable reports whether Omairc could put this nick in a PRIVMSG and
// reach a person, so that a conversation opened for it is one the user can
// reply in. It is deliberately not a nick-grammar test: some networks allow a
// leading digit, so `0day` stays routable while a bouncer pseudo-client such
// as `*status` does not. It mirrors ircNickIsRoutable.
func NickIsRoutable(nick string) bool {
	if nick == "" {
		return false
	}
	for index := 0; index < len(nick); index++ {
		mark := nick[index]
		if mark < 0x20 || strings.IndexByte(" ,*?!@.$:", mark) != -1 {
			return false
		}
	}
	return true
}

func canStartNick(mark byte) bool {
	if (mark >= 'A' && mark <= 'Z') || (mark >= 'a' && mark <= 'z') {
		return true
	}
	switch mark {
	case '-', '[', ']', '\\', '`', '_', '^', '{', '|', '}':
		return true
	}
	return false
}

func looksLikeChannel(target, channelTypes string) bool {
	if target == "" {
		return false
	}
	mark := target[0]
	if channelTypes != "" {
		return strings.IndexByte(channelTypes, mark) != -1
	}
	return !canStartNick(mark)
}

func endsWithFold(text, suffix string) bool {
	if len(text) < len(suffix) {
		return false
	}
	return strings.EqualFold(text[len(text)-len(suffix):], suffix)
}
