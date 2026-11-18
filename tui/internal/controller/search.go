package controller

import (
	"container/heap"
	"context"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

// Where the history search looks: the lines every conversation holds in
// memory, and the on-disk logs of the conversations on known networks. It
// mirrors `client/src/controller/search.rs`.
//
// The memory source is read on the shell goroutine and is cheap. The disk
// source is a plan the shell hands to a worker goroutine: DiskScan owns
// everything it needs, so it runs without touching the controller.
//
// A conversation's loaded lines are the newest the log holds, so the disk scan
// adds only the lines older than the oldest one loaded. A line the transcript
// policy never stores is never found, by either source.

const (
	// searchBatchHits and searchBatchWait: hits wait this long, or until a batch
	// is this big, before they are handed on.
	searchBatchHits = 20
	searchBatchWait = 30 * time.Millisecond
	// searchOrderSlack: log lines are mostly in time order, but a prepended
	// history page or a skewed server clock can put a line a little out of
	// place. A file's scan stops only once a line is older than the cutoff by
	// more than this.
	searchOrderSlack = 60 * time.Second
)

// The shell reads the search through these names: internal/ui does not import
// internal/irc, so the shapes it draws and the rules it applies are re-exported
// here as they are in irc/search.go.
type (
	// SearchQuery is the trimmed, case-folded text of the search field.
	SearchQuery = irc.SearchQuery
	// SearchHit is one matching line.
	SearchHit = irc.SearchHit
	// SearchResults is the newest SearchCap hits and whether more matched.
	SearchResults = irc.SearchResults
	// JumpRow is one loaded transcript row, as a jump finds its line.
	JumpRow = irc.JumpRow
)

// SearchCap is the most hits one search lists.
const SearchCap = irc.SearchCap

// OlderThanLoadedNote is what a jump says when its line is older than the loaded
// transcript.
const OlderThanLoadedNote = irc.OlderThanLoadedNote

// ParseSearchQuery is the query for what was typed, or false while it is blank.
func ParseSearchQuery(raw string) (SearchQuery, bool) { return irc.ParseSearchQuery(raw) }

// SearchFooter is the line under the result list.
func SearchFooter(queryBlank bool, results SearchResults, scanning bool) string {
	return irc.SearchFooter(queryBlank, results, scanning)
}

// SearchTimeLabel is the time a row shows.
func SearchTimeLabel(when, today time.Time) string { return irc.SearchTimeLabel(when, today) }

// JumpLanding reports the row of rows the hit's line is on, and whether it is
// among them at all.
func JumpLanding(hit SearchHit, rows []JumpRow) (int, bool) {
	outcome := irc.OutcomeOfJump(hit.Target, rows)
	return outcome.Row, outcome.Landed
}

// SearchJumpRows is the shown conversation's loaded rows as a jump finds its
// line: the author, the plain body, the msgid and the time in ms.
func (c *Controller) SearchJumpRows() []JumpRow {
	if c.ConsoleOpen() {
		return nil
	}
	messages := c.Messages()
	rows := make([]JumpRow, 0, len(messages))
	for _, message := range messages {
		rows = append(rows, JumpRow{
			Author: message.Author,
			Body:   c.PlainIrcText(message.Body),
			MsgID:  message.MsgID,
			Ms:     message.Time.UnixMilli(),
		})
	}
	return rows
}

// SelectSearchHit opens the hit's conversation, by its key and never its label,
// expanding its network so the row shows.
func (c *Controller) SelectSearchHit(hit SearchHit) {
	c.SetNetworkCollapsed(hit.Conversation.NetworkID, false)
	c.SelectConversationByID(irc.ConversationID(hit.Conversation))
}

// SearchLoaded is the newest hits among the lines every conversation holds in
// memory. Muted conversations count.
func (c *Controller) SearchLoaded(query irc.SearchQuery) irc.SearchResults {
	var hits []irc.SearchHit
	for key, conversation := range c.reducer.Conversations() {
		features := c.reducer.ServerFeatures(key.NetworkID)
		channelTypes := features.ChannelTypes()
		place := c.searchConversation(key, conversation.Target)
		for _, message := range conversation.Messages {
			var kind string
			switch message.Kind {
			case irc.KindMessage:
				kind = "message"
			case irc.KindNotice:
				kind = "notice"
			case irc.KindAction:
				kind = "action"
			default:
				continue
			}
			// A loaded line was admitted when it arrived. The same gate the log
			// writer uses runs again, so the two sources never disagree about a
			// credential.
			if !irc.AllowsTranscript(message.Body, channelTypes) || !irc.AllowsTranscript(message.Author, channelTypes) {
				continue
			}
			if hit, ok := place.Hit(query, irc.SearchLine{
				Time: message.Timestamp, Author: message.Author, Kind: kind,
				Body: message.Body, MsgID: message.MsgID.Value,
			}); ok {
				hits = append(hits, hit)
			}
		}
	}
	return irc.MergeResults([][]irc.SearchHit{hits}, irc.SearchCap)
}

// SearchDiskScan is what a worker needs to search the on-disk logs, or nil when
// nothing is kept on disk (the demo, and tests without a log root) or no
// conversation is on a known network.
func (c *Controller) SearchDiskScan() *DiskScan {
	if c.transcriptRoot == "" {
		return nil
	}
	var targets []DiskTarget
	for key, conversation := range c.reducer.Conversations() {
		if !c.isKnownNetwork(key.NetworkID) {
			continue
		}
		features := c.reducer.ServerFeatures(key.NetworkID)
		targets = append(targets, DiskTarget{
			Conversation: c.searchConversation(key, conversation.Target),
			Mapping:      features.CaseMapping(),
			Loaded:       loadedSpanOf(conversation.Messages),
		})
	}
	if len(targets) == 0 {
		return nil
	}
	return &DiskScan{Root: c.transcriptRoot, Targets: targets}
}

func (c *Controller) searchConversation(key irc.ConversationKey, label string) irc.SearchConversation {
	name := c.NetworkDisplayName(key.NetworkID)
	if name == "" {
		name = key.NetworkID
	}
	return irc.SearchConversation{NetworkID: key.NetworkID, NetworkName: name, Conversation: key, Label: label}
}

type loadedLineID struct{ msgid, author, body string }

// LoadedSpan is how far back a conversation's loaded lines reach: the log
// lines the search skips because memory already answered for them.
type LoadedSpan struct {
	HasOldest bool
	OldestMs  int64
	// AtOldest is the loaded lines stamped exactly at the oldest instant.
	// Several lines can share a millisecond, so the instant alone cannot say
	// which of them are loaded.
	AtOldest map[loadedLineID]struct{}
}

func loadedSpanOf(messages []irc.ReducedMessage) LoadedSpan {
	var span LoadedSpan
	for _, message := range messages {
		if message.Timestamp.IsZero() {
			continue
		}
		if ms := message.Timestamp.UnixMilli(); !span.HasOldest || ms < span.OldestMs {
			span.HasOldest, span.OldestMs = true, ms
		}
	}
	if !span.HasOldest {
		return span
	}
	span.AtOldest = map[loadedLineID]struct{}{}
	for _, message := range messages {
		if !message.Timestamp.IsZero() && message.Timestamp.UnixMilli() == span.OldestMs {
			span.AtOldest[loadedLineID{message.MsgID.Value, message.Author, message.Body}] = struct{}{}
		}
	}
	return span
}

// Covers reports whether the conversation's memory already holds this log line.
func (s LoadedSpan) Covers(line irc.TranscriptLine) bool {
	if !s.HasOldest {
		return false
	}
	ms := line.Timestamp.UnixMilli()
	if ms != s.OldestMs {
		return ms > s.OldestMs
	}
	for id := range s.AtOldest {
		if id.msgid != "" || line.MsgID != "" {
			if id.msgid == line.MsgID {
				return true
			}
		} else if id.author == line.Author && id.body == line.Body {
			return true
		}
	}
	return false
}

// DiskTarget is one conversation's log, and what memory already holds of it.
type DiskTarget struct {
	Conversation irc.SearchConversation
	Mapping      irc.CaseMapping
	Loaded       LoadedSpan
}

// DiskScan is the logs a search reads off the shell goroutine. It owns its
// inputs.
type DiskScan struct {
	Root    string
	Targets []DiskTarget
}

// timeHeap is a min-heap of hit times: the newest searchCap+1 found so far.
type timeHeap []time.Time

func (h timeHeap) Len() int           { return len(h) }
func (h timeHeap) Less(i, j int) bool { return h[i].Before(h[j]) }
func (h timeHeap) Swap(i, j int)      { h[i], h[j] = h[j], h[i] }
func (h *timeHeap) Push(x any)        { *h = append(*h, x.(time.Time)) }
func (h *timeHeap) Pop() any {
	old := *h
	last := old[len(old)-1]
	*h = old[:len(old)-1]
	return last
}

// Run reads each conversation's log newest first and hands the hits on in
// batches: the first at once, then every searchBatchHits or searchBatchWait. It
// stops early in a file once the lines left there are older than everything
// that could still make the list, and stops altogether when ctx is cancelled.
func (d *DiskScan) Run(ctx context.Context, query irc.SearchQuery, emit func([]irc.SearchHit)) {
	log := storage.NewConversationLog(d.Root)
	// The newest SearchCap+1 hit times so far: the extra one is what tells the
	// list there is more than it can show.
	newest := &timeHeap{}
	var batch []irc.SearchHit
	sentAny := false
	lastSent := time.Now()
	for _, target := range d.Targets {
		if ctx.Err() != nil {
			return
		}
		path := log.PathFor(target.Conversation.NetworkID, target.Conversation.Conversation.NormalizedTarget, target.Mapping)
		storage.ScanLog(ctx, path, func(line irc.TranscriptLine) bool {
			if newest.Len() > irc.SearchCap && line.Timestamp.Add(searchOrderSlack).Before((*newest)[0]) {
				return false
			}
			if target.Loaded.Covers(line) {
				return true
			}
			hit, ok := target.Conversation.Hit(query, irc.SearchLine{
				Time: line.Timestamp, Author: line.Author, Kind: line.Kind, Body: line.Body, MsgID: line.MsgID,
			})
			if !ok {
				return true
			}
			heap.Push(newest, hit.Time)
			if newest.Len() > irc.SearchCap+1 {
				heap.Pop(newest)
			}
			batch = append(batch, hit)
			if !sentAny || len(batch) >= searchBatchHits || time.Since(lastSent) >= searchBatchWait {
				sentAny = true
				lastSent = time.Now()
				emit(batch)
				batch = nil
			}
			return true
		})
	}
	if len(batch) > 0 && ctx.Err() == nil {
		emit(batch)
	}
}
