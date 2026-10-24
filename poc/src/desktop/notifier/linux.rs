//! freedesktop Notifications over the session D-Bus. Port of `notify_linux.go` and
//! `Backend::notifyDesktop` on Linux.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;

use async_channel::Sender;
use futures::executor::block_on;
use futures::StreamExt;
use zbus::{proxy, zvariant, Connection};

use super::util::{conversation_key, remember_conversation_notify_id};
use super::{Activation, Notifier, NullNotifier};

#[proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
trait Notifications {
    // The freedesktop Notify signature is fixed by the spec: eight wire arguments.
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: HashMap<&str, zvariant::Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;

    #[zbus(signal, name = "ActionInvoked")]
    fn action_invoked(&self, id: u32, action_key: &str) -> zbus::Result<()>;
}

struct NotifyState {
    conversation_ids: HashMap<String, u32>,
    by_id: HashMap<u32, Activation>,
}

struct LinuxNotifier {
    connection: Connection,
    state: Arc<Mutex<NotifyState>>,
}

impl Notifier for LinuxNotifier {
    fn notify(
        &self,
        summary: &str,
        body: &str,
        network_id: &str,
        target: &str,
        msgid: &str,
    ) {
        let replaces_id = if network_id.is_empty() || target.is_empty() {
            0
        } else {
            let key = conversation_key(network_id, target);
            self.state
                .lock()
                .expect("notify state")
                .conversation_ids
                .get(&key)
                .copied()
                .unwrap_or(0)
        };
        let summary = summary.to_owned();
        let body = body.to_owned();
        let network_id = network_id.to_owned();
        let target = target.to_owned();
        let msgid = msgid.to_owned();
        let connection = self.connection.clone();
        let state = Arc::clone(&self.state);
        thread::spawn(move || {
            block_on(async {
                let proxy = match NotificationsProxy::new(&connection).await {
                    Ok(proxy) => proxy,
                    Err(_) => return,
                };
                let hints = HashMap::new();
                let actions = ["default", "Open"];
                let id = match proxy
                    .notify(
                        "Omairc",
                        replaces_id,
                        "omairc",
                        &summary,
                        &body,
                        &actions,
                        hints,
                        -1,
                    )
                    .await
                {
                    Ok(id) => id,
                    Err(_) => return,
                };
                let mut guard = state.lock().expect("notify state");
                let notify_state = &mut *guard;
                remember_conversation_notify_id(
                    &mut notify_state.conversation_ids,
                    &mut notify_state.by_id,
                    &network_id,
                    &target,
                    &msgid,
                    id,
                );
            });
        });
    }
}

async fn run_signal_loop(connection: Connection, tx: Sender<Activation>, state: Arc<Mutex<NotifyState>>) {
    let proxy = match NotificationsProxy::new(&connection).await {
        Ok(proxy) => proxy,
        Err(_) => return,
    };
    let mut stream = match proxy.receive_action_invoked().await {
        Ok(stream) => stream,
        Err(_) => return,
    };
    while let Some(signal) = stream.next().await {
        let args = match signal.args() {
            Ok(args) => args,
            Err(_) => continue,
        };
        let action = args.action_key();
        if action != "default" && !action.is_empty() {
            continue;
        }
        let activation = state
            .lock()
            .expect("notify state")
            .by_id
            .get(&args.id())
            .cloned();
        if let Some(activation) = activation {
            let _ = tx.send_blocking(activation);
        }
    }
}

fn linux_session_bus_allowed() -> bool {
    match std::env::var("DBUS_SESSION_BUS_ADDRESS") {
        Ok(address) => {
            if address.starts_with("autolaunch:") {
                return false;
            }
            const PREFIX: &str = "unix:path=";
            if let Some(path_part) = address.strip_prefix(PREFIX) {
                let path = path_part.split(',').next().unwrap_or("");
                if path.is_empty() || !Path::new(path).exists() {
                    return false;
                }
            }
            true
        }
        Err(_) => true,
    }
}

pub fn open(tx: Sender<Activation>) -> Box<dyn Notifier> {
    if !linux_session_bus_allowed() {
        return Box::new(NullNotifier);
    }
    let connection = match block_on(Connection::session()) {
        Ok(connection) => connection,
        Err(_) => return Box::new(NullNotifier),
    };
    let state = Arc::new(Mutex::new(NotifyState {
        conversation_ids: HashMap::new(),
        by_id: HashMap::new(),
    }));
    let signals = connection.clone();
    let activation_state = Arc::clone(&state);
    thread::spawn(move || block_on(run_signal_loop(signals, tx, activation_state)));
    Box::new(LinuxNotifier { connection, state })
}
