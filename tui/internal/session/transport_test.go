package session

import (
	"bytes"
	"strings"
	"testing"
)

// recordingSink is a Sink that records the lifecycle and data events the
// C++ QSignalSpy collected in tst_transport.cpp.
type recordingSink struct {
	connectedCount    int
	encryptedCount    int
	disconnectedCount int
	received          [][]byte
	errors            []string
}

var _ Sink = (*recordingSink)(nil)

func (s *recordingSink) Connected() { s.connectedCount++ }
func (s *recordingSink) Encrypted() { s.encryptedCount++ }
func (s *recordingSink) Disconnected() {
	s.disconnectedCount++
}
func (s *recordingSink) BytesReceived(data []byte) {
	s.received = append(s.received, cloneBytes(data))
}
func (s *recordingSink) Error(message string) {
	s.errors = append(s.errors, message)
}

// TestTransportSucceedsConnect ports TransportTest::succeedsConnect.
func TestTransportSucceedsConnect(t *testing.T) {
	transport := NewLoopbackTransport()
	sink := &recordingSink{}
	transport.SetSink(sink)

	if got := transport.State(); got != ConnectionIdle {
		t.Fatalf("state = %v, want Idle", got)
	}
	transport.Connect("irc.example", 6667, false)
	if got := transport.State(); got != ConnectionConnecting {
		t.Fatalf("state = %v, want Connecting", got)
	}
	if got := transport.ConnectedHost(); got != "irc.example" {
		t.Fatalf("host = %q, want irc.example", got)
	}
	if got := transport.ConnectedPort(); got != 6667 {
		t.Fatalf("port = %d, want 6667", got)
	}
	if transport.TLSRequested() {
		t.Fatal("tlsRequested must be false")
	}

	transport.CompleteConnect()
	if got := transport.State(); got != ConnectionConnected {
		t.Fatalf("state = %v, want Connected", got)
	}
	if sink.connectedCount != 1 {
		t.Fatalf("connected = %d, want 1", sink.connectedCount)
	}
	if sink.encryptedCount != 0 {
		t.Fatalf("encrypted = %d, want 0", sink.encryptedCount)
	}
}

// TestTransportRecordsWrites ports TransportTest::recordsWrites.
func TestTransportRecordsWrites(t *testing.T) {
	transport := NewLoopbackTransport()
	transport.Connect("irc.example", 6667, false)
	transport.CompleteConnect()

	ping := []byte("PING :x\r\n")
	nick := []byte("NICK omairc\r\n")
	transport.Write(ping)
	transport.Write(nick)

	frames := transport.WrittenFrames()
	if len(frames) != 2 {
		t.Fatalf("writtenFrames = %d, want 2", len(frames))
	}
	if !bytes.Equal(frames[0], ping) {
		t.Fatalf("frame 0 = %q, want %q", frames[0], ping)
	}
	if !bytes.Equal(frames[1], nick) {
		t.Fatalf("frame 1 = %q, want %q", frames[1], nick)
	}
}

// TestTransportInjectsInboundBytes ports
// TransportTest::injectsInboundBytes.
func TestTransportInjectsInboundBytes(t *testing.T) {
	transport := NewLoopbackTransport()
	sink := &recordingSink{}
	transport.SetSink(sink)
	transport.Connect("irc.example", 6667, false)
	transport.CompleteConnect()

	pong := []byte("PONG :x\r\n")
	transport.InjectBytes(pong)

	if len(sink.received) != 1 {
		t.Fatalf("received = %d, want 1", len(sink.received))
	}
	if !bytes.Equal(sink.received[0], pong) {
		t.Fatalf("received[0] = %q, want %q", sink.received[0], pong)
	}
}

// TestTransportRemoteCloseEndsConnection ports
// TransportTest::remoteCloseEndsConnection.
func TestTransportRemoteCloseEndsConnection(t *testing.T) {
	transport := NewLoopbackTransport()
	sink := &recordingSink{}
	transport.SetSink(sink)
	transport.Connect("irc.example", 6667, false)
	transport.CompleteConnect()

	transport.RemoteClose()

	if got := transport.State(); got != ConnectionDisconnected {
		t.Fatalf("state = %v, want Disconnected", got)
	}
	if sink.disconnectedCount != 1 {
		t.Fatalf("disconnected = %d, want 1", sink.disconnectedCount)
	}
	if len(sink.errors) != 0 {
		t.Fatalf("errors = %d, want 0", len(sink.errors))
	}
}

// TestTransportShutdownWhileConnecting ports
// TransportTest::shutdownWhileConnecting.
func TestTransportShutdownWhileConnecting(t *testing.T) {
	transport := NewLoopbackTransport()
	sink := &recordingSink{}
	transport.SetSink(sink)
	transport.Connect("irc.example", 6697, true)
	if got := transport.State(); got != ConnectionConnecting {
		t.Fatalf("state = %v, want Connecting", got)
	}

	transport.Shutdown()

	if got := transport.State(); got != ConnectionDisconnected {
		t.Fatalf("state = %v, want Disconnected", got)
	}
	if sink.disconnectedCount != 1 {
		t.Fatalf("disconnected = %d, want 1", sink.disconnectedCount)
	}
}

// TestTransportTLSCertificateFailureSurfacesError ports
// TransportTest::tlsCertificateFailureSurfacesError.
func TestTransportTLSCertificateFailureSurfacesError(t *testing.T) {
	transport := NewLoopbackTransport()
	sink := &recordingSink{}
	transport.SetSink(sink)
	transport.Connect("irc.example", 6697, true)
	if !transport.TLSRequested() {
		t.Fatal("tlsRequested must be true")
	}

	const details = "TLS certificate error: The certificate has expired (CN=irc.example)"
	transport.FailTLS(details)

	if got := transport.State(); got != ConnectionFailed {
		t.Fatalf("state = %v, want Failed", got)
	}
	if len(sink.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(sink.errors))
	}
	if sink.errors[0] != details {
		t.Fatalf("error = %q, want %q", sink.errors[0], details)
	}
	if got := transport.LastError(); got != details {
		t.Fatalf("lastError = %q, want %q", got, details)
	}
	if sink.encryptedCount != 0 {
		t.Fatalf("encrypted = %d, want 0", sink.encryptedCount)
	}
}

// TestTransportSecondShutdownIsSafe ports
// TransportTest::secondShutdownIsSafe.
func TestTransportSecondShutdownIsSafe(t *testing.T) {
	transport := NewLoopbackTransport()
	sink := &recordingSink{}
	transport.SetSink(sink)
	transport.Connect("irc.example", 6667, false)
	transport.Shutdown()
	transport.Shutdown()
	transport.Shutdown()

	if got := transport.State(); got != ConnectionDisconnected {
		t.Fatalf("state = %v, want Disconnected", got)
	}
	if sink.disconnectedCount != 1 {
		t.Fatalf("disconnected = %d, want 1", sink.disconnectedCount)
	}
}

// TestTransportTimeoutFailsWithoutSleep ports
// TransportTest::timeoutFailsWithoutSleep.
func TestTransportTimeoutFailsWithoutSleep(t *testing.T) {
	transport := NewLoopbackTransport()
	sink := &recordingSink{}
	transport.SetSink(sink)
	transport.Connect("irc.example", 6667, false)
	transport.TimeoutConnect()

	if got := transport.State(); got != ConnectionFailed {
		t.Fatalf("state = %v, want Failed", got)
	}
	if len(sink.errors) != 1 {
		t.Fatalf("errors = %d, want 1", len(sink.errors))
	}
	if !strings.Contains(sink.errors[0], "timed out") {
		t.Fatalf("error = %q, want a timed out message", sink.errors[0])
	}
}
