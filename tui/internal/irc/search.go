package irc

import (
	"fmt"
	"sort"
	"strings"
	"time"
	"unicode/utf8"
)

// History search (Ctrl+Shift+F): what a query matches, how hits from the loaded
// transcripts and the on-disk logs merge into one newest-first list, and where
// Enter lands. It mirrors `client/src/model/search.rs` name for name and holds
// no IO: the shell feeds it lines and rows.
//
// A line matches as Ctrl+F matches a row: the case-folded text
// "{author} {plain body}" holds the case-folded query. Only the plain text is
// searched, never the wire body, so a mIRC control code is never matched and
// never shown.

// SearchCap is the most hits one search lists. A search that matched more says
// so (SearchResults.More).
const SearchCap = 200

// OlderThanLoadedNote is what a jump says when its line is older than the
// transcript the conversation has loaded.
const OlderThanLoadedNote = "That message is older than the loaded history."

// SearchQuery is what the search field asks for: the trimmed text, case-folded
// once. A blank field is no search.
type SearchQuery struct {
	text   string
	folded string
}

// ParseSearchQuery is the query for what was typed, or false while the field is
// blank.
func ParseSearchQuery(raw string) (SearchQuery, bool) {
	text := strings.TrimSpace(raw)
	if text == "" {
		return SearchQuery{}, false
	}
	return SearchQuery{text: text, folded: strings.ToLower(text)}, true
}

// Text is the trimmed query.
func (q SearchQuery) Text() string { return q.text }

// ByteRange is a half-open byte range into a hit's text.
type ByteRange struct{ Start, End int }

// LineMatch is how a line matched. Matches are byte ranges into the plain body
// on rune boundaries; a query that matched only the author, or only across the
// nick and the body, has none.
type LineMatch struct {
	Matches     []ByteRange
	AuthorMatch bool
}

// MatchesLine reports whether the line matches, and where in its body.
func (q SearchQuery) MatchesLine(author, plainBody string) (LineMatch, bool) {
	authorFolded := strings.ToLower(author)
	bodyFolded := strings.ToLower(plainBody)
	if !strings.Contains(authorFolded+" "+bodyFolded, q.folded) {
		return LineMatch{}, false
	}
	return LineMatch{
		Matches:     bodyRanges(plainBody, q.folded),
		AuthorMatch: strings.Contains(authorFolded, q.folded),
	}, true
}

// MatchesLine is SearchQuery.MatchesLine for a raw query. An empty query
// matches nothing.
func MatchesLine(query, author, plainBody string) (LineMatch, bool) {
	parsed, ok := ParseSearchQuery(query)
	if !ok {
		return LineMatch{}, false
	}
	return parsed.MatchesLine(author, plainBody)
}

// bodyRanges is the byte ranges of needle (already folded) in body, ignoring
// case. Folding a rune can change its length, so the match runs over the folded
// runes and each range maps back to the whole original runes it covers.
func bodyRanges(body, needle string) []ByteRange {
	want := []rune(needle)
	if len(want) == 0 {
		return nil
	}
	type folded struct {
		r          rune
		start, end int
	}
	var runes []folded
	for start, r := range body {
		end := start + utf8.RuneLen(r)
		if r == utf8.RuneError {
			end = start + 1
		}
		for _, lower := range []rune(strings.ToLower(string(r))) {
			runes = append(runes, folded{lower, start, end})
		}
	}
	var ranges []ByteRange
	for at := 0; at+len(want) <= len(runes); {
		match := true
		for i, w := range want {
			if runes[at+i].r != w {
				match = false
				break
			}
		}
		if !match {
			at++
			continue
		}
		next := ByteRange{runes[at].start, runes[at+len(want)-1].end}
		if n := len(ranges); n > 0 && next.Start < ranges[n-1].End {
			if next.End > ranges[n-1].End {
				ranges[n-1].End = next.End
			}
		} else {
			ranges = append(ranges, next)
		}
		at += len(want)
	}
	return ranges
}

// JumpTarget is where Enter lands for a hit: a JumpMsgid, or a JumpAnchor.
type JumpTarget interface{ jumpTarget() }

// JumpMsgid names the line by the server's msgid. Never empty.
type JumpMsgid string

// JumpAnchor finds the line by content, the way a saved place without a msgid
// is: author, kind token, plain body and the time in ms since the epoch.
type JumpAnchor struct {
	Author string
	Kind   string
	Body   string
	Ms     int64
}

func (JumpMsgid) jumpTarget()  {}
func (JumpAnchor) jumpTarget() {}

// SearchHit is one matching line, ready to draw and to jump to.
type SearchHit struct {
	NetworkID    string
	NetworkName  string
	Conversation ConversationKey
	// ConversationLabel is what a row shows for the conversation. Never read a
	// conversation back from it.
	ConversationLabel string
	Time              time.Time
	Author            string
	// Text is the plain text of the body.
	Text        string
	Matches     []ByteRange
	AuthorMatch bool
	Target      JumpTarget
}

// SearchConversation is the conversation a line was found in. Both sources
// build their hits through it.
type SearchConversation struct {
	NetworkID    string
	NetworkName  string
	Conversation ConversationKey
	Label        string
}

// SearchLine is one stored or loaded line, as the sources hand it over.
type SearchLine struct {
	Time   time.Time
	Author string
	// Kind is the stored token: message, notice, action or event.
	Kind string
	// Body is the wire or log body, mIRC codes and all.
	Body  string
	MsgID string
}

// Hit is the hit for line, when it is a chat line the query matches. Joins,
// parts and the like are not searched.
func (c SearchConversation) Hit(query SearchQuery, line SearchLine) (SearchHit, bool) {
	switch line.Kind {
	case "message", "notice", "action":
	default:
		return SearchHit{}, false
	}
	text := PlainIrcText(line.Body)
	found, ok := query.MatchesLine(line.Author, text)
	if !ok {
		return SearchHit{}, false
	}
	// The log keeps the time to the millisecond; a loaded line may carry more.
	when := time.UnixMilli(line.Time.UnixMilli()).UTC()
	var target JumpTarget
	if line.MsgID != "" {
		target = JumpMsgid(line.MsgID)
	} else {
		target = JumpAnchor{Author: line.Author, Kind: line.Kind, Body: text, Ms: when.UnixMilli()}
	}
	return SearchHit{
		NetworkID:         c.NetworkID,
		NetworkName:       c.NetworkName,
		Conversation:      c.Conversation,
		ConversationLabel: c.Label,
		Time:              when,
		Author:            line.Author,
		Text:              text,
		Matches:           found.Matches,
		AuthorMatch:       found.AuthorMatch,
		Target:            target,
	}, true
}

// SearchResults is the list the sheet shows: the newest SearchCap hits, and
// whether more matched.
type SearchResults struct {
	Hits []SearchHit
	More bool
}

// Absorb folds a batch of hits into the list. The list stays the newest cap of
// everything seen, and More stays set once it is.
func (r *SearchResults) Absorb(batch []SearchHit, limit int) {
	merged := MergeResults([][]SearchHit{r.Hits, batch}, limit)
	r.More = r.More || merged.More
	r.Hits = merged.Hits
}

type searchLineID struct {
	key    ConversationKey
	target JumpTarget
	ms     int64
}

// MergeResults merges the hits of every source, newest first, and keeps the
// first limit. Ties on the time go to the lower network id, then conversation,
// then the source's own order. A line two sources both found counts once, the
// earlier source's.
func MergeResults(sources [][]SearchHit, limit int) SearchResults {
	type entry struct {
		source, index int
		hit           SearchHit
	}
	seen := map[searchLineID]struct{}{}
	var all []entry
	for source, hits := range sources {
		for index, hit := range hits {
			id := searchLineID{hit.Conversation, hit.Target, hit.Time.UnixMilli()}
			if _, dup := seen[id]; dup {
				continue
			}
			seen[id] = struct{}{}
			all = append(all, entry{source, index, hit})
		}
	}
	sort.SliceStable(all, func(i, j int) bool {
		a, b := all[i], all[j]
		if !a.hit.Time.Equal(b.hit.Time) {
			return a.hit.Time.After(b.hit.Time)
		}
		if a.hit.NetworkID != b.hit.NetworkID {
			return a.hit.NetworkID < b.hit.NetworkID
		}
		if a.hit.Conversation != b.hit.Conversation {
			return conversationKeyLess(a.hit.Conversation, b.hit.Conversation)
		}
		if a.source != b.source {
			return a.source < b.source
		}
		return a.index < b.index
	})
	more := len(all) > limit
	if more {
		all = all[:limit]
	}
	hits := make([]SearchHit, 0, len(all))
	for _, e := range all {
		hits = append(hits, e.hit)
	}
	return SearchResults{Hits: hits, More: more}
}

// SearchFooter is the line under the result list, from where the search stands.
func SearchFooter(queryBlank bool, results SearchResults, scanning bool) string {
	if queryBlank {
		return "Type to search every conversation"
	}
	count := len(results.Hits)
	if results.More {
		return fmt.Sprintf("%d+ results, refine your search", SearchCap)
	}
	switch {
	case count == 0 && scanning:
		return "Searching…"
	case count == 0:
		return "No results"
	case count == 1 && scanning:
		return "1 result · Searching…"
	case count == 1:
		return "1 result"
	case scanning:
		return fmt.Sprintf("%d results · Searching…", count)
	}
	return fmt.Sprintf("%d results", count)
}

// SearchTimeLabel is the time a row shows: HH:MM for a line from today (in the
// zone of `today`), else the short date in front of it.
func SearchTimeLabel(when, today time.Time) string {
	local := when.In(today.Location())
	if local.Year() == today.Year() && local.YearDay() == today.YearDay() {
		return local.Format("15:04")
	}
	return local.Format("Jan 2 15:04")
}

// JumpRow is one loaded transcript row, as a jump finds its line among them.
type JumpRow struct {
	Author string
	Body   string // plain
	MsgID  string
	Ms     int64
}

// ResolveJump is the index of the row the target names, by msgid or by content;
// false when the line is not among rows.
func ResolveJump(target JumpTarget, rows []JumpRow) (int, bool) {
	switch t := target.(type) {
	case JumpMsgid:
		for index, row := range rows {
			if row.MsgID == string(t) {
				return index, true
			}
		}
	case JumpAnchor:
		for index, row := range rows {
			if row.Author == t.Author && row.Body == t.Body && row.Ms == t.Ms {
				return index, true
			}
		}
	}
	return 0, false
}

// JumpOutcome is what opening a hit did: Landed on Row, or the line is older
// than the loaded history.
type JumpOutcome struct {
	Landed bool
	Row    int
}

// OutcomeOfJump resolves target among rows.
func OutcomeOfJump(target JumpTarget, rows []JumpRow) JumpOutcome {
	row, ok := ResolveJump(target, rows)
	return JumpOutcome{Landed: ok, Row: row}
}
