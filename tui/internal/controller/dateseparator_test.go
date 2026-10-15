package controller

import (
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

func dateRoom(t *testing.T, c *Controller) irc.ConversationKey {
	t.Helper()
	c.Apply(irc.WelcomeEvent{NetworkID: "net", CurrentNick: "omairc"})
	key := c.reducer.ConversationKey("net", "#room")
	c.reducer.MarkSelected(key)
	return key
}

func applyChat(c *Controller, key irc.ConversationKey, body string, at time.Time) {
	c.Apply(irc.MessageEvent{
		Conversation: key,
		Author:       "alice",
		Body:         body,
		Timestamp:    at,
		Target:       "#room",
	})
}

func countLabels(labels []string, want string) int {
	count := 0
	for _, label := range labels {
		if label == want {
			count++
		}
	}
	return count
}

func TestDateSeparatorLabelsFirstDayWhenNotToday(t *testing.T) {
	c, _ := newController(t)
	key := dateRoom(t, c)
	applyChat(c, key, "only yesterday", time.Date(2026, 9, 25, 15, 4, 0, 0, time.UTC))

	labels := c.DateSeparators()
	if len(labels) != 1 || labels[0] != "Yesterday" {
		t.Fatalf("labels = %q, want [Yesterday]", labels)
	}
	if countLabels(labels, "Yesterday") != 1 {
		t.Fatalf("Yesterday count = %d, want 1", countLabels(labels, "Yesterday"))
	}
}

func TestDateSeparatorOmitsLabelWhenFirstDayIsToday(t *testing.T) {
	c, _ := newController(t)
	key := dateRoom(t, c)
	applyChat(c, key, "only today", time.Date(2026, 9, 26, 9, 15, 0, 0, time.UTC))
	applyChat(c, key, "also today", time.Date(2026, 9, 26, 9, 16, 0, 0, time.UTC))

	labels := c.DateSeparators()
	for index, label := range labels {
		if label != "" {
			t.Fatalf("labels[%d] = %q, want no date on a today-only transcript", index, label)
		}
	}
	if len(labels) != 2 {
		t.Fatalf("len(labels) = %d, want 2", len(labels))
	}
}

func TestDateSeparatorLabelsMidnight(t *testing.T) {
	c, _ := newController(t)
	key := dateRoom(t, c)
	applyChat(c, key, "before", time.Date(2026, 9, 25, 23, 59, 0, 0, time.UTC))
	applyChat(c, key, "after", time.Date(2026, 9, 26, 0, 0, 0, 0, time.UTC))

	labels := c.DateSeparators()
	if len(labels) != 2 || labels[0] != "Yesterday" || labels[1] != "Today" {
		t.Fatalf("labels = %q, want [Yesterday Today]", labels)
	}
}

func TestDateSeparatorFollowsClockZone(t *testing.T) {
	c := New()
	// 12:00 in UTC-10 is still 26 September there, and 22:00 UTC the same
	// calendar day. Both messages are 26 September in UTC, so a UTC comparison
	// would leave the first day unlabeled.
	loc := time.FixedZone("UTC-10", -10*60*60)
	c.SetClock(session.NewFakeClock(time.Date(2026, 9, 26, 12, 0, 0, 0, loc)))
	key := dateRoom(t, c)
	applyChat(c, key, "late", time.Date(2026, 9, 26, 9, 30, 0, 0, time.UTC))
	applyChat(c, key, "early", time.Date(2026, 9, 26, 10, 30, 0, 0, time.UTC))

	labels := c.DateSeparators()
	if len(labels) != 2 || labels[0] != "Yesterday" || labels[1] != "Today" {
		t.Fatalf("labels = %q, want [Yesterday Today] in UTC-10", labels)
	}
}

func TestDateSeparatorMovesWhenOlderHistoryPrepended(t *testing.T) {
	c, _ := newController(t)
	key := dateRoom(t, c)
	applyChat(c, key, "from yesterday", time.Date(2026, 9, 25, 15, 4, 0, 0, time.UTC))
	if got := c.DateSeparators(); len(got) != 1 || got[0] != "Yesterday" {
		t.Fatalf("before prepend labels = %q, want [Yesterday]", got)
	}

	c.Apply(irc.HistoryEvent{
		Conversation: key,
		Target:       "#room",
		Kind:         irc.HistoryChat,
		OlderPage:    true,
		Lines: []irc.ReplayLine{{
			Author:    "bob",
			Body:      "from older",
			Timestamp: time.Date(2026, 9, 24, 9, 0, 0, 0, time.UTC),
			MsgID:     irc.MsgID{Value: "old-1"},
		}},
	})
	labels := c.DateSeparators()
	if len(labels) != 2 || labels[0] != "9/24/26" || labels[1] != "Yesterday" {
		t.Fatalf("after older prepend labels = %q, want [9/24/26 Yesterday]", labels)
	}
	if countLabels(labels, "9/24/26") != 1 || countLabels(labels, "Yesterday") != 1 {
		t.Fatalf("duplicate date labels: %q", labels)
	}

	c.Apply(irc.HistoryEvent{
		Conversation: key,
		Target:       "#room",
		Kind:         irc.HistoryChat,
		OlderPage:    true,
		Lines: []irc.ReplayLine{{
			Author:    "bob",
			Body:      "also older",
			Timestamp: time.Date(2026, 9, 24, 8, 0, 0, 0, time.UTC),
			MsgID:     irc.MsgID{Value: "old-2"},
		}},
	})
	labels = c.DateSeparators()
	if len(labels) != 3 || labels[0] != "9/24/26" || labels[1] != "" || labels[2] != "Yesterday" {
		t.Fatalf("same-day prepend labels = %q, want one 9/24/26 then Yesterday", labels)
	}
	if countLabels(labels, "9/24/26") != 1 || countLabels(labels, "Yesterday") != 1 {
		t.Fatalf("duplicate date labels after same-day prepend: %q", labels)
	}
}
