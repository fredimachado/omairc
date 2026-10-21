//! Port of `tui/internal/session/loopback.go` (Go), `src/irc/ircloopbacktransport.*`
//! (C++) and the `FakeIrcTransport` of `tests/session/tst_transport.cpp`.
//!
//! The deterministic in-memory transport. Nothing happens on its own: the test
//! (or, in the PoC, the in-process demo peer) drives every transition through
//! the control methods below. Delivery is synchronous (rule 10 of the master
//! plan): `inject_bytes` calls straight into the session, so a frame written and
//! answered is observable on the next line of the caller.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

use super::transport::{ConnectionState, Transport, TransportSink};

/// Runs for every frame passed to `write`, with the recorded copy.
pub type FrameWrittenHook = Rc<dyn Fn(&[u8])>;

#[derive(Default)]
struct LoopbackState {
    state: ConnectionState,
    tls_on: bool,
    host: String,
    port: u16,
    written: Vec<Vec<u8>>,
    last_error: String,
}

/// The in-memory `Transport`.
#[derive(Default)]
pub struct LoopbackTransport {
    state: RefCell<LoopbackState>,
    sink: RefCell<Option<Weak<dyn TransportSink>>>,
    frame_written: RefCell<Option<FrameWrittenHook>>,
}

impl LoopbackTransport {
    /// An idle loopback with no sink and no hook.
    pub fn new() -> Self {
        Self::default()
    }

    /// Installs the hook that runs after every `write` (`frameWritten` in C++).
    /// It runs with no borrow held, so it may call `inject_bytes`.
    pub fn set_frame_written_hook(&self, hook: FrameWrittenHook) {
        *self.frame_written.borrow_mut() = Some(hook);
    }

    /// Removes the frame-written hook. Writes still record their frames.
    pub fn clear_frame_written_hook(&self) {
        *self.frame_written.borrow_mut() = None;
    }

    /// Completes a pending `connect`: emits `connected`, and `encrypted` when TLS
    /// was requested. A no-op unless the state is `Connecting`.
    pub fn complete_connect(&self) {
        let tls_on = {
            let mut state = self.state.borrow_mut();
            if state.state != ConnectionState::Connecting {
                return;
            }
            state.state = ConnectionState::Connected;
            state.tls_on
        };
        let sink = self.live_sink();
        if let Some(sink) = &sink {
            sink.connected();
        }
        if !tls_on {
            return;
        }
        self.state.borrow_mut().state = ConnectionState::Encrypted;
        if let Some(sink) = &sink {
            sink.encrypted();
        }
    }

    /// Fails a connection that is still connecting.
    pub fn fail_connect(&self, message: &str) {
        self.fail(message, &[ConnectionState::Connecting]);
    }

    /// Fails a connection during the TLS phase: while connecting, or after the
    /// plaintext socket opened but before encryption.
    pub fn fail_tls(&self, message: &str) {
        self.fail(
            message,
            &[ConnectionState::Connecting, ConnectionState::Connected],
        );
    }

    /// Fails a pending connection with the timeout message.
    pub fn timeout_connect(&self) {
        self.fail("Connection timed out", &[ConnectionState::Connecting]);
    }

    /// Delivers inbound bytes to the sink while the connection is open. Empty
    /// input and a closed connection deliver nothing.
    pub fn inject_bytes(&self, bytes: &[u8]) {
        if bytes.is_empty() || !self.is_open() {
            return;
        }
        if let Some(sink) = self.live_sink() {
            sink.bytes_received(bytes);
        }
    }

    /// Ends an open connection as if the peer hung up. Emits `disconnected`,
    /// not `error`.
    pub fn remote_close(&self) {
        {
            let mut state = self.state.borrow_mut();
            if !matches!(
                state.state,
                ConnectionState::Connected | ConnectionState::Encrypted
            ) {
                return;
            }
            state.state = ConnectionState::Disconnected;
        }
        if let Some(sink) = self.live_sink() {
            sink.disconnected();
        }
    }

    /// Every frame passed to `write`, in order.
    pub fn written_frames(&self) -> Vec<Vec<u8>> {
        self.state.borrow().written.clone()
    }

    /// Forgets the recorded frames.
    pub fn clear_written_frames(&self) {
        self.state.borrow_mut().written.clear();
    }

    /// The message recorded by the most recent failure.
    pub fn last_error(&self) -> String {
        self.state.borrow().last_error.clone()
    }

    /// The host from the most recent `connect`.
    pub fn connected_host(&self) -> String {
        self.state.borrow().host.clone()
    }

    /// The port from the most recent `connect`.
    pub fn connected_port(&self) -> u16 {
        self.state.borrow().port
    }

    /// Whether the most recent `connect` asked for TLS.
    pub fn tls_requested(&self) -> bool {
        self.state.borrow().tls_on
    }

    fn is_open(&self) -> bool {
        matches!(
            self.state.borrow().state,
            ConnectionState::Connected | ConnectionState::Encrypted
        )
    }

    fn live_sink(&self) -> Option<Rc<dyn TransportSink>> {
        self.sink.borrow().as_ref().and_then(Weak::upgrade)
    }

    /// Moves to `Failed` from one of `from`, recording `message`, and reports it
    /// to the sink. A transport already failed keeps its first error.
    fn fail(&self, message: &str, from: &[ConnectionState]) {
        {
            let mut state = self.state.borrow_mut();
            if !from.contains(&state.state) {
                return;
            }
            state.last_error = message.to_owned();
            state.state = ConnectionState::Failed;
        }
        if let Some(sink) = self.live_sink() {
            sink.error(message);
        }
    }
}

impl Transport for LoopbackTransport {
    /// Records the request and enters `Connecting` with no I/O. A connect while
    /// already connecting, connected, encrypted or closing is ignored.
    fn connect(&self, host: &str, port: u16, tls_enabled: bool) {
        let mut state = self.state.borrow_mut();
        if matches!(
            state.state,
            ConnectionState::Connecting
                | ConnectionState::Connected
                | ConnectionState::Encrypted
                | ConnectionState::Closing
        ) {
            return;
        }
        state.host = host.to_owned();
        state.port = port;
        state.tls_on = tls_enabled;
        state.last_error.clear();
        state.state = ConnectionState::Connecting;
    }

    /// Records the frame, then runs the hook with the recorded copy. It does not
    /// need an open connection, as `IrcLoopbackTransport::write` does not.
    fn write(&self, frame: &[u8]) {
        let copied = frame.to_vec();
        self.state.borrow_mut().written.push(copied.clone());
        let hook = self.frame_written.borrow().clone();
        if let Some(hook) = hook {
            hook(&copied);
        }
    }

    /// Ends the connection synchronously and emits `disconnected` once.
    fn shutdown(&self) {
        {
            let mut state = self.state.borrow_mut();
            if matches!(
                state.state,
                ConnectionState::Idle
                    | ConnectionState::Disconnected
                    | ConnectionState::Failed
                    | ConnectionState::Closing
            ) {
                return;
            }
            // The C++ loopback passes through Closing, but sets Disconnected in the
            // same synchronous step before it emits, so no observer sees Closing.
            state.state = ConnectionState::Disconnected;
        }
        if let Some(sink) = self.live_sink() {
            sink.disconnected();
        }
    }

    fn state(&self) -> ConnectionState {
        self.state.borrow().state
    }

    fn set_sink(&self, sink: Weak<dyn TransportSink>) {
        *self.sink.borrow_mut() = Some(sink);
    }
}
