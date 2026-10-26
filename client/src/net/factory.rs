//! The live transport factory: the one place the controller's transport seam meets the net
//! hub. Each network's session asks for a transport, and gets a socket transport the hub
//! has registered, so the results of its connection come back to it.
//! This file is the only one under `src/net/` that names the controller's seam, by the
//! `TransportFactory` path `tests/net_boundary.rs` allows.

use std::rc::Rc;

use super::hub::NetHub;
use crate::controller::transports::TransportFactory;
use crate::session::transport::Transport;

/// Hands every network's session a socket transport registered with `hub`.
pub struct NetTransports {
    hub: Rc<NetHub>,
}

impl NetTransports {
    pub fn new(hub: Rc<NetHub>) -> Self {
        Self { hub }
    }
}

impl TransportFactory for NetTransports {
    /// A fresh transport on the hub. The network id names nothing here: the hub gives each
    /// connection its own id, and the session holds the transport from then on.
    fn open(&self, _network_id: &str) -> Result<Rc<dyn Transport>, String> {
        let transport: Rc<dyn Transport> = self.hub.register();
        Ok(transport)
    }
}
