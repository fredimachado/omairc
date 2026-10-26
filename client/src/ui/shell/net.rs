//! The shell's side of the socket transport. The net thread's results arrive here one at a
//! time (`main.rs` spawns the loop), and `deliver_net` hands each to its transport, takes the
//! rest that queued behind it, and pumps the controller once for the batch. The controller
//! never learns about `net/`: it only sees the sessions' events, as it does for the demo.

use gpui::Context;

use super::Shell;
use crate::net::Delivery;

impl Shell {
    /// Delivers one result from the net hub, then every other result already queued, and pumps
    /// the controller once for all of them. The shell then repaints what the controller raised.
    pub fn deliver_net(&mut self, delivery: Delivery, cx: &mut Context<Self>) {
        let Some(hub) = self.net.clone() else {
            return;
        };
        hub.deliver(delivery);
        // A burst of bytes becomes one pump, not one per read.
        hub.drain_ready();
        self.controller.pump();
        self.drain_notifications(cx);
    }
}
