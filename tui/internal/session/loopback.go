package session

import "sync"

// LoopbackTransport is the deterministic in-memory Transport. It mirrors
// IrcLoopbackTransport in src/irc/ircloopbacktransport.* and the
// FakeIrcTransport used by tests/session/tst_transport.cpp: nothing happens on
// its own, and the test drives every transition through the explicit control
// methods below. All Sink calls are synchronous on the caller's goroutine, so
// tests need no sleeps and no polling.
type LoopbackTransport struct {
	mu        sync.Mutex
	sink      Sink
	state     ConnectionState
	tlsOn     bool
	host      string
	port      uint16
	written   [][]byte
	lastError string

	// OnFrameWritten mirrors the IrcLoopbackTransport::frameWritten signal: it
	// fires after every Write, with the recorded copy of the frame. It is
	// invoked outside the mutex because a handler typically calls back into
	// InjectBytes (which takes the mutex). A nil hook means no behavior.
	OnFrameWritten func(frame []byte)
}

// LoopbackTransport satisfies Transport.
var _ Transport = (*LoopbackTransport)(nil)

// NewLoopbackTransport returns an idle loopback transport.
func NewLoopbackTransport() *LoopbackTransport {
	return &LoopbackTransport{}
}

// SetSink installs the event receiver.
func (t *LoopbackTransport) SetSink(sink Sink) {
	t.mu.Lock()
	t.sink = sink
	t.mu.Unlock()
}

// Connect records the request and enters Connecting without any I/O.
func (t *LoopbackTransport) Connect(host string, port uint16, tlsEnabled bool) {
	t.mu.Lock()
	defer t.mu.Unlock()
	if t.state == ConnectionConnecting || t.state == ConnectionConnected ||
		t.state == ConnectionEncrypted || t.state == ConnectionClosing {
		return
	}
	t.host = host
	t.port = port
	t.tlsOn = tlsEnabled
	t.lastError = ""
	t.state = ConnectionConnecting
}

// Write records the frame and, when installed, invokes OnFrameWritten with the
// recorded copy. It does not require an open connection, matching
// IrcLoopbackTransport::write and its frameWritten signal.
func (t *LoopbackTransport) Write(frame []byte) {
	copied := cloneBytes(frame)
	if copied == nil {
		copied = []byte{}
	}
	t.mu.Lock()
	t.written = append(t.written, copied)
	hook := t.OnFrameWritten
	t.mu.Unlock()
	if hook != nil {
		hook(copied)
	}
}

// Shutdown ends the connection synchronously and emits Disconnected once.
func (t *LoopbackTransport) Shutdown() {
	t.mu.Lock()
	if t.isFinishedLocked() || t.state == ConnectionClosing {
		t.mu.Unlock()
		return
	}
	// QtIrcLoopbackTransport passes through Closing here, but it sets
	// Disconnected in the same synchronous step before emitting, so no observer
	// can see Closing.
	t.state = ConnectionDisconnected
	sink := t.sink
	t.mu.Unlock()
	if sink != nil {
		sink.Disconnected()
	}
}

// State returns the current lifecycle state.
func (t *LoopbackTransport) State() ConnectionState {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.state
}

// CompleteConnect fulfills a pending Connect: it emits Connected and, when TLS
// was requested, Encrypted immediately after.
func (t *LoopbackTransport) CompleteConnect() {
	t.mu.Lock()
	if t.state != ConnectionConnecting {
		t.mu.Unlock()
		return
	}
	t.state = ConnectionConnected
	tlsOn := t.tlsOn
	sink := t.sink
	t.mu.Unlock()

	if sink != nil {
		sink.Connected()
	}
	if !tlsOn {
		return
	}

	t.mu.Lock()
	t.state = ConnectionEncrypted
	sink = t.sink
	t.mu.Unlock()
	if sink != nil {
		sink.Encrypted()
	}
}

// FailConnect fails a connection that is still connecting.
func (t *LoopbackTransport) FailConnect(message string) {
	t.mu.Lock()
	if t.state != ConnectionConnecting {
		t.mu.Unlock()
		return
	}
	sink, notify := t.failLocked(message)
	t.mu.Unlock()
	if notify && sink != nil {
		sink.Error(message)
	}
}

// FailTLS fails a connection during the TLS phase, while connecting or after
// the plaintext socket opened but before encryption.
func (t *LoopbackTransport) FailTLS(message string) {
	t.mu.Lock()
	if t.state != ConnectionConnecting && t.state != ConnectionConnected {
		t.mu.Unlock()
		return
	}
	sink, notify := t.failLocked(message)
	t.mu.Unlock()
	if notify && sink != nil {
		sink.Error(message)
	}
}

// TimeoutConnect fails a pending connection with the timeout message.
func (t *LoopbackTransport) TimeoutConnect() {
	t.mu.Lock()
	if t.state != ConnectionConnecting {
		t.mu.Unlock()
		return
	}
	sink, notify := t.failLocked("Connection timed out")
	t.mu.Unlock()
	if notify && sink != nil {
		sink.Error("Connection timed out")
	}
}

// InjectBytes delivers inbound bytes to the sink while the connection is open.
func (t *LoopbackTransport) InjectBytes(bytes []byte) {
	if len(bytes) == 0 {
		return
	}
	t.mu.Lock()
	if !t.isOpenLocked() {
		t.mu.Unlock()
		return
	}
	sink := t.sink
	t.mu.Unlock()
	if sink != nil {
		sink.BytesReceived(cloneBytes(bytes))
	}
}

// RemoteClose ends an open connection as if the peer hung up. It emits
// Disconnected, not Error.
func (t *LoopbackTransport) RemoteClose() {
	t.mu.Lock()
	if !t.isOpenLocked() {
		t.mu.Unlock()
		return
	}
	t.state = ConnectionDisconnected
	sink := t.sink
	t.mu.Unlock()
	if sink != nil {
		sink.Disconnected()
	}
}

// WrittenFrames returns a copy of every frame passed to Write, in order.
func (t *LoopbackTransport) WrittenFrames() [][]byte {
	t.mu.Lock()
	defer t.mu.Unlock()
	out := make([][]byte, len(t.written))
	for index, frame := range t.written {
		out[index] = cloneBytes(frame)
	}
	return out
}

// LastError returns the message recorded by the most recent failure.
func (t *LoopbackTransport) LastError() string {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.lastError
}

// ConnectedHost returns the host from the most recent Connect.
func (t *LoopbackTransport) ConnectedHost() string {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.host
}

// ConnectedPort returns the port from the most recent Connect.
func (t *LoopbackTransport) ConnectedPort() uint16 {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.port
}

// TLSRequested reports whether the most recent Connect asked for TLS.
func (t *LoopbackTransport) TLSRequested() bool {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.tlsOn
}

// failLocked assumes t.mu is held. It records the failure unless the transport
// already failed, and returns the sink to notify.
func (t *LoopbackTransport) failLocked(message string) (Sink, bool) {
	if t.state == ConnectionFailed {
		return nil, false
	}
	t.lastError = message
	t.state = ConnectionFailed
	return t.sink, true
}

func (t *LoopbackTransport) isOpenLocked() bool {
	return t.state == ConnectionConnected || t.state == ConnectionEncrypted
}

func (t *LoopbackTransport) isFinishedLocked() bool {
	return t.state == ConnectionIdle || t.state == ConnectionDisconnected || t.state == ConnectionFailed
}
