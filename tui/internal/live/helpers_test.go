//go:build live

package live

import (
	"os"
	"strings"
	"testing"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// TestMain points every XDG root at a temporary directory so nothing a live
// controller might persist reaches the developer's config or state.
func TestMain(m *testing.M) {
	root, err := os.MkdirTemp("", "omairc-tui-live-")
	if err != nil {
		panic(err)
	}
	for _, key := range []string{"XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME"} {
		dir := root + "/" + strings.ToLower(strings.TrimPrefix(key, "XDG_"))
		if err := os.MkdirAll(dir, 0o700); err != nil {
			panic(err)
		}
		os.Setenv(key, dir)
	}
	code := m.Run()
	os.RemoveAll(root)
	os.Exit(code)
}

// requireLive fails closed when someone runs -tags live without the compose
// runner, like require_live in `client/tests/live/helpers.rs`.
func requireLive(t *testing.T) []DaemonInfo {
	t.Helper()
	if !Enabled() {
		t.Fatal("Run bin/test-live. Live tests need OMAIRC_LIVE_DAEMONS and published ports.")
	}
	daemons := Daemons()
	if len(daemons) == 0 {
		t.Fatal("OMAIRC_LIVE_DAEMONS was set but no daemon ports were exported.")
	}
	return daemons
}

// requireDaemon skips when the live profile does not include name.
func requireDaemon(t *testing.T, name string) DaemonInfo {
	t.Helper()
	for _, daemon := range requireLive(t) {
		if daemon.Name == name {
			return daemon
		}
	}
	t.Skipf("live profile has no %s", name)
	return DaemonInfo{}
}

func paramIs(message irc.Message, index int, want string) bool {
	return index < len(message.Params) && strings.EqualFold(message.Params[index], want)
}

func namesEndCount(incoming []irc.Message, channel string) int {
	count := 0
	for _, message := range incoming {
		if message.Command == "366" && paramIs(message, 1, channel) {
			count++
		}
	}
	return count
}

// sawChannelMode reports a MODE line for channel whose mode string (Params[1])
// contains mode inside a `+` run, matching Rust saw_channel_mode. `MODE #c -i`
// and `MODE #c +o nick` do not count.
func sawChannelMode(incoming []irc.Message, channel string, mode rune) bool {
	for _, message := range incoming {
		if message.Command != "MODE" || len(message.Params) < 2 || !paramIs(message, 0, channel) {
			continue
		}
		adding := false
		for _, character := range message.Params[1] {
			switch {
			case character == '+':
				adding = true
			case character == '-':
				adding = false
			case adding && character == mode:
				return true
			}
		}
	}
	return false
}

// directRowsFor returns direct-message rows for nick under the network's case
// mapping, matching Rust direct_rows_for.
func directRowsFor(c *Client, nick string) []controller.ConversationSnapshot {
	features := c.Features()
	mapping := features.CaseMapping()
	var rows []controller.ConversationSnapshot
	c.Do(func(ctrl *controller.Controller) {
		for _, row := range ctrl.Conversations() {
			if row.Direct && row.NetworkID == c.NetworkID && mapping.Equals(row.Conversation, nick) {
				rows = append(rows, row)
			}
		}
	})
	return rows
}

func sawPart(incoming []irc.Message, channel string) bool {
	for _, message := range incoming {
		if message.Command == "PART" && paramIs(message, 0, channel) {
			return true
		}
	}
	return false
}

func channelHasBody(c *Client, body string) bool {
	for _, message := range c.Messages() {
		if message.Body == body {
			return true
		}
	}
	return false
}

func statusTextContains(c *Client, needle string) bool {
	needle = strings.ToLower(needle)
	for _, entry := range c.StatusLines() {
		if strings.Contains(strings.ToLower(entry.Text()), needle) {
			return true
		}
	}
	return false
}

func waitNamesEnd(c *Client, channel string) bool {
	return c.WaitUntil(func(c *Client) bool { return namesEndCount(c.Incoming(), channel) >= 1 })
}

// registerErgoAccount registers nick on Ergo via a guest NickServ REGISTER,
// then disconnects so the nick is free for SASL. Shared by the read-marker case.
func registerErgoAccount(t *testing.T, daemon DaemonInfo, nick, password string) {
	t.Helper()
	guest := Connect(t, daemon, nick, daemon.UseTLS(), Options{})
	if !guest.WaitRegistered() {
		t.Fatalf("%s: guest register: %s", daemon.Name, guest.LastError())
	}
	notices := MessageCommandCount(guest.Incoming(), "NOTICE")
	var sent bool
	guest.Do(func(ctrl *controller.Controller) {
		sent = ctrl.SendToTarget(guest.NetworkID, "NickServ", "REGISTER "+password+" live@omairc.test")
	})
	if !sent {
		t.Fatalf("%s: NickServ REGISTER", daemon.Name)
	}
	if !guest.WaitUntil(func(c *Client) bool {
		return MessageCommandCount(c.Incoming(), "NOTICE") > notices
	}) {
		t.Fatalf("%s: NickServ REGISTER reply", daemon.Name)
	}
	if guest.HasConversation(guest.NetworkID, "NickServ") {
		t.Fatalf("%s: NickServ REGISTER opened a NickServ conversation", daemon.Name)
	}
	if statusTextContains(guest, password) {
		t.Fatalf("%s: NickServ REGISTER password reached Status", daemon.Name)
	}
	guest.Do(func(ctrl *controller.Controller) {
		ctrl.Disconnect(guest.NetworkID)
	})
	if !guest.waitState(guest.NetworkID, session.StateIdle, DefaultTimeout) {
		t.Fatalf("%s: guest disconnect to Idle", daemon.Name)
	}
}
