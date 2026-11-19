package irc

import (
	"fmt"
	"reflect"
	"strings"
	"testing"
	"time"
)

func searchAt(minute int64) time.Time {
	return time.UnixMilli(1_700_000_000_000 + minute*60_000).UTC()
}

func searchRoom(network, target string) SearchConversation {
	return SearchConversation{
		NetworkID:    network,
		NetworkName:  network,
		Conversation: ConversationKey{NetworkID: network, NormalizedTarget: target},
		Label:        target,
	}
}

func searchLine(minute int64, author, body, msgid string) SearchLine {
	return SearchLine{Time: searchAt(minute), Author: author, Kind: "message", Body: body, MsgID: msgid}
}

func searchHit(t *testing.T, network, target string, minute int64, body string) SearchHit {
	t.Helper()
	query, _ := ParseSearchQuery("x")
	hit, ok := searchRoom(network, target).Hit(query, searchLine(minute, "anna", "x "+body, ""))
	if !ok {
		t.Fatalf("line should hold the query")
	}
	hit.Text = body
	return hit
}

func searchTexts(hits []SearchHit) []string {
	out := make([]string, 0, len(hits))
	for _, hit := range hits {
		out = append(out, hit.Text)
	}
	return out
}

func TestMatchesLineIgnoresCaseBothWays(t *testing.T) {
	if _, ok := MatchesLine("BUILD", "dax", "the build is green"); !ok {
		t.Fatal("upper query should match lower body")
	}
	if _, ok := MatchesLine("build", "dax", "The BUILD is green"); !ok {
		t.Fatal("lower query should match upper body")
	}
}

func TestBlankQueryMatchesNothing(t *testing.T) {
	for _, query := range []string{"", "   ", "\t"} {
		if _, ok := MatchesLine(query, "anna", "hello"); ok {
			t.Fatalf("%q matched", query)
		}
	}
}

func TestQueryIsTrimmed(t *testing.T) {
	query, ok := ParseSearchQuery("  build  ")
	if !ok || query.Text() != "build" {
		t.Fatalf("got %q %v", query.Text(), ok)
	}
}

func TestAuthorMatchDoesNotMarkText(t *testing.T) {
	found, ok := MatchesLine("anna", "Anna", "tea?")
	if !ok || !found.AuthorMatch || len(found.Matches) != 0 {
		t.Fatalf("got %+v %v", found, ok)
	}
}

func TestBodyMatchMarksEveryOccurrence(t *testing.T) {
	found, ok := MatchesLine("go", "dax", "Go go gadget")
	if !ok || found.AuthorMatch {
		t.Fatalf("got %+v %v", found, ok)
	}
	want := []ByteRange{{0, 2}, {3, 5}}
	if !reflect.DeepEqual(found.Matches, want) {
		t.Fatalf("got %v want %v", found.Matches, want)
	}
}

func TestPhraseAcrossNickAndBodyMatchesAsFindDoes(t *testing.T) {
	found, ok := MatchesLine("anna h", "anna", "hello")
	if !ok || len(found.Matches) != 0 {
		t.Fatalf("got %+v %v", found, ok)
	}
}

func TestMircCodesAreNotText(t *testing.T) {
	room := searchRoom("omarchy", "#ricing")
	query, _ := ParseSearchQuery("bold")
	hit, ok := room.Hit(query, searchLine(0, "anna", "\x02bold\x02 and \x0304,05red\x03", ""))
	if !ok || hit.Text != "bold and red" || !reflect.DeepEqual(hit.Matches, []ByteRange{{0, 4}}) {
		t.Fatalf("got %+v %v", hit, ok)
	}
	for _, raw := range []string{"\x02", "04,05"} {
		query, _ := ParseSearchQuery(raw)
		if _, ok := room.Hit(query, searchLine(0, "anna", "\x02bold\x02 \x0304,05red\x03", "")); ok {
			t.Fatalf("control bytes %q matched", raw)
		}
	}
}

func TestMatchingFoldsNonASCIICase(t *testing.T) {
	found, ok := MatchesLine("CAFÉ", "anna", "un Café au lait")
	if !ok || !reflect.DeepEqual(found.Matches, []ByteRange{{3, 8}}) {
		t.Fatalf("got %+v %v", found, ok)
	}
}

func TestRangesStayOnRuneBoundaries(t *testing.T) {
	body := "xİx"
	found, ok := MatchesLine("x", "anna", body)
	if !ok {
		t.Fatal("no match")
	}
	for _, r := range found.Matches {
		if !utf8Boundary(body, r.Start) || !utf8Boundary(body, r.End) {
			t.Fatalf("range %v is not on rune boundaries", r)
		}
	}
	if got := body[found.Matches[0].Start:found.Matches[0].End]; got != "x" {
		t.Fatalf("first range is %q", got)
	}
}

func utf8Boundary(s string, i int) bool { return i == len(s) || (s[i]&0xC0) != 0x80 }

func TestOnlyChatLinesAreSearched(t *testing.T) {
	room := searchRoom("omarchy", "#ricing")
	query, _ := ParseSearchQuery("joined")
	event := searchLine(0, "", "anna joined", "")
	event.Kind = "event"
	if _, ok := room.Hit(query, event); ok {
		t.Fatal("an event line was searched")
	}
	for _, kind := range []string{"message", "notice", "action"} {
		chat := searchLine(0, "anna", "she joined late", "")
		chat.Kind = kind
		if _, ok := room.Hit(query, chat); !ok {
			t.Fatalf("%s line not searched", kind)
		}
	}
}

func TestHitKeepsTimeToTheMillisecond(t *testing.T) {
	query, _ := ParseSearchQuery("tea")
	line := searchLine(1, "anna", "tea", "m-1")
	line.Time = line.Time.Add(123456 * time.Nanosecond)
	hit, _ := searchRoom("omarchy", "#ricing").Hit(query, line)
	if !hit.Time.Equal(searchAt(1)) {
		t.Fatalf("got %v", hit.Time)
	}
}

func TestMergeNewestFirstAcrossConversationsAndSources(t *testing.T) {
	memory := []SearchHit{searchHit(t, "omarchy", "#ricing", 5, "memory five")}
	disk := []SearchHit{
		searchHit(t, "omarchy", "anna", 9, "disk nine"),
		searchHit(t, "omarchy", "#ricing", 2, "disk two"),
	}
	merged := MergeResults([][]SearchHit{memory, disk}, SearchCap)
	want := []string{"disk nine", "memory five", "disk two"}
	if !reflect.DeepEqual(searchTexts(merged.Hits), want) || merged.More {
		t.Fatalf("got %v more=%v", searchTexts(merged.Hits), merged.More)
	}
}

func TestMergeTieBreaksByNetworkConversationThenSource(t *testing.T) {
	merged := MergeResults([][]SearchHit{{
		searchHit(t, "oftc", "#a", 1, "oftc a"),
		searchHit(t, "libera", "#z", 1, "libera z"),
		searchHit(t, "libera", "#b", 1, "libera b one"),
		searchHit(t, "libera", "#b", 1, "libera b two"),
	}}, SearchCap)
	want := []string{"libera b one", "libera b two", "libera z", "oftc a"}
	if !reflect.DeepEqual(searchTexts(merged.Hits), want) {
		t.Fatalf("got %v", searchTexts(merged.Hits))
	}
}

func searchRun(t *testing.T, count int) []SearchHit {
	hits := make([]SearchHit, 0, count)
	for minute := 0; minute < count; minute++ {
		hits = append(hits, searchHit(t, "omarchy", "#ricing", int64(minute), fmt.Sprintf("line %d", minute)))
	}
	return hits
}

func TestMergeCapKeepsTheNewestAndSaysMore(t *testing.T) {
	merged := MergeResults([][]SearchHit{searchRun(t, 250)}, SearchCap)
	if len(merged.Hits) != SearchCap || !merged.More {
		t.Fatalf("len=%d more=%v", len(merged.Hits), merged.More)
	}
	if merged.Hits[0].Text != "line 249" || merged.Hits[SearchCap-1].Text != "line 50" {
		t.Fatalf("got %q .. %q", merged.Hits[0].Text, merged.Hits[SearchCap-1].Text)
	}
}

func TestMergeExactlyTheCapSaysNothingMore(t *testing.T) {
	merged := MergeResults([][]SearchHit{searchRun(t, 200)}, SearchCap)
	if len(merged.Hits) != 200 || merged.More {
		t.Fatalf("len=%d more=%v", len(merged.Hits), merged.More)
	}
}

func TestMergeCountsALineBothSourcesFoundOnce(t *testing.T) {
	query, _ := ParseSearchQuery("tea")
	room := searchRoom("omarchy", "#ricing")
	byID := func(source string) []SearchHit {
		hit, _ := room.Hit(query, searchLine(3, "anna", "tea "+source, "id-1"))
		return []SearchHit{hit}
	}
	merged := MergeResults([][]SearchHit{byID("memory"), byID("disk")}, SearchCap)
	if len(merged.Hits) != 1 || !strings.Contains(merged.Hits[0].Text, "memory") {
		t.Fatalf("got %v", searchTexts(merged.Hits))
	}
	anchored := func(minute int64) []SearchHit {
		hit, _ := room.Hit(query, searchLine(minute, "anna", "tea", ""))
		return []SearchHit{hit}
	}
	if got := len(MergeResults([][]SearchHit{anchored(4), anchored(4)}, SearchCap).Hits); got != 1 {
		t.Fatalf("same anchor counted %d times", got)
	}
	if got := len(MergeResults([][]SearchHit{anchored(4), anchored(5)}, SearchCap).Hits); got != 2 {
		t.Fatalf("different minutes counted %d times", got)
	}
}

func TestAbsorbKeepsTheNewestAndRemembersMore(t *testing.T) {
	var results SearchResults
	first := make([]SearchHit, 0, 150)
	for n := 0; n < 150; n++ {
		first = append(first, searchHit(t, "omarchy", "#a", int64(n), fmt.Sprintf("a%d", n)))
	}
	results.Absorb(first, SearchCap)
	if results.More {
		t.Fatal("150 hits are not more than the cap")
	}
	second := make([]SearchHit, 0, 150)
	for n := 100; n < 250; n++ {
		second = append(second, searchHit(t, "omarchy", "#b", int64(n), fmt.Sprintf("b%d", n)))
	}
	results.Absorb(second, SearchCap)
	if len(results.Hits) != SearchCap || !results.More || results.Hits[0].Text != "b249" {
		t.Fatalf("len=%d more=%v first=%q", len(results.Hits), results.More, results.Hits[0].Text)
	}
	results.Absorb(nil, SearchCap)
	if !results.More {
		t.Fatal("an empty batch forgot the overflow")
	}
}

func TestJumpTargetByMsgidOrAnchor(t *testing.T) {
	query, _ := ParseSearchQuery("tea")
	room := searchRoom("omarchy", "#ricing")
	byID, _ := room.Hit(query, searchLine(1, "anna", "tea?", "abc123"))
	if byID.Target != JumpTarget(JumpMsgid("abc123")) {
		t.Fatalf("got %#v", byID.Target)
	}
	anchored, _ := room.Hit(query, searchLine(1, "anna", "\x02tea\x02?", ""))
	want := JumpAnchor{Author: "anna", Kind: "message", Body: "tea?", Ms: searchAt(1).UnixMilli()}
	if anchored.Target != JumpTarget(want) {
		t.Fatalf("got %#v", anchored.Target)
	}
}

func TestResolveJumpFindsRowsByMsgidAndByAnchor(t *testing.T) {
	rows := []JumpRow{
		{Author: "anna", Body: "tea?", Ms: searchAt(0).UnixMilli()},
		{Author: "dax", Body: "tea?", MsgID: "m-2", Ms: searchAt(5).UnixMilli()},
		{Author: "anna", Body: "tea?", Ms: searchAt(9).UnixMilli()},
	}
	if row, ok := ResolveJump(JumpMsgid("m-2"), rows); !ok || row != 1 {
		t.Fatalf("msgid: %d %v", row, ok)
	}
	anchor := JumpAnchor{Author: "anna", Kind: "message", Body: "tea?", Ms: searchAt(9).UnixMilli()}
	if row, ok := ResolveJump(anchor, rows); !ok || row != 2 {
		t.Fatalf("anchor: %d %v", row, ok)
	}
}

func TestAbsentLineIsOlderThanLoaded(t *testing.T) {
	rows := []JumpRow{{Author: "anna", Body: "morning", MsgID: "m-1", Ms: searchAt(0).UnixMilli()}}
	if outcome := OutcomeOfJump(JumpMsgid("gone"), rows); outcome.Landed {
		t.Fatal("a missing msgid landed")
	}
	old := JumpAnchor{Author: "anna", Kind: "message", Body: "an older line", Ms: searchAt(-500).UnixMilli()}
	if OutcomeOfJump(old, rows).Landed || OutcomeOfJump(old, nil).Landed {
		t.Fatal("a missing anchor landed")
	}
	if outcome := OutcomeOfJump(JumpMsgid("m-1"), rows); !outcome.Landed || outcome.Row != 0 {
		t.Fatalf("got %+v", outcome)
	}
}

func TestSearchFooterSaysWhereTheSearchStands(t *testing.T) {
	none := SearchResults{}
	two := SearchResults{Hits: []SearchHit{{}, {}}}
	cases := []struct {
		blank    bool
		results  SearchResults
		scanning bool
		want     string
	}{
		{true, none, false, "Type to search every conversation"},
		{false, none, true, "Searching…"},
		{false, none, false, "No results"},
		{false, two, false, "2 results"},
		{false, two, true, "2 results · Searching…"},
		{false, SearchResults{Hits: two.Hits, More: true}, false, "200+ results, refine your search"},
	}
	for _, c := range cases {
		if got := SearchFooter(c.blank, c.results, c.scanning); got != c.want {
			t.Errorf("got %q want %q", got, c.want)
		}
	}
}

func TestSearchTimeLabel(t *testing.T) {
	zone := time.FixedZone("test", 0)
	when := time.Date(2026, 10, 19, 9, 5, 0, 0, zone)
	if got := SearchTimeLabel(when, time.Date(2026, 10, 19, 18, 0, 0, 0, zone)); got != "09:05" {
		t.Fatalf("today: %q", got)
	}
	if got := SearchTimeLabel(when, time.Date(2026, 10, 20, 9, 0, 0, 0, zone)); got != "Oct 19 09:05" {
		t.Fatalf("other day: %q", got)
	}
}
