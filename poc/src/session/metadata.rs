//! Port of the draft/metadata-2 helpers of `tui/internal/irc/presence.go` (Go)
//! and the metadata writers of `tui/internal/session/session.go` (Go),
//! `src/irc/ircmetadata.*` and `IrcSession::setOwnMetadata` (C++).
//!
//! The session needs four things from draft/metadata-2: the limits the server
//! advertised, which keys it may subscribe to, the value ceiling for a SET, and
//! the subscribe line itself. The presence model that consumes these values
//! (`NickPresence`, `NetworkPresence`) is Phase 3 (`irc/presence`). The helpers
//! live here until then, so the session does not wait on the presence port.

use super::session::Core;
use super::wire;
use crate::irc::capability::IrcCapability;
use crate::irc::message::MAX_CLASSIC_FRAME_BYTES;

/// The client ceiling on a metadata value, in UTF-8 bytes
/// (`IrcMetadata::maximumValueBytes`).
pub const MAXIMUM_VALUE_BYTES: usize = 512;

/// The keys the client subscribes to, in subscription order.
pub const SUBSCRIBED_KEYS: [&str; 7] = [
    "status",
    "avatar",
    "bot",
    "display-name",
    "pronouns",
    "homepage",
    "color",
];

/// The draft/metadata-2 limits a server advertised. `None` means the server did
/// not advertise that limit (`IrcMetadataCapability`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MetadataCapability {
    pub max_subs: Option<usize>,
    pub max_value_bytes: Option<usize>,
}

/// The keys to subscribe to, under the server's `max-subs` limit. `None` means
/// no limit; zero means none at all.
pub fn subscription_keys(max_subs: Option<usize>) -> Vec<&'static str> {
    match max_subs {
        None => SUBSCRIBED_KEYS.to_vec(),
        Some(0) => Vec::new(),
        Some(limit) => SUBSCRIBED_KEYS.iter().copied().take(limit).collect(),
    }
}

/// The canonical spelling of a known metadata key, matched without regard to
/// ASCII case.
pub fn canonical_key(key: &str) -> Option<&'static str> {
    SUBSCRIBED_KEYS
        .iter()
        .copied()
        .find(|known| known.eq_ignore_ascii_case(key))
}

/// Whether `key` is one of the keys the client handles.
pub fn is_known_key(key: &str) -> bool {
    canonical_key(key).is_some()
}

/// `value`, cut from the end until its UTF-8 length fits `max_bytes`. The cut is
/// made in UTF-16 units, as the Go and C++ ports do, so a surrogate pair may be
/// split and decodes to U+FFFD.
pub fn clamped_to(value: &str, max_bytes: usize) -> String {
    if max_bytes == 0 {
        return String::new();
    }
    let units: Vec<u16> = value.encode_utf16().collect();
    let mut end = units.len();
    loop {
        let text = String::from_utf16_lossy(&units[..end]);
        if text.len() <= max_bytes || end == 0 {
            return text;
        }
        end -= 1;
    }
}

/// The value ceiling for a server that advertised `advertised`: the smaller of
/// its limit and the client maximum.
pub fn effective_max_value_bytes(advertised: Option<usize>) -> usize {
    match advertised {
        Some(limit) if limit < MAXIMUM_VALUE_BYTES => limit,
        _ => MAXIMUM_VALUE_BYTES,
    }
}

/// Reads the `draft/metadata-2` token out of a CAP LS / NEW / ACK. The last
/// occurrence wins. `None` when no such token is present
/// (`ParseMetadataCapability`).
pub fn parse_metadata_capability(tokens: &[String]) -> Option<MetadataCapability> {
    let mut result: Option<MetadataCapability> = None;
    for token in tokens {
        if !token_name(token).eq_ignore_ascii_case("draft/metadata-2") {
            continue;
        }
        let mut current = MetadataCapability::default();
        let raw = token_value(token);
        for part in raw.split(',') {
            if part.is_empty() {
                continue;
            }
            let key = token_name(part).to_lowercase();
            let Some(value) = parse_non_negative(token_value(part)) else {
                continue;
            };
            if key == "max-subs" && current.max_subs.is_none() {
                current.max_subs = Some(value);
            } else if key == "max-value-bytes" && current.max_value_bytes.is_none() {
                current.max_value_bytes = Some(value);
            }
        }
        result = Some(current);
    }
    result
}

fn token_name(token: &str) -> &str {
    token.split_once('=').map_or(token, |(name, _)| name)
}

fn token_value(token: &str) -> &str {
    token.split_once('=').map_or("", |(_, value)| value)
}

fn parse_non_negative(raw: &str) -> Option<usize> {
    if raw.is_empty() {
        return None;
    }
    let value: i64 = raw.trim().parse().ok()?;
    usize::try_from(value).ok()
}

impl Core {
    /// Subscribes to the metadata keys the server allows, once member metadata
    /// and batch are both enabled (`subscribeToMemberMetadataLocked`).
    pub(crate) fn subscribe_to_member_metadata(&mut self) {
        let enabled = self.caps.enabled();
        if !enabled.contains(IrcCapability::MemberMetadata)
            || !enabled.contains(IrcCapability::Batch)
        {
            return;
        }
        let keys = subscription_keys(self.metadata.max_subs);
        if keys.is_empty() {
            return;
        }
        let line = format!("METADATA * SUB {}\r\n", keys.join(" "));
        self.send_line(line.as_bytes());
    }

    /// Sets (or, for an empty value, clears) one of this user's metadata keys.
    /// Returns the value placed on the wire, `Some("")` for a clear, or `None`
    /// when nothing was sent (`setOwnMetadataLocked`).
    pub(crate) fn set_own_metadata(&mut self, key: &str, value: &str) -> Option<String> {
        if self.state != super::events::SessionState::Registered {
            return None;
        }
        let enabled = self.caps.enabled();
        if !enabled.contains(IrcCapability::MemberMetadata)
            || !enabled.contains(IrcCapability::Batch)
        {
            return None;
        }
        if key.is_empty() || !is_known_key(key) {
            return None;
        }
        let stored = canonical_key(key)?;
        if value.is_empty() {
            if !self.send_command(&format!("METADATA * SET {stored}"), "") {
                return None;
            }
            return Some(String::new());
        }
        let prefix = format!("METADATA * SET {stored} :");
        let wire_budget = MAX_CLASSIC_FRAME_BYTES
            .saturating_sub(prefix.len())
            .saturating_sub(2);
        let max_bytes = effective_max_value_bytes(self.metadata.max_value_bytes).min(wire_budget);
        if max_bytes == 0 {
            return None;
        }
        let clamped = clamped_to(value, max_bytes);
        if clamped.is_empty() {
            return None;
        }
        if !self.send_command(&format!("{prefix}{clamped}"), "") {
            return None;
        }
        Some(clamped)
    }

    /// Handles `774`, which asks the client to retry a metadata sync later. The
    /// retry is capped at 60 seconds (`handleMetadataSyncLaterLocked`).
    pub(crate) fn handle_metadata_sync_later(&mut self, message: &crate::irc::message::IrcMessage) {
        let target = wire::param(message, 1);
        if target.is_empty() {
            return;
        }
        let retry_after = wire::param(message, 2)
            .trim()
            .parse::<i64>()
            .unwrap_or(0)
            .clamp(0, 60);
        let weak = self.weak.clone();
        let delay = std::time::Duration::from_secs(u64::try_from(retry_after).unwrap_or(0));
        let _ = self.clock.after(
            delay,
            Box::new(move || {
                if let Some(shared) = weak.upgrade() {
                    super::session::Session::from_shared(shared).locked(|core| {
                        core.send_command(&format!("METADATA {target} SYNC"), "");
                    });
                }
            }),
        );
    }
}
