//! The `HttpFetch` seam (plan rule 7): the one way the PoC reaches the network
//! for its desktop features. The update check and the installer download use it
//! now, and the avatar fetch will use it later. Go twin: none, since the TUI
//! calls `net/http` directly. C++: `QNetworkAccessManager` as used in
//! `src/omaircupdatecheck.cpp` and `src/irc/ircavatarstore.cpp`.
//!
//! `UreqFetch` is the one implementation that opens a socket. It blocks, so
//! callers run it on `cx.background_executor()`. It never follows a redirect
//! itself: `get_following` walks each hop, so the caller's policy judges every
//! target before it is requested. `FakeFetch` is the test double and serves
//! canned replies.

use std::fmt;
use std::time::Duration;

use ureq::Agent;
use url::Url;

/// One GET. `max_body` caps the bytes a 2xx reply may send. A longer body is a
/// transport failure, so a hostile peer cannot grow the buffer without bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub timeout: Duration,
    pub max_body: u64,
}

impl FetchRequest {
    pub fn get(url: &str, timeout: Duration, max_body: u64) -> Self {
        Self {
            url: url.to_string(),
            headers: Vec::new(),
            timeout,
            max_body,
        }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }
}

/// The status and the one header a redirect needs. A 2xx body is not returned
/// here: it goes to the caller's sink as it arrives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchReply {
    pub status: u16,
    pub location: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// The URL did not parse.
    InvalidUrl(String),
    /// DNS, TLS, connect, timeout, a reset, or a body over `max_body`.
    Transport(String),
    /// The body sink stopped the transfer.
    Stopped,
    /// A redirect the policy refused, or more hops than were allowed.
    Redirect(String),
}

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUrl(reason) => write!(f, "invalid URL: {reason}"),
            Self::Transport(reason) => write!(f, "transfer failed: {reason}"),
            Self::Stopped => write!(f, "transfer stopped by the reader"),
            Self::Redirect(reason) => write!(f, "redirect refused: {reason}"),
        }
    }
}

/// The seam. `get` makes one request and does not follow redirects. The bytes
/// of a 2xx body go to `on_body` in order; returning false stops the transfer
/// and the call fails with `Stopped`. Any other status delivers no body.
pub trait HttpFetch: Send + Sync {
    fn get(
        &self,
        request: &FetchRequest,
        on_body: &mut dyn FnMut(&[u8]) -> bool,
    ) -> Result<FetchReply, FetchError>;
}

/// Where a `get_following` call finished, and the status it got there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fetched {
    pub url: Url,
    pub status: u16,
}

/// Follows redirects one hop at a time. `allow` judges each target before it is
/// requested, and a refused hop or one past `max_redirects` fails the call. A
/// redirect with no `Location` ends the chain and returns its status as is.
pub fn get_following(
    fetch: &dyn HttpFetch,
    request: FetchRequest,
    max_redirects: u32,
    allow: &dyn Fn(&Url) -> bool,
    on_body: &mut dyn FnMut(&[u8]) -> bool,
) -> Result<Fetched, FetchError> {
    let mut request = request;
    let mut current =
        Url::parse(&request.url).map_err(|err| FetchError::InvalidUrl(err.to_string()))?;
    let mut hops = 0;
    loop {
        let reply = fetch.get(&request, on_body)?;
        let location = match reply.location.as_deref() {
            Some(location) if is_redirect(reply.status) => location,
            _ => {
                return Ok(Fetched {
                    url: current,
                    status: reply.status,
                })
            }
        };
        if hops >= max_redirects {
            return Err(FetchError::Redirect(format!(
                "more than {max_redirects} redirects"
            )));
        }
        hops += 1;
        let next = current
            .join(location)
            .map_err(|err| FetchError::Redirect(err.to_string()))?;
        if !allow(&next) {
            return Err(FetchError::Redirect(format!("refused {next}")));
        }
        request.url = next.to_string();
        current = next;
    }
}

fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// The ureq implementation. Each call builds its own agent, so its timeout and
/// its redirect policy (none: every 3xx comes back as a reply) stay per request.
pub struct UreqFetch;

impl HttpFetch for UreqFetch {
    fn get(
        &self,
        request: &FetchRequest,
        on_body: &mut dyn FnMut(&[u8]) -> bool,
    ) -> Result<FetchReply, FetchError> {
        let config = Agent::config_builder()
            .timeout_global(Some(request.timeout))
            .max_redirects(0)
            .http_status_as_error(false)
            .build();
        let agent = Agent::new_with_config(config);
        let mut builder = agent.get(request.url.as_str());
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        let mut response = builder
            .call()
            .map_err(|err| FetchError::Transport(err.to_string()))?;
        let status = response.status().as_u16();
        let location = response
            .headers()
            .get("location")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        if (200..300).contains(&status) {
            let mut reader = response
                .body_mut()
                .with_config()
                .limit(request.max_body)
                .reader();
            let mut buffer = [0u8; 16 * 1024];
            loop {
                let read = std::io::Read::read(&mut reader, &mut buffer)
                    .map_err(|err| FetchError::Transport(err.to_string()))?;
                if read == 0 {
                    break;
                }
                if !on_body(&buffer[..read]) {
                    return Err(FetchError::Stopped);
                }
            }
        }
        Ok(FetchReply { status, location })
    }
}

#[cfg(test)]
pub use fake::FakeFetch;

#[cfg(test)]
mod fake {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;

    /// A canned HTTP peer for tests. Each URL maps to one reply. A URL with no
    /// route fails as a transport error, so an unexpected request is visible.
    #[derive(Default)]
    pub struct FakeFetch {
        routes: Mutex<HashMap<String, Route>>,
        requests: Mutex<Vec<FetchRequest>>,
    }

    #[derive(Clone)]
    struct Route {
        reply: FetchReply,
        body: Vec<u8>,
        chunk: usize,
        failure: Option<String>,
    }

    impl FakeFetch {
        pub fn new() -> Self {
            Self::default()
        }

        /// Answers `url` with `status` and `body` in one chunk.
        pub fn serve(&self, url: &str, status: u16, body: &[u8]) {
            self.serve_chunked(url, status, body, body.len().max(1));
        }

        /// Answers `url` with `status` and `body`, delivered `chunk` bytes at a time.
        pub fn serve_chunked(&self, url: &str, status: u16, body: &[u8], chunk: usize) {
            self.insert(
                url,
                Route {
                    reply: FetchReply {
                        status,
                        location: None,
                    },
                    body: body.to_vec(),
                    chunk: chunk.max(1),
                    failure: None,
                },
            );
        }

        /// Answers `url` with a redirect to `location`.
        pub fn redirect(&self, url: &str, status: u16, location: &str) {
            self.insert(
                url,
                Route {
                    reply: FetchReply {
                        status,
                        location: Some(location.to_string()),
                    },
                    body: Vec::new(),
                    chunk: 1,
                    failure: None,
                },
            );
        }

        /// Fails `url` with a transport error.
        pub fn fail(&self, url: &str, message: &str) {
            self.insert(
                url,
                Route {
                    reply: FetchReply {
                        status: 0,
                        location: None,
                    },
                    body: Vec::new(),
                    chunk: 1,
                    failure: Some(message.to_string()),
                },
            );
        }

        /// Every request the seam received, in order.
        pub fn requests(&self) -> Vec<FetchRequest> {
            self.requests.lock().unwrap().clone()
        }

        fn insert(&self, url: &str, route: Route) {
            self.routes.lock().unwrap().insert(url.to_string(), route);
        }
    }

    impl HttpFetch for FakeFetch {
        fn get(
            &self,
            request: &FetchRequest,
            on_body: &mut dyn FnMut(&[u8]) -> bool,
        ) -> Result<FetchReply, FetchError> {
            self.requests.lock().unwrap().push(request.clone());
            let route = self.routes.lock().unwrap().get(&request.url).cloned();
            let Some(route) = route else {
                return Err(FetchError::Transport(format!(
                    "no route for {}",
                    request.url
                )));
            };
            if let Some(message) = route.failure {
                return Err(FetchError::Transport(message));
            }
            if (200..300).contains(&route.reply.status) {
                if route.body.len() as u64 > request.max_body {
                    return Err(FetchError::Transport("body over the limit".to_string()));
                }
                for piece in route.body.chunks(route.chunk) {
                    if !on_body(piece) {
                        return Err(FetchError::Stopped);
                    }
                }
            }
            Ok(route.reply)
        }
    }
}

/// A one-shot loopback server for the ureq tests. It answers the first
/// connection with `status_line`, `extra_headers` and `body`, and the handle
/// yields the request head the client sent.
#[cfg(test)]
pub(crate) fn serve_once(
    status_line: &'static str,
    extra_headers: &'static str,
    body: Vec<u8>,
) -> (String, std::thread::JoinHandle<String>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind loopback");
    let url = format!("http://{}/", listener.local_addr().expect("local addr"));
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept");
        let mut head = Vec::new();
        let mut chunk = [0u8; 512];
        while !head.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut chunk).expect("read request");
            if read == 0 {
                break;
            }
            head.extend_from_slice(&chunk[..read]);
        }
        let reply = format!(
            "HTTP/1.1 {status_line}\r\n{extra_headers}Content-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        // A client that stops reading early (an oversized body) makes a write fail.
        let _ = stream.write_all(reply.as_bytes());
        let _ = stream.write_all(&body);
        String::from_utf8_lossy(&head).into_owned()
    });
    (url, handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    const LIMIT: u64 = 1024 * 1024;

    fn request(url: &str) -> FetchRequest {
        FetchRequest::get(url, Duration::from_secs(5), LIMIT)
    }

    /// Runs `get_following` and returns the result with the body it delivered.
    fn follow(
        fake: &FakeFetch,
        url: &str,
        max_redirects: u32,
        allow: &dyn Fn(&Url) -> bool,
    ) -> (Result<Fetched, FetchError>, Vec<u8>) {
        let body = RefCell::new(Vec::new());
        let result = get_following(fake, request(url), max_redirects, allow, &mut |bytes| {
            body.borrow_mut().extend_from_slice(bytes);
            true
        });
        (result, body.into_inner())
    }

    #[test]
    fn get_following_follows_a_redirect_and_checks_each_hop() {
        let fake = FakeFetch::new();
        fake.redirect("https://a.example/x", 302, "https://b.example/y");
        fake.serve("https://b.example/y", 200, b"hi");
        let (result, body) = follow(&fake, "https://a.example/x", 3, &|url| {
            url.host_str() == Some("b.example")
        });
        let fetched = result.expect("followed");
        assert_eq!(fetched.status, 200);
        assert_eq!(fetched.url.as_str(), "https://b.example/y");
        assert_eq!(body, b"hi");
        let urls: Vec<String> = fake.requests().into_iter().map(|r| r.url).collect();
        assert_eq!(urls, ["https://a.example/x", "https://b.example/y"]);
    }

    #[test]
    fn get_following_resolves_a_relative_location() {
        let fake = FakeFetch::new();
        fake.redirect("https://a.example/dir/x", 301, "../final/y");
        fake.serve("https://a.example/final/y", 200, b"ok");
        let (result, _) = follow(&fake, "https://a.example/dir/x", 3, &|_| true);
        assert_eq!(
            result.expect("followed").url.as_str(),
            "https://a.example/final/y"
        );
    }

    #[test]
    fn get_following_never_requests_a_refused_hop() {
        let fake = FakeFetch::new();
        fake.redirect("https://a.example/x", 302, "https://evil.example/setup.exe");
        fake.serve("https://evil.example/setup.exe", 200, b"bad");
        let (result, body) = follow(&fake, "https://a.example/x", 3, &|url| {
            url.host_str() != Some("evil.example")
        });
        assert!(matches!(result, Err(FetchError::Redirect(_))), "{result:?}");
        assert!(body.is_empty());
        assert_eq!(fake.requests().len(), 1, "the refused hop was never requested");
    }

    #[test]
    fn get_following_stops_after_the_hop_limit() {
        let fake = FakeFetch::new();
        fake.redirect("https://a.example/loop", 302, "https://a.example/loop");
        let (result, _) = follow(&fake, "https://a.example/loop", 2, &|_| true);
        assert!(matches!(result, Err(FetchError::Redirect(_))), "{result:?}");
        assert_eq!(fake.requests().len(), 3, "the first request plus two hops");
    }

    #[test]
    fn get_following_returns_a_redirect_without_location_as_is() {
        let fake = FakeFetch::new();
        fake.serve("https://a.example/x", 302, b"");
        let (result, _) = follow(&fake, "https://a.example/x", 3, &|_| true);
        assert_eq!(result.expect("status returned").status, 302);
    }

    #[test]
    fn get_following_rejects_an_unparsable_start_url() {
        let fake = FakeFetch::new();
        let (result, _) = follow(&fake, "not a url", 3, &|_| true);
        assert!(matches!(result, Err(FetchError::InvalidUrl(_))), "{result:?}");
        assert!(fake.requests().is_empty());
    }

    #[test]
    fn a_sink_that_stops_fails_the_transfer() {
        let fake = FakeFetch::new();
        fake.serve("https://a.example/x", 200, b"0123456789");
        let result = fake.get(&request("https://a.example/x"), &mut |_| false);
        assert_eq!(result, Err(FetchError::Stopped));
    }

    #[test]
    fn bodies_arrive_in_order_in_chunks() {
        let fake = FakeFetch::new();
        fake.serve_chunked("https://a.example/x", 200, b"0123456789", 3);
        let pieces = RefCell::new(Vec::new());
        let reply = fake
            .get(&request("https://a.example/x"), &mut |bytes| {
                pieces.borrow_mut().push(bytes.to_vec());
                true
            })
            .expect("served");
        assert_eq!(reply.status, 200);
        assert_eq!(pieces.borrow().len(), 4);
        assert_eq!(pieces.borrow().concat(), b"0123456789");
    }

    #[test]
    fn an_oversized_body_is_a_transport_failure() {
        let fake = FakeFetch::new();
        fake.serve("https://a.example/x", 200, b"12345");
        let small = FetchRequest::get("https://a.example/x", Duration::from_secs(5), 3);
        let result = fake.get(&small, &mut |_| true);
        assert!(matches!(result, Err(FetchError::Transport(_))), "{result:?}");
    }

    #[test]
    fn a_non_2xx_reply_delivers_no_body() {
        let fake = FakeFetch::new();
        fake.serve("https://a.example/x", 404, b"not here");
        let mut delivered = false;
        let reply = fake
            .get(&request("https://a.example/x"), &mut |_| {
                delivered = true;
                true
            })
            .expect("served");
        assert_eq!(reply.status, 404);
        assert!(!delivered);
    }

    #[test]
    fn unrouted_and_failed_urls_fail_closed() {
        let fake = FakeFetch::new();
        fake.fail("https://a.example/down", "connection reset");
        assert!(matches!(
            fake.get(&request("https://a.example/down"), &mut |_| true),
            Err(FetchError::Transport(_))
        ));
        assert!(matches!(
            fake.get(&request("https://a.example/unknown"), &mut |_| true),
            Err(FetchError::Transport(_))
        ));
    }

    #[test]
    fn ureq_fetch_reads_a_served_body() {
        let (url, server) = serve_once("200 OK", "", b"hello".to_vec());
        let mut body = Vec::new();
        let reply = UreqFetch
            .get(&request(&url), &mut |bytes| {
                body.extend_from_slice(bytes);
                true
            })
            .expect("served");
        server.join().expect("server thread");
        assert_eq!(reply.status, 200);
        assert_eq!(body, b"hello");
    }

    #[test]
    fn ureq_fetch_sends_its_headers() {
        let (url, server) = serve_once("200 OK", "", Vec::new());
        let fetch_request = request(&url).header("X-Test", "1");
        UreqFetch
            .get(&fetch_request, &mut |_| true)
            .expect("served");
        let head = server.join().expect("server thread").to_ascii_lowercase();
        assert!(head.contains("x-test: 1"), "head was {head}");
    }

    #[test]
    fn ureq_fetch_returns_a_redirect_without_following_it() {
        let (url, server) = serve_once(
            "302 Found",
            "Location: https://elsewhere.example/x\r\n",
            Vec::new(),
        );
        let mut delivered = false;
        let reply = UreqFetch
            .get(&request(&url), &mut |_| {
                delivered = true;
                true
            })
            .expect("reply");
        server.join().expect("server thread");
        assert_eq!(reply.status, 302);
        assert_eq!(reply.location.as_deref(), Some("https://elsewhere.example/x"));
        assert!(!delivered);
    }

    #[test]
    fn ureq_fetch_status_errors_are_replies_not_failures() {
        let (url, server) = serve_once("404 Not Found", "", br#"{"message":"gone"}"#.to_vec());
        let reply = UreqFetch
            .get(&request(&url), &mut |_| true)
            .expect("a 404 is a reply");
        server.join().expect("server thread");
        assert_eq!(reply.status, 404);
    }

    #[test]
    fn ureq_fetch_fails_when_nothing_listens() {
        // Bind then drop, so the port is free and the connect is refused.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .expect("bind")
            .local_addr()
            .expect("addr")
            .port();
        let result = UreqFetch.get(
            &request(&format!("http://127.0.0.1:{port}/")),
            &mut |_| true,
        );
        assert!(matches!(result, Err(FetchError::Transport(_))), "{result:?}");
    }
}
