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
