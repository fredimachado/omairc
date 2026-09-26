package irc

import (
	"strings"
)

// This file is the Go port of the slash-command core in
// src/irc/irccommand.{h,cpp}: the composer surface, the verb scope, the
// parsed command, the verb table, and the dispatch outcome text.

// ComposerSurface selects which composer a command was typed into. It mirrors
// IrcComposerSurface.
type ComposerSurface int

const (
	// SurfaceConversation is the channel or direct-message composer.
	SurfaceConversation ComposerSurface = iota
	// SurfaceStatus is the Status console composer.
	SurfaceStatus
)

// VerbScope is the composer surface a verb applies to. It mirrors
// IrcVerbScope.
type VerbScope int

const (
	// ScopeConversation is a conversation-only verb.
	ScopeConversation VerbScope = iota
	// ScopeStatus is a Status-only verb.
	ScopeStatus
	// ScopeEither is allowed everywhere.
	ScopeEither
)

// Verb identifies one slash command. It mirrors IrcCommand::Verb.
type Verb int

const (
	VerbEmpty Verb = iota
	VerbSay
	VerbAction
	VerbJoin
	VerbPart
	VerbNick
	VerbQuit
	VerbClear
	VerbClose
	VerbQuery
	VerbMsg
	VerbTopic
	VerbNotice
	VerbAway
	VerbBack
	VerbAutoaway
	VerbPref
	VerbStatus
	VerbAvatar
	VerbWhois
	VerbPing
	VerbTime
	VerbVersion
	VerbMode
	VerbKick
	VerbInvite
	VerbIgnore
	VerbUnignore
	VerbIgnored
	VerbMonitor
	VerbUnmonitor
	VerbMonitored
	VerbMute
	VerbUnmute
	VerbMuted
	VerbHighlight
	VerbUnhighlight
	VerbHighlights
	VerbOp
	VerbDeop
	VerbVoice
	VerbDevoice
	VerbBan
	VerbNs
	VerbCs
	VerbZnc
	VerbRaw
	VerbHelp
	VerbList
	VerbUnknown
)

// Command is one parsed composer line. It mirrors IrcCommand. Name keeps the
// leading '/' for a slash command and is empty for plain text.
type Command struct {
	Verb     Verb
	Name     string
	Argument string
}

// ParseCommand parses composer input. It mirrors IrcCommand::parse: blank is
// VerbEmpty; plain text is VerbSay; a leading "//" escapes to a literal
// message; otherwise the first token names the verb and the rest is the
// argument.
func ParseCommand(input string) Command {
	command := Command{Verb: VerbEmpty}
	trimmed := strings.TrimSpace(input)
	if trimmed == "" {
		return command
	}
	if trimmed[0] != '/' {
		command.Verb = VerbSay
		command.Argument = trimmed
		return command
	}
	if strings.HasPrefix(trimmed, "//") {
		command.Verb = VerbSay
		command.Argument = trimmed[1:]
		return command
	}

	space := strings.IndexByte(trimmed, ' ')
	if space < 0 {
		command.Name = trimmed
		command.Argument = ""
	} else {
		command.Name = trimmed[:space]
		command.Argument = strings.TrimSpace(trimmed[space+1:])
	}

	if spec := LookupVerb(command.Name[1:]); spec != nil {
		command.Verb = spec.Verb
	} else {
		command.Verb = VerbUnknown
	}
	if command.Verb == VerbBack {
		command.Argument = ""
	}
	return command
}

// IsLiveMessage reports whether the command sends channel or direct text. It
// mirrors IrcCommand::isLiveMessage.
func (c Command) IsLiveMessage() bool {
	return c.Verb == VerbSay || c.Verb == VerbAction
}

// AllowedOn reports whether the command may run on surface. It mirrors
// IrcCommand::allowedOn.
func (c Command) AllowedOn(surface ComposerSurface) bool {
	if c.Verb == VerbSay {
		return surface == SurfaceConversation
	}
	if spec := FindVerb(c.Verb); spec != nil {
		return spec.AllowedOn(surface)
	}
	return true
}

// VerbSpec is one row of the verb table. It mirrors IrcVerbSpec.
type VerbSpec struct {
	Verb           Verb
	Name           string
	Aliases        []string
	Usage          string
	Scope          VerbScope
	WrongScopeText string
}

// AllowedOn reports whether the verb may run on surface. It mirrors
// IrcVerbSpec::allowedOn.
func (s VerbSpec) AllowedOn(surface ComposerSurface) bool {
	switch s.Scope {
	case ScopeConversation:
		return surface == SurfaceConversation
	case ScopeStatus:
		return surface == SurfaceStatus
	case ScopeEither:
		return true
	}
	return false
}

// WrongScopeMessage returns the refusal text for a verb on the wrong surface.
// It mirrors IrcVerbSpec::wrongScopeMessage.
func (s VerbSpec) WrongScopeMessage() string {
	if s.WrongScopeText == "" {
		return "Select a connected conversation first"
	}
	return s.WrongScopeText
}

// verbCatalog is the static ordered table. The order is the completion
// ranking order, so rows are appended in exactly the C++ order and never
// re-sorted.
var verbCatalog = []VerbSpec{
	{VerbAction, "me", nil, "/me <text>", ScopeConversation, ""},
	{VerbJoin, "join", []string{"j"}, "/join [channel] [key][, ...]", ScopeEither, ""},
	{VerbPart, "part", []string{"leave"}, "/part [channel]", ScopeEither,
		"Part applies to channels"},
	{VerbNick, "nick", nil, "/nick <nickname>", ScopeEither, ""},
	{VerbQuit, "disconnect", []string{"quit"}, "/disconnect [reason]", ScopeEither, ""},
	{VerbClear, "clear", nil, "/clear", ScopeEither, ""},
	{VerbClose, "close", nil, "/close", ScopeConversation,
		"Close applies to direct messages"},
	{VerbQuery, "query", nil, "/query <nick> [text]", ScopeEither, ""},
	{VerbMsg, "msg", nil, "/msg <nick> <text>", ScopeEither, ""},
	{VerbTopic, "topic", nil, "/topic [text]", ScopeConversation,
		"Topic applies to channels"},
	{VerbNotice, "notice", nil, "/notice <target> <text>", ScopeEither, ""},
	{VerbAway, "away", nil, "/away [reason]", ScopeEither, ""},
	{VerbBack, "back", nil, "/back", ScopeEither, ""},
	{VerbAutoaway, "autoaway", nil,
		"/autoaway [off|on|duration [reason]|reason [text]]", ScopeEither, ""},
	{VerbPref, "pref", nil, "/pref [directs|avatars|unread] [on|off]", ScopeEither, ""},
	{VerbStatus, "status", nil, "/status [text]", ScopeEither, ""},
	{VerbAvatar, "avatar", nil, "/avatar [url|email]", ScopeEither, ""},
	{VerbWhois, "whois", nil, "/whois [nick]", ScopeEither, "Name a nick"},
	{VerbPing, "ping", nil, "/ping [nick]", ScopeEither, "Name a nick"},
	{VerbTime, "time", nil, "/time [nick]", ScopeEither, "Name a nick"},
	{VerbVersion, "version", nil, "/version [nick]", ScopeEither, "Name a nick"},
	{VerbMode, "mode", nil,
		"/mode <channel> [[+|-]modechars [parameters]]", ScopeEither, ""},
	{VerbKick, "kick", nil, "/kick [channel] <nick> [reason]", ScopeEither,
		"Kick applies to channels"},
	{VerbInvite, "invite", nil, "/invite <nick> [channel]", ScopeEither,
		"Invite applies to channels"},
	{VerbIgnore, "ignore", nil, "/ignore <nick>", ScopeEither, ""},
	{VerbUnignore, "unignore", nil, "/unignore <nick>", ScopeEither, ""},
	{VerbIgnored, "ignored", nil, "/ignored", ScopeEither, ""},
	{VerbMonitor, "monitor", nil, "/monitor <nick>", ScopeEither, ""},
	{VerbUnmonitor, "unmonitor", nil, "/unmonitor <nick>", ScopeEither, ""},
	{VerbMonitored, "monitored", nil, "/monitored", ScopeEither, ""},
	{VerbMute, "mute", nil, "/mute [target]", ScopeEither,
		"Mute applies to conversations"},
	{VerbUnmute, "unmute", nil, "/unmute [target]", ScopeEither,
		"Mute applies to conversations"},
	{VerbMuted, "muted", nil, "/muted", ScopeEither, ""},
	{VerbHighlight, "highlight", nil, "/highlight <word>", ScopeEither, ""},
	{VerbUnhighlight, "unhighlight", nil, "/unhighlight <word>", ScopeEither, ""},
	{VerbHighlights, "highlights", nil, "/highlights", ScopeEither, ""},
	{VerbOp, "op", nil, "/op <nick>", ScopeConversation, "Op applies to channels"},
	{VerbDeop, "deop", nil, "/deop <nick>", ScopeConversation, "Deop applies to channels"},
	{VerbVoice, "voice", nil, "/voice <nick>", ScopeConversation,
		"Voice applies to channels"},
	{VerbDevoice, "devoice", nil, "/devoice <nick>", ScopeConversation,
		"Devoice applies to channels"},
	{VerbBan, "ban", nil, "/ban <mask>", ScopeConversation, "Ban applies to channels"},
	{VerbNs, "ns", nil, "/ns <text>", ScopeEither, ""},
	{VerbCs, "cs", nil, "/cs <text>", ScopeEither, ""},
	{VerbZnc, "znc", nil, "/znc <text>", ScopeEither, ""},
	{VerbRaw, "raw", []string{"quote"}, "/raw <line>", ScopeEither, ""},
	{VerbHelp, "help", nil, "/help", ScopeEither, ""},
	{VerbList, "list", nil, "/list [mask]", ScopeEither, ""},
}

// VerbTable returns a copy of the full ordered verb table. It mirrors
// IrcVerbTable::all.
func VerbTable() []VerbSpec {
	rows := make([]VerbSpec, len(verbCatalog))
	copy(rows, verbCatalog)
	return rows
}

// LookupVerb resolves a verb name or alias, ASCII-case-insensitively. It
// mirrors IrcVerbTable::lookup and returns nil when nothing matches.
func LookupVerb(token string) *VerbSpec {
	folded := strings.ToLower(token)
	for index := range verbCatalog {
		row := &verbCatalog[index]
		if row.Name == folded {
			return row
		}
		for _, alias := range row.Aliases {
			if alias == folded {
				return row
			}
		}
	}
	return nil
}

// FindVerb resolves a verb enumerator to its table row. It mirrors
// IrcVerbTable::find and returns nil for Empty, Say, and Unknown.
func FindVerb(verb Verb) *VerbSpec {
	if verb == VerbEmpty || verb == VerbSay || verb == VerbUnknown {
		return nil
	}
	for index := range verbCatalog {
		if verbCatalog[index].Verb == verb {
			return &verbCatalog[index]
		}
	}
	return nil
}

// VisibleVerbsOn returns the table rows allowed on surface, in table order.
// It mirrors IrcVerbTable::visibleOn.
func VisibleVerbsOn(surface ComposerSurface) []VerbSpec {
	var rows []VerbSpec
	for _, row := range verbCatalog {
		if row.AllowedOn(surface) {
			rows = append(rows, row)
		}
	}
	return rows
}

// CommandOutcome is the result of dispatching a command. It mirrors
// IrcCommandOutcome.
type CommandOutcome int

const (
	// OutcomeSent means the command was written to the wire.
	OutcomeSent CommandOutcome = iota
	// OutcomeNotConnected means the session was not registered.
	OutcomeNotConnected
	// OutcomeRefused means the connection layer rejected the command.
	OutcomeRefused
	// OutcomeUnsupported means the verb was unknown.
	OutcomeUnsupported
	// OutcomeWrongScope means the verb is not valid on this surface.
	OutcomeWrongScope
)

// CommandOutcomeText renders the user-facing text for an outcome. It mirrors
// ircCommandOutcomeText.
func CommandOutcomeText(outcome CommandOutcome, command Command) string {
	switch outcome {
	case OutcomeSent:
		return ""
	case OutcomeNotConnected:
		return "Not connected"
	case OutcomeRefused:
		return "Command was refused"
	case OutcomeUnsupported:
		if command.Name == "" {
			return "That command is not supported"
		}
		return "Unknown command: " + command.Name
	case OutcomeWrongScope:
		if spec := FindVerb(command.Verb); spec != nil {
			return spec.WrongScopeMessage()
		}
		return "Select a connected conversation first"
	}
	return "That command is not supported"
}

// splitSpaceSkipEmpty splits text on single ASCII spaces, dropping empty
// tokens. It mirrors QString::split(' ', Qt::SkipEmptyParts); unlike
// strings.Fields it does not treat tabs or other whitespace as separators.
func splitSpaceSkipEmpty(text string) []string {
	var tokens []string
	start := 0
	for index := 0; index <= len(text); index++ {
		if index != len(text) && text[index] != ' ' {
			continue
		}
		if index > start {
			tokens = append(tokens, text[start:index])
		}
		start = index + 1
	}
	return tokens
}
