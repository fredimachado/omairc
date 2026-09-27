package controller

import (
	"slices"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
	"github.com/fredimachado/omairc/tui/internal/storage"
)

// This file ports the Phase 11 PlaybackCoordinator matrix from
// src/irc/ircplaybackcoordinator.cpp and the controller wiring it.

func recordZnc(sent *[]string) func(*session.Session, string, string) bool {
	return func(_ *session.Session, target, from string) bool {
		*sent = append(*sent, target+" "+from)
		return true
	}
}

func stringPtr(value string) *string { return &value }

func newPlaybackFixture() (*PlaybackCoordinator, *storage.PlaybackTimeStore, *irc.EventReducer) {
	times := storage.NewPlaybackTimeStore()
	times.SetEphemeral(true)
	reducer := irc.NewEventReducer()
	return NewPlaybackCoordinator(times, reducer), times, reducer
}

func TestPlaybackOnRegisteredSnapshot(t *testing.T) {
	p, times, reducer := newPlaybackFixture()
	features := reducer.ServerFeatures("libera")
	when := time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)
	if !times.Note("libera", "#chan", when, features.CaseMapping()) {
		t.Fatalf("Note = false")
	}

	p.OnRegistered("libera", []string{"#auto"})
	rows := p.playbackSnapshot["libera"]
	if len(rows) != 1 || rows[0].Target != "#chan" || !rows[0].When.Equal(when) {
		t.Fatalf("snapshot = %+v, want [#chan @ %v]", rows, when)
	}
	if got := p.zncAutojoin["libera"]; len(got) != 1 || got[0] != "#auto" {
		t.Fatalf("autojoin = %v, want [#auto]", got)
	}

	p.OnLeftRegistration("libera")
	if _, ok := p.playbackSnapshot["libera"]; ok {
		t.Fatalf("OnLeftRegistration kept the snapshot")
	}
}

func TestPlaybackNoteClockNoticesNewerServerTime(t *testing.T) {
	p, times, reducer := newPlaybackFixture()
	features := reducer.ServerFeatures("libera")
	mapping := features.CaseMapping()
	when := time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)

	message := irc.Message{
		Command: "PRIVMSG",
		Prefix:  &irc.Prefix{Nick: "alice", User: "u", Host: "h"},
		Params:  []string{"omairc", "hi"},
		Tags:    []irc.Tag{{Name: "time", Value: stringPtr(when.Format(time.RFC3339Nano))}},
	}
	p.NotePlaybackClock("libera", message, "omairc", false)
	if got, ok := times.Noted("libera", "alice", mapping); !ok || !got.Equal(when) {
		t.Fatalf("Noted = %v (ok=%v), want %v", got, ok, when)
	}

	// An older line must not move the exclusive PLAY bound.
	older := when.Add(-time.Hour)
	message.Tags = []irc.Tag{{Name: "time", Value: stringPtr(older.Format(time.RFC3339Nano))}}
	p.NotePlaybackClock("libera", message, "omairc", false)
	if got, _ := times.Noted("libera", "alice", mapping); !got.Equal(when) {
		t.Fatalf("older line moved the clock to %v", got)
	}
}

func TestPlaybackNoteClockIgnoresUnknownSelfQuery(t *testing.T) {
	p, times, reducer := newPlaybackFixture()
	features := reducer.ServerFeatures("libera")
	mapping := features.CaseMapping()
	when := time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)

	message := irc.Message{
		Command: "PRIVMSG",
		Prefix:  &irc.Prefix{Nick: "omairc", User: "u", Host: "h"},
		Params:  []string{"alice", "hi"},
		Tags:    []irc.Tag{{Name: "time", Value: stringPtr(when.Format(time.RFC3339Nano))}},
	}
	p.NotePlaybackClock("libera", message, "omairc", false)
	if _, ok := times.Noted("libera", "alice", mapping); ok {
		t.Fatalf("a self line whose query does not exist moved the clock")
	}
}

func keepHistoryLine(reducer *irc.EventReducer, networkID, target, msgid string, when time.Time) {
	key := reducer.ConversationKey(networkID, target)
	reducer.EnsureConversation(key, target, irc.CauseInboundOther)
	serverTime := when
	reducer.Apply(irc.HistoryEvent{
		Conversation: key,
		Target:       target,
		Kind:         irc.HistoryBouncerPlayback,
		Lines: []irc.ReplayLine{{
			Author:     "alice",
			Body:       "hi",
			Timestamp:  when,
			Kind:       irc.MessageKindChat,
			MsgID:      irc.MsgID{Value: msgid},
			ServerTime: &serverTime,
		}},
	}, when)
}

func TestPlaybackNoteKeptReplayRequiresCoverage(t *testing.T) {
	p, times, reducer := newPlaybackFixture()
	features := reducer.ServerFeatures("libera")
	mapping := features.CaseMapping()
	when := time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)
	keepHistoryLine(reducer, "libera", "alice", "m1", when)

	// znc.in/playback is negotiated but no PLAY line was sent yet, so the
	// replay is not covered and must not move the clock.
	p.NoteKeptReplay(func(string) bool { return true })
	if _, ok := times.Noted("libera", "alice", mapping); ok {
		t.Fatalf("an uncovered kept replay moved the clock")
	}
}

func TestPlaybackNoteKeptReplayNotesWhenUncapped(t *testing.T) {
	p, times, reducer := newPlaybackFixture()
	features := reducer.ServerFeatures("libera")
	mapping := features.CaseMapping()
	when := time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)
	keepHistoryLine(reducer, "libera", "alice", "m1", when)

	p.NoteKeptReplay(func(string) bool { return false })
	if got, ok := times.Noted("libera", "alice", mapping); !ok || !got.Equal(when) {
		t.Fatalf("Noted = %v (ok=%v), want %v", got, ok, when)
	}
}

func TestPlaybackRekeyMovesSnapshot(t *testing.T) {
	p, _, reducer := newPlaybackFixture()
	features := reducer.ServerFeatures("libera")
	mapping := features.CaseMapping()
	when := time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)
	if !p.times.Note("libera", "Alice", when, mapping) {
		t.Fatalf("Note = false")
	}
	p.OnRegistered("libera", nil)

	p.Rekey("libera", "Alice", "Alicia")
	rows := p.playbackSnapshot["libera"]
	if len(rows) != 1 || rows[0].Target != "Alicia" || !rows[0].When.Equal(when) {
		t.Fatalf("after rekey = %+v", rows)
	}

	// A case-only rekey rewrites the spelling in place.
	p.Rekey("libera", "Alicia", "ALICIA")
	rows = p.playbackSnapshot["libera"]
	if len(rows) != 1 || rows[0].Target != "ALICIA" {
		t.Fatalf("after case rekey = %+v", rows)
	}
}

func TestPlaybackTrimBouncerBatchBoundaries(t *testing.T) {
	p, _, reducer := newPlaybackFixture()
	features := reducer.ServerFeatures("libera")
	when := time.Date(2026, 9, 26, 10, 0, 0, 0, time.UTC)
	if !p.times.Note("libera", "#chan", when, features.CaseMapping()) {
		t.Fatalf("Note = false")
	}
	p.OnRegistered("libera", nil)
	p.zncPlaybackSent["libera"] = zncPlaybackSent{queries: true}

	older := when.Add(-time.Millisecond)
	newer := when.Add(time.Millisecond)
	zero := time.Time{}
	event := irc.HistoryEvent{
		Target: "#chan",
		Lines: []irc.ReplayLine{
			{Body: "older", ServerTime: &older},
			{Body: "equal-no-msgid", ServerTime: &when},
			{Body: "equal-msgid", ServerTime: &when, MsgID: irc.MsgID{Value: "m1"}},
			{Body: "newer", ServerTime: &newer},
			{Body: "notime"},
			{Body: "zerotime", ServerTime: &zero},
		},
	}
	p.TrimBouncerBatch("libera", &event)

	bodies := make([]string, 0, len(event.Lines))
	for _, line := range event.Lines {
		bodies = append(bodies, line.Body)
	}
	want := []string{"equal-msgid", "newer", "notime", "zerotime"}
	if !slices.Equal(bodies, want) {
		t.Fatalf("kept = %v, want %v", bodies, want)
	}
}

func TestPlaybackRequestEmptySnapshotSendsPlayAll(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "znc.in/playback")
	s := c.Session("libera")

	var sent []string
	c.playback = NewPlaybackCoordinator(c.playbackTimes, c.reducer)
	c.playback.sendZnc = recordZnc(&sent)
	c.playback.OnRegistered("libera", nil)
	c.playback.Request(s, true, true, nil, func(string) bool { return true })

	if !slices.Equal(sent, []string{"* 0"}) {
		t.Fatalf("sent = %v, want [* 0]", sent)
	}
}

func TestPlaybackRequestSnapshotSendsPerTarget(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "znc.in/playback")
	s := c.Session("libera")

	when := time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)
	features := c.reducer.ServerFeatures("libera")
	if !c.playbackTimes.Note("libera", "#chan", when, features.CaseMapping()) {
		t.Fatalf("Note = false")
	}

	var sent []string
	c.playback = NewPlaybackCoordinator(c.playbackTimes, c.reducer)
	c.playback.sendZnc = recordZnc(&sent)
	c.playback.OnRegistered("libera", nil)
	c.playback.Request(s, true, true, nil, func(string) bool { return true })

	want := []string{
		"#chan " + storage.PlaybackPlayStamp(when, true),
		"* 0",
	}
	if !slices.Equal(sent, want) {
		t.Fatalf("sent = %v, want %v", sent, want)
	}
}

func TestPlaybackRequestGuards(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "znc.in/playback")
	s := c.Session("libera")

	var sent []string
	c.playback = NewPlaybackCoordinator(c.playbackTimes, c.reducer)
	c.playback.sendZnc = recordZnc(&sent)
	c.playback.OnRegistered("libera", nil)

	c.playback.Request(s, true, false, nil, func(string) bool { return true })
	c.playback.Request(s, false, true, nil, func(string) bool { return true })
	c.playback.Request(nil, true, true, nil, func(string) bool { return true })
	if len(sent) != 0 {
		t.Fatalf("guarded requests sent %v", sent)
	}

	// A failing send stops the batch immediately.
	c.playback.sendZnc = func(*session.Session, string, string) bool { return false }
	c.playback.Request(s, true, true, nil, func(string) bool { return true })
}

func TestPlaybackDefaultSendWritesRawLine(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "znc.in/playback")
	s := c.Session("libera")

	p := NewPlaybackCoordinator(c.playbackTimes, c.reducer)
	if !p.sendZnc(s, "#chan", "0") {
		t.Fatalf("sendZnc = false")
	}
	if got := lastWrittenFrame(t, transport); got != "ZNC *playback PLAY #chan 0\r\n" {
		t.Fatalf("frame = %q", got)
	}
}

func TestPlaybackRequestChannelPlaybackSends(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "znc.in/playback")
	inject(t, transport, ":omairc!u@h JOIN :#chan\r\n")
	s := c.Session("libera")

	var sent []string
	c.playback.sendZnc = recordZnc(&sent)
	c.playback.RequestChannelPlayback(s, "#chan", true, true)

	if !slices.Equal(sent, []string{"#chan 0"}) {
		t.Fatalf("sent = %v, want [#chan 0]", sent)
	}
}

func TestPlaybackRequestChannelPlaybackRefusesAfterKeptBatch(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "znc.in/playback")
	inject(t, transport, ":omairc!u@h JOIN :#chan\r\n")
	s := c.Session("libera")

	when := time.Date(2026, 9, 26, 9, 0, 0, 0, time.UTC)
	key := c.reducer.ConversationKey("libera", "#chan")
	serverTime := when
	c.reducer.Apply(irc.HistoryEvent{
		Conversation: key,
		Target:       "#chan",
		Kind:         irc.HistoryBouncerPlayback,
		Lines: []irc.ReplayLine{{
			Author:     "alice",
			Body:       "hi",
			Timestamp:  when,
			Kind:       irc.MessageKindChat,
			MsgID:      irc.MsgID{Value: "m1"},
			ServerTime: &serverTime,
		}},
	}, when)
	if !c.reducer.PlaybackBatchKept("libera", "#chan") {
		t.Fatalf("precondition: playback batch not kept")
	}

	var sent []string
	c.playback.sendZnc = recordZnc(&sent)
	c.playback.RequestChannelPlayback(s, "#chan", true, true)
	if len(sent) != 0 {
		t.Fatalf("sent after a kept batch: %v", sent)
	}
}

func TestPlaybackRequestChannelPlaybackGuards(t *testing.T) {
	c, clock := newController(t)
	transport := addAndStart(t, c, clock, baseConfig("libera", "omairc"))
	registerNetwork(t, transport, "omairc", "znc.in/playback")
	s := c.Session("libera")

	var sent []string
	c.playback.sendZnc = recordZnc(&sent)
	c.playback.RequestChannelPlayback(s, "alice", true, true)  // not a channel
	c.playback.RequestChannelPlayback(s, "#chan", true, false) // no MOTD yet
	c.playback.RequestChannelPlayback(s, "#chan", false, true) // no playback cap
	if len(sent) != 0 {
		t.Fatalf("guarded channel requests sent %v", sent)
	}
}
