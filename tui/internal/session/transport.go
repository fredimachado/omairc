package session

// ConnectionState is the transport lifecycle, mirroring
// IrcTransport::ConnectionState in src/irc/irctransport.h. The names and the
// transition order are significant: the session reducer treats Encrypted as
// "open and secure" and Connected as "open but plaintext".
//
// The constants are prefixed Connection* (not State*, as SessionState uses)
// so a transport state and a session state can never be confused at a call
// site; both types live in this package.
type ConnectionState int

const (
	// ConnectionIdle is the initial, never-connected state.
	ConnectionIdle ConnectionState = iota
	// ConnectionConnecting covers the TCP connect and, for TLS, the handshake.
	ConnectionConnecting
	// ConnectionConnected is an open plaintext socket.
	ConnectionConnected
	// ConnectionEncrypted is an open TLS socket.
	ConnectionEncrypted
	// ConnectionClosing is a shutdown in progress.
	ConnectionClosing
	// ConnectionDisconnected is a cleanly ended connection.
	ConnectionDisconnected
	// ConnectionFailed is an error end; Sink.Error carries the reason.
	ConnectionFailed
)

// String renders the C++ enumerator name for logs and tests.
func (s ConnectionState) String() string {
	switch s {
	case ConnectionIdle:
		return "Idle"
	case ConnectionConnecting:
		return "Connecting"
	case ConnectionConnected:
		return "Connected"
	case ConnectionEncrypted:
		return "Encrypted"
	case ConnectionClosing:
		return "Closing"
	case ConnectionDisconnected:
		return "Disconnected"
	case ConnectionFailed:
		return "Failed"
	}
	return "Unknown"
}

// Sink receives transport lifecycle and data events. It is the Go shape of the
// IrcTransport signals connected/encrypted/disconnected/bytesReceived/
// errorOccurred in src/irc/irctransport.h.
//
// Implementations must not block: NetTransport calls these from its connect and
// read goroutines, which also means a Sink may call back into the Transport
// (for example Write) without deadlocking. LoopbackTransport calls them
// synchronously on the caller's goroutine.
type Sink interface {
	// Connected fires once the socket is open. For TLS it fires before
	// Encrypted, after the TCP connect and before the handshake.
	Connected()
	// Encrypted fires once the TLS handshake finished.
	Encrypted()
	// Disconnected fires when the connection ends cleanly.
	Disconnected()
	// BytesReceived delivers raw inbound bytes, in order, without framing.
	BytesReceived(bytes []byte)
	// Error reports a transport failure. The message is presentation-ready.
	Error(message string)
}

// Transport is the duplex byte pipe the session drives. It mirrors
// IrcTransport: Connect/Write/Shutdown are commands, State reports the
// lifecycle, and SetSink installs the event receiver.
type Transport interface {
	// Connect starts an asynchronous connection to host:port. tlsEnabled
	// selects a TLS handshake before the pipe opens.
	Connect(host string, port uint16, tlsEnabled bool)
	// Write queues or sends one framed IRC line. Bytes written before the pipe
	// opens are buffered and flushed once it is ready.
	Write(frame []byte)
	// Shutdown closes the connection if open, or abandons a connect in flight.
	Shutdown()
	// State returns the current lifecycle state.
	State() ConnectionState
	// SetSink installs the event receiver. It must be set before Connect for
	// lifecycle events to be observed.
	SetSink(sink Sink)
}

// cloneBytes copies inbound and outbound payloads so callers and sinks cannot
// alias a buffer that is reused on the next read or write.
func cloneBytes(bytes []byte) []byte {
	if bytes == nil {
		return nil
	}
	out := make([]byte, len(bytes))
	copy(out, bytes)
	return out
}
