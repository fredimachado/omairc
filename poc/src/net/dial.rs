//! Resolving and dialing a host, the TCP half of `DialContext` in Go's `transport_net.go`.
//!
//! Each resolved address gets one attempt, in order. The attempts share the connect
//! budget: each takes an equal share of what is left, so an address that hangs cannot
//! starve the ones after it. Expiry of the whole budget reads as a timeout.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use socket2::{SockRef, TcpKeepalive};
use tokio::net::{lookup_host, TcpStream};
use tokio::time::timeout;

use crate::session::transport::{TransportError, TransportErrorKind};

/// The keepalive idle time, the same value Go's dialer leaves on by default.
const KEEPALIVE_IDLE: Duration = Duration::from_secs(60);

/// The text of a connect that ran out of time, the same as the loopback's.
pub(super) const TIMED_OUT: &str = "Connection timed out";

/// Resolves `host` and connects to `port` within `budget`. The stream comes back with
/// `TCP_NODELAY` and keepalive set.
pub(super) async fn connect(
    host: &str,
    port: u16,
    budget: Duration,
) -> Result<TcpStream, TransportError> {
    let deadline = Instant::now() + budget;
    match timeout(budget, dial(host, port, deadline)).await {
        Ok(result) => result,
        Err(_elapsed) => Err(timed_out()),
    }
}

async fn dial(host: &str, port: u16, deadline: Instant) -> Result<TcpStream, TransportError> {
    let addresses: Vec<SocketAddr> = match lookup_host((host, port)).await {
        Ok(found) => found.collect(),
        Err(error) => {
            log::debug!("resolving {host} failed: {error}");
            Vec::new()
        }
    };
    if addresses.is_empty() {
        return Err(TransportError {
            kind: TransportErrorKind::Resolve,
            message: format!("lookup {host}: no such host"),
        });
    }

    let mut last_failure = None;
    for (index, address) in addresses.iter().enumerate() {
        let remaining_attempts = u32::try_from(addresses.len() - index).unwrap_or(u32::MAX);
        let share = deadline.saturating_duration_since(Instant::now()) / remaining_attempts;
        match timeout(share, TcpStream::connect(address)).await {
            Ok(Ok(stream)) => {
                tune(&stream);
                return Ok(stream);
            }
            Ok(Err(error)) => {
                last_failure = Some(TransportError {
                    kind: TransportErrorKind::Network,
                    message: format!("dial tcp {address}: {error}"),
                });
            }
            Err(_elapsed) => last_failure = Some(timed_out()),
        }
    }
    Err(last_failure.unwrap_or_else(timed_out))
}

/// Turns on `TCP_NODELAY` and keepalive. A socket that refuses either still works, so
/// the failure is logged and the connection goes on.
fn tune(stream: &TcpStream) {
    if let Err(error) = stream.set_nodelay(true) {
        log::warn!("setting TCP_NODELAY failed: {error}");
    }
    let keepalive = TcpKeepalive::new().with_time(KEEPALIVE_IDLE);
    if let Err(error) = SockRef::from(stream).set_tcp_keepalive(&keepalive) {
        log::warn!("setting TCP keepalive failed: {error}");
    }
}

/// The timeout error, shared with the worker's handshake deadline.
pub(super) fn timed_out() -> TransportError {
    TransportError {
        kind: TransportErrorKind::Timeout,
        message: TIMED_OUT.to_owned(),
    }
}
