package irc

import (
	"sort"
	"strings"
	"unicode"
	"unicode/utf16"
)

// This file is the Go port of IrcSlashComplete and IrcSlashProbe in
// src/irc/ircslashcomplete.{h,cpp}: projecting composer text into the slash
// completion popup, and the /pref value completion shapes.

// SlashHit is one completion row. It mirrors IrcSlashHit.
type SlashHit struct {
	Label string
	Usage string
}

// SlashProbe is the projected completion state. Open is false when no popup
// should show. It mirrors IrcSlashProbe.
type SlashProbe struct {
	Open   bool
	Needle string
	Hits   []SlashHit
}

// HasLabel reports whether a hit has exactly this label. It mirrors
// IrcSlashProbe::containsLabel.
func (p SlashProbe) HasLabel(label string) bool {
	for _, hit := range p.Hits {
		if hit.Label == label {
			return true
		}
	}
	return false
}

// closedSlashProbe returns the closed probe. It mirrors IrcSlashProbe::closed.
func closedSlashProbe() SlashProbe {
	return SlashProbe{}
}

// openSlashProbe returns an open probe, or closed when there is no needle or
// no hits. It mirrors IrcSlashProbe::open.
func openSlashProbe(needle string, hits []SlashHit) SlashProbe {
	if len(hits) == 0 || needle == "" {
		return closedSlashProbe()
	}
	return SlashProbe{Open: true, Needle: needle, Hits: hits}
}

// ProjectSlash projects composer text into a completion probe. It mirrors
// IrcSlashComplete::project.
func ProjectSlash(text string, surface ComposerSurface) SlashProbe {
	needle := openableNeedle(text)
	if needle != "" {
		return openSlashProbe(needle, rankSlash(needle, surface))
	}
	return projectPref(text)
}

// openableNeedle extracts the verb needle from a bare "/verb" prefix, or ""
// when the text is not a single unspaced slash token. It mirrors
// IrcSlashComplete::openableNeedle.
func openableNeedle(composerText string) string {
	runes := []rune(composerText)
	start := 0
	for start < len(runes) && unicode.IsSpace(runes[start]) {
		start++
	}
	if start >= len(runes) {
		return ""
	}
	if runes[start] != '/' {
		return ""
	}
	afterSlash := start + 1
	if afterSlash >= len(runes) {
		return ""
	}
	if runes[afterSlash] == '/' || unicode.IsSpace(runes[afterSlash]) {
		return ""
	}
	for index := afterSlash; index < len(runes); index++ {
		if unicode.IsSpace(runes[index]) {
			return ""
		}
	}
	return strings.ToLower(string(runes[afterSlash:]))
}

// rankSlash scores the visible verbs against foldedNeedle and returns the top
// hits, highest score first and catalog order breaking ties. It mirrors
// IrcSlashComplete::rank (including the six-hit cap).
func rankSlash(foldedNeedle string, surface ComposerSurface) []SlashHit {
	visible := VisibleVerbsOn(surface)
	type scored struct {
		catalogIndex int
		score        int
		hit          SlashHit
	}
	var rows []scored
	for index := range visible {
		spec := visible[index]
		score := scoreSpec(foldedNeedle, spec)
		if score < 0 {
			continue
		}
		rows = append(rows, scored{
			catalogIndex: index,
			score:        score,
			hit:          SlashHit{Label: "/" + spec.Name, Usage: spec.Usage},
		})
	}
	sort.SliceStable(rows, func(left, right int) bool {
		if rows[left].score != rows[right].score {
			return rows[left].score > rows[right].score
		}
		return rows[left].catalogIndex < rows[right].catalogIndex
	})
	keep := len(rows)
	if keep > 6 {
		keep = 6
	}
	hits := make([]SlashHit, 0, keep)
	for index := 0; index < keep; index++ {
		hits = append(hits, rows[index].hit)
	}
	return hits
}

// projectPref handles the /pref completion shapes: naming a toggle, then
// naming its on/off value. It mirrors IrcSlashComplete::projectPref.
func projectPref(composerText string) SlashProbe {
	runes := []rune(composerText)
	start := 0
	for start < len(runes) && unicode.IsSpace(runes[start]) {
		start++
	}
	if start >= len(runes) || runes[start] != '/' {
		return closedSlashProbe()
	}

	body := string(runes[start+1:])
	verbSpace := strings.IndexByte(body, ' ')
	if verbSpace < 0 {
		return closedSlashProbe()
	}
	if strings.ToLower(body[:verbSpace]) != "pref" {
		return closedSlashProbe()
	}

	rest := body[verbSpace+1:]
	if strings.HasPrefix(rest, " ") {
		return closedSlashProbe()
	}

	needle := strings.ToLower(body)
	nameSpace := strings.IndexByte(rest, ' ')
	if nameSpace < 0 {
		token := strings.ToLower(rest)
		var hits []SlashHit
		if token == "" {
			for _, spec := range prefCatalog {
				hits = append(hits, SlashHit{
					Label: "/pref " + spec.Token,
					Usage: spec.Label,
				})
			}
		} else {
			type scored struct {
				index int
				score int
				hit   SlashHit
			}
			var rows []scored
			for index := range prefCatalog {
				spec := prefCatalog[index]
				score := scoreToken(token, spec.Token)
				if score < 0 {
					continue
				}
				rows = append(rows, scored{
					index: index,
					score: score,
					hit:   SlashHit{Label: "/pref " + spec.Token, Usage: spec.Label},
				})
			}
			sort.SliceStable(rows, func(left, right int) bool {
				if rows[left].score != rows[right].score {
					return rows[left].score > rows[right].score
				}
				return rows[left].index < rows[right].index
			})
			for _, row := range rows {
				hits = append(hits, row.hit)
			}
		}
		return openSlashProbe(needle, hits)
	}

	name := rest[:nameSpace]
	value := rest[nameSpace+1:]
	if strings.Contains(value, " ") {
		return closedSlashProbe()
	}
	spec := PrefFind(name)
	if spec == nil {
		return closedSlashProbe()
	}

	prefix := "/pref " + spec.Token + " "
	values := []string{"on", "off"}
	foldedValue := strings.ToLower(value)
	var hits []SlashHit
	if foldedValue == "" {
		for _, word := range values {
			label := prefix + word
			hits = append(hits, SlashHit{Label: label, Usage: label})
		}
	} else {
		type scored struct {
			index int
			score int
			hit   SlashHit
		}
		var rows []scored
		for index := range values {
			score := scoreToken(foldedValue, values[index])
			if score < 0 {
				continue
			}
			label := prefix + values[index]
			rows = append(rows, scored{
				index: index,
				score: score,
				hit:   SlashHit{Label: label, Usage: label},
			})
		}
		sort.SliceStable(rows, func(left, right int) bool {
			if rows[left].score != rows[right].score {
				return rows[left].score > rows[right].score
			}
			return rows[left].index < rows[right].index
		})
		for _, row := range rows {
			hits = append(hits, row.hit)
		}
	}
	return openSlashProbe(needle, hits)
}

// scoreSpec scores one verb against the needle: the best of its name and an
// exact alias match. It mirrors IrcSlashComplete::scoreSpec.
func scoreSpec(foldedNeedle string, spec VerbSpec) int {
	best := scoreToken(foldedNeedle, spec.Name)
	for _, alias := range spec.Aliases {
		if foldedNeedle == alias && best < 100 {
			best = 100
		}
	}
	return best
}

// scoreToken ranks a needle against one token. It mirrors
// IrcSlashComplete::scoreToken. -1 means no match.
func scoreToken(foldedNeedle, token string) int {
	if foldedNeedle == "" || token == "" {
		return -1
	}
	needleUnits := utf16.Encode([]rune(foldedNeedle))
	tokenUnits := utf16.Encode([]rune(token))
	if needleUnits[0] != tokenUnits[0] {
		return -1
	}
	if foldedNeedle == token {
		return 100
	}
	if strings.HasPrefix(token, foldedNeedle) {
		return 80
	}
	if strings.Contains(token, foldedNeedle) {
		return 50
	}
	ti := 0
	for _, unit := range needleUnits {
		found := -1
		for index := ti; index < len(tokenUnits); index++ {
			if tokenUnits[index] == unit {
				found = index
				break
			}
		}
		if found < 0 {
			return -1
		}
		ti = found + 1
	}
	return 20 + (len(needleUnits)*10)/ti
}
