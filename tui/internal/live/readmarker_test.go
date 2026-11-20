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

// sawPrivmsgBody reports a PRIVMSG carrying body on the wire. Paired with a DM
// row's unread so a non-selected DM can prove it holds a line without selecting
// it (selecting would clear unread via markSelected).
func sawPrivmsgBody(c *Client, body string) bool {
	for _, message := range c.Incoming() {
		if !strings.EqualFold(message.Command, "PRIVMSG") || len(message.Params) == 0 {
			continue
		}
		if message.Params[len(message.Params)-1] == body {
			return true
		}
	}
	return false
}

func dmUnreadWithBody(c *Client, nick, body string) bool {
	unread, ok := dmUnread(c, nick)
	return ok && unread >= 1 && sawPrivmsgBody(c, body)
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

	// Own channels first so neither session auto-selects the DM as its first
	// conversation (claimFirstConversation → markSelected clears unread).
	readerChannel := UniqueChannel()
	otherChannel := UniqueChannel()
	if !reader.JoinChannel(readerChannel) || !waitNamesEnd(reader, readerChannel) {
		t.Fatalf("%s: reader join %s", daemon.Name, readerChannel)
	}
	reader.SelectChannel(readerChannel)
	if !other.JoinChannel(otherChannel) || !waitNamesEnd(other, otherChannel) {
		t.Fatalf("%s: other join %s", daemon.Name, otherChannel)
	}
	other.SelectChannel(otherChannel)

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
	if !other.WaitUntil(func(c *Client) bool { return dmUnreadWithBody(c, peerNick, one) }) {
		t.Fatalf("%s: other DM unread before reader select", daemon.Name)
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
	// Keep A's DM selected with the window inactive so a selected-DM server
	// marker clear is exercised (live selected chat counts unread while unfocused).
	reader.Do(func(ctrl *controller.Controller) { ctrl.SetWindowActive(false) })
	two := "marker-two-" + account
	peer.Do(func(ctrl *controller.Controller) {
		sent = ctrl.SendToTarget(peer.NetworkID, account, two)
	})
	if !sent {
		t.Fatalf("%s: peer send two", daemon.Name)
	}
	if !reader.WaitUntil(func(c *Client) bool {
		return strings.EqualFold(c.SelectedTarget(), peerNick) && dmUnreadWithBody(c, peerNick, two)
	}) {
		t.Fatalf("%s: reader selected DM unread before other publishes", daemon.Name)
	}
	other.SelectChannel(peerNick)
	if !other.WaitUntil(func(c *Client) bool { return channelHasBody(c, two) }) {
		t.Fatalf("%s: other saw %s", daemon.Name, two)
	}
	other.Do(func(ctrl *controller.Controller) {
		ctrl.SetWindowActive(true)
		ctrl.NoteTranscriptViewport(true, true)
	})
	// Wait for other's SET before reader's unread clear (mirrors Rust, where
	// reader.wait_until only pumps reader and would miss other's GET reply).
	if !other.WaitUntil(func(c *Client) bool {
		return len(markreadSets(c.Sent(), peerNick)) == 1
	}) {
		t.Fatalf("%s: other SET after activate; markread=%v", daemon.Name, markreadLines(other.Sent(), peerNick))
	}
	if !reader.WaitUntil(func(c *Client) bool {
		unread, ok := dmUnread(c, peerNick)
		return ok && unread == 0
	}) {
		t.Fatalf("%s: reader unread cleared by other SET", daemon.Name)
	}
	// Off the DM and back while active for the I2 fence (no second GET expected).
	reader.SelectChannel(readerChannel)
	reader.SelectChannel(peerNick)
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
	if got := markreadGets(reader.Sent(), peerNick); got != 1 {
		t.Fatalf("%s: still one GET before reconnect got %d; markread=%v", daemon.Name, got, markreadLines(reader.Sent(), peerNick))
	}
	if got := len(markreadSets(reader.Sent(), peerNick)); got != 1 {
		t.Fatalf("%s: reader must not SET again at known marker; markread=%v", daemon.Name, markreadLines(reader.Sent(), peerNick))
	}
	if got := len(markreadSets(other.Sent(), peerNick)); got != 1 {
		t.Fatalf("%s: other one SET for stamp two; markread=%v", daemon.Name, markreadLines(other.Sent(), peerNick))
	}

	// Leg 3: reconnect writes one GET and no SET.
	sentBefore := len(reader.Sent())
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
	// I4: no SET after reconnect. That is also the proof the reconnect did not
	// move B's marker forward (a forward SET would have been written here if A
	// had anything newer). MARKREAD is consumed before Incoming(), so a
	// "no MARKREAD in B.Incoming" check can never fail.
	if got := markreadSets(after, peerNick); len(got) != 0 {
		t.Fatalf("%s: no SET after reconnect; markread=%v", daemon.Name, markreadLines(after, peerNick))
	}
	if !other.Fence("leg3-b") {
		t.Fatalf("%s: other fence after reconnect", daemon.Name)
	}
	if unread, ok := dmUnread(other, peerNick); !ok || unread != 0 {
		t.Fatalf("%s: other unread stays cleared (ok=%v unread=%d)", daemon.Name, ok, unread)
	}
}
