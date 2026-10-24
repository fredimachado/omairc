//! Host network reachability. Qt loads `QNetworkInformation` with the Reachability
//! feature (`src/irc/ircsession.cpp`); on Linux that backend reads NetworkManager.
//! A reachable signal lets a session in `Reconnecting` start its attempt without
//! waiting out the backoff.

/// Fires when the host network becomes reachable again. `None` when the platform
/// has no watcher (non-Linux builds, or D-Bus is unavailable).
#[cfg(target_os = "linux")]
pub fn start_network_reachability_watcher() -> Option<async_channel::Receiver<()>> {
    linux::start_network_reachability_watcher()
}

#[cfg(not(target_os = "linux"))]
pub fn start_network_reachability_watcher() -> Option<async_channel::Receiver<()>> {
    None
}

#[cfg(target_os = "linux")]
mod linux {
    use std::path::Path;

    use async_channel::Sender;
    use futures::StreamExt;
    use zbus::{proxy, Connection};

    // NetworkManager connectivity matches Qt's reachable set (local, site, online).
    const NM_CONNECTIVITY_PORTAL: u32 = 2;
    const NM_CONNECTIVITY_FULL: u32 = 4;

    pub fn start_network_reachability_watcher() -> Option<async_channel::Receiver<()>> {
        if !linux_session_bus_allowed() {
            return None;
        }
        let (sender, receiver) = async_channel::unbounded();
        std::thread::spawn(move || {
            futures::executor::block_on(run_network_manager_watcher(sender));
        });
        Some(receiver)
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

    fn connectivity_reachable(connectivity: u32) -> bool {
        connectivity >= NM_CONNECTIVITY_PORTAL && connectivity <= NM_CONNECTIVITY_FULL
    }

    #[proxy(
        interface = "org.freedesktop.NetworkManager",
        default_service = "org.freedesktop.NetworkManager",
        default_path = "/org/freedesktop/NetworkManager"
    )]
    trait NetworkManager {
        #[zbus(property, name = "Connectivity")]
        fn connectivity(&self) -> zbus::Result<u32>;
    }

    async fn run_network_manager_watcher(sender: Sender<()>) {
        let connection = match Connection::session().await {
            Ok(connection) => connection,
            Err(_) => return,
        };
        let manager = match NetworkManagerProxy::new(&connection).await {
            Ok(manager) => manager,
            Err(_) => return,
        };
        let mut reachable = manager
            .connectivity()
            .await
            .ok()
            .is_some_and(connectivity_reachable);
        let mut stream = manager.receive_connectivity_changed().await;
        while let Some(change) = stream.next().await {
            let connectivity = match change.get().await {
                Ok(value) => value,
                Err(_) => continue,
            };
            let now = connectivity_reachable(connectivity);
            if now && !reachable {
                let _ = sender.send_blocking(());
            }
            reachable = now;
        }
    }
}
