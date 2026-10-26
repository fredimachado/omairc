//! Port of `tui/internal/session/sts.go` (Go) and `src/irc/ircsts.{h,cpp}` (C++): the
//! `sts` capability value, the policy cache's contract and its group key.
//!
//! A server advertises `sts=port=N,duration=S` in CAP. A plaintext session that sees a port
//! upgrades to TLS on it; a TLS session caches the policy for `duration` seconds, so the next
//! start connects securely even when the profile says plaintext. The cache is a `StsStore`:
//! `MemoryStsStore` here, and the Qt-compatible INI file in `storage/stspolicies.rs`. The
//! store takes `now` as a parameter, so expiry is deterministic under a fake clock.
//!
//! The session side (upgrade, reconnect, expiry on disconnect) is in `stsflow.rs`.

use std::cell::RefCell;
use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};

/// One `sts` capability value. A `None` field means the advertisement did not carry that key
/// (`IrcStsAdvertisement`, `STSAdvertisement`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StsAdvertisement {
    pub port: Option<u16>,
    pub duration_seconds: Option<i64>,
}

/// One cached policy (`IrcStsCached`, `STSCached`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StsCached {
    pub port: u16,
    pub duration_seconds: i64,
    pub expiry_epoch_seconds: i64,
}

/// The policy cache. `lookup` drops an expired or invalid entry, `store` with a zero duration
/// clears the host, and every method takes the host as the server named it: the group key
/// folds case and trims it.
pub trait StsStore {
    /// The cached policy for `host` at `now`, or `None` when there is none or it has expired.
    fn lookup(&self, host: &str, now: DateTime<Utc>) -> Option<StsCached>;
    /// Caches `port` for `duration_seconds` from `now`. A zero duration clears the host.
    fn store(&self, host: &str, port: u16, duration_seconds: i64, now: DateTime<Utc>);
    /// Drops the policy for `host`, if any.
    fn clear(&self, host: &str);
}

/// Scans CAP LS or NEW tokens for the last `sts` token and parses its value. `None` when no
/// `sts` token is present. A bare `sts` gives an empty advertisement
/// (`parseIrcStsAdvertisement`).
pub fn parse_sts_advertisement(tokens: &[String]) -> Option<StsAdvertisement> {
    let mut advertisement = None;
    for token in tokens {
        if token_name(token).eq_ignore_ascii_case("sts") {
            advertisement = Some(parse_sts_value(token_value(token)));
        }
    }
    advertisement
}

/// Parses a comma-separated `key=value` list. The first valid port and the first valid
/// duration win, and a malformed or out-of-range value is ignored (`parseStsValue`).
fn parse_sts_value(value: &str) -> StsAdvertisement {
    let mut advertisement = StsAdvertisement::default();
    for part in value.split(',').filter(|part| !part.is_empty()) {
        let raw = token_value(part);
        match token_name(part).to_ascii_lowercase().as_str() {
            "port" if advertisement.port.is_none() => {
                advertisement.port = parse_port(raw);
            }
            "duration" if advertisement.duration_seconds.is_none() => {
                advertisement.duration_seconds =
                    raw.parse::<i64>().ok().filter(|&seconds| seconds >= 0);
            }
            _ => {}
        }
    }
    advertisement
}

/// A port in 1..=65535, written in digits only.
fn parse_port(raw: &str) -> Option<u16> {
    if !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    raw.parse::<u16>().ok().filter(|&port| port >= 1)
}

/// The capability name of a token, without any `=value`.
fn token_name(token: &str) -> &str {
    token.split_once('=').map_or(token, |(name, _)| name)
}

/// The text after the first `=`, or empty.
fn token_value(token: &str) -> &str {
    token.split_once('=').map_or("", |(_, value)| value)
}

/// The cache key for `host`: the SHA-256 of its trimmed, lowercased form, as lowercase hex.
/// It is the group name in the Qt file, so both clients read the same entries. `None` for an
/// empty host (`groupKey` in `ircsts.cpp`).
pub fn sts_group_key(host: &str) -> Option<String> {
    let folded = host.trim().to_lowercase();
    if folded.is_empty() {
        return None;
    }
    let digest = Sha256::digest(folded.as_bytes());
    Some(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// The policy cache that keeps what it is given for the life of the process
/// (`Backing::Ephemeral`). The Go store (`STSStore`) works the same way.
#[derive(Debug, Default)]
pub struct MemoryStsStore {
    entries: RefCell<BTreeMap<String, StsCached>>,
}

impl StsStore for MemoryStsStore {
    fn lookup(&self, host: &str, now: DateTime<Utc>) -> Option<StsCached> {
        let key = sts_group_key(host)?;
        let mut entries = self.entries.borrow_mut();
        let cached = *entries.get(&key)?;
        if cached.expiry_epoch_seconds <= now.timestamp() {
            entries.remove(&key);
            return None;
        }
        Some(cached)
    }

    fn store(&self, host: &str, port: u16, duration_seconds: i64, now: DateTime<Utc>) {
        if duration_seconds == 0 {
            self.clear(host);
            return;
        }
        let Some(key) = sts_group_key(host) else {
            return;
        };
        if port == 0 {
            return;
        }
        let cached = StsCached {
            port,
            duration_seconds,
            expiry_epoch_seconds: now.timestamp().saturating_add(duration_seconds),
        };
        self.entries.borrow_mut().insert(key, cached);
    }

    fn clear(&self, host: &str) {
        if let Some(key) = sts_group_key(host) {
            self.entries.borrow_mut().remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(seconds, 0).single().unwrap()
    }

    fn tokens(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    #[test]
    fn a_bare_sts_token_is_an_empty_advertisement() {
        assert_eq!(
            parse_sts_advertisement(&tokens("multi-prefix sts")),
            Some(StsAdvertisement::default())
        );
    }

    #[test]
    fn no_sts_token_gives_no_advertisement() {
        assert_eq!(parse_sts_advertisement(&tokens("multi-prefix sasl")), None);
    }

    #[test]
    fn the_sts_name_matches_any_case_and_the_last_token_wins() {
        let advertisement = parse_sts_advertisement(&tokens("STS=port=6697 sts=port=6698"));
        assert_eq!(advertisement.unwrap().port, Some(6698));
    }

    #[test]
    fn port_and_duration_parse_from_the_comma_list() {
        let advertisement = parse_sts_advertisement(&tokens("sts=port=6697,duration=86400"));
        assert_eq!(
            advertisement,
            Some(StsAdvertisement {
                port: Some(6697),
                duration_seconds: Some(86400),
            })
        );
    }

    #[test]
    fn the_first_valid_value_of_each_key_wins() {
        let advertisement = parse_sts_advertisement(&tokens(
            "sts=port=0,port=6697,port=6698,duration=5,duration=9",
        ));
        assert_eq!(
            advertisement,
            Some(StsAdvertisement {
                port: Some(6697),
                duration_seconds: Some(5),
            })
        );
    }

    #[test]
    fn malformed_and_out_of_range_values_are_ignored() {
        for value in [
            "port=0",
            "port=65536",
            "port=-1",
            "port=+6697",
            "port=abc",
            "duration=-1",
            "duration=soon",
        ] {
            let advertisement = parse_sts_advertisement(&tokens(&format!("sts={value}")));
            assert_eq!(
                advertisement,
                Some(StsAdvertisement::default()),
                "{value} must not set a field"
            );
        }
    }

    #[test]
    fn a_zero_duration_is_kept_so_the_session_can_clear() {
        let advertisement = parse_sts_advertisement(&tokens("sts=duration=0"));
        assert_eq!(advertisement.unwrap().duration_seconds, Some(0));
    }

    #[test]
    fn an_empty_list_item_is_skipped() {
        let advertisement = parse_sts_advertisement(&tokens("sts=,,port=6697,"));
        assert_eq!(advertisement.unwrap().port, Some(6697));
    }

    #[test]
    fn the_group_key_is_the_sha256_of_the_folded_host() {
        assert_eq!(
            sts_group_key("irc.example").as_deref(),
            Some("a84a91b2bc2ea526cb7d244872ddffe0770a9ca3737ee9ef355e9018a76767de")
        );
        assert_eq!(
            sts_group_key("  IRC.Example \t"),
            sts_group_key("irc.example")
        );
    }

    #[test]
    fn an_empty_host_has_no_key() {
        assert_eq!(sts_group_key(""), None);
        assert_eq!(sts_group_key(" \t "), None);
    }

    #[test]
    fn a_stored_policy_is_found_until_it_expires() {
        let store = MemoryStsStore::default();
        store.store("irc.example", 6697, 60, at(1000));
        let cached = store.lookup("irc.example", at(1059)).expect("still valid");
        assert_eq!(
            cached,
            StsCached {
                port: 6697,
                duration_seconds: 60,
                expiry_epoch_seconds: 1060,
            }
        );
        assert_eq!(
            store.lookup("irc.example", at(1060)),
            None,
            "the expiry instant has passed"
        );
    }

    #[test]
    fn lookup_folds_case_and_trims_the_host() {
        let store = MemoryStsStore::default();
        store.store("IRC.example", 6697, 60, at(0));
        assert!(store.lookup("  irc.EXAMPLE ", at(1)).is_some());
    }

    #[test]
    fn an_expired_entry_is_dropped_by_lookup() {
        let store = MemoryStsStore::default();
        store.store("irc.example", 6697, 60, at(0));
        assert!(store.lookup("irc.example", at(61)).is_none());
        assert!(
            store.entries.borrow().is_empty(),
            "an expired entry is removed"
        );
    }

    #[test]
    fn a_zero_duration_clears_the_host() {
        let store = MemoryStsStore::default();
        store.store("irc.example", 6697, 60, at(0));
        store.store("irc.example", 6697, 0, at(0));
        assert_eq!(store.lookup("irc.example", at(1)), None);
    }

    #[test]
    fn a_zero_port_or_an_empty_host_is_not_stored() {
        let store = MemoryStsStore::default();
        store.store("irc.example", 0, 60, at(0));
        store.store("  ", 6697, 60, at(0));
        assert!(store.entries.borrow().is_empty());
    }

    #[test]
    fn clear_drops_only_the_named_host() {
        let store = MemoryStsStore::default();
        store.store("a.example", 6697, 60, at(0));
        store.store("b.example", 6697, 60, at(0));
        store.clear("a.example");
        assert!(store.lookup("a.example", at(1)).is_none());
        assert!(store.lookup("b.example", at(1)).is_some());
    }

    #[test]
    fn a_huge_duration_saturates_instead_of_wrapping() {
        let store = MemoryStsStore::default();
        store.store("irc.example", 6697, i64::MAX, at(1));
        let cached = store.lookup("irc.example", at(2)).expect("valid");
        assert_eq!(cached.expiry_epoch_seconds, i64::MAX);
    }
}
