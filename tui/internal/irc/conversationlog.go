package irc

import "time"

// TranscriptLine is one persisted transcript entry. It mirrors
// IrcTranscriptLine.
type TranscriptLine struct {
	Timestamp time.Time
	Author    string
	Kind      string
	Body      string
	MsgID     string
}

// ConversationLog persists and reads back conversation transcripts, one file
// per conversation keyed by the network and case-mapped target. It mirrors the
// IrcConversationLog interface surface; the on-disk implementation lands in a
// later phase.
type ConversationLog interface {
	// Append stores one line, reporting whether it was written.
	Append(networkID, target string, mapping CaseMapping, line TranscriptLine) bool
	// ReadTail returns up to maxLines most recent lines, oldest first.
	ReadTail(networkID, target string, mapping CaseMapping, maxLines int) []TranscriptLine
}
