//! The sidebar's network order and collapse state. Port of `NetworkOrder`,
//! `SetNetworkOrder`, `SetNetworkCollapsed`, `SetAllNetworksCollapsed`,
//! `IsNetworkCollapsed` and `MoveNetwork` in `tui/internal/controller/controller.go`
//! (Go), and the sidebar's order roles in `src/irc/irccontroller.cpp` (C++).
//!
//! The order and the collapse set live on the controller, because the sidebar reads
//! the controller's order rather than the connection's. Each change is saved through the
//! preference store (`persistence.rs`), so the layout comes back on the next launch.

use std::collections::BTreeSet;

use crate::irc::viewnotify::{self, ViewNotify};
use crate::session::events::SessionState;

use super::controller::Controller;

impl Controller {
    /// The networks the controller knows, in id order: every registered session, and
    /// every network that has a recorded last error or a Status line. A network whose
    /// start was refused has no session, but Qt lists its profile in the sidebar, and
    /// its last error is the row's status line, so the row has to exist here.
    pub fn known_network_ids(&self) -> Vec<String> {
        let mut ids: BTreeSet<String> = self.manager.network_ids().into_iter().collect();
        ids.extend(self.last_errors.keys().filter(|id| !id.is_empty()).cloned());
        ids.extend(self.console.network_ids());
        ids.extend(self.profiles.keys().cloned());
        ids.into_iter().collect()
    }

    /// The network ids in display order (`NetworkOrder`): the recorded order first,
    /// keeping only known networks (`known_network_ids`), then any other known network
    /// in id order.
    pub fn network_order(&self) -> Vec<String> {
        let known = self.known_network_ids();
        if self.network_order.is_empty() {
            return known;
        }
        let mut seen = BTreeSet::new();
        let mut order = Vec::with_capacity(known.len());
        for id in &self.network_order {
            if seen.contains(id) || !known.contains(id) {
                continue;
            }
            seen.insert(id.clone());
            order.push(id.clone());
        }
        for id in known {
            if seen.insert(id.clone()) {
                order.push(id);
            }
        }
        order
    }

    /// Records the sidebar order (`SetNetworkOrder`). An unchanged order does nothing.
    pub fn set_network_order(&mut self, order: &[String]) {
        if self.network_order == order {
            return;
        }
        self.network_order = order.to_vec();
        self.persist_network_layout();
        self.queue(vec![ViewNotify::Conversations]);
    }

    /// Whether a network's rows are folded in the sidebar (`IsNetworkCollapsed`).
    pub fn is_network_collapsed(&self, network_id: &str) -> bool {
        !network_id.is_empty() && self.network_collapsed.contains(network_id)
    }

    /// Folds or unfolds one network's rows (`SetNetworkCollapsed`).
    pub fn set_network_collapsed(&mut self, network_id: &str, collapsed: bool) {
        if network_id.is_empty() {
            return;
        }
        let changed = if collapsed {
            self.network_collapsed.insert(network_id.to_owned())
        } else {
            self.network_collapsed.remove(network_id)
        };
        if changed {
            self.persist_network_layout();
            self.queue(vec![ViewNotify::Conversations]);
        }
    }

    /// Folds or unfolds every network at once (`SetAllNetworksCollapsed`).
    pub fn set_all_networks_collapsed(&mut self, collapsed: bool) {
        let order = self.network_order();
        let changed = order
            .iter()
            .any(|id| self.network_collapsed.contains(id) != collapsed);
        if !changed {
            return;
        }
        for id in order {
            if collapsed {
                self.network_collapsed.insert(id);
            } else {
                self.network_collapsed.remove(&id);
            }
        }
        self.persist_network_layout();
        self.queue(vec![ViewNotify::Conversations]);
    }

    /// Forgets a network the Connect sheet removed (`ForgetNetwork`). A live session
    /// quits and stops; the network's conversations, Status log, last error, order,
    /// collapse state and inbox rows go with it. The selection leaves it when it held
    /// the selection. False when the network was never known.
    pub fn forget_network(&mut self, network_id: &str) -> bool {
        self.pump();
        if network_id.is_empty() {
            return false;
        }
        let known = self.manager.find(network_id).is_some()
            || self.profiles.contains_key(network_id)
            || !self.console.lines(network_id).is_empty()
            || self.reducer.conversations().keys().any(|key| key.network_id == network_id);
        if self.profiles.contains_key(network_id) {
            let status = self.remove_profile(network_id);
            if matches!(status, crate::storage::settings::Status::AccessError | crate::storage::settings::Status::FormatError) {
                self.pump();
                return false;
            }
        }
        if let Some(session) = self.manager.find(network_id) {
            if session.state() == SessionState::Registered
                && self.reducer.server_features(network_id).monitor_advertised()
            {
                session.send_monitor('C', &[]);
                self.pump();
            }
        }
        if self.manager.is_live(network_id) {
            self.disconnect(network_id);
        }
        self.manager.discard(network_id);
        self.autoaway_forget_network(network_id);
        self.read_marker_forget_network(network_id);
        self.monitor_forget_presence(network_id);
        self.forget_channel_list(network_id);
        self.scroll_places.forget(network_id);
        self.open_directs.forget(network_id);
        self.playback_times.borrow_mut().forget(network_id);
        self.playback.borrow_mut().drop_snapshot(network_id);
        self.closed.borrow_mut().forget(network_id);
        self.open_directs_motd_seen.remove(network_id);
        self.reducer.forget_network(network_id);
        self.cancelled_joins.retain(|key| key.network_id != network_id);
        self.console.clear(network_id);
        self.last_errors.remove(network_id);
        self.capabilities.remove(network_id);
        self.applied_profile_avatars.remove(network_id);
        self.current_nicks.remove(network_id);
        self.inbox.purge_network(network_id);
        self.network_order.retain(|id| id != network_id);
        self.network_collapsed.remove(network_id);
        self.persist_network_layout();
        if self.console_network == network_id {
            self.console_network.clear();
            self.console_open = false;
        }
        if self
            .selected
            .as_ref()
            .is_some_and(|key| key.network_id == network_id)
        {
            self.selected = None;
            self.selected_target.clear();
        }
        self.queue(viewnotify::reset_all());
        self.queue(vec![ViewNotify::Status]);
        self.pump();
        known
    }

    /// Moves a network `delta` places up (negative) or down (positive) in the
    /// sidebar (`MoveNetwork`). False when the network is unknown, the move is zero,
    /// or it would leave the list.
    pub fn move_network(&mut self, network_id: &str, delta: isize) -> bool {
        let mut order = self.network_order();
        let Some(index) = order.iter().position(|id| id == network_id) else {
            return false;
        };
        if delta == 0 {
            return false;
        }
        let Some(target) = index.checked_add_signed(delta) else {
            return false;
        };
        if target >= order.len() {
            return false;
        }
        let moved = order.remove(index);
        order.insert(target, moved);
        self.set_network_order(&order);
        true
    }
}
