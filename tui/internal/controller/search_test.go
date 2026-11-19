package controller

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

// The history search sources: the loaded lines, and the worker that reads the
// logs. The Rust twin is `client/src/controller/tests/search.rs`.

func searchStamp(second int64) time.Time {
	return time.UnixMilli(1_700_000_000_000 + second*1000).UTC()
}

func stored(second int64, author, body, msgid string) irc.TranscriptLine {
	return irc.TranscriptLine{Timestamp: searchStamp(second), Author: author, Kind: "message", Body: body, MsgID: msgid}
}

func diskTarget(network, name string, loaded LoadedSpan) DiskTarget {
	return DiskTarget{
		Conversation: irc.SearchConversation{
			NetworkID: network, NetworkName: network,
			Conversation: irc.ConversationKey{NetworkID: network, NormalizedTarget: name}, Label: name,
		},
		Loaded: loaded,
	}
}

func writeLog(t *testing.T, root, network, name string, lines ...irc.TranscriptLine) {
	t.Helper()
	log := storage.NewConversationLog(root)
	for _, line := range lines {
		if !log.Append(network, name, irc.CaseMapping{}, line) {
			t.Fatalf("append failed")
		}
	}
}

func runScan(t *testing.T, scan *DiskScan, text string) ([]irc.SearchHit, int) {
	t.Helper()
	query, _ := irc.ParseSearchQuery(text)
	var hits []irc.SearchHit
	batches := 0
	scan.Run(context.Background(), query, func(batch []irc.SearchHit) {
		batches++
		hits = append(hits, batch...)
	})
	return hits, batches
}

func hitTexts(hits []irc.SearchHit) []string {
	out := make([]string, 0, len(hits))
	for _, hit := range hits {
		out = append(out, hit.Text)
	}
	return out
}

func TestDiskScanFindsLinesInEveryConversationAndNamesThem(t *testing.T) {
	root := t.TempDir()
	writeLog(t, root, "omarchy", "#ricing", stored(1, "anna", "tea at five", "m1"))
	writeLog(t, root, "omarchy", "dax", stored(2, "dax", "more TEA?", ""))
	scan := &DiskScan{Root: root, Targets: []DiskTarget{
		diskTarget("omarchy", "#ricing", LoadedSpan{}), diskTarget("omarchy", "dax", LoadedSpan{}),
	}}
	hits, _ := runScan(t, scan, "tea")
	merged := irc.MergeResults([][]irc.SearchHit{hits}, irc.SearchCap)
	if got := strings.Join(hitTexts(merged.Hits), "|"); got != "more TEA?|tea at five" {
		t.Fatalf("got %q", got)
	}
	if merged.Hits[0].Conversation.NormalizedTarget != "dax" || merged.Hits[1].Target != irc.JumpTarget(irc.JumpMsgid("m1")) {
		t.Fatalf("got %+v", merged.Hits)
	}
}

func TestDiskScanSkipsWhatMemoryHoldsAndFindsOlderLines(t *testing.T) {
	root := t.TempDir()
	writeLog(t, root, "omarchy", "#ricing",
		stored(1, "anna", "old tea", ""), stored(5, "anna", "tea five", "m5"), stored(6, "anna", "tea six", "m6"))
	loaded := LoadedSpan{HasOldest: true, OldestMs: searchStamp(5).UnixMilli(),
		AtOldest: map[loadedLineID]struct{}{{"m5", "anna", "tea five"}: {}}}
	scan := &DiskScan{Root: root, Targets: []DiskTarget{diskTarget("omarchy", "#ricing", loaded)}}
	hits, _ := runScan(t, scan, "tea")
	if got := strings.Join(hitTexts(hits), "|"); got != "old tea" {
		t.Fatalf("got %q", got)
	}
}

func TestDiskScanTellsLinesSharingTheOldestMillisecondApartByContent(t *testing.T) {
	root := t.TempDir()
	writeLog(t, root, "omarchy", "#ricing", stored(5, "anna", "tea not loaded", "m4"), stored(5, "anna", "tea loaded", "m5"))
	loaded := LoadedSpan{HasOldest: true, OldestMs: searchStamp(5).UnixMilli(),
		AtOldest: map[loadedLineID]struct{}{{"m5", "anna", "tea loaded"}: {}}}
	scan := &DiskScan{Root: root, Targets: []DiskTarget{diskTarget("omarchy", "#ricing", loaded)}}
	hits, _ := runScan(t, scan, "tea")
	if got := strings.Join(hitTexts(hits), "|"); got != "tea not loaded" {
		t.Fatalf("got %q", got)
	}
}

func TestDiskScanNeverFindsACredential(t *testing.T) {
	root := t.TempDir()
	path := storage.NewConversationLog(root).PathFor("omarchy", "#ricing", irc.CaseMapping{})
	if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
		t.Fatal(err)
	}
	record := func(second int64, author, body string) string {
		raw, _ := json.Marshal(map[string]string{
			"timestamp": searchStamp(second).Format("2006-01-02T15:04:05.000Z"),
			"author":    author, "kind": "message", "body": body,
		})
		return string(raw)
	}
	text := strings.Join([]string{
		record(1, "fred", "hunter2 is not it"),
		record(2, "fred", "IDENTIFY hunter2"),
		record(3, "NickServ", "PRIVMSG NickServ :IDENTIFY hunter2"),
		record(4, "anna", "a hunter2 joke"),
	}, "\n") + "\n"
	if err := os.WriteFile(path, []byte(text), 0o600); err != nil {
		t.Fatal(err)
	}
	scan := &DiskScan{Root: root, Targets: []DiskTarget{diskTarget("omarchy", "#ricing", LoadedSpan{})}}
	hits, _ := runScan(t, scan, "hunter2")
	texts := hitTexts(hits)
	for _, body := range texts {
		if strings.Contains(body, "IDENTIFY") {
			t.Fatalf("a credential line was found: %v", texts)
		}
	}
	if len(texts) != 2 {
		t.Fatalf("got %v", texts)
	}
}

func TestDiskScanKeepsTheNewestTwoHundredAndSaysMore(t *testing.T) {
	root := t.TempDir()
	var lines []irc.TranscriptLine
	for n := int64(0); n < 250; n++ {
		lines = append(lines, stored(n, "anna", fmt.Sprintf("tea %d", n), ""))
	}
	writeLog(t, root, "omarchy", "#ricing", lines...)
	scan := &DiskScan{Root: root, Targets: []DiskTarget{diskTarget("omarchy", "#ricing", LoadedSpan{})}}
	hits, _ := runScan(t, scan, "tea")
	merged := irc.MergeResults([][]irc.SearchHit{hits}, irc.SearchCap)
	if len(merged.Hits) != irc.SearchCap || !merged.More || merged.Hits[0].Text != "tea 249" || merged.Hits[irc.SearchCap-1].Text != "tea 50" {
		t.Fatalf("len=%d more=%v first=%q last=%q", len(merged.Hits), merged.More, merged.Hits[0].Text, merged.Hits[len(merged.Hits)-1].Text)
	}
}

func TestDiskScanStopsAFileOnceOlderLinesCannotMakeTheList(t *testing.T) {
	root := t.TempDir()
	var recent, old []irc.TranscriptLine
	for n := int64(0); n < 300; n++ {
		recent = append(recent, stored(100_000+n, "anna", fmt.Sprintf("tea recent %d", n), ""))
	}
	for n := int64(0); n < 2000; n++ {
		old = append(old, stored(n, "anna", fmt.Sprintf("tea old %d", n), ""))
	}
	writeLog(t, root, "omarchy", "#recent", recent...)
	writeLog(t, root, "omarchy", "#old", old...)
	scan := &DiskScan{Root: root, Targets: []DiskTarget{
		diskTarget("omarchy", "#recent", LoadedSpan{}), diskTarget("omarchy", "#old", LoadedSpan{}),
	}}
	hits, _ := runScan(t, scan, "tea")
	for _, hit := range hits {
		if !strings.HasPrefix(hit.Text, "tea recent") {
			t.Fatalf("the old backlog was read past the point it could matter: %q", hit.Text)
		}
	}
	merged := irc.MergeResults([][]irc.SearchHit{hits}, irc.SearchCap)
	if !merged.More || merged.Hits[0].Text != "tea recent 299" {
		t.Fatalf("more=%v first=%q", merged.More, merged.Hits[0].Text)
	}
}

func TestDiskScanSendsTheFirstHitAtOnceAndTheRestInBatches(t *testing.T) {
	root := t.TempDir()
	var lines []irc.TranscriptLine
	for n := int64(0); n < 100; n++ {
		lines = append(lines, stored(n, "anna", fmt.Sprintf("tea %d", n), ""))
	}
	writeLog(t, root, "omarchy", "#ricing", lines...)
	scan := &DiskScan{Root: root, Targets: []DiskTarget{diskTarget("omarchy", "#ricing", LoadedSpan{})}}
	hits, batches := runScan(t, scan, "tea")
	if len(hits) != 100 || batches < 2 || batches >= 100 {
		t.Fatalf("hits=%d batches=%d", len(hits), batches)
	}
}

func TestDiskScanCancelSendsNothingMore(t *testing.T) {
	root := t.TempDir()
	var lines []irc.TranscriptLine
	for n := int64(0); n < 100; n++ {
		lines = append(lines, stored(n, "anna", fmt.Sprintf("tea %d", n), ""))
	}
	writeLog(t, root, "omarchy", "#ricing", lines...)
	scan := &DiskScan{Root: root, Targets: []DiskTarget{diskTarget("omarchy", "#ricing", LoadedSpan{})}}
	ctx, cancel := context.WithCancel(context.Background())
	query, _ := irc.ParseSearchQuery("tea")
	sent := 0
	scan.Run(ctx, query, func(batch []irc.SearchHit) {
		sent += len(batch)
		cancel()
	})
	if sent != 1 {
		t.Fatalf("sent=%d, want only the first hit", sent)
	}
}

// searchLoadedSetup registers a network with two joined channels and a direct
// message holding one line each, all with "zebra".
func searchLoadedSetup(t *testing.T) *Controller {
	t.Helper()
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("freenode", "omairc"))
	registerNetwork(t, transport, "omairc", "message-tags")
	inject(t, transport, ":omairc!u@h JOIN :#chan\r\n:omairc!u@h JOIN :#lab\r\n")
	inject(t, transport, "@msgid=a1;time=2026-09-26T08:00:00.000Z :alice!u@h PRIVMSG #lab :a zebra in the lab\r\n"+
		"@msgid=a2;time=2026-09-26T08:05:00.000Z :bob!u@h PRIVMSG #chan :\x02Zebra\x02 crossing\r\n"+
		"@msgid=a3;time=2026-09-26T08:10:00.000Z :carol!u@h PRIVMSG omairc :private zebra\r\n")
	return c
}

func TestSearchLoadedFindsLinesNewestFirstWithTheirConversation(t *testing.T) {
	c := searchLoadedSetup(t)
	query, _ := irc.ParseSearchQuery("zebra")
	found := c.SearchLoaded(query)
	if len(found.Hits) != 3 || found.More {
		t.Fatalf("got %v more=%v", hitTexts(found.Hits), found.More)
	}
	if found.Hits[0].Text != "private zebra" || found.Hits[1].Text != "Zebra crossing" || found.Hits[2].Text != "a zebra in the lab" {
		t.Fatalf("order: %v", hitTexts(found.Hits))
	}
	if found.Hits[1].Conversation.NormalizedTarget != "#chan" || found.Hits[1].Target != irc.JumpTarget(irc.JumpMsgid("a2")) {
		t.Fatalf("got %+v", found.Hits[1])
	}
	byAuthor := c.SearchLoaded(mustQuery("ALICE"))
	if len(byAuthor.Hits) != 1 || !byAuthor.Hits[0].AuthorMatch {
		t.Fatalf("author match: %+v", byAuthor.Hits)
	}
}

func mustQuery(text string) irc.SearchQuery {
	query, _ := irc.ParseSearchQuery(text)
	return query
}

func TestSearchDiskScanNeedsALogRootAndPlansEachConversation(t *testing.T) {
	c := searchLoadedSetup(t)
	if c.SearchDiskScan() != nil {
		t.Fatal("a controller with no log root plans no disk scan")
	}
	root := t.TempDir()
	c.SetConversationLog(storage.NewConversationLog(root))
	scan := c.SearchDiskScan()
	if scan == nil || scan.Root != root || len(scan.Targets) < 3 {
		t.Fatalf("got %+v", scan)
	}
	// The loaded lines define what the log scan skips.
	for _, target := range scan.Targets {
		if target.Conversation.Conversation.NormalizedTarget == "#lab" && !target.Loaded.HasOldest {
			t.Fatal("#lab has loaded lines")
		}
	}
}

// TestSearchScanLarge is a timing run, not a gate:
//
//	OMAIRC_SEARCH_LARGE=1 go test ./internal/controller -run SearchScanLarge -v
//
// It writes 200k lines (one big log and fifty small ones) and times the first
// hit, a full scan and a cancel, as the Rust `search_scan_large` does.
func TestSearchScanLarge(t *testing.T) {
	if os.Getenv("OMAIRC_SEARCH_LARGE") == "" {
		t.Skip("set OMAIRC_SEARCH_LARGE=1 for the timing run")
	}
	root := t.TempDir()
	var targets []DiskTarget
	write := func(name string, count int, base int64) {
		path := storage.NewConversationLog(root).PathFor("omarchy", name, irc.CaseMapping{})
		if err := os.MkdirAll(filepath.Dir(path), 0o700); err != nil {
			t.Fatal(err)
		}
		var out strings.Builder
		for n := 0; n < count; n++ {
			marker := ""
			if n%4000 == 0 {
				marker = " needle"
			}
			raw, _ := json.Marshal(map[string]string{
				"timestamp": searchStamp(base + int64(n)).Format("2006-01-02T15:04:05.000Z"),
				"author":    fmt.Sprintf("nick%d", n%17), "kind": "message",
				"body":  fmt.Sprintf("line %d of %s: the quick brown fox jumps over the lazy dog%s", n, name, marker),
				"msgid": fmt.Sprintf("%s-%d", name, n),
			})
			out.Write(raw)
			out.WriteByte('\n')
		}
		if err := os.WriteFile(path, []byte(out.String()), 0o600); err != nil {
			t.Fatal(err)
		}
		targets = append(targets, diskTarget("omarchy", name, LoadedSpan{}))
	}
	write("#big", 150_000, 0)
	for room := 0; room < 50; room++ {
		write(fmt.Sprintf("#small%d", room), 1_000, 1_000_000)
	}
	scan := &DiskScan{Root: root, Targets: targets}

	started := time.Now()
	var first time.Duration
	scan.Run(context.Background(), mustQuery("quick brown"), func([]irc.SearchHit) {
		if first == 0 {
			first = time.Since(started)
		}
	})
	t.Logf("first hit: %v (run with an early exit: %v)", first, time.Since(started))

	started = time.Now()
	hits := 0
	scan.Run(context.Background(), mustQuery("needle"), func(batch []irc.SearchHit) { hits += len(batch) })
	t.Logf("full scan: %v for 200000 lines, %d hits", time.Since(started), hits)

	ctx, cancel := context.WithCancel(context.Background())
	var asked time.Time
	go func() {
		time.Sleep(20 * time.Millisecond)
		asked = time.Now()
		cancel()
	}()
	scan.Run(ctx, mustQuery("no such text anywhere"), func([]irc.SearchHit) {})
	t.Logf("cancel latency: %v", time.Since(asked))
}
