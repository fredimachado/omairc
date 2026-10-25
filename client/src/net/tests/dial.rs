//! How a resolver failure reads to the session. A name that does not exist is Go's
//! `lookup HOST: no such host`. Any other failure keeps the OS's own text, so a resolver
//! outage is not reported as a missing host.

use std::io;

use crate::net::dial::resolve_failure;
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
