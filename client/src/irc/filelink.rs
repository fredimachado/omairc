//! Upload link resolution and FILEHOST Basic auth policy. Ports
//! `src/irc/ircfilelink.cpp` and `tui/internal/irc/filelink.go`. The IRC core
//! stays free of sockets; callers pass resolved URLs to the composer.

use base64::Engine;
use url::Url;

fn has_control_character(text: &str) -> bool {
    text.bytes().any(|byte| byte < 0x20 || byte == 0x7F)
}

fn web_scheme(scheme: &str) -> bool {
    scheme.eq_ignore_ascii_case("https") || scheme.eq_ignore_ascii_case("http")
}

/// Resolves `location` against `endpoint`. The result is an absolute http or
/// https URL with no userinfo. Anything else is refused.
pub fn absolute_file_link(endpoint: &str, location: &str) -> Option<String> {
    if endpoint.is_empty() || location.is_empty() {
        return None;
    }
    if has_control_character(endpoint) || has_control_character(location) {
        return None;
    }
    let base = Url::parse(endpoint).ok()?;
    if !web_scheme(base.scheme()) || base.host_str().unwrap_or("").is_empty() {
        return None;
    }
    let resolved = base.join(location).ok()?;
    if !web_scheme(resolved.scheme()) || resolved.host_str().unwrap_or("").is_empty() {
        return None;
    }
    if !resolved.username().is_empty() || resolved.password().is_some() {
        return None;
    }
    let mut clean = resolved;
    clean.set_username("").ok();
    clean.set_password(None).ok();
    clean.set_fragment(None);
    let text = clean.to_string();
    if text.is_empty() || text.contains(' ') || text.contains('\n') || text.contains('\r') {
        return None;
    }
    Some(text)
}

/// Whether the IRC password may be sent to this upload endpoint. The scheme
/// must be http or https, the URL must have no userinfo, and cleartext upload is
/// refused when the IRC connection is encrypted. `server_host` is unused.
pub fn file_host_sends_basic_auth(server_host: &str, upload_url: &str, server_encrypted: bool) -> bool {
    let _ = server_host;
    if upload_url.is_empty() {
        return false;
    }
    let parsed = match Url::parse(upload_url) {
        Ok(url) => url,
        Err(_) => return false,
    };
    if parsed.host_str().unwrap_or("").is_empty()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return false;
    }
    let https = parsed.scheme().eq_ignore_ascii_case("https");
    let http = parsed.scheme().eq_ignore_ascii_case("http");
    if !https && !http {
        return false;
    }
    if server_encrypted && !https {
        return false;
    }
    true
}

/// The `Authorization` header value, or `None` when the pair cannot be sent.
/// Callers must not log it.
pub fn basic_authorization_value(user: &str, secret: &str) -> Option<String> {
    if user.is_empty() || secret.is_empty() {
        return None;
    }
    if has_control_character(user) || has_control_character(secret) {
        return None;
    }
    let token = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{secret}"));
    Some(format!("Basic {token}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absolute_file_link_vectors() {
        assert_eq!(
            absolute_file_link("https://irc.example.org/upload", "/upload/hoh5eFThae4e.jpeg"),
            Some("https://irc.example.org/upload/hoh5eFThae4e.jpeg".to_owned())
        );
        assert_eq!(
            absolute_file_link("https://irc.example.org/upload", "https://cdn.example/a.png"),
            Some("https://cdn.example/a.png".to_owned())
        );
        assert_eq!(
            absolute_file_link("https://irc.example.org/upload", "http://cdn.example/a.png"),
            Some("http://cdn.example/a.png".to_owned())
        );
        assert_eq!(
            absolute_file_link("https://irc.example.org/upload", "/a.png#part"),
            Some("https://irc.example.org/a.png".to_owned())
        );
        assert!(absolute_file_link("", "/a").is_none());
        assert!(absolute_file_link("https://irc.example.org/upload", "").is_none());
        assert!(absolute_file_link("https://irc.example.org/upload", "javascript:alert(1)").is_none());
        assert!(
            absolute_file_link("https://irc.example.org/upload", "https://user:pw@cdn.example/a.png")
                .is_none()
        );
    }

    #[test]
    fn file_host_sends_basic_auth_vectors() {
        assert!(file_host_sends_basic_auth("irc.example", "https://irc.example/upload", true));
        assert!(!file_host_sends_basic_auth("irc.example", "http://irc.example/upload", true));
        assert!(file_host_sends_basic_auth("irc.example", "http://IRC.EXAMPLE./upload", false));
        assert!(file_host_sends_basic_auth("irc.example", "https://uploads.example/upload", true));
        assert!(!file_host_sends_basic_auth(
            "irc.example",
            "https://user:pw@irc.example/upload",
            true
        ));
        assert!(file_host_sends_basic_auth("", "https://irc.example/upload", true));
        assert!(!file_host_sends_basic_auth("irc.example", "ftp://irc.example/upload", false));
    }

    #[test]
    fn basic_authorization_vectors() {
        assert_eq!(
            basic_authorization_value("seunghye", "no").as_deref(),
            Some("Basic c2V1bmdoeWU6bm8=")
        );
        assert!(basic_authorization_value("", "no").is_none());
        assert!(basic_authorization_value("seunghye", "").is_none());
        assert!(basic_authorization_value("bad\nname", "no").is_none());
    }
}
