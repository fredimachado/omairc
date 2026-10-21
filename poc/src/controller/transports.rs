//! The seam where a network's session gets its transport. It mirrors the
//! transport choice in `tui/internal/controller/controller.go` (`AddSession`) and
//! the connect path the Connect sheet drives in Phase 6. The socket transport
//! (`tui/internal/session/transport_net.go`) is Phase 11, so `LiveTransports`
//! refuses every live network. Seeded demo networks never reach this factory:
//! `Controller::start` gives them a fresh loopback and the `DemoServer`.

use std::rc::Rc;

use crate::session::transport::Transport;

/// The Status line and last error for a live network, until the socket
/// transport lands.
pub const LIVE_IRC_UNAVAILABLE: &str = "Live IRC is not built yet";

/// Opens the transport a network's session runs over.
pub trait TransportFactory {
    /// A transport for `network_id`'s next session, or the reason there is none.
    /// The error text goes to that network's Status and last error.
    fn open(&self, network_id: &str) -> Result<Rc<dyn Transport>, String>;
}

/// The factory for the build without a socket transport: every live network is
/// refused with `LIVE_IRC_UNAVAILABLE`.
#[derive(Clone, Copy, Debug, Default)]
pub struct LiveTransports;

impl TransportFactory for LiveTransports {
    fn open(&self, _network_id: &str) -> Result<Rc<dyn Transport>, String> {
        Err(LIVE_IRC_UNAVAILABLE.to_owned())
    }
}
