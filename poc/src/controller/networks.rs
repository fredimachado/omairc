//! The sidebar's network order and collapse state. Port of `NetworkOrder`,
//! `SetNetworkOrder`, `SetNetworkCollapsed`, `SetAllNetworksCollapsed`,
//! `IsNetworkCollapsed` and `MoveNetwork` in `tui/internal/controller/controller.go`
//! (Go), and the sidebar's order roles in `src/irc/irccontroller.cpp` (C++).
//!
//! The order and the collapse set live on the controller, because the sidebar reads
//! the controller's order rather than the connection's. Persisting them is Phase 8.

use std::collections::BTreeSet;

use crate::irc::viewnotify::ViewNotify;

use super::controller::Controller;

impl Controller {
    /// The network ids in display order (`NetworkOrder`): the recorded order first,
    /// keeping only networks that have a session, then any other registered network
    /// in id order.
    pub fn network_order(&self) -> Vec<String> {
        let registered = self.manager.network_ids();
        if self.network_order.is_empty() {
            return registered;
        }
        let mut seen = BTreeSet::new();
        let mut order = Vec::with_capacity(registered.len());
        for id in &self.network_order {
            if seen.contains(id) || self.manager.find(id).is_none() {
                continue;
            }
            seen.insert(id.clone());
            order.push(id.clone());
        }
        for id in registered {
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
        self.queue(vec![ViewNotify::Conversations]);
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
