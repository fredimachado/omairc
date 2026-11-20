//! Outbound lines each live session wrote, so a case can count what the client sent.
//!
//! The log is the order of `Transport::write` calls. It matches wire order for
//! writes from one turn (one session drain); interleaving between concurrent
//! writers is unspecified, as it is on the wire.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

use crate::controller::transports::TransportFactory;
use crate::net::NetTransports;
use crate::session::transport::{ConnectionState, Transport, TransportSink};

/// Network id -> lines in write order, CRLF trimmed, lossy UTF-8.
pub(super) type SentLog = Rc<RefCell<BTreeMap<String, Vec<String>>>>;

/// Wraps the live factory; every transport it hands out records its writes.
pub(super) struct RecordingTransports {
    inner: NetTransports,
    log: SentLog,
}

impl RecordingTransports {
    pub(super) fn new(inner: NetTransports, log: SentLog) -> Self {
        Self { inner, log }
    }
}

impl TransportFactory for RecordingTransports {
    fn open(&self, network_id: &str) -> Result<Rc<dyn Transport>, String> {
        let inner = self.inner.open(network_id)?;
        Ok(Rc::new(RecordingTransport {
            inner,
            network_id: network_id.to_owned(),
            log: Rc::clone(&self.log),
        }))
    }
}

struct RecordingTransport {
    inner: Rc<dyn Transport>,
    network_id: String,
    log: SentLog,
}

impl Transport for RecordingTransport {
    fn connect(&self, host: &str, port: u16, tls_enabled: bool) {
        self.inner.connect(host, port, tls_enabled)
    }

    fn write(&self, frame: &[u8]) {
        // Parse once at the boundary: one framed line in, one trimmed String stored.
        let line = String::from_utf8_lossy(frame)
            .trim_end_matches(['\r', '\n'])
            .to_owned();
        self.log
            .borrow_mut()
            .entry(self.network_id.clone())
            .or_default()
            .push(line);
        self.inner.write(frame);
    }

    fn shutdown(&self) {
        self.inner.shutdown()
    }

    fn state(&self) -> ConnectionState {
        self.inner.state()
    }

    fn set_sink(&self, sink: Weak<dyn TransportSink>) {
        self.inner.set_sink(sink)
    }
}
