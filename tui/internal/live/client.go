//go:build live

// Package live is the live-ircd harness for the Go client: the OMAIRC_LIVE_*
// environment bin/test-live exports, a controller over the real
// session.NetTransport, and wait helpers. It mirrors `client/src/live/`. The
// cases live beside it in *_test.go. Every file carries the `live` build tag,
// so `go test ./...` and tui/bin/test stay offline; bin/test-live runs
// `go test -tags live ./internal/live/`.
package live

import (
	"crypto/tls"
	"crypto/x509"
	"fmt"
	"os"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/irc"
	"github.com/fredimachado/omairc/tui/internal/session"
)

var (
	nickSeq    atomic.Uint32
	channelSeq atomic.Uint32
	networkSeq atomic.Uint32
	fenceSeq   atomic.Uint32
)

// UniqueNick returns a nick within the daemon's NICKLEN budget. The `g` lead
// keeps it apart from the Rust suite's `o` nicks on the same fixtures.
func UniqueNick(maxLength int) string {
	serial := nickSeq.Add(1) - 1
	pid := uint32(os.Getpid() % 4096)
	nick := fmt.Sprintf("g%04x", (serial+pid)&0xffff)
	if maxLength > 0 && len(nick) > maxLength {
		nick = nick[:maxLength]
	}
	return nick
}

// UniqueChannel returns a channel name no other live run uses.
func UniqueChannel() string {
	serial := channelSeq.Add(1) - 1
	return fmt.Sprintf("#g%03x%05x", os.Getpid()%4096, serial&0xfffff)
}

// Options are the optional knobs for Connect.
type Options struct {
	Password         string
	NetworkID        string
	ReconnectEnabled bool
	AutojoinChannels []string
}

// recorder sits between a session and the controller and keeps every inbound
// message, like `Session::enable_received_log` in the Rust harness.
type recorder struct {
	session.Handler
	mu       sync.Mutex
	received []irc.Message
}

func (r *recorder) MessageReceived(networkID string, message irc.Message) {
	r.mu.Lock()
	r.received = append(r.received, message)
	r.mu.Unlock()
	r.Handler.MessageReceived(networkID, message)
}

func (r *recorder) messages() []irc.Message {
	r.mu.Lock()
	defer r.mu.Unlock()
	out := make([]irc.Message, len(r.received))
	copy(out, r.received)
	return out
}

// recordingTransport keeps every line its session wrote, in Write-call order.
// That matches wire order for writes from one Client.Do turn or the session's
// own drain; interleaving between concurrent goroutines is unspecified, as it
// is on the wire. The recorder mutex is not held across the inner Write:
// NetTransport.Write can call back into the session on error and re-enter Write.
type recordingTransport struct {
	session.Transport
	mu    sync.Mutex
	lines []string
}

func (r *recordingTransport) Write(frame []byte) {
	line := strings.TrimRight(string(frame), "\r\n")
	r.mu.Lock()
	r.lines = append(r.lines, line)
	r.mu.Unlock()
	r.Transport.Write(frame)
}

func (r *recordingTransport) sent() []string {
	r.mu.Lock()
	defer r.mu.Unlock()
	out := make([]string, len(r.lines))
	copy(out, r.lines)
	return out
}

// Client is one controller with one or more live sessions. Every controller
// read and write goes through Do, which holds the controller lock the way the
// shell's Update does; session goroutines take the same lock themselves.
type Client struct {
	t          testing.TB
	Controller *controller.Controller
	NetworkID  string
	networks   []string
	recorders  map[string]*recorder
	transports map[string]*recordingTransport
	tlsConfig  *tls.Config
}

// Connect dials daemon as nick (TLS when tls is set or the daemon has no plain
// port) and registers a cleanup that quits every session.
func Connect(t testing.TB, daemon DaemonInfo, nick string, useTLS bool, options Options) *Client {
	t.Helper()
	pem, err := CertPEM()
	if err != nil {
		t.Fatalf("read live.pem: %v", err)
	}
	pool := x509.NewCertPool()
	if !pool.AppendCertsFromPEM(pem) {
		t.Fatalf("live.pem has no certificate")
	}
	c := &Client{
		t:          t,
		Controller: controller.New(),
		recorders:  make(map[string]*recorder),
		transports: make(map[string]*recordingTransport),
		tlsConfig:  &tls.Config{RootCAs: pool, MinVersion: tls.VersionTLS12},
	}
	t.Cleanup(c.close)
	c.NetworkID = c.start(daemon, nick, useTLS, options)
	return c
}

// ConnectAdditional dials another daemon on this controller and returns the
// new network id.
func (c *Client) ConnectAdditional(daemon DaemonInfo, nick string, useTLS bool, options Options) string {
	c.t.Helper()
	return c.start(daemon, nick, useTLS, options)
}

func (c *Client) start(daemon DaemonInfo, nick string, useTLS bool, options Options) string {
	c.t.Helper()
	useTLS = useTLS || daemon.UseTLS()
	port := daemon.Port(useTLS)
	if port == 0 {
		c.t.Fatalf("%s has no port for tls=%v", daemon.Name, useTLS)
	}
	networkID := options.NetworkID
	if networkID == "" {
		networkID = fmt.Sprintf("live-%d-%d", time.Now().UnixNano(), networkSeq.Add(1))
	}
	config := session.DefaultSessionConfig(networkID, daemon.Name, Host(), nick)
	config.Port = port
	config.TLSEnabled = useTLS
	config.Username = "omairc"
	config.Realname = "Omairc live"
	config.Password = options.Password
	config.ReconnectEnabled = options.ReconnectEnabled
	config.AutojoinChannels = options.AutojoinChannels
	config.CapabilityTimeoutMs = 8000

	transport := &recordingTransport{Transport: session.NewNetTransportWithTLS(c.tlsConfig)}
	var startErr error
	c.Do(func(ctrl *controller.Controller) {
		s, err := ctrl.AddSession(config, transport, nil)
		if err != nil {
			startErr = err
			return
		}
		rec := &recorder{Handler: ctrl}
		s.SetHandler(rec)
		c.recorders[networkID] = rec
		c.transports[networkID] = transport
		c.networks = append(c.networks, networkID)
		ctrl.Start(networkID)
	})
	if startErr != nil {
		c.t.Fatalf("AddSession: %v", startErr)
	}
	return networkID
}

func (c *Client) close() {
	c.Do(func(ctrl *controller.Controller) {
		for index := len(c.networks) - 1; index >= 0; index-- {
			ctrl.Disconnect(c.networks[index])
		}
	})
	WaitUntil(func() bool {
		for _, networkID := range c.networks {
			switch c.StateOf(networkID) {
			case session.StateIdle, session.StateFailed:
			default:
				return false
			}
		}
		return true
	}, 3*time.Second)
	c.Controller.Shutdown()
}

// Do runs fn with the controller lock held.
func (c *Client) Do(fn func(ctrl *controller.Controller)) {
	c.Controller.Lock()
	defer c.Controller.Unlock()
	fn(c.Controller)
}

// Session returns the primary network's session.
func (c *Client) Session() *session.Session { return c.SessionOf(c.NetworkID) }

// SessionOf returns one network's session.
func (c *Client) SessionOf(networkID string) *session.Session {
	var s *session.Session
	c.Do(func(ctrl *controller.Controller) { s = ctrl.Session(networkID) })
	if s == nil {
		c.t.Fatalf("no live session %s", networkID)
	}
	return s
}

// Incoming returns every message the primary session received so far.
func (c *Client) Incoming() []irc.Message { return c.recorders[c.NetworkID].messages() }

// Nick returns the primary session's current nick.
func (c *Client) Nick() string { return c.Session().Nick() }

// State returns the primary session's state.
func (c *Client) State() session.SessionState { return c.StateOf(c.NetworkID) }

// StateOf returns one network's session state.
func (c *Client) StateOf(networkID string) session.SessionState {
	return c.SessionOf(networkID).State()
}

// Capabilities returns the primary session's negotiated capabilities.
func (c *Client) Capabilities() irc.CapabilitySet { return c.Session().Capabilities() }

// Features returns the primary network's ISUPPORT view.
func (c *Client) Features() irc.ServerFeatures {
	var features irc.ServerFeatures
	c.Do(func(ctrl *controller.Controller) {
		features = ctrl.Reducer().ServerFeatures(c.NetworkID)
	})
	return features
}

// StatusLines returns the primary network's Status entries.
func (c *Client) StatusLines() []irc.StatusEntry { return c.StatusLinesOf(c.NetworkID) }

// StatusLinesOf returns one network's Status entries.
func (c *Client) StatusLinesOf(networkID string) []irc.StatusEntry {
	var lines []irc.StatusEntry
	c.Do(func(ctrl *controller.Controller) { lines = ctrl.Console().Lines(networkID) })
	return lines
}

// HasServerLabel reports whether a server Status line carries label.
func (c *Client) HasServerLabel(label string) bool {
	for _, entry := range c.StatusLines() {
		if entry.Label() == label && entry.Source() == irc.LogSourceServer {
			return true
		}
	}
	return false
}

// LastError returns the primary network's last error, or "no error".
func (c *Client) LastError() string {
	var text string
	c.Do(func(ctrl *controller.Controller) { text = ctrl.LastErrorFor(c.NetworkID) })
	if text == "" {
		return "no error"
	}
	return text
}

// WaitUntil polls predicate for DefaultTimeout.
func (c *Client) WaitUntil(predicate func(*Client) bool) bool {
	return WaitUntil(func() bool { return predicate(c) }, DefaultTimeout)
}

func (c *Client) waitState(networkID string, state session.SessionState, timeout time.Duration) bool {
	return WaitUntil(func() bool { return c.StateOf(networkID) == state }, timeout)
}

// WaitRegistered waits for the primary session to register.
func (c *Client) WaitRegistered() bool { return c.WaitRegisteredNetwork(c.NetworkID) }

// WaitRegisteredNetwork waits for one network's session to register.
func (c *Client) WaitRegisteredNetwork(networkID string) bool {
	return c.waitState(networkID, session.StateRegistered, DefaultTimeout)
}

// WaitFailed waits for the primary session to fail.
func (c *Client) WaitFailed() bool {
	return c.waitState(c.NetworkID, session.StateFailed, DefaultTimeout)
}

// WaitReconnecting waits for the primary session to enter reconnecting.
func (c *Client) WaitReconnecting() bool {
	return c.waitState(c.NetworkID, session.StateReconnecting, DefaultTimeout)
}

// WaitRegisteredAgain waits for the primary session to register again after a
// forced disconnect, matching Rust wait_registered_again's 40s budget.
func (c *Client) WaitRegisteredAgain() bool {
	return c.waitState(c.NetworkID, session.StateRegistered, 40*time.Second)
}

// Sent returns every outbound line the primary network wrote, CRLF trimmed.
// Order matches recordingTransport: Write-call order, wire-ordered within one
// Do turn or session drain; concurrent interleaving is unspecified.
func (c *Client) Sent() []string {
	var transport *recordingTransport
	c.Do(func(*controller.Controller) {
		transport = c.transports[c.NetworkID]
	})
	if transport == nil {
		return nil
	}
	return transport.sent()
}

// Fence writes PING :<token>-<serial> and waits for that wire token's PONG.
// The serial is per-process so an earlier pending PONG for token cannot
// satisfy this call.
func (c *Client) Fence(token string) bool {
	wire := fmt.Sprintf("%s-%d", token, fenceSeq.Add(1)-1)
	before := len(c.Incoming())
	if !c.Session().SendRaw("PING :" + wire) {
		return false
	}
	return c.WaitUntil(func(client *Client) bool {
		incoming := client.Incoming()
		if before > len(incoming) {
			return false
		}
		for _, message := range incoming[before:] {
			if message.Command != "PONG" || len(message.Params) == 0 {
				continue
			}
			if message.Params[len(message.Params)-1] == wire {
				return true
			}
		}
		return false
	})
}

// ForceDisconnect shuts the primary transport down while Registered, so the
// session takes the unexpected-drop reconnect path. Call it outside Do.
func (c *Client) ForceDisconnect() bool {
	if c.State() != session.StateRegistered {
		return false
	}
	var transport *recordingTransport
	c.Do(func(*controller.Controller) {
		transport = c.transports[c.NetworkID]
	})
	if transport == nil {
		return false
	}
	transport.Shutdown()
	return true
}

// WaitMotd waits for the end of MOTD (376) or its absence (422).
func (c *Client) WaitMotd() bool {
	return c.WaitUntil(func(c *Client) bool {
		return c.HasServerLabel("376") || c.HasServerLabel("422")
	})
}

// WaitServerLabel waits for a server Status line with label.
func (c *Client) WaitServerLabel(label string) bool {
	return c.WaitUntil(func(c *Client) bool { return c.HasServerLabel(label) })
}

// WaitCommand waits for an inbound message carrying command.
func (c *Client) WaitCommand(command string) bool {
	return c.WaitUntil(func(c *Client) bool { return MessageHasCommand(c.Incoming(), command) })
}

// JoinChannel sends /join from the network's Status surface, the way a user
// joins before any conversation is open. It reports false when the session is
// not registered.
func (c *Client) JoinChannel(channel string) bool {
	if c.State() != session.StateRegistered {
		return false
	}
	c.Do(func(ctrl *controller.Controller) {
		ctrl.OpenStatus(c.NetworkID)
		ctrl.ConsoleSubmit("/join " + channel)
	})
	return true
}

// SelectChannel selects target on the primary network.
func (c *Client) SelectChannel(target string) {
	c.Do(func(ctrl *controller.Controller) { ctrl.SelectConversation(c.NetworkID, target) })
}

// Submit sends one composer line on the conversation surface.
func (c *Client) Submit(text string) bool {
	var sent bool
	c.Do(func(ctrl *controller.Controller) { sent = ctrl.SendMessage(text) })
	return sent
}

// SelectedTarget returns the selected conversation's target.
func (c *Client) SelectedTarget() string {
	var target string
	c.Do(func(ctrl *controller.Controller) { target = ctrl.SelectedTarget() })
	return target
}

// Messages returns the selected conversation's transcript rows.
func (c *Client) Messages() []controller.MessageSnapshot {
	var rows []controller.MessageSnapshot
	c.Do(func(ctrl *controller.Controller) {
		rows = append(rows, ctrl.Messages()...)
	})
	return rows
}

// Topic returns the selected conversation's topic.
func (c *Client) Topic() string {
	var topic string
	c.Do(func(ctrl *controller.Controller) { topic = ctrl.Topic() })
	return topic
}

// Member returns the selected channel's member row for nick.
func (c *Client) Member(nick string) (controller.MemberSnapshot, bool) {
	var found controller.MemberSnapshot
	var ok bool
	c.Do(func(ctrl *controller.Controller) {
		for _, member := range ctrl.Members() {
			if strings.EqualFold(member.Nick, nick) {
				found, ok = member, true
				return
			}
		}
	})
	return found, ok
}

// HasConversation reports whether the sidebar has a row for target on
// networkID.
func (c *Client) HasConversation(networkID, target string) bool {
	var found bool
	c.Do(func(ctrl *controller.Controller) {
		for _, row := range ctrl.Conversations() {
			if row.NetworkID == networkID && strings.EqualFold(row.Conversation, target) {
				found = true
				return
			}
		}
	})
	return found
}
