package session

import (
	"context"
	"crypto/tls"
	"errors"
	"io"
	"net"
	"strconv"
	"sync"
)

// NetTransport is the production Transport. It dials TCP with net.Dialer and,
// when requested, wraps the socket with crypto/tls. It mirrors
// QtIrcTransport in src/irc/qtirctransport.*:
//
//   - Connect is asynchronous. A goroutine performs the dial and, for TLS, the
//     handshake; Sink calls happen on that goroutine and on the read loop.
//   - Write before the pipe is open is buffered. The buffer is flushed after
//     the TCP connect for plaintext, or after the TLS handshake completes for
//     TLS. This is the "buffer-until-encrypted" ordering: a queued frame never
//     reaches the wire in cleartext when TLS was requested.
//   - For TLS, Sink.Connected fires after the TCP connect and before the
//     handshake; Sink.Encrypted fires when the handshake finishes. This matches
//     QSslSocket::connected / encrypted.
//   - TLS failures are reported through Sink.Error like any other failure and
//     are additionally flagged by TLSFailed so the session can distinguish a
//     certificate/handshake problem from a socket problem.
//   - A read end (EOF or a locally closed socket) maps to Sink.Disconnected; a
//     more specific read failure maps to ConnectionFailed and Sink.Error.
//
// The zero value is usable; NewNetTransport is the intended constructor.
type NetTransport struct {
	mu        sync.Mutex
	sink      Sink
	state     ConnectionState
	tlsOn     bool
	tlsFailed bool
	pending   [][]byte
	conn      net.Conn
	cancel    context.CancelFunc
	epoch     uint64

	dialer    *net.Dialer
	tlsConfig *tls.Config
}

// NetTransport satisfies Transport.
var _ Transport = (*NetTransport)(nil)

// NewNetTransport returns a NetTransport with net.Dialer defaults.
func NewNetTransport() *NetTransport {
	return &NetTransport{dialer: &net.Dialer{}}
}

// NewNetTransportWithTLS returns a NetTransport that clones config for each
// TLS connection (so ServerName can be filled in from the host without mutating
// the caller's value). A nil config uses the crypto/tls defaults.
func NewNetTransportWithTLS(config *tls.Config) *NetTransport {
	return &NetTransport{dialer: &net.Dialer{}, tlsConfig: config}
}

// SetSink installs the event receiver.
func (t *NetTransport) SetSink(sink Sink) {
	t.mu.Lock()
	t.sink = sink
	t.mu.Unlock()
}

// State returns the current lifecycle state.
func (t *NetTransport) State() ConnectionState {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.state
}

// TLSFailed reports whether the most recent connection failed during the TLS
// handshake rather than at the socket layer. It is reset by Connect.
func (t *NetTransport) TLSFailed() bool {
	t.mu.Lock()
	defer t.mu.Unlock()
	return t.tlsFailed
}

// Connect starts an asynchronous connect. It is a no-op while a connection is
// in flight, matching QtIrcTransport::connectToHost.
func (t *NetTransport) Connect(host string, port uint16, tlsEnabled bool) {
	t.mu.Lock()
	if t.state == ConnectionConnecting || t.state == ConnectionConnected ||
		t.state == ConnectionEncrypted || t.state == ConnectionClosing {
		t.mu.Unlock()
		return
	}
	t.pending = nil
	t.tlsOn = tlsEnabled
	t.tlsFailed = false
	t.state = ConnectionConnecting
	t.epoch++
	epoch := t.epoch
	ctx, cancel := context.WithCancel(context.Background())
	t.cancel = cancel
	dialer := t.dialer
	if dialer == nil {
		dialer = &net.Dialer{}
	}
	config := t.tlsConfig
	t.mu.Unlock()

	go t.runConnect(ctx, epoch, dialer, host, port, tlsEnabled, config)
}

// Write sends one frame, or buffers it until the pipe is ready.
func (t *NetTransport) Write(frame []byte) {
	if len(frame) == 0 {
		return
	}

	t.mu.Lock()
	if t.state == ConnectionConnected || t.state == ConnectionEncrypted {
		conn := t.conn
		epoch := t.epoch
		t.mu.Unlock()
		if conn == nil {
			return
		}
		if _, err := conn.Write(frame); err != nil {
			t.fail(epoch, err.Error(), false)
		}
		return
	}
	t.pending = append(t.pending, cloneBytes(frame))
	t.mu.Unlock()
}

// Shutdown closes the connection, or abandons a connect in flight, and emits
// Disconnected exactly once. Repeated calls are safe.
func (t *NetTransport) Shutdown() {
	t.mu.Lock()
	if t.isFinishedLocked() || t.state == ConnectionClosing {
		t.mu.Unlock()
		return
	}
	t.state = ConnectionClosing
	t.pending = nil
	t.epoch++
	cancel := t.cancel
	t.cancel = nil
	conn := t.conn
	t.conn = nil
	t.mu.Unlock()

	if cancel != nil {
		cancel()
	}
	if conn != nil {
		_ = conn.Close()
	}

	t.mu.Lock()
	if t.state == ConnectionClosing {
		t.state = ConnectionDisconnected
		sink := t.sink
		t.mu.Unlock()
		if sink != nil {
			sink.Disconnected()
		}
		return
	}
	t.mu.Unlock()
}

func (t *NetTransport) runConnect(
	ctx context.Context,
	epoch uint64,
	dialer *net.Dialer,
	host string,
	port uint16,
	tlsEnabled bool,
	config *tls.Config,
) {
	address := net.JoinHostPort(host, strconv.FormatUint(uint64(port), 10))
	raw, err := dialer.DialContext(ctx, "tcp", address)
	if err != nil {
		if ctx.Err() != nil {
			return // Shutdown abandoned the connect.
		}
		t.fail(epoch, err.Error(), false)
		return
	}

	conn := raw
	if tlsEnabled {
		tlsConfig := config
		if tlsConfig == nil {
			tlsConfig = &tls.Config{}
		} else {
			tlsConfig = tlsConfig.Clone()
		}
		if tlsConfig.ServerName == "" {
			tlsConfig.ServerName = host
		}
		tlsConn := tls.Client(raw, tlsConfig)
		if err := tlsConn.HandshakeContext(ctx); err != nil {
			_ = raw.Close()
			if ctx.Err() != nil {
				return // Shutdown abandoned the handshake.
			}
			t.fail(epoch, tlsFailureMessage(err), true)
			return
		}
		conn = tlsConn
	}

	if !t.attach(epoch, conn, tlsEnabled) {
		_ = conn.Close()
		return
	}
	t.readLoop(epoch, conn)
}

// tlsFailureMessage renders a TLS handshake failure. Certificate verification
// failures carry Qt's "TLS certificate error: " prefix; other handshake errors
// are passed through verbatim.
func tlsFailureMessage(err error) string {
	var verification *tls.CertificateVerificationError
	if errors.As(err, &verification) {
		return "TLS certificate error: " + verification.Error()
	}
	return err.Error()
}

// attach publishes the open socket and emits Connected, then, for TLS,
// Encrypted. It reports false when Shutdown or a newer Connect superseded this
// attempt. Pending writes flush after the pipe is genuinely open.
func (t *NetTransport) attach(epoch uint64, conn net.Conn, tlsEnabled bool) bool {
	t.mu.Lock()
	if t.epoch != epoch || t.state != ConnectionConnecting {
		t.mu.Unlock()
		return false
	}
	t.conn = conn
	t.state = ConnectionConnected
	sink := t.sink
	t.mu.Unlock()

	if sink != nil {
		sink.Connected()
	}

	if !tlsEnabled {
		t.flushPending(epoch)
		return true
	}

	t.mu.Lock()
	if t.epoch != epoch || t.state != ConnectionConnected {
		t.mu.Unlock()
		return false
	}
	t.state = ConnectionEncrypted
	sink = t.sink
	t.mu.Unlock()

	if sink != nil {
		sink.Encrypted()
	}
	t.flushPending(epoch)
	return true
}

func (t *NetTransport) flushPending(epoch uint64) {
	t.mu.Lock()
	if t.epoch != epoch || !t.isOpenLocked() {
		t.mu.Unlock()
		return
	}
	pending := t.pending
	t.pending = nil
	conn := t.conn
	t.mu.Unlock()

	for _, frame := range pending {
		if _, err := conn.Write(frame); err != nil {
			t.fail(epoch, err.Error(), false)
			return
		}
	}
}

func (t *NetTransport) readLoop(epoch uint64, conn net.Conn) {
	buffer := make([]byte, 4096)
	for {
		count, err := conn.Read(buffer)
		if count > 0 {
			t.deliver(epoch, buffer[:count])
		}
		if err != nil {
			t.handleReadError(epoch, err)
			return
		}
	}
}

func (t *NetTransport) deliver(epoch uint64, bytes []byte) {
	t.mu.Lock()
	if t.epoch != epoch {
		t.mu.Unlock()
		return
	}
	sink := t.sink
	t.mu.Unlock()
	if sink != nil {
		sink.BytesReceived(cloneBytes(bytes))
	}
}

func (t *NetTransport) handleReadError(epoch uint64, err error) {
	t.mu.Lock()
	if t.epoch != epoch || t.state == ConnectionClosing || t.state == ConnectionFailed ||
		t.state == ConnectionDisconnected || t.state == ConnectionIdle {
		t.mu.Unlock()
		return
	}
	t.conn = nil
	t.pending = nil

	if errors.Is(err, io.EOF) || errors.Is(err, net.ErrClosed) {
		t.state = ConnectionDisconnected
		sink := t.sink
		t.mu.Unlock()
		if sink != nil {
			sink.Disconnected()
		}
		return
	}

	t.state = ConnectionFailed
	sink := t.sink
	t.mu.Unlock()
	if sink != nil {
		sink.Error(err.Error())
	}
}

// fail marks the connection failed and emits Error once. A TLS failure also
// raises TLSFailed. It mirrors QtIrcTransport::fail.
func (t *NetTransport) fail(epoch uint64, message string, tlsFailure bool) {
	t.mu.Lock()
	if t.epoch != epoch || t.state == ConnectionFailed {
		t.mu.Unlock()
		return
	}
	t.state = ConnectionFailed
	if tlsFailure {
		t.tlsFailed = true
	}
	t.pending = nil
	conn := t.conn
	t.conn = nil
	cancel := t.cancel
	t.cancel = nil
	sink := t.sink
	t.mu.Unlock()

	if cancel != nil {
		cancel()
	}
	if conn != nil {
		_ = conn.Close()
	}
	if sink != nil {
		sink.Error(message)
	}
}

func (t *NetTransport) isOpenLocked() bool {
	return t.state == ConnectionConnected || t.state == ConnectionEncrypted
}

func (t *NetTransport) isFinishedLocked() bool {
	return t.state == ConnectionIdle || t.state == ConnectionDisconnected || t.state == ConnectionFailed
}
