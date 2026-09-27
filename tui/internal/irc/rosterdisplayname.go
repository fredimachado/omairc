package irc

import "strings"

// RosterDisplayName composes the network roster label the way
// IrcConnection::rosterDisplayName does (src/irc/ircconnection.cpp:1901-1918):
// the resolved label stands alone unless another network resolves to the same
// label case-insensitively, in which case a non-empty nick disambiguates it.
//
// label is the already-resolved (trimmed) name; callers pass
// IrcNetworkProfile::resolvedName equivalent. otherLabels holds every other
// registered network's resolved label; the label's own network must be
// excluded by the caller, exactly as the C++ loop skips the profile's own
// networkId.
//
// The middle dot is U+00B7, the same separator the Qt roster uses.
func RosterDisplayName(label, nick string, otherLabels []string) string {
	if label == "" {
		return "New network"
	}
	clash := false
	for _, other := range otherLabels {
		if strings.EqualFold(other, label) {
			clash = true
			break
		}
	}
	trimmedNick := strings.TrimSpace(nick)
	if clash && trimmedNick != "" {
		return label + " · " + trimmedNick
	}
	return label
}
