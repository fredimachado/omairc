//! The single-instance guard and the server that answers the product CLI. Port of
//! `src/singleinstance.{h,cpp}` (C++). The first window owns the endpoint and serves it. A second
//! launch sends the raise ping and exits, as Qt's `acquireOrNotify` does.
//!
//! The listener runs on background threads, one per client, and never touches the controller. Each
//! request crosses to the UI thread as an `IpcJob` on a channel. The window drains that channel on its
//! own gpui loop (`ipchandler::serve`), so every intent and read runs where the controller lives.

use std::fs::File;
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use super::endpoint::{self, Endpoint, Listener, Stream};
use super::ipc::{self, MAX_LINE_BYTES, RAISE_PING};

/// The most clients served at once (`kMaxIpcClients`).
const MAX_CLIENTS: usize = 32;
/// How long a client may sit idle before its request is dropped (`kIpcIdleTimeoutMs`).
const IDLE_TIMEOUT: Duration = Duration::from_millis(1000);
/// How long a client waits for the window to answer a request.
const REPLY_TIMEOUT: Duration = Duration::from_millis(4000);
/// How long a second launch waits to reach the running window.
const NOTIFY_TIMEOUT: Duration = Duration::from_millis(1000);
/// How many times a second launch retries a window that is taken but does not answer.
const CLAIM_ATTEMPTS: usize = 3;

/// One request from a CLI client, on its way to the UI thread.
pub struct IpcJob {
    line: String,
    reply: Sender<String>,
}

impl IpcJob {
    /// The request line, as the client sent it, without its newline.
    pub fn line(&self) -> &str {
        &self.line
    }

    /// Sends the answer to the waiting client. The client may already have gone, which is not an error.
    pub fn respond(self, reply: String) {
        let _ = self.reply.send(reply);
    }
}

/// The primary's side: the job queue the UI drains, and the lock that marks the primary on Unix.
pub struct IpcServer {
    jobs: Receiver<IpcJob>,
    /// Held for the process lifetime: while it is locked, no other window can be the primary.
    _lock: Option<File>,
}

impl IpcServer {
    /// The next request waiting for the UI thread, if any. Only the UI thread calls this.
    pub fn try_next(&self) -> Option<IpcJob> {
        match self.jobs.try_recv() {
            Ok(job) => Some(job),
            Err(TryRecvError::Empty | TryRecvError::Disconnected) => None,
        }
    }

    /// Starts the accept loop on its own thread and returns the queue the UI drains.
    fn serve(mut listener: Listener, lock: Option<File>) -> io::Result<Self> {
        let (jobs_tx, jobs) = mpsc::channel();
        let clients = Arc::new(AtomicUsize::new(0));
        thread::Builder::new()
            .name("omairc-ipc-accept".to_owned())
            .spawn(move || {
                let mut failures = 0u32;
                loop {
                    match listener.accept() {
                        Ok(stream) => {
                            failures = 0;
                            // A stream over the client limit is dropped, which closes it.
                            if clients.load(Ordering::SeqCst) >= MAX_CLIENTS {
                                continue;
                            }
                            clients.fetch_add(1, Ordering::SeqCst);
                            let jobs = jobs_tx.clone();
                            let clients = Arc::clone(&clients);
                            thread::spawn(move || {
                                serve_client(stream, &jobs);
                                clients.fetch_sub(1, Ordering::SeqCst);
                            });
                        }
                        Err(_) => {
                            failures += 1;
                            if failures > 50 {
                                return;
                            }
                            thread::sleep(Duration::from_millis(20));
                        }
                    }
                }
            })?;
        Ok(Self { jobs, _lock: lock })
    }
}

/// What this launch turned out to be.
pub enum Startup {
    /// This process owns the endpoint and serves it.
    Primary(IpcServer),
    /// A running window was raised; this process should exit.
    Secondary,
    /// `OMAIRC_ALLOW_MULTI` is set, so the launch is not guarded and serves nothing.
    Unguarded,
}

/// The guard for the app's own endpoint, honouring `OMAIRC_ALLOW_MULTI` as Qt does.
pub fn acquire_for_app() -> io::Result<Startup> {
    if std::env::var_os("OMAIRC_ALLOW_MULTI").is_some_and(|value| !value.is_empty()) {
        return Ok(Startup::Unguarded);
    }
    acquire(&Endpoint::app())
}

/// Claims `endpoint` as the primary, or raises the window that holds it (`acquireOrNotify`).
#[cfg(unix)]
pub fn acquire(endpoint: &Endpoint) -> io::Result<Startup> {
    let lock = File::options()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(endpoint.lock_path())?;
    match lock.try_lock() {
        Ok(()) => {}
        Err(std::fs::TryLockError::WouldBlock) => return notify_running(endpoint),
        Err(std::fs::TryLockError::Error(error)) => return Err(error),
    }
    let listener = Listener::bind(endpoint)?;
    Ok(Startup::Primary(IpcServer::serve(listener, Some(lock))?))
}

/// Claims `endpoint` as the primary, or raises the window that holds it (`acquireOrNotify`). The
/// first pipe instance is the lock.
#[cfg(windows)]
pub fn acquire(endpoint: &Endpoint) -> io::Result<Startup> {
    for _ in 0..CLAIM_ATTEMPTS {
        match Listener::bind(endpoint) {
            Ok(listener) => return Ok(Startup::Primary(IpcServer::serve(listener, None)?)),
            Err(error) if endpoint::is_taken(&error) => {
                if notify_primary(endpoint) {
                    return Ok(Startup::Secondary);
                }
                // The holder is between instances or closing. Try again rather than give up.
                thread::sleep(Duration::from_millis(50));
            }
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other("Omairc is running but did not answer"))
}

#[cfg(unix)]
fn notify_running(endpoint: &Endpoint) -> io::Result<Startup> {
    for _ in 0..CLAIM_ATTEMPTS {
        if notify_primary(endpoint) {
            return Ok(Startup::Secondary);
        }
        thread::sleep(Duration::from_millis(50));
    }
    Err(io::Error::other("Omairc is running but did not answer"))
}

/// Sends the raise ping to the running window. True when the ping was written.
///
/// The notifier then holds the connection until the window answers or the wait runs out. A pipe can
/// drop bytes that the writer closes before the reader takes them, so closing at once is not safe.
pub fn notify_primary(endpoint: &Endpoint) -> bool {
    let Ok(mut stream) = endpoint::connect(endpoint, NOTIFY_TIMEOUT) else {
        return false;
    };
    let ping = format!("{RAISE_PING}\n");
    if stream.write_all(ping.as_bytes()).is_err() {
        return false;
    }
    stream.set_read_deadline(Some(Instant::now() + NOTIFY_TIMEOUT));
    let mut answer = [0u8; 256];
    while matches!(stream.read(&mut answer), Ok(read) if read > 0) {}
    true
}

/// Serves one client: reads its request line, hands it to the UI thread, and writes the answer.
fn serve_client(mut stream: Stream, jobs: &Sender<IpcJob>) {
    let Some(line) = read_request(&mut stream) else {
        return;
    };
    let (reply_tx, reply_rx) = mpsc::channel();
    if jobs.send(IpcJob { line, reply: reply_tx }).is_err() {
        return;
    }
    let reply = reply_rx
        .recv_timeout(REPLY_TIMEOUT)
        .unwrap_or_else(|_| ipc::error_response("Omairc did not answer in time"));
    // Qt drops an answer the client could not read, rather than cut it short.
    if reply.len() + 1 > MAX_LINE_BYTES {
        return;
    }
    let mut bytes = reply.into_bytes();
    bytes.push(b'\n');
    if stream.write_all(&bytes).is_ok() {
        let _ = stream.finish();
    }
}

/// Reads one request line. A client that goes idle, or closes before a newline, sends nothing; the
/// one exception is a bare raise ping, which counts without its newline (`consumeSocketData`).
fn read_request(stream: &mut Stream) -> Option<String> {
    use std::io::Read;

    let mut buffer: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 4096];
    let mut idle_until = Instant::now() + IDLE_TIMEOUT;
    loop {
        stream.set_read_deadline(Some(idle_until));
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                buffer.extend_from_slice(&chunk[..read]);
                idle_until = Instant::now() + IDLE_TIMEOUT;
                if let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
                    if newline > MAX_LINE_BYTES {
                        return None;
                    }
                    buffer.truncate(newline);
                    return Some(String::from_utf8_lossy(&buffer).into_owned());
                }
                if buffer.len() > MAX_LINE_BYTES {
                    return None;
                }
            }
        }
    }
    (String::from_utf8_lossy(&buffer).trim() == RAISE_PING).then(|| RAISE_PING.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::time::Instant;

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    /// A unique endpoint, so parallel tests never meet each other's window or lock.
    fn endpoint() -> Endpoint {
        let serial = NEXT.fetch_add(1, Ordering::SeqCst);
        Endpoint::named(&format!(
            "{}-test-{}-{serial}",
            crate::identity::APP_FOLDER,
            std::process::id()
        ))
    }

    fn primary(endpoint: &Endpoint) -> IpcServer {
        match acquire(endpoint).expect("the endpoint is free") {
            Startup::Primary(server) => server,
            _ => panic!("the first claim becomes the primary"),
        }
    }

    /// Waits for the primary to queue a request, as the UI loop would poll for it.
    fn next_job(server: &IpcServer) -> IpcJob {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(job) = server.try_next() {
                return job;
            }
            assert!(Instant::now() < deadline, "no request reached the primary");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_request_round_trips_over_the_endpoint() {
        let endpoint = endpoint();
        let server = primary(&endpoint);
        let client_endpoint = endpoint.clone();
        let client = thread::spawn(move || {
            let mut stream = endpoint::connect(&client_endpoint, Duration::from_secs(2)).expect("connect");
            stream.write_all(b"{\"cmd\":\"raise\"}\n").expect("write");
            stream.set_read_deadline(Some(Instant::now() + Duration::from_secs(5)));
            let mut reply = Vec::new();
            let mut byte = [0u8; 1];
            loop {
                match std::io::Read::read(&mut stream, &mut byte) {
                    Ok(1) if byte[0] == b'\n' => break,
                    Ok(1) => reply.push(byte[0]),
                    other => panic!("no reply line: {other:?}"),
                }
            }
            String::from_utf8(reply).expect("utf-8")
        });
        let job = next_job(&server);
        assert_eq!(job.line(), r#"{"cmd":"raise"}"#);
        job.respond(r#"{"ok":true}"#.to_owned());
        assert_eq!(client.join().expect("client"), r#"{"ok":true}"#);
    }

    #[test]
    fn a_second_claim_raises_the_primary_and_queues_a_ping() {
        let endpoint = endpoint();
        let server = primary(&endpoint);
        assert!(matches!(acquire(&endpoint), Ok(Startup::Secondary)));
        assert_eq!(next_job(&server).line(), RAISE_PING);
    }

    #[test]
    fn a_bare_ping_without_a_newline_still_counts() {
        let endpoint = endpoint();
        let server = primary(&endpoint);
        let client_endpoint = endpoint.clone();
        let client = thread::spawn(move || {
            let mut stream =
                endpoint::connect(&client_endpoint, Duration::from_secs(2)).expect("connect");
            stream.write_all(b"!").expect("write");
            stream.set_read_deadline(Some(Instant::now() + Duration::from_secs(10)));
            let mut answer = [0u8; 64];
            while matches!(stream.read(&mut answer), Ok(read) if read > 0) {}
        });
        // The window hears a bare ping once the client goes idle, as Qt's idle timer does.
        let job = next_job(&server);
        assert_eq!(job.line(), RAISE_PING);
        job.respond(ipc::ok_response());
        client.join().expect("client");
    }

    #[test]
    fn an_oversized_request_never_reaches_the_window() {
        let endpoint = endpoint();
        let server = primary(&endpoint);
        {
            let mut stream = endpoint::connect(&endpoint, Duration::from_secs(2)).expect("connect");
            let blob = vec![b'x'; MAX_LINE_BYTES + 1];
            let _ = stream.write_all(&blob);
            let _ = stream.finish();
        }
        thread::sleep(Duration::from_millis(100));
        assert!(server.try_next().is_none());
    }
}
