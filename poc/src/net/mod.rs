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
//! thread. The wire between them is `wire`. Client certificates are out of scope.

// Phase 11: wired in step 8. Until the controller opens a socket, nothing outside the
// tests reaches this module, so its items and re-exports look unused to a plain build.
#![allow(dead_code, unused_imports)]

mod dial;
mod hub;
mod tls;
mod transport;
mod wire;
mod worker;

#[cfg(test)]
mod tests;

pub use hub::{NetHub, NetOptions};
pub use transport::NetTransport;
pub use wire::{ConnId, Delivery};
