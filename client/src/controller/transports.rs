//! The seam where a network's session gets its transport. It mirrors the
//! transport choice in `tui/internal/controller/controller.go` (`AddSession`) and
//! the Connect sheet connect path. The live factory is `net::NetTransports`,
//! which hands out socket transports from the net hub
//! (`tui/internal/session/transport_net.go`). `NoTransports` refuses every
//! network: the tests use it, and so does the demo for a network it does not
//! seed. Seeded demo networks never reach this factory: `Controller::start`
//! gives them a fresh loopback and the `DemoServer`.

use std::rc::Rc;

use crate::session::transport::Transport;

/// The Status line and last error for a network a factory refuses.
pub const LIVE_IRC_UNAVAILABLE: &str = "Live IRC is not built yet";

/// Opens the transport a network's session runs over.
pub trait TransportFactory {
    /// A transport for `network_id`'s next session, or the reason there is none.
    /// The error text goes to that network's Status and last error.
    fn open(&self, network_id: &str) -> Result<Rc<dyn Transport>, String>;
}

/// The factory that refuses every network with `LIVE_IRC_UNAVAILABLE`.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoTransports;

impl TransportFactory for NoTransports {
    fn open(&self, _network_id: &str) -> Result<Rc<dyn Transport>, String> {
        Err(LIVE_IRC_UNAVAILABLE.to_owned())
    }
}
