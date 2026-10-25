//! The worker's read loop, run over a reader that fails on cue. On TLS, an end of stream
//! without close-notify is a clean close. Any other read failure is a network failure.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use async_channel::unbounded;
use tokio::io::{AsyncRead, ReadBuf};
use tokio::runtime::Builder;

use crate::net::wire::{ConnId, IoEvent};
use crate::net::worker::{read_loop, Tag};
use crate::session::transport::TransportErrorKind;

const TAG: Tag = Tag {
    conn: ConnId(0),
    epoch: 1,
};

/// A reader whose every read fails with `kind`.
struct Failing(io::ErrorKind);

impl AsyncRead for Failing {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        _buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Poll::Ready(Err(io::Error::from(self.0)))
    }
}

/// Runs the read loop once over `reader` and returns the one event it reports.
fn only_event(reader: Failing, tls: bool) -> IoEvent {
    let (events, received) = unbounded();
    let runtime = Builder::new_current_thread()
        .build()
        .expect("the runtime builds");
    runtime.block_on(read_loop(TAG, reader, events, tls));
    received
        .try_recv()
        .expect("the read loop reports one event")
        .event
}

#[test]
fn tls_end_of_stream_without_close_notify_is_a_clean_close() {
    let event = only_event(Failing(io::ErrorKind::UnexpectedEof), true);
    assert!(matches!(event, IoEvent::Closed));
}

#[test]
fn an_unexpected_eof_on_plaintext_stays_a_network_failure() {
    let IoEvent::Failed(error) = only_event(Failing(io::ErrorKind::UnexpectedEof), false) else {
        panic!("a plaintext end of stream without a close is a failure");
    };
    assert_eq!(error.kind, TransportErrorKind::Network);
}

#[test]
fn a_tls_reset_stays_a_network_failure() {
    let IoEvent::Failed(error) = only_event(Failing(io::ErrorKind::ConnectionReset), true) else {
        panic!("a reset is a failure, not a clean close");
    };
    assert_eq!(error.kind, TransportErrorKind::Network);
    assert!(!error.message.is_empty());
}
