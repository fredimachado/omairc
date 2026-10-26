//! The in-process peer. A server on `127.0.0.1` runs on its own thread, with a
//! current-thread runtime, so it can speak TLS through `tokio-rustls`. The test thread
//! sends it commands and waits for each reply. The peer never shares a socket with the
//! transport under test: the transport only ever sees bytes on the wire.
//! The certificate is the checked-in `tests/live/certs/live.pem`, a self-signed
//! `localhost` and `127.0.0.1` certificate. Test trust takes that file as a root, and
//! nothing else in the crate does.

use std::io;
use std::net::{SocketAddr, TcpListener as StdListener};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use rustls::crypto::ring::default_provider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ClientConfig, ServerConfig};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::runtime::Builder;
use tokio::time::timeout;
use tokio_rustls::TlsAcceptor;

/// The test certificate, shared with the live ircd harness.
const LIVE_CERT: &[u8] = include_bytes!("../../../../tests/live/certs/live.pem");
/// Its private key. The certificate and key are test-only fixtures.
const LIVE_KEY: &[u8] = include_bytes!("../../../../tests/live/certs/live.key");

/// How long the peer waits for a client to connect before it fails the case.
const ACCEPT_WAIT: Duration = Duration::from_secs(5);
/// How long the peer's own handshake may take.
const HANDSHAKE_WAIT: Duration = Duration::from_secs(5);
/// The quiet time that ends a read: the peer stops once nothing has arrived for this long.
pub(super) const QUIET: Duration = Duration::from_millis(150);

/// How the peer answers a pending connect.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    /// Accept the TCP connection and speak plaintext.
    Plain,
    /// Accept, then complete a TLS handshake with the test certificate.
    Tls,
    /// Accept, then answer the ClientHello with bytes that are not TLS.
    BrokenTls,
    /// Accept, then say nothing at all, so the client's handshake waits.
    Silent,
}

/// One request to the peer thread. Every variant that expects an answer carries a reply.
enum Command {
    Accept {
        mode: Mode,
        reply: Sender<Result<(), String>>,
    },
    Send {
        bytes: Vec<u8>,
        reply: Sender<()>,
    },
    Received {
        reply: Sender<Vec<u8>>,
    },
    Close {
        reply: Sender<()>,
    },
    Abandon {
        reply: Sender<()>,
    },
    Shutdown,
}

/// The test-side handle. Dropping it stops the thread and joins it.
pub(super) struct Peer {
    port: u16,
    commands: Sender<Command>,
    thread: Option<JoinHandle<()>>,
}

impl Peer {
    /// A peer listening on an ephemeral.
    pub(super) fn spawn() -> Self {
        let listener = StdListener::bind("127.0.0.1:0").expect("the peer binds a port");
        let port = listener
            .local_addr()
            .expect("the peer has an address")
            .port();
        let (commands, command_rx) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("net-test-peer".to_owned())
            .spawn(move || serve(listener, command_rx))
            .expect("the peer thread starts");
        Self {
            port,
            commands,
            thread: Some(thread),
        }
    }

    pub(super) fn port(&self) -> u16 {
        self.port
    }

    /// Waits for the next connect and answers it as `mode`. The error is the peer's
    /// own: no client came, or its side of the handshake failed. A client that rejects
    /// the certificate makes the peer's handshake fail too, which the case may expect.
    pub(super) fn accept(&self, mode: Mode) -> Result<(), String> {
        let (reply, answer) = mpsc::channel();
        self.ask(Command::Accept { mode, reply });
        answer.recv().expect("the peer thread answers")
    }

    /// Writes raw bytes to the connection the peer holds. Nothing is written when no
    /// connection is open.
    pub(super) fn send(&self, bytes: &[u8]) {
        let (reply, answer) = mpsc::channel();
        self.ask(Command::Send {
            bytes: bytes.to_vec(),
            reply,
        });
        answer.recv().expect("the peer thread answers");
    }

    /// The bytes the peer has read since the last call, in order. A TLS connection
    /// starts with the raw ClientHello the peer saw before its handshake.
    pub(super) fn received(&self) -> Vec<u8> {
        let (reply, answer) = mpsc::channel();
        self.ask(Command::Received { reply });
        answer.recv().expect("the peer thread answers")
    }

    /// Ends the connection the peer holds, with TLS close-notify when it is encrypted.
    pub(super) fn close(&self) {
        let (reply, answer) = mpsc::channel();
        self.ask(Command::Close { reply });
        answer.recv().expect("the peer thread answers");
    }

    /// Drops the connection the peer holds without closing it: no TLS close-notify and
    /// no shutdown, so the client sees the socket end in the middle of a session.
    pub(super) fn abandon(&self) {
        let (reply, answer) = mpsc::channel();
        self.ask(Command::Abandon { reply });
        answer.recv().expect("the peer thread answers");
    }

    fn ask(&self, command: Command) {
        self.commands
            .send(command)
            .expect("the peer thread is running");
    }
}

impl Drop for Peer {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Any byte stream the peer can hold: plaintext TCP or the TLS stream over it.
trait Duplex: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T: AsyncRead + AsyncWrite + Unpin + Send> Duplex for T {}

/// The peer thread's state. Its runtime drives every command to completion.
struct Server {
    listener: TcpListener,
    acceptor: TlsAcceptor,
    /// The connection the peer holds for the test.
    stream: Option<Box<dyn Duplex>>,
    /// Raw bytes read before the handshake, returned by the next `received`.
    hello: Vec<u8>,
}

fn serve(listener: StdListener, commands: Receiver<Command>) {
    let runtime = Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("the peer runtime builds");
    let mut server = runtime.block_on(async {
        listener
            .set_nonblocking(true)
            .expect("the peer listener is nonblocking");
        Server {
            listener: TcpListener::from_std(listener).expect("the peer listener registers"),
            acceptor: TlsAcceptor::from(server_config()),
            stream: None,
            hello: Vec::new(),
        }
    });
    while let Ok(command) = commands.recv() {
        match command {
            Command::Accept { mode, reply } => {
                let result = runtime.block_on(server.accept(mode));
                let _ = reply.send(result);
            }
            Command::Send { bytes, reply } => {
                runtime.block_on(server.write(&bytes));
                let _ = reply.send(());
            }
            Command::Received { reply } => {
                let bytes = runtime.block_on(server.read_quiet());
                let _ = reply.send(bytes);
            }
            Command::Close { reply } => {
                runtime.block_on(server.close());
                let _ = reply.send(());
            }
            Command::Abandon { reply } => {
                runtime.block_on(server.abandon());
                let _ = reply.send(());
            }
            Command::Shutdown => break,
        }
    }
    runtime.shutdown_timeout(Duration::from_secs(1));
}

impl Server {
    async fn accept(&mut self, mode: Mode) -> Result<(), String> {
        let (tcp, _) = timeout(ACCEPT_WAIT, self.listener.accept())
            .await
            .map_err(|_| "no client connected".to_owned())?
            .map_err(|error| error.to_string())?;
        match mode {
            Mode::Plain | Mode::Silent => self.stream = Some(Box::new(tcp)),
            Mode::Tls => {
                self.hello = read_hello(&tcp).await?;
                let handshake = self.acceptor.accept(tcp);
                let stream = timeout(HANDSHAKE_WAIT, handshake)
                    .await
                    .map_err(|_| "the handshake timed out".to_owned())?
                    .map_err(|error| error.to_string())?;
                self.stream = Some(Box::new(stream));
            }
            Mode::BrokenTls => {
                self.hello = read_hello(&tcp).await?;
                let mut tcp = tcp;
                tcp.write_all(b"HTTP/1.1 400 Bad Request\r\n\r\n")
                    .await
                    .map_err(|error| error.to_string())?;
                // Held open, so the client's failure ends the handshake, not a reset.
                self.stream = Some(Box::new(tcp));
            }
        }
        Ok(())
    }

    async fn write(&mut self, bytes: &[u8]) {
        if let Some(stream) = self.stream.as_mut() {
            // A client that already went away refuses the write. The case sees that
            // through the transport, not here.
            let _ = stream.write_all(bytes).await;
            let _ = stream.flush().await;
        }
    }

    /// Reads until nothing arrives for `QUIET`, after the raw hello bytes if any.
    async fn read_quiet(&mut self) -> Vec<u8> {
        let mut bytes = std::mem::take(&mut self.hello);
        let Some(stream) = self.stream.as_mut() else {
            return bytes;
        };
        let mut buffer = [0_u8; 4096];
        loop {
            match timeout(QUIET, stream.read(&mut buffer)).await {
                Ok(Ok(count)) if count > 0 => bytes.extend_from_slice(&buffer[..count]),
                _ => break,
            }
        }
        bytes
    }

    async fn close(&mut self) {
        if let Some(mut stream) = self.stream.take() {
            let _ = stream.shutdown().await;
        }
    }

    /// Drops the held stream as it stands. For TLS that sends no close-notify.
    async fn abandon(&mut self) {
        self.stream = None;
    }
}

/// The first bytes a client sends, read without consuming them. The handshake still
/// gets to read them, so the peer can show the wire carried a ClientHello.
async fn read_hello(tcp: &TcpStream) -> Result<Vec<u8>, String> {
    let mut buffer = vec![0_u8; 1024];
    let read = timeout(HANDSHAKE_WAIT, async {
        loop {
            let count = tcp.peek(&mut buffer).await?;
            if count == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "the client closed before its hello",
                ));
            }
            if count >= 5 {
                return Ok(count);
            }
        }
    })
    .await;
    match read {
        Ok(Ok(count)) => {
            buffer.truncate(count);
            Ok(buffer)
        }
        Ok(Err(error)) => Err(error.to_string()),
        Err(_) => Err("no ClientHello arrived".to_owned()),
    }
}

/// The peer's TLS server config: the test certificate and its key, on the ring provider.
fn server_config() -> Arc<ServerConfig> {
    let certs = CertificateDer::pem_slice_iter(LIVE_CERT)
        .collect::<Result<Vec<_>, _>>()
        .expect("the test certificate parses");
    let key = PrivateKeyDer::from_pem_slice(LIVE_KEY).expect("the test key parses");
    let config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .expect("the ring provider supports the default protocol versions")
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("the test key matches the test certificate");
    Arc::new(config)
}

/// A client config that trusts the test certificate and nothing else.
pub(super) fn pinned_client_config() -> Arc<ClientConfig> {
    crate::net::client_config_trusting_pem(LIVE_CERT).expect("the test certificate pins")
}

/// A loopback address nothing listens on: the.
pub(super) fn dead_address() -> SocketAddr {
    let listener = StdListener::bind("127.0.0.1:0").expect("a probe port binds");
    listener.local_addr().expect("the probe has an address")
}
