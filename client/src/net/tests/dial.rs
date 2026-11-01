//! How a resolver failure reads to the session. A name that does not exist is Go's
//! `lookup HOST: no such host`. Any other failure keeps the OS's own text, so a resolver
//! outage is not reported as a missing host.

use std::io;

use crate::net::dial::{normalize_resolve_error, resolve_failure};
use crate::net::NetOptions;
use crate::session::transport::TransportErrorKind;

const HOST: &str = "irc.example";

#[test]
fn a_not_found_resolve_reads_as_no_such_host() {
    let error = resolve_failure(HOST, &io::Error::from(io::ErrorKind::NotFound));
    assert_eq!(error.kind, TransportErrorKind::Resolve);
    assert_eq!(error.message, "lookup irc.example: no such host");
}

#[test]
fn a_windows_host_not_found_reads_as_no_such_host() {
    let error = resolve_failure(HOST, &io::Error::from_raw_os_error(11001));
    assert_eq!(error.message, "lookup irc.example: no such host");
}

#[test]
fn other_resolver_failures_keep_the_os_text() {
    let error = resolve_failure(
        HOST,
        &io::Error::other("temporary failure in name resolution"),
    );
    assert_eq!(error.kind, TransportErrorKind::Resolve);
    assert_eq!(
        error.message,
        "lookup irc.example: temporary failure in name resolution"
    );
}

#[test]
fn a_try_again_resolve_is_not_no_such_host() {
    let error = resolve_failure(HOST, &io::Error::from_raw_os_error(11002));
    assert!(error.message.starts_with("lookup irc.example: "));
    assert!(!error.message.contains("no such host"), "{}", error.message);
}

/// Exercises the production resolver and the same normalization `dial` applies before
/// `resolve_failure`, without injecting `NotFound`.
#[test]
fn system_resolver_unknown_host_reads_as_no_such_host() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let resolve = NetOptions::default().resolve_host;
    let host = "nonexistent.invalid";
    let error = rt
        .block_on(async { resolve(host.to_owned(), 6667).await })
        .expect_err("resolver should fail for a missing host");
    let error = normalize_resolve_error(host, 6667, error);
    let mapped = resolve_failure(host, &error);
    assert_eq!(mapped.kind, TransportErrorKind::Resolve);
    assert_eq!(mapped.message, "lookup nonexistent.invalid: no such host");
}
