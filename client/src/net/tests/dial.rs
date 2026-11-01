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

/// A production-style resolve host must not block the runtime while `blocking` sleeps.
#[cfg(unix)]
#[test]
fn system_resolve_host_leaves_runtime_responsive() {
    use crate::net::hub::resolve_host_with_blocking;

    let resolve = resolve_host_with_blocking(|_host, _port| {
        std::thread::sleep(Duration::from_millis(400));
        Ok(Vec::new())
    });

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    rt.block_on(async {
        use std::sync::atomic::{AtomicU32, Ordering};
        use std::sync::Arc;
        use std::time::Instant;

        let ticks = Arc::new(AtomicU32::new(0));
        let counter = ticks.clone();
        let ticker = tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(10)).await;
                counter.fetch_add(1, Ordering::Relaxed);
            }
        });

        let resolve = resolve("hang.example".to_owned(), 6667);
        tokio::pin!(resolve);
        let deadline = Instant::now() + Duration::from_millis(150);
        let mut saw_ticks_while_pending = false;
        while Instant::now() < deadline {
            match tokio::time::timeout(Duration::from_millis(10), &mut resolve).await {
                Ok(_) => break,
                Err(_) => {
                    if ticks.load(Ordering::Relaxed) >= 3 {
                        saw_ticks_while_pending = true;
                        break;
                    }
                }
            }
        }
        ticker.abort();
        assert!(
            saw_ticks_while_pending,
            "ticker should advance while resolve is still pending on a worker thread"
        );
        let _ = resolve.await;
    });
}
