package demo

import (
	"bytes"

	"github.com/fredimachado/omairc/tui/internal/controller"
	"github.com/fredimachado/omairc/tui/internal/session"
)

// DemoServer is the Go port of IrcDemoServer. It owns the two loopback
// transports it seeds, the per-network MONITOR watch lists, and the last
// attach error.
type DemoServer struct {
	omarchy *session.LoopbackTransport
	oftc    *session.LoopbackTransport

	// monitorLists holds each network's watched nicks, keyed by network id. It
	// mirrors the C++ QHash<IrcLoopbackTransport *, QStringList> keyed by the
	// transport pointer; the network id is the stable equivalent.
	monitorLists map[string][]string
	err          string
}

// New returns an empty demo server. Call Attach to seed a controller.
func New() *DemoServer {
	return &DemoServer{monitorLists: make(map[string][]string)}
}

// OmarchyNetworkID returns the primary demo network id.
func (d *DemoServer) OmarchyNetworkID() string { return "omarchy" }

// OftcNetworkID returns the secondary demo network id.
func (d *DemoServer) OftcNetworkID() string { return "oftc" }

// OmarchyTransport returns the primary network's loopback transport, or nil
// before Attach.
func (d *DemoServer) OmarchyTransport() *session.LoopbackTransport { return d.omarchy }

// OftcTransport returns the secondary network's loopback transport, or nil
// before Attach.
func (d *DemoServer) OftcTransport() *session.LoopbackTransport { return d.oftc }

// LastError returns the last attach failure, or "".
func (d *DemoServer) LastError() string { return d.err }

// InjectOmarchy delivers inbound bytes on the primary network.
func (d *DemoServer) InjectOmarchy(payload []byte) {
	if d.omarchy != nil {
		d.omarchy.InjectBytes(payload)
	}
}

// InjectOftc delivers inbound bytes on the secondary network.
func (d *DemoServer) InjectOftc(payload []byte) {
	if d.oftc != nil {
		d.oftc.InjectBytes(payload)
	}
}

// EchoLastOmarchyPrivmsg replays the last client PRIVMSG on the primary
// network as a self echo. It reports false when there is no such frame.
func (d *DemoServer) EchoLastOmarchyPrivmsg(nick string) bool {
	return d.echoLastPrivmsg("omarchy", nick)
}

// EchoLastOftcPrivmsg replays the last client PRIVMSG on the secondary
// network as a self echo.
func (d *DemoServer) EchoLastOftcPrivmsg(nick string) bool {
	return d.echoLastPrivmsg("oftc", nick)
}

// Attach seeds controller with the two demo networks in the exact C++ order:
// start both sessions, register both, inject the world, select #omarchy, replay
// the transcripts, mark the seeded reads, then inject typing and the live
// account change. When autoEcho is set it also answers client traffic. It
// mirrors IrcDemoServer::attach.
func (d *DemoServer) Attach(c *controller.Controller, autoEcho bool) bool {
	omarchy := omarchyWorld()
	oftc := oftcWorld()

	omarchyTransport, ok := d.startNetwork(c, omarchy.NetworkID, omarchy.Nick, autojoinNames(omarchy))
	if !ok {
		return false
	}
	oftcTransport, ok := d.startNetwork(c, oftc.NetworkID, oftc.Nick, autojoinNames(oftc))
	if !ok {
		return false
	}
	d.omarchy = omarchyTransport
	d.oftc = oftcTransport

	c.SetNetworkOrder([]string{omarchy.NetworkID, oftc.NetworkID})

	d.omarchy.InjectBytes(registrationBytes(omarchy.Nick, omarchy.Welcome, omarchy.IconURL))
	d.oftc.InjectBytes(registrationBytes(oftc.Nick, oftc.Welcome, ""))
	injectWorld(d.omarchy, omarchy)
	injectWorld(d.oftc, oftc)

	c.SelectConversation(omarchy.NetworkID, "#omarchy")
	injectTranscript(d.omarchy, omarchy)
	injectTranscript(d.oftc, oftc)
	markRead(c, omarchy)
	markRead(c, oftc)
	c.SelectConversation(omarchy.NetworkID, "#omarchy")
	d.omarchy.InjectBytes(typingBytes(omarchy))
	d.omarchy.InjectBytes(liveAccountBytes(omarchy))

	if autoEcho {
		d.omarchy.OnFrameWritten = d.hookAutoEcho(omarchy.NetworkID, omarchy.Nick, demoOnlineNicks(omarchy))
		d.oftc.OnFrameWritten = d.hookAutoEcho(oftc.NetworkID, oftc.Nick, demoOnlineNicks(oftc))
	}
	return true
}

// startNetwork creates and starts one loopback session and completes its
// connect. It mirrors IrcDemoServer::startNetwork.
func (d *DemoServer) startNetwork(c *controller.Controller, networkID, nick string, autojoin []string) (*session.LoopbackTransport, bool) {
	config := session.DefaultSessionConfig(networkID, networkID, "irc.example", nick)
	config.TLSEnabled = true
	config.AutojoinChannels = autojoin
	config.ReconnectEnabled = false

	transport := session.NewLoopbackTransport()
	if _, err := c.AddSession(config, transport, nil); err != nil {
		d.err = "addSession " + networkID
		return nil, false
	}
	if !c.Start(networkID) {
		d.err = "start " + networkID
		return nil, false
	}
	transport.CompleteConnect()
	return transport, true
}

// hookAutoEcho returns the frameWritten handler: answer ping, list, ctcp, away,
// metadata, and monitor in order, then echo PRIVMSG and NOTICE back to the
// client. Because tryAnswerMonitor uses a non-nil empty slice for a handled
// no-op, the ladder tests `reply != nil`, not `len(reply) > 0`. It mirrors
// IrcDemoServer::hookAutoEcho.
func (d *DemoServer) hookAutoEcho(networkID, nick string, online []string) func([]byte) {
	seed := seedFor(networkID)
	return func(frame []byte) {
		if reply := d.tryAnswerPing(frame); reply != nil {
			d.inject(networkID, reply)
			return
		}
		if reply := d.tryAnswerList(seed, frame); reply != nil {
			d.inject(networkID, reply)
			return
		}
		if reply := d.tryAnswerCtcp(nick, frame); reply != nil {
			d.inject(networkID, reply)
			return
		}
		if reply := d.tryAnswerAway(nick, frame); reply != nil {
			d.inject(networkID, reply)
			return
		}
		if reply := d.tryAnswerMetadata(nick, frame); reply != nil {
			d.inject(networkID, reply)
			return
		}
		if reply := d.tryAnswerMonitor(networkID, nick, frame, online); reply != nil {
			d.inject(networkID, reply)
			return
		}
		if bytes.HasPrefix(frame, []byte("PRIVMSG ")) || bytes.HasPrefix(frame, []byte("NOTICE ")) {
			d.inject(networkID, injectClientEcho(nick, frame))
		}
	}
}

// inject delivers reply bytes on one network. An empty payload is a handled
// no-op, so it is dropped here rather than looping back into the transport.
func (d *DemoServer) inject(networkID string, payload []byte) {
	if len(payload) == 0 {
		return
	}
	if transport := d.transportFor(networkID); transport != nil {
		transport.InjectBytes(payload)
	}
}

func (d *DemoServer) transportFor(networkID string) *session.LoopbackTransport {
	switch networkID {
	case "omarchy":
		return d.omarchy
	case "oftc":
		return d.oftc
	}
	return nil
}

// seedFor resolves a network id to its seed world, the Go equivalent of the
// C++ hook's nick comparison.
func seedFor(networkID string) SeedNetwork {
	if networkID == "oftc" {
		return oftcWorld()
	}
	return omarchyWorld()
}
