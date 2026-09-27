package controller

import (
	"strings"

	"github.com/fredimachado/omairc/tui/internal/irc"
)

// SlashHit is one slash-completion row projected for the shell. It mirrors the
// label/usage pair of IrcSlashHit, plus Exact: whether the current needle names
// this verb (or one of its aliases) so Enter sends instead of inserting.
type SlashHit struct {
	Label string
	Usage string
	Exact bool
}

// SlashProbe is the slash-completion state for one composer value. It mirrors
// IrcSlashProbe.
type SlashProbe struct {
	Open   bool
	Needle string
	Hits   []SlashHit
}

// SlashProject resolves the completion list for composerText. statusSurface
// selects the Status verb scope. It is the controller bridge to the pure
// irc.ProjectSlash projection: the shell cannot import internal/irc, so it asks
// here.
func (c *Controller) SlashProject(composerText string, statusSurface bool) SlashProbe {
	surface := irc.SurfaceConversation
	if statusSurface {
		surface = irc.SurfaceStatus
	}
	probe := irc.ProjectSlash(composerText, surface)
	hits := make([]SlashHit, 0, len(probe.Hits))
	for _, hit := range probe.Hits {
		hits = append(hits, SlashHit{
			Label: hit.Label,
			Usage: hit.Usage,
			Exact: slashHitIsExact(probe.Needle, hit.Label),
		})
	}
	return SlashProbe{Open: probe.Open, Needle: probe.Needle, Hits: hits}
}

// slashHitIsExact mirrors IrcSlashSession::tokenPassesSelected: the needle
// names the hit's verb, or is one of the verb's aliases.
func slashHitIsExact(needle, label string) bool {
	name := strings.TrimPrefix(label, "/")
	if needle == name {
		return true
	}
	spec := irc.LookupVerb(name)
	if spec == nil {
		return false
	}
	for _, alias := range spec.Aliases {
		if needle == alias {
			return true
		}
	}
	return false
}
