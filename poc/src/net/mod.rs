//! The network layer: the only place in the PoC that holds sockets, TLS sessions or
//! the async runtime. `tests/net_boundary.rs` keeps it that way.
//!
//! The gate enforces three rules:
//!
//! - `tokio`, `tokio_rustls`, `rustls`, `rustls_native_certs` and `webpki_roots` are
//!   named only under `src/net/`.
//! - `std::net` is forbidden in `irc/`, `session/`, `controller/`, `model/`, `ui/` and
//!   `storage/`, so those layers never open a socket. `desktop/` keeps its loopback
//!   tests.
//! - Nothing under `src/net/` names `gpui`, `crate::ui` or `crate::controller`, except
//!   `controller::transports::TransportFactory` in `factory.rs`, the one seam the
//!   controller hands transports through.
//!
//! The socket transport: `NetHub` owns the `omairc-net` thread, `NetTransport` is the
//! UI-side `Transport` registered with it, and `worker` runs one connection on that
//! thread. The wire between them is `wire`. `NetTransports` is the factory the shell
//! hands the controller, and it opens each network's transport on the hub. Client
//! certificates are out of scope.

mod dial;
mod factory;
mod hub;
mod tls;
mod transport;
mod wire;
mod worker;

#[cfg(test)]
mod tests;

pub use factory::NetTransports;
pub use hub::NetHub;
pub use wire::Delivery;

// The tests build hubs with other options and name the transport type; nothing outside
// `net/` needs either name.
#[cfg(test)]
pub use hub::NetOptions;
#[cfg(test)]
pub use transport::NetTransport;
