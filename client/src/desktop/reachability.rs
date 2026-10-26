//! Host network reachability. Qt loads `QNetworkInformation` with the Reachability
//! feature; on Linux that backend reads NetworkManager. Go twin: the
//! `ReachabilitySource` seam in `tui/internal/session/session.go`. A reachable
//! signal lets a session in `Reconnecting` start its attempt without waiting out
//! the backoff.

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
    use async_channel::Sender;
    use futures::StreamExt;
    use zbus::{proxy, Connection};

    // Matches Qt `QNetworkInformation` / NetworkManager `NMState` reachability.
    pub(crate) const NM_STATE_CONNECTING: u32 = 40;
    pub(crate) const NM_STATE_CONNECTED_LOCAL: u32 = 50;
    pub(crate) const NM_STATE_CONNECTED_SITE: u32 = 60;
    pub(crate) const NM_STATE_CONNECTED_GLOBAL: u32 = 70;

    pub fn start_network_reachability_watcher() -> Option<async_channel::Receiver<()>> {
        let (sender, receiver) = async_channel::unbounded();
        std::thread::spawn(move || {
            futures::executor::block_on(run_network_manager_watcher(sender));
        });
        Some(receiver)
    }

    pub(crate) fn nm_state_reachable(state: u32) -> bool {
        state >= NM_STATE_CONNECTED_LOCAL
    }

    async fn open_network_manager_connection() -> Result<Connection, zbus::Error> {
        Connection::system().await
    }

    #[proxy(
        interface = "org.freedesktop.NetworkManager",
        default_service = "org.freedesktop.NetworkManager",
        default_path = "/org/freedesktop/NetworkManager"
    )]
    trait NetworkManager {
        #[zbus(property, name = "State")]
        fn state(&self) -> zbus::Result<u32>;
    }

    async fn run_network_manager_watcher(sender: Sender<()>) {
        let connection = match open_network_manager_connection().await {
            Ok(connection) => connection,
            Err(_) => return,
        };
        let manager = match NetworkManagerProxy::new(&connection).await {
            Ok(manager) => manager,
            Err(_) => return,
        };
        let mut reachable = manager
            .state()
            .await
            .ok()
            .is_some_and(nm_state_reachable);
        let mut stream = manager.receive_state_changed().await;
        while let Some(change) = stream.next().await {
            let state = match change.get().await {
                Ok(value) => value,
                Err(_) => continue,
            };
            let now = nm_state_reachable(state);
            if now && !reachable {
                let _ = sender.send_blocking(());
            }
            reachable = now;
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn nm_state_below_connected_local_is_not_reachable() {
            assert!(!nm_state_reachable(0));
            assert!(!nm_state_reachable(20));
            assert!(!nm_state_reachable(NM_STATE_CONNECTING));
            assert!(!nm_state_reachable(NM_STATE_CONNECTED_LOCAL - 1));
        }

        #[test]
        fn nm_state_connected_local_site_global_are_reachable() {
            assert!(nm_state_reachable(NM_STATE_CONNECTED_LOCAL));
            assert!(nm_state_reachable(NM_STATE_CONNECTED_SITE));
            assert!(nm_state_reachable(NM_STATE_CONNECTED_GLOBAL));
        }

        #[test]
        fn reachable_watcher_uses_system_bus() {
            let source = include_str!("reachability.rs");
            let connection_fn = source
                .split_once("async fn open_network_manager_connection")
                .and_then(|(_, rest)| rest.split("async fn ").next())
                .expect("open_network_manager_connection");
            assert!(
                !connection_fn.contains("Connection::session()"),
                "NetworkManager is on the system bus"
            );
            assert!(
                connection_fn.contains("Connection::system()"),
                "NetworkManager watcher must open the system bus"
            );
        }
    }
}
