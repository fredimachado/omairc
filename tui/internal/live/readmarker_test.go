//go:build live

package live

import (
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
)

// Mirrors `client/tests/live/readmarker.rs`.

func markreadGets(lines []string, target string) int {
	want := "MARKREAD " + target
	count := 0
	for _, line := range lines {
		if line == want {
			count++
		}
	}
	return count
}

func markreadSets(lines []string, target string) []string {
	prefix := "MARKREAD " + target + " timestamp="
	var out []string
	for _, line := range lines {
		if strings.HasPrefix(line, prefix) {
			out = append(out, line)
		}
	}
	return out
}

func dmUnread(c *Client, nick string) (int, bool) {
	rows := directRowsFor(c, nick)
	if len(rows) == 0 {
		return 0, false
	}
	return rows[0].Unread, true
}

func markreadLines(lines []string, target string) []string {
	prefix := "MARKREAD " + target
	var out []string
	for _, line := range lines {
		if line == prefix || strings.HasPrefix(line, prefix+" ") {
			out = append(out, line)
		}
	}
	return out
}

func hasReadMarker(c *Client) bool {
	return c.Session().Capabilities().Contains(irc.CapabilityReadMarker)
}

func TestReadMarkerAcrossSessions(t *testing.T) {
	// Ergo only: needs SASL, multiclient, and draft/read-marker. Solanum/ngIRCd
	// have no SASL accounts in the fixture (DaemonInfo.SASL == false).
	daemon := requireDaemon(t, "ergo")
	tls := daemon.UseTLS()
	account := UniqueNick(daemon.NickLength)
	password := "live-marker-secret"
	registerErgoAccount(t, daemon, account, password)

	reader := Connect(t, daemon, account, tls, Options{
		Password:         password,
		ReconnectEnabled: true,
	})
	other := Connect(t, daemon, account, tls, Options{Password: password})
	if !reader.WaitRegistered() {
		t.Fatalf("%s: reader register: %s", daemon.Name, reader.LastError())
	}
	if !other.WaitRegistered() {
		t.Fatalf("%s: other register: %s", daemon.Name, other.LastError())
	}
	if !hasReadMarker(reader) && !reader.WaitUntil(func(c *Client) bool { return hasReadMarker(c) }) {
		t.Fatalf("%s: reader missing ReadMarker", daemon.Name)
	}
	if !hasReadMarker(other) && !other.WaitUntil(func(c *Client) bool { return hasReadMarker(c) }) {
		t.Fatalf("%s: other missing ReadMarker", daemon.Name)
	}
	if got := other.Nick(); !strings.EqualFold(got, account) {
		t.Fatalf("%s: other nick = %q, want %q", daemon.Name, got, account)
	}

	other.Do(func(ctrl *controller.Controller) { ctrl.SetWindowActive(false) })
	reader.Do(func(ctrl *controller.Controller) { ctrl.NoteTranscriptViewport(true, true) })

	peer := Connect(t, daemon, UniqueNick(daemon.NickLength), tls, Options{})
	if !peer.WaitRegistered() {
		t.Fatalf("%s: peer register: %s", daemon.Name, peer.LastError())
	}
	peerNick := peer.Nick()

	// Leg 1: A reads, B's marker moves.
	one := "marker-one-" + account
	var sent bool
	peer.Do(func(ctrl *controller.Controller) {
		sent = ctrl.SendToTarget(peer.NetworkID, account, one)
	})
	if !sent {
		t.Fatalf("%s: peer send one", daemon.Name)
	}
	if !reader.WaitUntil(func(c *Client) bool { return len(directRowsFor(c, peerNick)) > 0 }) {
		t.Fatalf("%s: reader DM row for %s", daemon.Name, peerNick)
	}
	reader.SelectChannel(peerNick)
	if !reader.WaitUntil(func(c *Client) bool {
		return len(markreadSets(c.Sent(), peerNick)) == 1
	}) {
		t.Fatalf("%s: reader SET after select; markread=%v", daemon.Name, markreadLines(reader.Sent(), peerNick))
	}
	if !other.WaitUntil(func(c *Client) bool {
		unread, ok := dmUnread(c, peerNick)
		return ok && unread == 0
	}) {
		t.Fatalf("%s: other unread cleared by reader SET", daemon.Name)
	}
	if !reader.Fence("leg1") {
		t.Fatalf("%s: reader fence after leg 1", daemon.Name)
	}
	if got := markreadGets(reader.Sent(), peerNick); got != 1 {
		t.Fatalf("%s: one GET after leg 1 got %d; markread=%v", daemon.Name, got, markreadLines(reader.Sent(), peerNick))
	}
	if got := len(markreadSets(reader.Sent(), peerNick)); got != 1 {
		t.Fatalf("%s: one SET after leg 1 got %d; markread=%v", daemon.Name, got, markreadLines(reader.Sent(), peerNick))
	}

	// Leg 2: B reads, A must not answer with an equal SET.
	reader.Do(func(ctrl *controller.Controller) { ctrl.SetWindowActive(false) })
	two := "marker-two-" + account
	peer.Do(func(ctrl *controller.Controller) {
		sent = ctrl.SendToTarget(peer.NetworkID, account, two)
	})
	if !sent {
		t.Fatalf("%s: peer send two", daemon.Name)
	}
	if !strings.EqualFold(other.SelectedTarget(), peerNick) {
		other.SelectChannel(peerNick)
	}
	if !other.WaitUntil(func(c *Client) bool { return channelHasBody(c, two) }) {
		t.Fatalf("%s: other saw %s", daemon.Name, two)
	}
	other.Do(func(ctrl *controller.Controller) {
		ctrl.SetWindowActive(true)
		ctrl.NoteTranscriptViewport(true, true)
	})
	if !reader.WaitUntil(func(c *Client) bool {
		unread, ok := dmUnread(c, peerNick)
		return channelHasBody(c, two) && ok && unread == 0
	}) {
		t.Fatalf("%s: reader unread cleared by other SET", daemon.Name)
	}
	reader.Do(func(ctrl *controller.Controller) {
		ctrl.SetWindowActive(true)
		ctrl.NoteTranscriptViewport(true, true)
	})
	if !reader.Fence("leg2-a") {
		t.Fatalf("%s: reader fence after leg 2", daemon.Name)
	}
	if !other.Fence("leg2-b") {
		t.Fatalf("%s: other fence after leg 2", daemon.Name)
	}
	if got := len(markreadSets(reader.Sent(), peerNick)); got != 1 {
		t.Fatalf("%s: reader must not SET again at known marker; markread=%v", daemon.Name, markreadLines(reader.Sent(), peerNick))
	}
	if got := len(markreadSets(other.Sent(), peerNick)); got != 1 {
		t.Fatalf("%s: other one SET for stamp two; markread=%v", daemon.Name, markreadLines(other.Sent(), peerNick))
	}

	// Leg 3: reconnect writes one GET and no SET.
	sentBefore := len(reader.Sent())
	otherIncomingBefore := len(other.Incoming())
	if !reader.ForceDisconnect() {
		t.Fatalf("%s: ForceDisconnect", daemon.Name)
	}
	if !reader.WaitReconnecting() {
		t.Fatalf("%s: WaitReconnecting", daemon.Name)
	}
	if !reader.WaitRegisteredAgain() {
		t.Fatalf("%s: WaitRegisteredAgain: %s", daemon.Name, reader.LastError())
	}
	if !strings.EqualFold(reader.SelectedTarget(), peerNick) {
		reader.SelectChannel(peerNick)
	}
	// Deviation: plan waits for MARKREAD in Incoming(), but deliverReadMarker
	// consumes those lines before MessageReceived. Wait for the outbound GET
	// instead; the fence then orders the reply (Ergo answers in write order).
	if !reader.WaitUntil(func(c *Client) bool {
		after := c.Sent()
		if sentBefore > len(after) {
			return false
		}
		return markreadGets(after[sentBefore:], peerNick) == 1
	}) {
		t.Fatalf("%s: GET after reconnect; markread=%v", daemon.Name, markreadLines(reader.Sent()[sentBefore:], peerNick))
	}
	if !reader.Fence("leg3-a") {
		t.Fatalf("%s: reader fence after reconnect", daemon.Name)
	}
	after := reader.Sent()[sentBefore:]
	if got := markreadGets(after, peerNick); got != 1 {
		t.Fatalf("%s: one GET after reconnect got %d; markread=%v", daemon.Name, got, markreadLines(after, peerNick))
	}
	if got := markreadSets(after, peerNick); len(got) != 0 {
		t.Fatalf("%s: no SET after reconnect; markread=%v", daemon.Name, markreadLines(after, peerNick))
	}
	if !other.Fence("leg3-b") {
		t.Fatalf("%s: other fence after reconnect", daemon.Name)
	}
	otherNew := other.Incoming()[otherIncomingBefore:]
	for _, message := range otherNew {
		if strings.EqualFold(message.Command, "MARKREAD") && paramIs(message, 0, peerNick) {
			t.Fatalf("%s: other must not see a MARKREAD for peer after reconnect", daemon.Name)
		}
	}
	if unread, ok := dmUnread(other, peerNick); !ok || unread != 0 {
		t.Fatalf("%s: other unread stays cleared (ok=%v unread=%d)", daemon.Name, ok, unread)
	}
}
