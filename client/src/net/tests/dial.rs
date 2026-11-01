//! How a resolver failure reads to the session. A name that does not exist is Go's
//! `lookup HOST: no such host`. Any other failure keeps the OS's own text, so a resolver
//! outage is not reported as a missing host.

use std::io;
use std::time::Duration;

use crate::net::dial::resolve_failure;
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

#[cfg(unix)]
mod gai {
    use super::*;
    use crate::net::resolve::io_error_from_gai_code;

    #[test]
    fn noname_and_nodata_map_to_not_found() {
        for code in [libc::EAI_NONAME, libc::EAI_NODATA] {
            assert_eq!(
                io_error_from_gai_code(code).kind(),
                io::ErrorKind::NotFound,
                "code {code}"
            );
            let mapped = resolve_failure(HOST, &io_error_from_gai_code(code));
            assert_eq!(mapped.message, "lookup irc.example: no such host");
        }
    }

    #[test]
    fn again_and_fail_do_not_map_to_not_found() {
        for code in [libc::EAI_AGAIN, libc::EAI_FAIL] {
            let error = io_error_from_gai_code(code);
            assert_ne!(error.kind(), io::ErrorKind::NotFound, "code {code}");
            let mapped = resolve_failure(HOST, &error);
            assert!(!mapped.message.contains("no such host"), "{}", mapped.message);
        }
    }
}

/// Exercises production DNS when the resolver returns NXDOMAIN for `.invalid` names.
#[test]
#[ignore = "requires a resolver that does not synthesize addresses for .invalid"]
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
    let mapped = resolve_failure(host, &error);
    assert_eq!(mapped.kind, TransportErrorKind::Resolve);
    assert_eq!(mapped.message, "lookup nonexistent.invalid: no such host");
}

/// A resolver that blocks on a worker thread must not prevent other tasks on the runtime
/// from making progress.
#[test]
fn spawn_blocking_resolve_leaves_runtime_responsive() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    rt.block_on(async {
        let blocking = tokio::task::spawn_blocking(|| {
            std::thread::sleep(Duration::from_millis(400));
            Ok::<(), io::Error>(())
        });
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(
            !blocking.is_finished(),
            "short sleep should finish before a blocking resolve worker"
        );
    });
}
