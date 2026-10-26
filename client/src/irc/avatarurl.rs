//! `/avatar` accepts an HTTPS URL or an email address. An email becomes a Gravatar
//! SHA-256 URL, and only a safe HTTPS URL is kept. The URL is parsed by the `url`
//! crate, the same WHATWG parser behind QUrl's strict mode, and IP literals are
//! judged from their canonical text. Nothing here uses `std::net`, so the
//! portability gate holds. The fetch and the DNS pin are I/O and live in

use sha2::{Digest, Sha256};
use url::Url;

/// Qt `kMaximumAvatarUrlLength`, applied to the substituted and the rendered URL.
pub const MAXIMUM_AVATAR_URL_LENGTH: usize = 2048;

/// An IP literal, parsed from its text. The `url` crate has already normalized
/// the host (`127.1` is `127.0.0.1`, and an IPv6 literal is compressed), so the
/// text is canonical and a textual parse is enough to judge it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpLiteral {
    V4([u8; 4]),
    V6([u16; 8]),
}

/// The SHA-256 of `bytes` as lowercase hex. Gravatar and the cache key use it.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A `/avatar` argument resolved into the metadata value to store. An email
/// becomes its Gravatar URL. A safe HTTPS URL is kept as typed, trimmed. Anything
/// else is `None`. Go `AvatarMetadataValue`, Qt `ircAvatarMetadataValue`.
pub fn avatar_metadata_value(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    if looks_like_avatar_email(trimmed) {
        return Some(gravatar_metadata_url(trimmed));
    }
    let resolved = resolved_avatar_url(trimmed, 32)?;
    avatar_url_is_safe(&resolved).then(|| trimmed.to_string())
}

/// An email address, with an optional `mailto:` prefix. It is not a URL.
/// Go `looksLikeAvatarEmail`.
fn looks_like_avatar_email(input: &str) -> bool {
    let mut trimmed = input.trim();
    if let Some(rest) = strip_mailto(trimmed) {
        trimmed = rest.trim();
    }
    if trimmed.contains("://") {
        return false;
    }
    let Some(at) = trimmed.find('@') else {
        return false;
    };
    if at == 0 || at >= trimmed.len() - 1 {
        return false;
    }
    let (local, domain) = (&trimmed[..at], &trimmed[at + 1..]);
    if local.is_empty() || domain.is_empty() || domain.contains('@') {
        return false;
    }
    domain.contains('.')
}

/// The text after a case-insensitive `mailto:`, or `None`.
fn strip_mailto(text: &str) -> Option<&str> {
    let prefix = text.get(..7)?;
    prefix
        .eq_ignore_ascii_case("mailto:")
        .then(|| &text[7..])
}

/// The Gravatar URL for an email, with the address lowercased and SHA-256 hashed.
/// The `{size}` placeholder stays until a fetch size is known. Go
fn gravatar_metadata_url(email: &str) -> String {
    let mut normalized = email.trim();
    if let Some(rest) = strip_mailto(normalized) {
        normalized = rest.trim();
    }
    let hash = sha256_hex(normalized.to_lowercase().as_bytes());
    format!("https://www.gravatar.com/avatar/{hash}?s={{size}}&d=404")
}

/// Substitutes `{size}` with `max(16, pixel_size)` and parses the result as an
/// absolute URL with a scheme and a host. Go `ResolvedAvatarURL`, Qt
/// `ircResolvedAvatarUrl`. A URL with userinfo is refused before parsing, so the
/// empty `user@` form fails closed too.
pub fn resolved_avatar_url(raw: &str, pixel_size: u32) -> Option<Url> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let size = pixel_size.max(16);
    let substituted = trimmed.replace("{size}", &size.to_string());
    if substituted.len() > MAXIMUM_AVATAR_URL_LENGTH || has_userinfo(&substituted) {
        return None;
    }
    let url = Url::parse(&substituted).ok()?;
    if url.cannot_be_a_base() || url.host().is_none() {
        return None;
    }
    Some(url)
}

/// Whether the authority of `text` (between `://` and the first `/`, `?` or `#`)
/// carries an `@`. The `url` crate drops an empty userinfo, so the check reads the
/// raw text.
fn has_userinfo(text: &str) -> bool {
    let Some(index) = text.find("://") else {
        return false;
    };
    let rest = &text[index + 3..];
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    rest[..end].contains('@')
}

/// Quantizes a layout size to the fetch size. Zero disables the fetch. Otherwise
/// the answer is 32 when the layout size is at least as close to 32 as to 64, and
/// 64 when it is closer. Go `AvatarFetchPixelSize`, Qt `ircAvatarFetchPixelSize`.
pub fn avatar_fetch_pixel_size(layout_pixels: i32) -> u32 {
    if layout_pixels <= 0 {
        return 0;
    }
    let distance_32 = (layout_pixels - 32).abs();
    let distance_64 = (layout_pixels - 64).abs();
    if distance_32 <= distance_64 {
        32
    } else {
        64
    }
}

/// Whether `url` may be fetched: https, no userinfo, a non-local host, port 443 or
/// unset, and a global-unicast address when the host is an IP literal. Go
/// `AvatarURLIsSafe`, Qt `ircAvatarUrlIsSafe`.
pub fn avatar_url_is_safe(url: &Url) -> bool {
    if url.scheme() != "https" {
        return false;
    }
    if url.as_str().len() > MAXIMUM_AVATAR_URL_LENGTH {
        return false;
    }
    if !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    let Some(raw_host) = url.host_str() else {
        return false;
    };
    let host = raw_host.trim_end_matches('.');
    if host.is_empty() || host_looks_local(host) {
        return false;
    }
    // The url crate drops the default port, so only a non-default one is seen here.
    if url.port().is_some_and(|port| port != 443) {
        return false;
    }
    match parse_ip_literal(host) {
        Some(address) => !host_address_is_unsafe(address),
        None => true,
    }
}

fn host_looks_local(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    lower == "localhost" || lower.ends_with(".local") || lower.ends_with(".localhost")
}

/// Parses an IP literal from its text: a dotted IPv4 address, or an IPv6 address
/// with or without brackets. Anything else is `None`, which the caller treats as
/// a host name. Go's `netip.ParseAddr` stands in for this.
pub fn parse_ip_literal(text: &str) -> Option<IpLiteral> {
    if let Some(inner) = text
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
    {
        return parse_v6(inner).map(IpLiteral::V6);
    }
    if text.contains(':') {
        return parse_v6(text).map(IpLiteral::V6);
    }
    parse_v4(text).map(IpLiteral::V4)
}

fn parse_v4(text: &str) -> Option<[u8; 4]> {
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut octets = [0u8; 4];
    for (slot, part) in octets.iter_mut().zip(parts) {
        let digits_only = !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
        if !digits_only || part.len() > 3 || (part.len() > 1 && part.starts_with('0')) {
            return None;
        }
        *slot = part.parse::<u8>().ok()?;
    }
    Some(octets)
}

fn parse_v6(text: &str) -> Option<[u16; 8]> {
    let mut groups = [0u16; 8];
    match text.find("::") {
        Some(index) => {
            let head = &text[..index];
            let tail = &text[index + 2..];
            if tail.contains("::") {
                return None;
            }
            let head = parse_groups(head)?;
            let tail = parse_groups_with_v4(tail)?;
            if head.len() + tail.len() > 7 {
                return None;
            }
            groups[..head.len()].copy_from_slice(&head);
            groups[8 - tail.len()..].copy_from_slice(&tail);
        }
        None => {
            let all = parse_groups_with_v4(text)?;
            if all.len() != 8 {
                return None;
            }
            groups.copy_from_slice(&all);
        }
    }
    Some(groups)
}

/// Hex groups separated by `:`, with an optional dotted IPv4 tail that counts as
/// two groups. It is used only where the tail may sit (the end of the address).
fn parse_groups_with_v4(part: &str) -> Option<Vec<u16>> {
    if !part.contains('.') {
        return parse_groups(part);
    }
    let (rest, v4) = match part.rfind(':') {
        Some(index) => (&part[..index], &part[index + 1..]),
        None => ("", part),
    };
    let mut groups = parse_groups(rest)?;
    let octets = parse_v4(v4)?;
    groups.push(u16::from(octets[0]) << 8 | u16::from(octets[1]));
    groups.push(u16::from(octets[2]) << 8 | u16::from(octets[3]));
    Some(groups)
}

fn parse_groups(part: &str) -> Option<Vec<u16>> {
    if part.is_empty() {
        return Some(Vec::new());
    }
    part.split(':')
        .map(|group| {
            let hex = !group.is_empty()
                && group.len() <= 4
                && group.bytes().all(|byte| byte.is_ascii_hexdigit());
            if hex {
                u16::from_str_radix(group, 16).ok()
            } else {
                None
            }
        })
        .collect()
}

/// Whether an IP literal is non-global or in a range Qt's `QHostAddress` treats
/// as unsafe. IPv4-mapped IPv6 is judged by its IPv4 rules. IPv6 is safe only
/// inside 2000::/3, minus the documentation, Teredo and 6to4 blocks. That check
/// fails closed for anything the lists miss. Go `HostAddressIsUnsafe`, Qt
pub fn host_address_is_unsafe(address: IpLiteral) -> bool {
    match address {
        IpLiteral::V4(octets) => v4_is_unsafe(octets),
        IpLiteral::V6(groups) => match mapped_v4(groups) {
            Some(octets) => v4_is_unsafe(octets),
            None => !is_global_v6(groups),
        },
    }
}

/// The IPv4 address inside `::ffff:a.b.c.d`, if the address is that form.
fn mapped_v4(groups: [u16; 8]) -> Option<[u8; 4]> {
    if groups[..5].iter().all(|group| *group == 0) && groups[5] == 0xffff {
        Some([
            (groups[6] >> 8) as u8,
            groups[6] as u8,
            (groups[7] >> 8) as u8,
            groups[7] as u8,
        ])
    } else {
        None
    }
}

fn v4_is_unsafe(octets: [u8; 4]) -> bool {
    let [a, b, c, _] = octets;
    // 0/8, private 10/8, loopback 127/8, carrier-grade NAT 100.64/10, link-local
    // 169.254/16, private 172.16/12 and 192.168/16, the 192.0.0/24, 192.0.2/24 and
    // 192.88.99/24 special blocks, benchmarking 198.18/15, the 198.51.100/24 and
    // 203.0.113/24 test nets, and multicast, reserved and broadcast 224/4.
    a == 0
        || a == 10
        || a == 127
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && b == 0 && (c == 0 || c == 2))
        || (a == 192 && b == 88 && c == 99)
        || (a == 192 && b == 168)
        || (a == 198 && (b == 18 || b == 19))
        || (a == 198 && b == 51 && c == 100)
        || (a == 203 && b == 0 && c == 113)
        || a >= 224
}

/// IPv6 is global only inside 2000::/3, and not in the documentation (2001:db8::/32)
/// Teredo (2001::/32) or 6to4 (2002::/16) blocks, which embed other addresses.
fn is_global_v6(groups: [u16; 8]) -> bool {
    let in_global_unicast = groups[0] & 0xe000 == 0x2000;
    let documentation = groups[0] == 0x2001 && groups[1] == 0x0db8;
    let teredo = groups[0] == 0x2001 && groups[1] == 0;
    let six_to_four = groups[0] == 0x2002;
    in_global_unicast && !documentation && !teredo && !six_to_four
}

/// The first address that is safe to connect to, or `None` when none are.
/// Go `SelectSafeAvatarAddress`, Qt `ircSelectSafeAvatarAddress`.
pub fn select_safe_avatar_address(addresses: &[IpLiteral]) -> Option<IpLiteral> {
    addresses
        .iter()
        .copied()
        .find(|address| !host_address_is_unsafe(*address))
}

/// `url` with its host replaced by `address`, so the fetch connects to that IP
/// without a second DNS lookup. The port and path stay. Go
/// `AvatarURLPinnedToAddress`, Qt `ircAvatarUrlPinnedToAddress`.
#[cfg(test)]
pub fn avatar_url_pinned_to_address(url: &Url, address: IpLiteral) -> Option<Url> {
    let literal = match address {
        IpLiteral::V4([a, b, c, d]) => format!("{a}.{b}.{c}.{d}"),
        IpLiteral::V6(groups) => {
            let joined: Vec<String> = groups.iter().map(|group| format!("{group:x}")).collect();
            format!("[{}]", joined.join(":"))
        }
    };
    let mut pinned = url.clone();
    pinned.set_host(Some(literal.as_str())).ok()?;
    Some(pinned)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `resolved_avatar_url` composed with `avatar_url_is_safe`, as the Qt tests do.
    fn resolved_is_safe(raw: &str) -> bool {
        resolved_avatar_url(raw, 32).is_some_and(|url| avatar_url_is_safe(&url))
    }

    fn v4(a: u8, b: u8, c: u8, d: u8) -> IpLiteral {
        IpLiteral::V4([a, b, c, d])
    }

    #[test]
    fn substitutes_the_size_placeholder() {
        let sized = resolved_avatar_url("https://example.com/avatars/{size}/mira.png", 34)
            .expect("resolved");
        assert_eq!(sized.as_str(), "https://example.com/avatars/34/mira.png");
        assert!(avatar_url_is_safe(&sized));

        let floor = resolved_avatar_url("https://example.com/avatars/{size}/mira.png", 8)
            .expect("resolved");
        assert_eq!(floor.as_str(), "https://example.com/avatars/16/mira.png");
        assert!(avatar_url_is_safe(&floor));
    }

    #[test]
    fn accepts_https() {
        assert!(resolved_is_safe("https://cdn.example.com/a.png"));
        assert!(resolved_is_safe("HTTPS://cdn.example.com/a.png"));
        assert!(resolved_is_safe("https://example.com:443/a.png"));
    }

    #[test]
    fn rejects_http() {
        assert!(!resolved_is_safe("http://example.com/a.png"));
    }

    #[test]
    fn rejects_localhost() {
        assert!(!resolved_is_safe("https://localhost/a.png"));
        assert!(!resolved_is_safe("https://LOCALHOST/a.png"));
        assert!(!resolved_is_safe("https://localhost./a.png"));
        assert!(resolved_is_safe("https://example.com./a.png"));
    }

    #[test]
    fn rejects_loopback() {
        assert!(!resolved_is_safe("https://127.0.0.1/a.png"));
        assert!(!resolved_is_safe("https://[::1]/a.png"));
        assert!(!resolved_is_safe("https://[::ffff:127.0.0.1]/a.png"));
        assert!(host_address_is_unsafe(parse_ip_literal("::ffff:127.0.0.1").unwrap()));
    }

    #[test]
    fn rejects_private() {
        assert!(!resolved_is_safe("https://10.0.0.4/a.png"));
        assert!(!resolved_is_safe("https://192.168.1.10/a.png"));
        assert!(!resolved_is_safe("https://100.64.0.1/a.png"));
        assert!(host_address_is_unsafe(v4(100, 64, 0, 1)));
    }

    #[test]
    fn rejects_zero_network() {
        assert!(!resolved_is_safe("https://0.0.0.0/a.png"));
        assert!(!resolved_is_safe("https://0.1.2.3/a.png"));
        assert!(host_address_is_unsafe(v4(0, 0, 0, 0)));
        assert!(host_address_is_unsafe(v4(0, 1, 2, 3)));
    }

    #[test]
    fn rejects_link_local() {
        assert!(!resolved_is_safe("https://169.254.12.34/a.png"));
        assert!(host_address_is_unsafe(v4(169, 254, 12, 34)));
    }

    #[test]
    fn rejects_non_https_schemes() {
        assert!(!resolved_is_safe("file:///tmp/a.png"));
        assert!(!resolved_is_safe("data:image/png;base64,aaaa"));
    }

    #[test]
    fn rejects_empty_input() {
        assert!(resolved_avatar_url("", 32).is_none());
        assert!(resolved_avatar_url("   ", 32).is_none());
    }

    #[test]
    fn rejects_userinfo_and_local_suffixes() {
        assert!(!resolved_is_safe("https://user:pass@example.com/a.png"));
        assert!(!resolved_is_safe("https://@example.com/a.png"));
        assert!(!resolved_is_safe("https://printer.local/a.png"));
    }

    #[test]
    fn rejects_non_standard_ports() {
        assert!(!resolved_is_safe("https://example.com:8443/a.png"));
        assert!(resolved_is_safe("https://example.com/a.png"));
    }

    #[test]
    fn quantizes_the_fetch_pixel_size() {
        let cases = [
            (0, 0),
            (-1, 0),
            (22, 32),
            (32, 32),
            (34, 32),
            (48, 32),
            (49, 64),
            (64, 64),
        ];
        for (layout, want) in cases {
            assert_eq!(avatar_fetch_pixel_size(layout), want, "layout {layout}");
        }
    }

    #[test]
    fn selects_a_safe_address_and_pins_the_url() {
        assert_eq!(select_safe_avatar_address(&[]), None);
        assert_eq!(select_safe_avatar_address(&[v4(127, 0, 0, 1)]), None);
        assert_eq!(
            select_safe_avatar_address(&[v4(10, 0, 0, 1), v4(93, 184, 216, 34)]),
            Some(v4(93, 184, 216, 34))
        );

        let host_url = resolved_avatar_url("https://cdn.example/a.png", 32).expect("resolved");
        let pinned = avatar_url_pinned_to_address(&host_url, v4(93, 184, 216, 34)).expect("pinned");
        assert_eq!(pinned.as_str(), "https://93.184.216.34/a.png");
        assert!(avatar_url_is_safe(&pinned));
    }

    #[test]
    fn pinning_keeps_an_explicit_port_path_and_query() {
        let url = Url::parse("https://cdn.example:443/a.png?x=1").expect("parsed");
        let pinned = avatar_url_pinned_to_address(&url, v4(93, 184, 216, 34)).expect("pinned");
        assert_eq!(pinned.as_str(), "https://93.184.216.34/a.png?x=1");
    }

    #[test]
    fn a_v6_address_pins_as_a_bracketed_host() {
        let url = Url::parse("https://cdn.example/a.png").expect("parsed");
        let address = parse_ip_literal("2606:4700:4700::1111").expect("v6");
        let pinned = avatar_url_pinned_to_address(&url, address).expect("pinned");
        assert!(pinned.host_str().is_some_and(|host| host.starts_with('[')));
        assert!(avatar_url_is_safe(&pinned));
    }

    #[test]
    fn metadata_accepts_an_https_url() {
        let url = "https://cdn.example.com/a.png";
        assert_eq!(avatar_metadata_value(url).as_deref(), Some(url));
        assert_eq!(
            avatar_metadata_value(&format!("  {url}  ")).as_deref(),
            Some(url)
        );
    }

    #[test]
    fn metadata_builds_a_gravatar_from_an_email() {
        let expected = format!(
            "https://www.gravatar.com/avatar/{}?s={{size}}&d=404",
            sha256_hex(b"me@example.com")
        );
        assert_eq!(avatar_metadata_value("Me@Example.COM").as_deref(), Some(expected.as_str()));
        assert_eq!(
            avatar_metadata_value("mailto:Me@Example.COM").as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn metadata_rejects_unsafe_input() {
        let inputs = [
            "",
            "   ",
            "http://example.com/a.png",
            "https://localhost/a.png",
            "not-an-email",
            "missing-domain@host",
        ];
        for input in inputs {
            assert_eq!(avatar_metadata_value(input), None, "input {input:?}");
        }
    }

    #[test]
    fn the_sha256_hex_matches_a_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn ip_literals_parse_textually() {
        assert_eq!(parse_ip_literal("93.184.216.34"), Some(v4(93, 184, 216, 34)));
        assert_eq!(parse_ip_literal("256.1.1.1"), None);
        assert_eq!(parse_ip_literal("01.1.1.1"), None, "leading zeros fail closed");
        assert_eq!(parse_ip_literal("1.2.3"), None);
        assert_eq!(parse_ip_literal("example.com"), None);
        assert_eq!(
            parse_ip_literal("[2606:4700::1111]"),
            Some(IpLiteral::V6([0x2606, 0x4700, 0, 0, 0, 0, 0, 0x1111]))
        );
        assert_eq!(
            parse_ip_literal("::ffff:1.2.3.4"),
            Some(IpLiteral::V6([0, 0, 0, 0, 0, 0xffff, 0x0102, 0x0304]))
        );
        assert_eq!(parse_ip_literal("1::2::3"), None);
        assert_eq!(parse_ip_literal("12345::"), None);
    }

    #[test]
    fn ipv6_is_global_only_inside_2000_3() {
        assert!(!host_address_is_unsafe(parse_ip_literal("2606:4700::1111").unwrap()));
        assert!(host_address_is_unsafe(parse_ip_literal("fd00::1").unwrap()), "unique local");
        assert!(host_address_is_unsafe(parse_ip_literal("fe80::1").unwrap()), "link local");
        assert!(host_address_is_unsafe(parse_ip_literal("ff02::1").unwrap()), "multicast");
        assert!(host_address_is_unsafe(parse_ip_literal("::").unwrap()), "unspecified");
        assert!(host_address_is_unsafe(parse_ip_literal("2001:db8::1").unwrap()), "documentation");
        assert!(host_address_is_unsafe(parse_ip_literal("2002:0a00:0001::1").unwrap()), "6to4");
        assert!(host_address_is_unsafe(parse_ip_literal("64:ff9b::1").unwrap()), "NAT64 prefix");
    }

    #[test]
    fn ipv4_special_blocks_are_unsafe() {
        for octets in [
            (192, 0, 0, 9),
            (192, 0, 2, 1),
            (192, 88, 99, 1),
            (198, 18, 0, 1),
            (198, 51, 100, 1),
            (203, 0, 113, 1),
            (224, 0, 0, 1),
            (255, 255, 255, 255),
        ] {
            assert!(host_address_is_unsafe(v4(octets.0, octets.1, octets.2, octets.3)), "{octets:?}");
        }
        assert!(!host_address_is_unsafe(v4(8, 8, 8, 8)));
    }

    #[test]
    fn host_names_are_not_judged_as_addresses() {
        let url = Url::parse("https://example.com./a.png").expect("parsed");
        assert!(avatar_url_is_safe(&url));
    }
}
