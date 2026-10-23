//! The net hub: where the UI thread meets the net thread.
//!
//! `NetHub` owns the `omairc-net` thread, which runs a current-thread tokio runtime.
//! That thread holds one task per connection and forwards `Command`s to it. Results
//! come back as `Delivery`s on an `async_channel`, which the UI drains and routes to
//! the transport registered under the connection's `ConnId`.
//!
//! The hub is made once per process. Tests build their own with `with_options`.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use async_channel::{Receiver, Sender};
use rustls::ClientConfig;
use tokio::runtime::Builder;
use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle as TaskHandle;

use super::tls;
use super::transport::NetTransport;
use super::wire::{Command, ConnId, Delivery};
use super::worker::{self, Tag, Target};

/// How long the net thread's runtime waits for its tasks once it has stopped.
const RUNTIME_SHUTDOWN: Duration = Duration::from_secs(1);
/// The default TCP connect budget, covering resolution and every address tried.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
/// The default TLS handshake budget, counted from the end of the TCP connect.
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(20);

/// The knobs a hub's connections run under. `Default` is production: 20 s for the
/// connect and for the handshake, and the process-wide TLS config.
#[derive(Clone)]
pub struct NetOptions {
    pub connect_timeout: Duration,
    pub handshake_timeout: Duration,
    pub tls: Arc<ClientConfig>,
}

impl Default for NetOptions {
    fn default() -> Self {
        Self {
            connect_timeout: CONNECT_TIMEOUT,
            handshake_timeout: HANDSHAKE_TIMEOUT,
            tls: tls::client_config(),
        }
    }
}

/// The UI-side handle to the net thread. Dropping it stops the thread and joins it.
pub struct NetHub {
    commands: UnboundedSender<Command>,
    events: Receiver<Delivery>,
    transports: RefCell<HashMap<ConnId, Weak<NetTransport>>>,
    next_conn: Cell<u64>,
    thread: Option<JoinHandle<()>>,
}

/// What the net thread keeps for one connection.
struct ConnTask {
    epoch: u64,
    writes: UnboundedSender<Vec<u8>>,
    task: TaskHandle<()>,
}

impl NetHub {
    /// A hub with the production options.
    pub fn new() -> Self {
        Self::with_options(NetOptions::default())
    }

    /// A hub with `options`. Spawns the net thread, which runs until the hub drops.
    pub fn with_options(options: NetOptions) -> Self {
        let (commands, command_rx) = unbounded_channel();
        let (event_tx, events) = async_channel::unbounded();
        let thread = std::thread::Builder::new()
            .name("omairc-net".to_owned())
            .spawn(move || run_thread(command_rx, event_tx, options))
            .expect("the net thread starts");
        Self {
            commands,
            events,
            transports: RefCell::default(),
            next_conn: Cell::new(0),
            thread: Some(thread),
        }
    }

    /// A new transport on this hub, registered so its deliveries reach it. The caller
    /// holds the `Rc`; the hub keeps only a weak reference.
    pub fn register(&self) -> Rc<NetTransport> {
        let conn = ConnId(self.next_conn.get());
        self.next_conn.set(conn.0 + 1);
        let transport = Rc::new(NetTransport::new(conn, self.commands.clone()));
        let mut transports = self.transports.borrow_mut();
        transports.retain(|_, weak| weak.strong_count() > 0);
        transports.insert(conn, Rc::downgrade(&transport));
        transport
    }

    /// Routes one delivery to its transport on the UI thread. A transport that is gone
    /// takes nothing.
    pub fn deliver(&self, delivery: Delivery) {
        let transport = self
            .transports
            .borrow()
            .get(&delivery.conn)
            .and_then(Weak::upgrade);
        if let Some(transport) = transport {
            transport.apply(delivery.epoch, delivery.event);
        }
    }

    /// Delivers every result the net thread has queued so far. Returns how many.
    pub fn drain_ready(&self) -> usize {
        let mut delivered = 0;
        while let Ok(delivery) = self.events.try_recv() {
            self.deliver(delivery);
            delivered += 1;
        }
        delivered
    }

    /// A clone of the delivery channel, for a shell task that waits on it.
    pub fn receiver(&self) -> Receiver<Delivery> {
        self.events.clone()
    }
}

impl Drop for NetHub {
    fn drop(&mut self) {
        // The net thread may already have ended; a missing receiver means nothing to stop.
        let _ = self.commands.send(Command::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The net thread's body: a current-thread runtime that runs until `Stop`.
fn run_thread(commands: UnboundedReceiver<Command>, events: Sender<Delivery>, options: NetOptions) {
    let runtime = Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("the net runtime builds");
    runtime.block_on(pump(commands, events, options));
    runtime.shutdown_timeout(RUNTIME_SHUTDOWN);
}

/// Serves commands until `Stop`. A connect replaces any task still open for its
/// connection. A write reaches the task only when its epoch is the task's. Close
/// tears the task down. On stop every task is aborted.
async fn pump(
    mut commands: UnboundedReceiver<Command>,
    events: Sender<Delivery>,
    options: NetOptions,
) {
    let mut tasks: HashMap<ConnId, ConnTask> = HashMap::new();
    while let Some(command) = commands.recv().await {
        match command {
            Command::Connect {
                conn,
                epoch,
                host,
                port,
                tls,
            } => {
                if let Some(old) = tasks.remove(&conn) {
                    old.task.abort();
                }
                let (writes, frames) = unbounded_channel();
                let task = tokio::spawn(worker::run(
                    Tag { conn, epoch },
                    Target { host, port, tls },
                    frames,
                    options.clone(),
                    events.clone(),
                ));
                tasks.insert(
                    conn,
                    ConnTask {
                        epoch,
                        writes,
                        task,
                    },
                );
            }
            Command::Write { conn, epoch, frame } => {
                if let Some(task) = tasks.get(&conn) {
                    if task.epoch == epoch {
                        // The worker is gone when its write channel is closed; the frame has no reader then.
                        let _ = task.writes.send(frame);
                    }
                }
            }
            Command::Close { conn, .. } => {
                if let Some(task) = tasks.remove(&conn) {
                    task.task.abort();
                }
            }
            Command::Stop => break,
        }
    }
    for (_, task) in tasks {
        task.task.abort();
    }
}
