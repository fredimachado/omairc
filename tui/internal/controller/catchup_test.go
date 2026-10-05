package controller

import (
	"strings"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

type closedQueryLog struct{}

func (closedQueryLog) Append(string, string, irc.CaseMapping, irc.TranscriptLine) bool {
	return true
}

func (closedQueryLog) Prepend(string, string, irc.CaseMapping, []irc.TranscriptLine) bool {
	return true
}

func (closedQueryLog) ReadTail(_, target string, mapping irc.CaseMapping, maxLines int) []irc.TranscriptLine {
	if maxLines <= 0 || !mapping.Equals(target, "filed") {
		return nil
	}
	return []irc.TranscriptLine{{Body: "kept"}}
}

func byteFramesContain(frames [][]byte, needle string) bool {
	for _, frame := range frames {
		if strings.Contains(string(frame), needle) {
			return true
		}
	}
	return false
}

func byteFrameCount(frames [][]byte, needle string) int {
	count := 0
	for _, frame := range frames {
		if strings.Contains(string(frame), needle) {
			count++
		}
	}
	return count
}

func TestChatHistoryAfterFillsLinesFromWhileAway(t *testing.T) {
	c := New()
	clock := session.NewFakeClock(time.Date(2026, 10, 4, 12, 0, 0, 0, time.UTC))
	c.SetClock(clock)
	transport := session.NewLoopbackTransport()
	config := session.SessionConfig{
		NetworkID:  "libera",
		Host:       "irc.example",
		Port:       6697,
		TLSEnabled: true,
		Nick:       "omairc",
		Username:   "omairc",
		Realname:   "Omairc User",
	}
	s, err := c.AddSession(config, transport, clock)
	if err != nil {
		t.Fatal(err)
	}
	when := time.Date(2024, 3, 9, 16, 0, 0, 620000000, time.UTC)
	features := c.reducer.ServerFeatures("libera")
	mapping := features.CaseMapping()
	if !c.playbackTimes.Note("libera", "#omarchy", when, mapping) ||
		!c.playbackTimes.Note("libera", "lena", when, mapping) ||
		!c.playbackTimes.Note("libera", "ghost", when, mapping) {
		t.Fatal("Note")
	}
	if !c.openDirects.Add("libera", "lena", mapping) || !c.openDirects.Add("libera", "bob", mapping) {
		t.Fatal("Add open direct")
	}
	c.reducer.SetConversationLog(closedQueryLog{})

	s.Start()
	transport.CompleteConnect()
	transport.InjectBytes([]byte(
		":server CAP omairc LS :batch chathistory\r\n" +
			":server CAP omairc ACK :batch chathistory\r\n" +
			":server 001 omairc :Welcome\r\n" +
			":server 376 omairc :End of MOTD\r\n" +
			":omairc!u@h JOIN :#omarchy\r\n"))

	frames := transport.WrittenFrames()
	if !byteFramesContain(frames, "CHATHISTORY AFTER #omarchy timestamp=2024-03-09T16:00:00.620Z 100\r\n") {
		t.Fatalf("channel AFTER missing: %q", frames)
	}
	if byteFramesContain(frames, "CHATHISTORY LATEST #omarchy ") {
		t.Fatal("stamped channel sent LATEST")
	}
	if !byteFramesContain(frames, "CHATHISTORY AFTER lena timestamp=2024-03-09T16:00:00.620Z 100\r\n") {
		t.Fatal("open direct AFTER missing")
	}
	if !byteFramesContain(frames, "CHATHISTORY LATEST bob * 100\r\n") {
		t.Fatal("open direct without a stamp did not send LATEST")
	}
	if !byteFramesContain(frames, "CHATHISTORY TARGETS timestamp=2024-03-09T15:59:59.620Z timestamp=2026-10-04T12:00:10.000Z 100\r\n") {
		t.Fatalf("TARGETS missing: %q", frames)
	}
	if byteFrameCount(frames, "CHATHISTORY AFTER ghost ") != 0 {
		t.Fatal("closed direct was asked")
	}
	if byteFramesContain(frames, "*playback PLAY") {
		t.Fatal("chathistory server sent ZNC PLAY")
	}
	if !hasConversation(c, "libera", "lena") || !hasConversation(c, "libera", "bob") {
		t.Fatalf("open directs = %v", conversationTargets(c))
	}
	if hasConversation(c, "libera", "ghost") {
		t.Fatal("closed direct is open before TARGETS")
	}

	transport.InjectBytes([]byte(
		":irc.host BATCH +t draft/chathistory-targets\r\n" +
			"@batch=t :irc.host CHATHISTORY TARGETS lena 2024-03-09T16:00:00.620Z\r\n" +
			"@batch=t :irc.host CHATHISTORY TARGETS ghost 2024-03-09T16:00:00.620Z\r\n" +
			"@batch=t :irc.host CHATHISTORY TARGETS filed 2024-03-09T16:00:00.620Z\r\n" +
			"@batch=t :irc.host CHATHISTORY TARGETS BouncerServ 2024-03-09T16:00:00.620Z\r\n" +
			"@batch=t :irc.host CHATHISTORY TARGETS alice 2024-03-09T16:00:01.000Z\r\n" +
			"@batch=t :irc.host CHATHISTORY TARGETS #parted 2024-03-09T16:00:00.620Z\r\n" +
			"@batch=t :irc.host CHATHISTORY TARGETS omairc 2024-03-09T16:00:00.620Z\r\n" +
			":irc.host BATCH -t\r\n"))

	if !hasConversation(c, "libera", "alice") {
		t.Fatalf("never-opened direct missing: %v", conversationTargets(c))
	}
	for _, closed := range []string{"ghost", "filed", "BouncerServ", "#parted"} {
		if hasConversation(c, "libera", closed) {
			t.Fatalf("%s was opened", closed)
		}
	}
	after := transport.WrittenFrames()
	if byteFrameCount(after, "CHATHISTORY AFTER lena ") != 1 {
		t.Fatal("open direct was asked twice")
	}
	if byteFrameCount(after, "CHATHISTORY AFTER ghost ") != 0 ||
		byteFrameCount(after, "CHATHISTORY AFTER filed ") != 0 ||
		byteFrameCount(after, "CHATHISTORY AFTER BouncerServ ") != 0 {
		t.Fatal("closed or service target was asked")
	}
	if byteFrameCount(after, "CHATHISTORY AFTER alice timestamp=2024-03-09T15:59:59.620Z 100\r\n") != 1 {
		t.Fatal("never-opened direct AFTER missing")
	}
	listed := c.openDirects.Listed("libera", mapping)
	for _, nick := range listed {
		if mapping.Equals(nick, "alice") {
			t.Fatal("discovered direct was persisted")
		}
	}

	transport.InjectBytes([]byte(
		":alice!u@h PRIVMSG #omarchy :seen\r\n" +
			":irc.host BATCH +gap chathistory #omarchy\r\n" +
			"@batch=gap;time=2024-03-09T16:00:01.000Z;msgid=gap :bob!u@h PRIVMSG #omarchy :gap\r\n" +
			":irc.host BATCH -gap\r\n" +
			":irc.host BATCH +away chathistory alice\r\n" +
			"@batch=away;time=2024-03-09T16:00:01.000Z;msgid=away :alice!u@h PRIVMSG omairc :while away\r\n" +
			":irc.host BATCH -away\r\n"))
	c.SelectConversation("libera", "#omarchy")
	bodies := messageBodies(c)
	if !stringListContains(bodies, "seen") || !stringListContains(bodies, "gap") {
		t.Fatalf("channel bodies = %v", bodies)
	}
	c.SelectConversation("libera", "alice")
	if !stringListContains(messageBodies(c), "while away") {
		t.Fatalf("alice bodies = %v", messageBodies(c))
	}
}

func TestZncPlaybackWithoutChatHistoryDoesNotCatchUp(t *testing.T) {
	c := New()
	transport := session.NewLoopbackTransport()
	config := session.SessionConfig{
		NetworkID:  "libera",
		Host:       "irc.example",
		Port:       6697,
		TLSEnabled: true,
		Nick:       "omairc",
		Username:   "omairc",
		Realname:   "Omairc User",
	}
	s, err := c.AddSession(config, transport, nil)
	if err != nil {
		t.Fatal(err)
	}
	when := time.Date(2024, 3, 9, 16, 0, 0, 620000000, time.UTC)
	features := c.reducer.ServerFeatures("libera")
	mapping := features.CaseMapping()
	if !c.playbackTimes.Note("libera", "#omarchy", when, mapping) {
		t.Fatal("Note")
	}
	s.Start()
	transport.CompleteConnect()
	transport.InjectBytes([]byte(
		":server CAP omairc LS :batch znc.in/playback\r\n" +
			":server CAP omairc ACK :batch znc.in/playback\r\n" +
			":server 001 omairc :Welcome\r\n" +
			":server 376 omairc :End of MOTD\r\n" +
			":omairc!u@h JOIN :#omarchy\r\n"))
	frames := transport.WrittenFrames()
	if !byteFramesContain(frames, "*playback PLAY") {
		t.Fatalf("PLAY missing: %q", frames)
	}
	if byteFramesContain(frames, "CHATHISTORY ") {
		t.Fatalf("znc-only server sent CHATHISTORY: %q", frames)
	}
}

func stringListContains(rows []string, needle string) bool {
	for _, row := range rows {
		if row == needle {
			return true
		}
	}
	return false
}
