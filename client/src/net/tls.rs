//! TLS client policy: the trust roots, the server name, and how a failed handshake maps
//! to the transport's error kinds. The message text follows `tlsFailureMessage` in
//! Go's `transport_net.go`.
//!
//! Client certificates (CertFP and SASL EXTERNAL) are out of scope here. The config
//! offers no client identity, so a server that demands one fails the handshake.

use std::io;
use std::sync::{Arc, OnceLock};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::ring::default_provider;
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, WebPkiSupportedAlgorithms};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{
    CertificateError, ClientConfig, DigitallySignedStruct, Error as RustlsError, RootCertStore,
    SignatureScheme,
};

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

/// A client config that trusts exactly the first certificate in `pem` by its bytes.
/// The server name is not checked; the pin is the trust decision. Used by live ircd
/// tests (and the net test peer) for `tests/live/certs/live.pem`: webpki refuses that
/// self-signed leaf as a CA root, so chain validation is skipped.
pub fn client_config_trusting_pem(pem: &[u8]) -> Result<Arc<ClientConfig>, String> {
    let pinned = CertificateDer::pem_slice_iter(pem)
        .next()
        .ok_or_else(|| "PEM has no certificate".to_owned())?
        .map_err(|error| format!("PEM certificate: {error}"))?;
    let provider = default_provider();
    let verifier = PinnedCertificate {
        pinned,
        algorithms: provider.signature_verification_algorithms,
    };
    let config = ClientConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .map_err(|error| format!("TLS protocol versions: {error}"))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    Ok(Arc::new(config))
}

/// Trusts exactly one server certificate by its bytes. Handshake signatures are still
/// checked with the provider's algorithms.
#[derive(Debug)]
struct PinnedCertificate {
    pinned: CertificateDer<'static>,
    algorithms: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for PinnedCertificate {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if end_entity.as_ref() == self.pinned.as_ref() {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::InvalidCertificate(
                CertificateError::UnknownIssuer,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(message, cert, dss, &self.algorithms)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(message, cert, dss, &self.algorithms)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.algorithms.supported_schemes()
    }
}

/// The system trust store, with the bundled roots as the fallback for an empty store.
fn system_roots() -> RootCertStore {
    let native = rustls_native_certs::load_native_certs();
    for error in &native.errors {
        log::warn!("loading a system root certificate: {error}");
    }
    let mut roots = RootCertStore::empty();
    let (added, ignored) = roots.add_parsable_certificates(native.certs);
    log::info!("loaded {added} system root certificates, {ignored} unparsable");
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
