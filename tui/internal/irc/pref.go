package irc

import "strings"

// This file is the Go port of src/irc/ircpref.{h,cpp}: the /pref catalog, its
// usage and avatar note, the query/state/list formatting, and the argument
// parser.

// PrefName identifies one /pref toggle. It mirrors IrcPrefName.
type PrefName int

const (
	// PrefDirects reopens direct messages on startup.
	PrefDirects PrefName = iota
	// PrefAvatars shows peer avatars.
	PrefAvatars
	// PrefUnread opens conversations at unread.
	PrefUnread
)

// PrefKind is the parsed shape of a /pref argument. It mirrors IrcPrefKind.
type PrefKind int

const (
	// PrefQueryAll lists every toggle.
	PrefQueryAll PrefKind = iota
	// PrefQueryOne queries one toggle.
	PrefQueryOne
	// PrefSet sets one toggle.
	PrefSet
	// PrefUsageKind is the fallback that prints the usage line. It is named
	// with a Kind suffix because Go cannot declare both a constant PrefUsage
	// (mirroring IrcPrefKind::Usage) and a function PrefUsage (mirroring
	// ircPrefUsage); the free function keeps the unsuffixed name.
	PrefUsageKind
)

// PrefSpec is one /pref catalog row. It mirrors IrcPrefSpec.
type PrefSpec struct {
	Name  PrefName
	Token string
	Label string
}

// PrefRequest is a parsed /pref argument. It mirrors IrcPrefRequest.
type PrefRequest struct {
	Kind    PrefKind
	Name    PrefName
	Enabled bool
}

// prefCatalog is the static ordered /pref table.
var prefCatalog = []PrefSpec{
	{PrefDirects, "directs", "Reopen direct messages on startup"},
	{PrefAvatars, "avatars", "Show peer avatars"},
	{PrefUnread, "unread", "Open conversations at unread"},
}

// PrefCatalog returns a copy of the ordered /pref catalog. It mirrors
// ircPrefCatalog.
func PrefCatalog() []PrefSpec {
	rows := make([]PrefSpec, len(prefCatalog))
	copy(rows, prefCatalog)
	return rows
}

// PrefFind resolves a /pref token, ASCII-case-insensitively. It mirrors
// ircPrefFind and returns nil when nothing matches.
func PrefFind(token string) *PrefSpec {
	folded := strings.ToLower(token)
	for index := range prefCatalog {
		if prefCatalog[index].Token == folded {
			return &prefCatalog[index]
		}
	}
	return nil
}

// PrefUsage returns the /pref usage line. It mirrors ircPrefUsage.
func PrefUsage() string {
	return "/pref [directs|avatars|unread] [on|off]"
}

// PrefAvatarNote returns the avatar privacy note. It mirrors ircPrefAvatarNote.
func PrefAvatarNote() string {
	return "Turn off to keep avatar hosts from seeing your IP on busy channels."
}

// FormatPrefState renders "<label>: on|off". It mirrors ircFormatPrefState.
func FormatPrefState(name PrefName, enabled bool) string {
	label := ""
	for index := range prefCatalog {
		if prefCatalog[index].Name == name {
			label = prefCatalog[index].Label
			break
		}
	}
	state := "off"
	if enabled {
		state = "on"
	}
	return label + ": " + state
}

// FormatPrefQuery renders one toggle's state, appending the avatar note for
// avatars. It mirrors ircFormatPrefQuery.
func FormatPrefQuery(name PrefName, enabled bool) string {
	text := FormatPrefState(name, enabled)
	if name == PrefAvatars {
		text += "\n" + PrefAvatarNote()
	}
	return text
}

// FormatPrefList renders all three toggles, one per line. It mirrors
// ircFormatPrefList.
func FormatPrefList(directs, avatars, unread bool) string {
	return FormatPrefState(PrefDirects, directs) + "\n" +
		FormatPrefState(PrefAvatars, avatars) + "\n" +
		FormatPrefState(PrefUnread, unread)
}

// ParsePrefArgument parses a /pref argument. It mirrors ircParsePrefArgument:
// empty lists all, one known token queries it, two tokens set it for
// on/off, and anything else falls back to usage.
func ParsePrefArgument(argument string) PrefRequest {
	request := PrefRequest{Kind: PrefUsageKind}
	parts := splitSpaceSkipEmpty(strings.TrimSpace(argument))
	if len(parts) == 0 {
		request.Kind = PrefQueryAll
		return request
	}
	if len(parts) > 2 {
		return request
	}

	spec := PrefFind(parts[0])
	if spec == nil {
		return request
	}
	request.Name = spec.Name
	if len(parts) == 1 {
		request.Kind = PrefQueryOne
		return request
	}

	switch strings.ToLower(parts[1]) {
	case "on":
		request.Kind = PrefSet
		request.Enabled = true
	case "off":
		request.Kind = PrefSet
		request.Enabled = false
	}
	return request
}
