//! Check for Updates for the About sheet, ported from `src/omaircupdatecheck.cpp`.
//! It reads the latest GitHub release, orders its version against the running
//! one, and words the single status line as Qt does. Only the version and the
//! release page are kept. No installer is downloaded: a newer version opens its
//! page in the browser.

use std::cmp::Ordering;
use std::time::Duration;

use url::Url;

use crate::desktop::httpfetch::{get_following, FetchRequest, HttpFetch};

/// Qt `omaircGithubRepoUrl()`.
pub const REPO_URL: &str = "https://github.com/fredimachado/omairc";
/// Qt `omaircGithubLatestReleaseApiUrl()`.
pub const LATEST_RELEASE_API_URL: &str =
    "https://api.github.com/repos/fredimachado/omairc/releases/latest";
/// The page used when the reply has no `html_url`, as Qt's parser falls back.
const LATEST_RELEASE_PAGE_URL: &str = "https://github.com/fredimachado/omairc/releases/latest";
/// The only path a release page may sit under.
const REPO_PATH: &str = "/fredimachado/omairc";
/// Qt `kUpdateCheckTimeoutMs`.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10);
/// Qt `kMaximumReleaseBytes`. A longer reply is a failed check.
const MAXIMUM_RELEASE_BYTES: u64 = 256 * 1024;

/// The latest release, reduced to what the sheet needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// The tag without its leading `v`.
    pub version: String,
    /// The release page. Always on the repository.
    pub html_url: String,
}

/// The About sheet's update state. Qt `OmaircUpdateCheck` status, without the
/// installer states the PoC does not reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpdateStatus {
    /// No check has run in this sheet, so the status line is hidden.
    Idle,
    Checking,
    UpToDate,
    Available {
        version: String,
        url: String,
    },
    Failed,
}

impl UpdateStatus {
    /// The status line text, or `None` when the line is hidden. Qt
    /// `OmaircUpdateCheck::message`.
    pub fn message(&self) -> Option<String> {
        Some(match self {
            Self::Idle => return None,
            Self::Checking => "Checking…".to_string(),
            Self::UpToDate => "Omairc is up to date.".to_string(),
            Self::Available { version, .. } => format!("Version {version} is available."),
            Self::Failed => "Could not check for updates.".to_string(),
        })
    }

    /// The page a click on the status line opens. Only a newer version has one.
    pub fn link(&self) -> Option<&str> {
        match self {
            Self::Available { url, .. } => Some(url),
            _ => None,
        }
    }
}

/// The User-Agent sent with the check. Qt `OmaircUpdateCheck::check`.
pub fn user_agent(current: &str) -> String {
    format!("Omairc/{current} (+{REPO_URL})")
}

/// Asks GitHub for the latest release and judges it against `current`. This blocks,
/// so the caller runs it on the background executor. The request goes through the
/// `HttpFetch` seam. A transport error, a redirect the policy refuses, a non-200
/// reply, an oversized body, or a reply that does not parse is `Failed`.
pub fn check_latest(fetch: &dyn HttpFetch, current: &str, api_url: &str) -> UpdateStatus {
    let request = FetchRequest::get(api_url, CHECK_TIMEOUT, MAXIMUM_RELEASE_BYTES)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", &user_agent(current));
    let mut body = Vec::new();
    let fetched = get_following(
        fetch,
        request,
        5,
        &|url: &Url| url.scheme() == "https",
        &mut |bytes| {
            if body.len() as u64 + bytes.len() as u64 > MAXIMUM_RELEASE_BYTES {
                return false;
            }
            body.extend_from_slice(bytes);
            true
        },
    );
    match fetched {
        Ok(fetched) => status_for_reply(current, fetched.status, &String::from_utf8_lossy(&body)),
        Err(_) => UpdateStatus::Failed,
    }
}

/// Judges one HTTP reply. Only a 200 that carries a release can say anything.
pub fn status_for_reply(current: &str, status: u16, body: &str) -> UpdateStatus {
    if status != 200 {
        return UpdateStatus::Failed;
    }
    match parse_release(body) {
        Some(release) => status_for_release(current, &release),
        None => UpdateStatus::Failed,
    }
}

/// A release at or below `current` is up to date. Qt `finishWithBody`.
pub fn status_for_release(current: &str, release: &Release) -> UpdateStatus {
    match compare_versions(current, &release.version) {
        Ordering::Less => UpdateStatus::Available {
            version: release.version.clone(),
            url: release.html_url.clone(),
        },
        Ordering::Equal | Ordering::Greater => UpdateStatus::UpToDate,
    }
}

/// Reads the release JSON that GitHub returns. Qt `omaircParseGithubRelease`
/// without the installer assets, which the PoC never fetches. `None` when the
/// body is not an object, has no tag, or its page is not on the repository.
pub fn parse_release(body: &str) -> Option<Release> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    let object = value.as_object()?;
    let tag = object.get("tag_name")?.as_str()?.trim();
    let version = strip_v(tag);
    if version.is_empty() {
        return None;
    }
    let html_url = object
        .get("html_url")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .unwrap_or(LATEST_RELEASE_PAGE_URL);
    if !release_url_is_safe(html_url) {
        return None;
    }
    Some(Release {
        version: version.to_string(),
        html_url: html_url.to_string(),
    })
}

/// Orders two version strings the way Qt's `omaircCompareVersions` does: the
/// numeric parts first, missing parts count as zero, then the suffix. An empty
/// suffix (a release) outranks alpha, beta and rc, and any other suffix sorts
/// lowest. A leading `v` and a `-` or `+` before the suffix are ignored.
pub fn compare_versions(left: &str, right: &str) -> Ordering {
    let a = parse_version(left);
    let b = parse_version(right);
    let count = a.numeric.len().max(b.numeric.len());
    for index in 0..count {
        let av = a.numeric.get(index).copied().unwrap_or(0);
        let bv = b.numeric.get(index).copied().unwrap_or(0);
        if av != bv {
            return av.cmp(&bv);
        }
    }
    suffix_rank(&a.suffix)
        .cmp(&suffix_rank(&b.suffix))
        .then_with(|| a.suffix.cmp(&b.suffix))
}

/// A release page must be on `github.com` under the repository, over https. The
/// host is matched without userinfo or a port, so anything odd fails closed.
fn release_url_is_safe(url: &str) -> bool {
    let Some(rest) = strip_https(url) else {
        return false;
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(authority_end);
    if !authority.eq_ignore_ascii_case("github.com") {
        return false;
    }
    let path = tail.split(['?', '#']).next().unwrap_or("");
    path == REPO_PATH
        || path
            .strip_prefix(REPO_PATH)
            .is_some_and(|after| after.starts_with('/'))
}

fn strip_https(url: &str) -> Option<&str> {
    let scheme = url.get(.."https://".len())?;
    scheme
        .eq_ignore_ascii_case("https://")
        .then(|| &url[scheme.len()..])
}

fn strip_v(tag: &str) -> &str {
    tag.strip_prefix(['v', 'V']).unwrap_or(tag)
}

/// The parts of one version string.
struct Parsed {
    numeric: Vec<u64>,
    suffix: String,
}

fn parse_version(text: &str) -> Parsed {
    let text = strip_v(text.trim());
    let bytes = text.as_bytes();
    let mut numeric = Vec::new();
    let mut index = 0;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        let mut value: u64 = 0;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            value = value
                .saturating_mul(10)
                .saturating_add(u64::from(bytes[index] - b'0'));
            index += 1;
        }
        numeric.push(value);
        if index < bytes.len() && bytes[index] == b'.' {
            index += 1;
            continue;
        }
        break;
    }
    // Only ASCII bytes were consumed, so `index` is a character boundary.
    let mut suffix = text[index..].to_lowercase();
    if suffix.starts_with(['-', '+']) {
        suffix.remove(0);
    }
    Parsed { numeric, suffix }
}

fn suffix_rank(suffix: &str) -> u8 {
    if suffix.is_empty() {
        100
    } else if suffix.starts_with("alpha") {
        10
    } else if suffix.starts_with("beta") {
        20
    } else if suffix.starts_with("rc") {
        30
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use crate::desktop::httpfetch::UreqFetch;

    /// A trimmed `releases/latest` reply, shaped as GitHub sends it.
    const LATEST_RELEASE: &str = r#"{
        "url": "https://api.github.com/repos/fredimachado/omairc/releases/123",
        "html_url": "https://github.com/fredimachado/omairc/releases/tag/v1.11.0",
        "tag_name": "v1.11.0",
        "name": "Omairc 1.11.0",
        "draft": false,
        "prerelease": false,
        "published_at": "2026-10-10T09:00:00Z",
        "assets": [
            {
                "name": "omairc-1.11.0-windows-x64-setup.exe",
                "size": 12345678,
                "browser_download_url": "https://github.com/fredimachado/omairc/releases/download/v1.11.0/omairc-1.11.0-windows-x64-setup.exe"
            }
        ]
    }"#;

    #[test]
    fn compares_versions_as_qt_does() {
        use Ordering::{Equal, Greater, Less};
        assert_eq!(compare_versions("0.7.0", "0.7.0"), Equal);
        assert_eq!(compare_versions("v0.7.0", "0.7.0"), Equal);
        assert_eq!(compare_versions("0.7", "0.7.0"), Equal);
        assert_eq!(compare_versions("0.7.0alpha", "0.7.0"), Less);
        assert_eq!(compare_versions("0.7.0", "0.7.0alpha"), Greater);
        assert_eq!(compare_versions("0.7.0alpha", "0.7.0beta"), Less);
        assert_eq!(compare_versions("0.7.0rc1", "0.7.0beta"), Greater);
        assert_eq!(compare_versions("0.7.0", "0.8.0"), Less);
        assert_eq!(compare_versions("0.8.0", "0.7.9"), Greater);
        // Numeric parts compare as numbers, not text.
        assert_eq!(compare_versions("1.10.3", "1.9.9"), Greater);
        assert_eq!(compare_versions("1.10.3", "1.10.3-beta.1"), Greater);
    }

    #[test]
    fn parses_the_latest_release_fixture() {
        let release = parse_release(LATEST_RELEASE).expect("fixture parses");
        assert_eq!(release.version, "1.11.0");
        assert_eq!(
            release.html_url,
            "https://github.com/fredimachado/omairc/releases/tag/v1.11.0"
        );
    }

    #[test]
    fn falls_back_to_the_latest_page_without_html_url() {
        let release = parse_release(r#"{"tag_name":"v1.0.0"}"#).expect("tag is enough");
        assert_eq!(release.version, "1.0.0");
        assert_eq!(
            release.html_url,
            "https://github.com/fredimachado/omairc/releases/latest"
        );
    }

    #[test]
    fn rejects_replies_that_are_not_a_release() {
        assert_eq!(parse_release("{}"), None);
        assert_eq!(parse_release("not json"), None);
        assert_eq!(parse_release("[]"), None);
        assert_eq!(parse_release(r#"{"tag_name":""}"#), None);
        assert_eq!(parse_release(r#"{"tag_name":"v"}"#), None);
        assert_eq!(parse_release(r#"{"tag_name":42}"#), None);
        assert_eq!(
            parse_release(
                r#"{"tag_name":"v1.11.0","html_url":"https://evil.example/fredimachado/omairc"}"#
            ),
            None
        );
    }

    #[test]
    fn release_pages_must_sit_under_the_repository() {
        for url in [
            "https://github.com/fredimachado/omairc",
            "https://github.com/fredimachado/omairc/releases/tag/v1.11.0",
            "https://github.com/fredimachado/omairc?tab=readme",
            "HTTPS://GitHub.com/fredimachado/omairc/releases",
        ] {
            assert!(release_url_is_safe(url), "{url} should pass");
        }
        for url in [
            "http://github.com/fredimachado/omairc",
            "https://github.com/fredimachado/omairc-evil",
            "https://github.com/other/omairc",
            "https://github.com.evil.example/fredimachado/omairc",
            "https://user@github.com/fredimachado/omairc",
            "https://github.com:443/fredimachado/omairc",
            "github.com/fredimachado/omairc",
            "",
        ] {
            assert!(!release_url_is_safe(url), "{url} should fail");
        }
    }

    #[test]
    fn a_newer_fixture_release_offers_its_page() {
        let status = status_for_reply("1.10.3", 200, LATEST_RELEASE);
        assert_eq!(
            status,
            UpdateStatus::Available {
                version: "1.11.0".to_string(),
                url: "https://github.com/fredimachado/omairc/releases/tag/v1.11.0".to_string(),
            }
        );
        assert_eq!(
            status.message().as_deref(),
            Some("Version 1.11.0 is available.")
        );
        assert_eq!(
            status.link(),
            Some("https://github.com/fredimachado/omairc/releases/tag/v1.11.0")
        );
    }

    #[test]
    fn the_same_or_an_older_release_is_up_to_date() {
        for current in ["1.11.0", "v1.11.0", "1.12.0"] {
            let status = status_for_reply(current, 200, LATEST_RELEASE);
            assert_eq!(status, UpdateStatus::UpToDate, "current {current}");
            assert_eq!(status.message().as_deref(), Some("Omairc is up to date."));
            assert_eq!(status.link(), None);
        }
    }

    #[test]
    fn a_failed_reply_says_so() {
        let not_found = r#"{"message":"Not Found","documentation_url":"https://docs.github.com"}"#;
        assert_eq!(
            status_for_reply("1.10.3", 404, not_found),
            UpdateStatus::Failed
        );
        assert_eq!(
            status_for_reply("1.10.3", 200, "garbage"),
            UpdateStatus::Failed
        );
        assert_eq!(
            UpdateStatus::Failed.message().as_deref(),
            Some("Could not check for updates.")
        );
        assert_eq!(UpdateStatus::Idle.message(), None);
        assert_eq!(
            UpdateStatus::Checking.message().as_deref(),
            Some("Checking…")
        );
    }

    #[test]
    fn the_user_agent_names_the_app_and_repository() {
        assert_eq!(
            user_agent("1.10.3"),
            "Omairc/1.10.3 (+https://github.com/fredimachado/omairc)"
        );
    }

    /// Serves one canned HTTP response on a loopback port and returns the URL and
    /// a handle that yields the request head the client sent.
    fn serve_once(
        status_line: &'static str,
        body: Vec<u8>,
    ) -> (String, thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let url = format!("http://{}/releases/latest", listener.local_addr().unwrap());
        let handle = thread::spawn(move || {
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
                "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            // The client may stop reading early (an oversized body), so a write can fail.
            let _ = stream.write_all(reply.as_bytes());
            let _ = stream.write_all(&body);
            String::from_utf8_lossy(&head).into_owned()
        });
        (url, handle)
    }

    #[test]
    fn check_latest_reads_a_served_release_and_sends_the_user_agent() {
        let (url, server) = serve_once("200 OK", LATEST_RELEASE.as_bytes().to_vec());
        let status = check_latest(&UreqFetch, "1.10.3", &url);
        let request = server.join().expect("server thread").to_ascii_lowercase();
        assert!(status.message().unwrap().contains("1.11.0"), "{status:?}");
        // HTTP/1.1 header names arrive lowercased, so compare the head lowercased.
        assert!(
            request.contains("user-agent: omairc/1.10.3 (+https://github.com/fredimachado/omairc)"),
            "request was {request}"
        );
        assert!(request.contains("accept: application/vnd.github+json"));
    }

    #[test]
    fn check_latest_fails_on_an_error_status_and_on_an_oversized_body() {
        let (url, server) = serve_once("404 Not Found", br#"{"message":"Not Found"}"#.to_vec());
        assert_eq!(check_latest(&UreqFetch, "1.10.3", &url), UpdateStatus::Failed);
        server.join().expect("server thread");

        let oversized = vec![b' '; (MAXIMUM_RELEASE_BYTES + 1) as usize];
        let (url, server) = serve_once("200 OK", oversized);
        assert_eq!(check_latest(&UreqFetch, "1.10.3", &url), UpdateStatus::Failed);
        server.join().expect("server thread");
    }
}
