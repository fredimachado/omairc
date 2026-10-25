//! Live ircd harness: env from `bin/test-live`, a controller over the real net
//! transport, and wait helpers. Integration tests in `tests/live/` drive the cases;
//! the raw second peer (TLS) lives in that test crate so `rustls` stays under `net/`
//! for production sources.

mod client;
mod env;
mod wait;

pub use client::{unique_channel, unique_nick, LiveClient, LiveClientOptions};
pub use env::{
    live_cert_pem, live_daemons, live_enabled, live_host, CaseMappingExpect, DaemonInfo,
};
pub use wait::{message_command_count, message_has_command, wait_until, DEFAULT_TIMEOUT};
