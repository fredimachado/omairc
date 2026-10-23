//! TLS client policy: the trust roots, the server name, and how a failed handshake maps
//! to the transport's error kinds. The message text follows `tlsFailureMessage` in
//! Go's `transport_net.go`.
//!
//! Client certificates (CertFP and SASL EXTERNAL) are out of scope here. The config
//! offers no client identity, so a server that demands one fails the handshake.

use std::io;
use std::sync::{Arc, OnceLock};

use rustls::crypto::ring::default_provider;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, Error as RustlsError, RootCertStore};

use crate::session::transport::{TransportError, TransportErrorKind};

/// The process-wide client config. It trusts the system roots, or the bundled Mozilla
/// roots when the system store is empty. It is built on first use.
pub(super) fn client_config() -> Arc<ClientConfig> {
    static CONFIG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CONFIG
        .get_or_init(|| config_with_roots(system_roots()))
        .clone()
}

/// A client config that trusts `roots` and nothing else. The ring provider is named
/// explicitly, so a process without a default rustls provider installed does not panic.
pub(super) fn config_with_roots(roots: RootCertStore) -> Arc<ClientConfig> {
    let config = ClientConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .expect("the ring provider supports the default protocol versions")
        .with_root_certificates(roots)
        .with_no_client_auth();
    Arc::new(config)
}

/// The system trust store, with the bundled roots as the fallback for an empty store.
fn system_roots() -> RootCertStore {
    let native = rustls_native_certs::load_native_certs();
    for error in &native.errors {
        log::warn!("loading a system root certificate: {error}");
    }
    let mut roots = RootCertStore::empty();
    let (added, ignored) = roots.add_parsable_certificates(native.certs);
    log::warn!("loaded {added} system root certificates, {ignored} unparsable");
    if roots.is_empty() {
        log::warn!("the system trust store is empty; trusting the bundled Mozilla roots");
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    }
    roots
}

/// The name the handshake checks the certificate against. IP literals work too.
pub(super) fn server_name(host: &str) -> Result<ServerName<'static>, TransportError> {
    ServerName::try_from(host.to_owned()).map_err(|_| TransportError {
        kind: TransportErrorKind::Tls,
        message: format!("invalid TLS server name: {host}"),
    })
}

/// Maps a failed handshake or TLS read to the transport error the session classifies.
/// tokio-rustls carries the rustls error inside the io error, so it is found there.
pub(super) fn map_error(error: io::Error) -> TransportError {
    match error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<RustlsError>())
    {
        Some(RustlsError::InvalidCertificate(cause)) => TransportError {
            kind: TransportErrorKind::Certificate,
            message: format!("TLS certificate error: {cause}"),
        },
        Some(other) => TransportError {
            kind: TransportErrorKind::Tls,
            message: other.to_string(),
        },
        None => TransportError {
            kind: TransportErrorKind::Network,
            message: error.to_string(),
        },
    }
}
