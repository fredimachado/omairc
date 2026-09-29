package controller_test

// This file pins the display clock on MessageSnapshot.Time. The view renders,
// groups, and copies that cell directly, so it is the one place the reducer's
// stored instant is turned into the zone the user reads. It mirrors
// MessageListModel::displayTime's QDateTime::toLocalTime.

import (
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/demo"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// TestMessagesUseTheClockZone proves the snapshot hands the view a display
// clock. The reducer keeps a parsed server-time tag in UTC (translator.go's
// ircServerTimeOf) and the clock is local, so a raw passthrough rendered the UTC
// cell: the demo's 10:12Z row read "10:12" everywhere instead of the seeded
// wall time in the reader's zone.
func TestMessagesUseTheClockZone(t *testing.T) {
	// The anna DM's only seeded row is 2026-09-12T10:12:00Z: 20:12 in UTC+10.
	zone := time.FixedZone("UTC+10", 10*60*60)
	c := controller.New()
	c.SetClock(session.NewFakeClock(time.Date(2026, 9, 12, 10, 12, 30, 0, time.UTC).In(zone)))
	d := demo.New()
	if !d.Attach(c, true) {
		t.Fatalf("demo Attach failed: %q", d.LastError())
	}
	c.SelectConversation("omarchy", "anna")

	rows := c.Messages()
	if len(rows) == 0 {
		t.Fatal("the anna DM must have a seeded row")
	}
	if got := rows[0].Time.Format("15:04"); got != "20:12" {
		t.Fatalf("displayed clock = %q, want the clock zone's 20:12", got)
	}
	if got := rows[0].Time.Location(); got != zone {
		t.Fatalf("display location = %v, want the clock's %v", got, zone)
	}
}
