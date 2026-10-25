//! One connection's task on the net thread. Port of the connect and I/O goroutines in
//! Go's `transport_net.go`.
//!
//! `run` dials, then, for TLS, completes the handshake under its own deadline. Only
//! after the pipe is open does it start the writer. Frames queued earlier waited in
//! the channel, so none can reach the wire in cleartext ahead of the handshake.
//!
//! Every event carries the `(conn, epoch)` the task started with. The UI side drops
//! an event whose epoch is stale, so a task that outlives its connection cannot
//! speak for the next one.
//!
//! Closing is the hub's call, made through the write queue. Once the pipe is open, the
//! writer drains what is queued, says goodbye, and stops the reader. See `hub::wind_down`.

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_channel::Sender;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::AbortHandle;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

use super::dial;
use super::hub::NetOptions;
use super::tls;
use super::wire::{ConnId, Delivery, IoEvent};
use crate::session::transport::{TransportError, TransportErrorKind};

/// The most bytes one read asks the socket for. Matches the Go transport's buffer.
const READ_BUFFER: usize = 8 * 1024;

/// The connection and epoch a task reports under.
#[derive(Clone, Copy, Debug)]
pub(super) struct Tag {
    pub(super) conn: ConnId,
    pub(super) epoch: u64,
}

/// Where a task connects: the host name, the port, and whether to handshake TLS.
pub(super) struct Target {
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) tls: bool,
}

/// Connects, handshakes when asked, then runs the reader and the writer until the
/// connection ends. Aborting the task tears down its I/O too. `open` is set once the
/// pipe is open, which is when a close starts draining the queue instead of aborting.
pub(super) async fn run(
    tag: Tag,
    target: Target,
    writes: UnboundedReceiver<Vec<u8>>,
    open: Arc<AtomicBool>,
    options: NetOptions,
    events: Sender<Delivery>,
) {
    let tcp = match dial::connect(&target.host, target.port, options.connect_timeout).await {
        Ok(tcp) => tcp,
        Err(error) => {
            report(&events, tag, IoEvent::Failed(error));
            return;
        }
    };
    report(&events, tag, IoEvent::Connected);
    if !target.tls {
        pump(tag, tcp, writes, open, events, false).await;
        return;
    }

    let handshake = async {
        let name = tls::server_name(&target.host)?;
        TlsConnector::from(options.tls.clone())
            .connect(name, tcp)
            .await
            .map_err(tls::map_error)
    };
    match timeout(options.handshake_timeout, handshake).await {
        Ok(Ok(stream)) => {
            report(&events, tag, IoEvent::Encrypted);
            pump(tag, stream, writes, open, events, true).await;
        }
        Ok(Err(error)) => report(&events, tag, IoEvent::Failed(error)),
        Err(_elapsed) => report(&events, tag, IoEvent::Failed(dial::timed_out())),
    }
}

/// Runs the reader and the writer over an open stream until the reader ends. A write
/// error aborts the reader, so the worker ends either way. A close drains the writer's
/// queue and then ends the reader too.
async fn pump<S>(
    tag: Tag,
    stream: S,
    writes: UnboundedReceiver<Vec<u8>>,
    open: Arc<AtomicBool>,
    events: Sender<Delivery>,
    tls: bool,
) where
    S: AsyncRead + AsyncWrite + Send + 'static,
{
    open.store(true, Ordering::Release);
    let (read, write) = tokio::io::split(stream);
    let reader = tokio::spawn(read_loop(tag, read, events.clone(), tls));
    let writer = tokio::spawn(write_loop(
        tag,
        write,
        writes,
        events,
        reader.abort_handle(),
    ));
    // However the worker ends, whether it finishes or is aborted, both halves stop.
    let _children = AbortOnDrop(vec![reader.abort_handle(), writer.abort_handle()]);
    let _ = reader.await;
}

/// Forwards inbound bytes until the peer closes or the read fails. On TLS, an end of
/// stream without close-notify is still a clean remote close: many ircds drop the socket
/// right after `ERROR :Closing link`. Any other read error is a network failure.
pub(super) async fn read_loop<R>(tag: Tag, mut read: R, events: Sender<Delivery>, tls: bool)
where
    R: AsyncRead + Unpin + Send + 'static,
{
    let mut buffer = vec![0_u8; READ_BUFFER];
    loop {
        match read.read(&mut buffer).await {
            Ok(0) => {
                report(&events, tag, IoEvent::Closed);
                return;
            }
            Ok(count) => report(&events, tag, IoEvent::Bytes(buffer[..count].to_vec())),
            Err(error) if tls && error.kind() == io::ErrorKind::UnexpectedEof => {
                report(&events, tag, IoEvent::Closed);
                return;
            }
            Err(error) => {
                report(&events, tag, IoEvent::Failed(network(&error)));
                return;
            }
        }
    }
}

/// Writes queued frames in order. A failed write reports once and aborts the reader.
/// When the queue's sender is gone (a close), the frames already queued are written
/// first. Then the writer says goodbye (TLS close-notify, or FIN) and aborts the reader.
async fn write_loop<W>(
    tag: Tag,
    mut write: W,
    mut frames: UnboundedReceiver<Vec<u8>>,
    events: Sender<Delivery>,
    reader: AbortHandle,
) where
    W: AsyncWrite + Unpin + Send + 'static,
{
    while let Some(frame) = frames.recv().await {
        if let Err(error) = write_frame(&mut write, &frame).await {
            report(&events, tag, IoEvent::Failed(network(&error)));
            reader.abort();
            return;
        }
    }
    let _ = write.shutdown().await;
    reader.abort();
}

async fn write_frame<W>(write: &mut W, frame: &[u8]) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    write.write_all(frame).await?;
    write.flush().await
}

/// Aborts the tasks it holds when dropped, so a worker's end or abort ends its I/O.
struct AbortOnDrop(Vec<AbortHandle>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        for handle in &self.0 {
            handle.abort();
        }
    }
}

/// Sends one event under `tag`. The channel is unbounded, so the only refusal is a
/// hub that has already gone, and then nobody is left to hear it.
fn report(events: &Sender<Delivery>, tag: Tag, event: IoEvent) {
    let _ = events.try_send(Delivery {
        conn: tag.conn,
        epoch: tag.epoch,
        event,
    });
}

/// A socket error as the session sees it. The text is the OS's, never frame bytes.
fn network(error: &io::Error) -> TransportError {
    TransportError {
        kind: TransportErrorKind::Network,
        message: error.to_string(),
    }
}
