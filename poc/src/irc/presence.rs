//! Port of `tui/internal/irc/presence.go` (Go), `src/irc/ircmetadata.{h,cpp}`,
//! `src/irc/ircpresence.{h,cpp}` and `src/irc/ircnetworkpresence.{h,cpp}` (C++).
//!
//! Three layers share this file, all pure:
//! - the draft/metadata-2 vocabulary: the subscribed keys, the value ceiling,
//!   the CAP token parse that reads the server's limits (the session's
//!   `subscribe_to_member_metadata` and `set_own_metadata` read them);
//! - the per-nick facts a network keeps: away state, metadata values, the
//!   services account, the GECOS and the operator flag (`NickPresence`), and the
//!   per-network store that holds them (`NetworkPresence`);
//! - the display rules the query header and jump use over those facts.
//!
//! Nothing here reads a clock or a socket. Nick arguments are the normalized
//! (folded) spelling the caller already produced with the network's case
//! mapping, as the Go store expects.

use std::collections::BTreeMap;

/// The client ceiling on a metadata value, in UTF-8 bytes
/// (`IrcMetadata::maximumValueBytes`).
pub const MAXIMUM_VALUE_BYTES: usize = 512;

/// The `status` metadata key. Status comes first in the subscription list so
/// `/status` works under a `max-subs` limit.
pub const STATUS_KEY: &str = "status";
/// The `avatar` metadata key.
pub const AVATAR_KEY: &str = "avatar";
/// The `bot` metadata key. Its value names the software.
pub const BOT_KEY: &str = "bot";
/// The `display-name` metadata key.
pub const DISPLAY_NAME_KEY: &str = "display-name";
/// The `pronouns` metadata key.
pub const PRONOUNS_KEY: &str = "pronouns";
/// The `homepage` metadata key.
pub const HOMEPAGE_KEY: &str = "homepage";
/// The `color` metadata key.
pub const COLOR_KEY: &str = "color";

/// The keys the client subscribes to, in subscription priority order
/// (`IrcMetadata::subscribedKeys`).
pub const SUBSCRIBED_KEYS: [&str; 7] = [
    STATUS_KEY,
    AVATAR_KEY,
    BOT_KEY,
    DISPLAY_NAME_KEY,
    PRONOUNS_KEY,
    HOMEPAGE_KEY,
    COLOR_KEY,
];

/// The draft/metadata-2 limits a server advertised. `None` means the server did
/// not advertise that limit (`IrcMetadataCapability`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MetadataCapability {
    pub max_subs: Option<usize>,
    pub max_value_bytes: Option<usize>,
}

/// The keys to subscribe to, under the server's `max-subs` limit
/// (`SubscriptionKeys`). `None` means no limit; zero means none at all.
pub fn subscription_keys(max_subs: Option<usize>) -> Vec<&'static str> {
    match max_subs {
        None => SUBSCRIBED_KEYS.to_vec(),
        Some(0) => Vec::new(),
        Some(limit) => SUBSCRIBED_KEYS.iter().copied().take(limit).collect(),
    }
}

/// The canonical spelling of a known metadata key, matched without regard to
/// ASCII case (`CanonicalKey`).
pub fn canonical_key(key: &str) -> Option<&'static str> {
    SUBSCRIBED_KEYS
        .iter()
        .copied()
        .find(|known| known.eq_ignore_ascii_case(key))
}

/// Whether `key` is one of the keys the client handles (`IsKnownKey`).
pub fn is_known_key(key: &str) -> bool {
    canonical_key(key).is_some()
}

/// `value`, cut to `MAXIMUM_VALUE_BYTES` (`Clamped`).
pub fn clamped(value: &str) -> String {
    clamped_to(value, MAXIMUM_VALUE_BYTES)
}

/// `value`, cut from the end until its UTF-8 length fits `max_bytes`
/// (`ClampedTo`). The cut is made in UTF-16 units, as the Go and C++ ports do,
/// so a surrogate pair may be split and decodes to U+FFFD. A zero `max_bytes`
/// yields the empty string.
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
/// its limit and the client maximum (`EffectiveMaxValueBytes`). An explicit zero
/// means no non-empty values.
pub fn effective_max_value_bytes(advertised: Option<usize>) -> usize {
    match advertised {
        Some(limit) if limit < MAXIMUM_VALUE_BYTES => limit,
        _ => MAXIMUM_VALUE_BYTES,
    }
}

/// Reads the `draft/metadata-2` token out of a CAP LS / NEW / ACK
/// (`ParseMetadataCapability`). The last occurrence wins. `None` when no such
/// token is present.
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

/// The capability name of a CAP token, dropping any `=value` suffix. It does not
/// drop a leading `-`, matching the anonymous `tokenName` in `ircpresence.cpp`.
fn token_name(token: &str) -> &str {
    token.split_once('=').map_or(token, |(name, _)| name)
}

/// Everything after the first `=`, or empty.
fn token_value(token: &str) -> &str {
    token.split_once('=').map_or("", |(_, value)| value)
}

/// A non-negative integer, or `None` for an empty, non-numeric, overflowing or
/// negative value (`parseNonNegativeInt` in `ircpresence.cpp`).
fn parse_non_negative(raw: &str) -> Option<usize> {
    if raw.is_empty() {
        return None;
    }
    let value: i64 = raw.trim().parse().ok()?;
    usize::try_from(value).ok()
}

/// An away notice and its optional reason (`Away`, `IrcAway`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Away {
    pub reason: String,
}

/// The presence Omairc tracks for one nick on one network: away state, the
/// metadata values, the services account, the GECOS and the operator flag
/// (`NickPresence`, `IrcNickPresence`). The zero value is the default, which
/// the store forgets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NickPresence {
    pub away: Option<Away>,
    /// Canonical metadata keys mapped to their values.
    pub keys: BTreeMap<&'static str, String>,
    /// The services account from account-tag, account-notify, extended-join, or
    /// WHOIS 330. Empty with `account_known` means logged out. Empty without it
    /// means no account fact has arrived. Both display as nothing.
    pub account: String,
    /// True once an account fact has been stored, including a logout. It tells
    /// "logged out" from "never heard".
    pub account_known: bool,
    /// The GECOS from extended-join, WHO (352), or WHOIS (311). Empty means
    /// unknown.
    pub realname: String,
    /// The WHO `*` flag or WHOIS 313.
    pub server_operator: bool,
}

impl NickPresence {
    /// The stored value for `key`, or empty when unset. The key is resolved
    /// through `canonical_key` first (`Metadata`).
    pub fn metadata(&self, key: &str) -> &str {
        canonical_key(key)
            .and_then(|stored| self.keys.get(stored))
            .map_or("", String::as_str)
    }

    /// Whether a metadata key is present, whatever its value (`HasKey`).
    pub fn has_key(&self, key: &str) -> bool {
        canonical_key(key).is_some_and(|stored| self.keys.contains_key(stored))
    }

    /// Whether the bot key marks the client (`IsBot`). The registry treats setting
    /// `bot` as the flag; the value names the software, so `0`, `false` and `no`
    /// (ASCII case-insensitive, trimmed) are not bots.
    pub fn is_bot(&self) -> bool {
        let raw = self.metadata(BOT_KEY);
        if raw.is_empty() {
            return false;
        }
        let trimmed = raw.trim();
        !["0", "false", "no"]
            .iter()
            .any(|off| trimmed.eq_ignore_ascii_case(off))
    }

    /// The `status` metadata value, or empty (`Status`).
    pub fn status(&self) -> &str {
        self.metadata(STATUS_KEY)
    }

    /// The `avatar` metadata value, or empty (`Avatar`).
    pub fn avatar(&self) -> &str {
        self.metadata(AVATAR_KEY)
    }

    /// Whether the presence carries no fact worth keeping (`IsDefault`).
    pub fn is_default(&self) -> bool {
        self.away.is_none()
            && self.keys.is_empty()
            && self.account.is_empty()
            && !self.account_known
            && self.realname.is_empty()
            && !self.server_operator
    }
}

/// Whether a realname is worth matching in jump (`MeaningfulRealname`). A
/// mandatory GECOS is often a placeholder. Equality with the nick is exact.
pub fn meaningful_realname(realname: &str, nick: &str) -> bool {
    if realname.is_empty() || realname == nick {
        return false;
    }
    !matches!(
        realname.to_lowercase().as_str(),
        "realname" | "unknown" | "fullname"
    )
}

/// A realname worth showing on a query header (`DisplayedRealname`). Empty,
/// equal to the nick, and the placeholders `realname`, `unknown` and `fullname`
/// are hidden; comparison ignores case. Jump matching stays on
/// `meaningful_realname`.
pub fn displayed_realname<'a>(realname: &'a str, nick: &str) -> &'a str {
    let trimmed = realname.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case(nick) {
        return "";
    }
    match trimmed.to_lowercase().as_str() {
        "realname" | "unknown" | "fullname" => "",
        _ => trimmed,
    }
}

/// The short query-header labels, in display order (`PeerFactLabels`). An
/// account that is not the nick's own shows; a known logout shows
/// `unauthenticated`; then `server operator`, then `bot`. An unknown account
/// adds nothing.
pub fn peer_fact_labels(facts: &NickPresence, account_matches_nick: bool) -> Vec<&str> {
    let mut labels = Vec::new();
    if !facts.account.is_empty() && !account_matches_nick {
        labels.push(facts.account.as_str());
    } else if facts.account_known && facts.account.is_empty() {
        labels.push("unauthenticated");
    }
    if facts.server_operator {
        labels.push("server operator");
    }
    if facts.is_bot() {
        labels.push("bot");
    }
    labels
}

/// The per-network presence store (`NetworkPresence`, `IrcNetworkPresence`). The
/// default value is empty. Entries that become default are forgotten.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NetworkPresence {
    nicks: BTreeMap<String, NickPresence>,
}

impl NetworkPresence {
    /// Stores away state for `nick`. `None` clears it (`SetAway`).
    pub fn set_away(&mut self, nick: &str, away: Option<Away>) {
        if nick.is_empty() {
            return;
        }
        self.nicks.entry(nick.to_owned()).or_default().away = away;
        self.erase_if_default(nick);
    }

    /// Stores one metadata value for `nick`. An empty value, an unknown key or an
    /// empty nick is ignored; an empty value on a known key clears it
    /// (`SetMetadata`).
    pub fn set_metadata(&mut self, nick: &str, key: &str, value: &str) {
        if nick.is_empty() {
            return;
        }
        let Some(stored) = canonical_key(key) else {
            return;
        };
        if value.is_empty() {
            if let Some(presence) = self.nicks.get_mut(nick) {
                presence.keys.remove(stored);
            }
            self.erase_if_default(nick);
            return;
        }
        self.nicks
            .entry(nick.to_owned())
            .or_default()
            .keys
            .insert(stored, value.to_owned());
    }

    /// Stores the services account for `nick`. `*` and an empty value clear it,
    /// and the fact is recorded as known either way. A missing account tag must
    /// never call this (`SetAccount`).
    pub fn set_account(&mut self, nick: &str, account: &str) {
        if nick.is_empty() {
            return;
        }
        let presence = self.nicks.entry(nick.to_owned()).or_default();
        presence.account_known = true;
        presence.account = if account.is_empty() || account == "*" {
            String::new()
        } else {
            account.to_owned()
        };
        self.erase_if_default(nick);
    }

    /// Stores whether `nick` is a server operator (`SetServerOperator`).
    pub fn set_server_operator(&mut self, nick: &str, operator: bool) {
        if nick.is_empty() {
            return;
        }
        self.nicks
            .entry(nick.to_owned())
            .or_default()
            .server_operator = operator;
        self.erase_if_default(nick);
    }

    /// Stores the GECOS for `nick`. An empty value clears it. Callers skip a
    /// blank WHO or WHOIS field instead of wiping a stored name (`SetRealname`).
    pub fn set_realname(&mut self, nick: &str, realname: &str) {
        if nick.is_empty() {
            return;
        }
        if realname.is_empty() {
            if let Some(presence) = self.nicks.get_mut(nick) {
                presence.realname.clear();
            }
            self.erase_if_default(nick);
            return;
        }
        self.nicks.entry(nick.to_owned()).or_default().realname = realname.to_owned();
    }

    /// Moves the presence from one normalized nick to another, replacing any
    /// entry already at the destination (`Rekey`).
    pub fn rekey(&mut self, from: &str, to: &str) {
        if from == to {
            return;
        }
        self.nicks.remove(to);
        if let Some(presence) = self.nicks.remove(from) {
            self.nicks.insert(to.to_owned(), presence);
        }
    }

    /// Drops every fact about `nick` (`Forget`).
    pub fn forget(&mut self, nick: &str) {
        self.nicks.remove(nick);
    }

    /// Drops every nick (`Clear`).
    pub fn clear(&mut self) {
        self.nicks.clear();
    }

    /// Drops away state for every nick, forgetting those that become default
    /// (`ClearAway`).
    pub fn clear_away(&mut self) {
        self.nicks.retain(|_, presence| {
            presence.away = None;
            !presence.is_default()
        });
    }

    /// Drops metadata for every nick, forgetting those that become default
    /// (`ClearMetadata`).
    pub fn clear_metadata(&mut self) {
        self.nicks.retain(|_, presence| {
            presence.keys.clear();
            !presence.is_default()
        });
    }

    /// Whether any fact is stored for `nick` (`Knows`).
    pub fn knows(&self, nick: &str) -> bool {
        self.nicks.contains_key(nick)
    }

    /// A copy of the stored presence, or the default when absent (`Lookup`).
    pub fn lookup(&self, nick: &str) -> NickPresence {
        self.nicks.get(nick).cloned().unwrap_or_default()
    }

    /// Forgets `nick` when nothing worth keeping remains.
    fn erase_if_default(&mut self, nick: &str) {
        if self.nicks.get(nick).is_some_and(NickPresence::is_default) {
            self.nicks.remove(nick);
        }
    }
}

/// What the transcript shows for one nick, folded from its member records across
/// the channels of a network. For each field the first non-empty value wins, so a
/// nick listed in several channels keeps its real name, account and avatar even
/// where one record leaves them out. `bot` is sticky once any record marks it.
/// `account` is the display account (`DisplayAccount`): empty when it adds nothing.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NickFacts {
    pub account: String,
    pub realname: String,
    pub bot: bool,
    pub avatar: Option<String>,
}

impl NickFacts {
    /// Folds one member record in.
    pub fn absorb(&mut self, account: &str, realname: &str, bot: bool, avatar: Option<&str>) {
        if self.account.is_empty() {
            self.account = account.to_owned();
        }
        if self.realname.is_empty() {
            self.realname = realname.to_owned();
        }
        self.bot |= bot;
        if self.avatar.is_none() {
            self.avatar = avatar.map(str::to_owned);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn presence_with(f: impl FnOnce(&mut NetworkPresence)) -> NetworkPresence {
        let mut store = NetworkPresence::default();
        f(&mut store);
        store
    }

    fn strings(tokens: &[&str]) -> Vec<String> {
        tokens.iter().map(|t| (*t).to_owned()).collect()
    }

    #[test]
    fn subscription_order_puts_status_first() {
        assert_eq!(SUBSCRIBED_KEYS[0], "status");
        assert_eq!(SUBSCRIBED_KEYS.len(), 7);
        assert_eq!(subscription_keys(None), SUBSCRIBED_KEYS.to_vec());
    }

    #[test]
    fn max_subs_truncates_and_zero_subscribes_to_nothing() {
        assert!(subscription_keys(Some(0)).is_empty());
        assert_eq!(subscription_keys(Some(2)), vec!["status", "avatar"]);
        assert_eq!(subscription_keys(Some(99)).len(), 7);
    }

    #[test]
    fn canonical_keys_match_case_insensitively() {
        assert_eq!(canonical_key("DISPLAY-NAME"), Some("display-name"));
        assert_eq!(canonical_key("Avatar"), Some("avatar"));
        assert_eq!(canonical_key("nope"), None);
        assert!(is_known_key("Color"));
        assert!(!is_known_key(""));
    }

    #[test]
    fn clamping_cuts_to_the_byte_budget_in_utf16_units() {
        assert_eq!(clamped_to("hello", 0), "");
        assert_eq!(clamped_to("hello", 3), "hel");
        assert_eq!(clamped_to("hello", 99), "hello");
        // A four-byte emoji is two UTF-16 units. Cutting one unit splits the
        // pair, which decodes to U+FFFD, so the budget shrinks in whole units.
        assert_eq!(clamped_to("a\u{1F600}", 4), "a\u{FFFD}");
        assert_eq!(clamped_to("a\u{1F600}", 5), "a\u{1F600}");
        assert_eq!(clamped("x").len(), 1);
        // At the 512-byte ceiling, 511 letters and an emoji are 515 bytes. Cutting the
        // low surrogate leaves a lone high surrogate, which decodes to three bytes of
        // U+FFFD and does not fit, so only the 511 letters survive.
        let straddling = format!("{}\u{1F600}", "a".repeat(511));
        assert_eq!(clamped(&straddling), "a".repeat(511));
    }

    #[test]
    fn effective_limit_is_the_smaller_of_advertised_and_client_maximum() {
        assert_eq!(effective_max_value_bytes(None), MAXIMUM_VALUE_BYTES);
        assert_eq!(effective_max_value_bytes(Some(0)), 0);
        assert_eq!(effective_max_value_bytes(Some(100)), 100);
        assert_eq!(effective_max_value_bytes(Some(4096)), MAXIMUM_VALUE_BYTES);
    }

    #[test]
    fn metadata_capability_reads_both_limits_last_token_wins() {
        let tokens = strings(&[
            "draft/metadata-2=max-subs=5,max-value-bytes=300",
            "away-notify",
            "draft/metadata-2=max-subs=2",
        ]);
        let parsed = parse_metadata_capability(&tokens).expect("present");
        assert_eq!(parsed.max_subs, Some(2));
        assert_eq!(parsed.max_value_bytes, None, "the later token replaces");
    }

    #[test]
    fn metadata_capability_ignores_bad_numbers_and_absent_tokens() {
        assert_eq!(parse_metadata_capability(&strings(&["away-notify"])), None);
        let tokens = strings(&["draft/metadata-2=max-subs=-1,max-value-bytes=abc,max-subs=7"]);
        let parsed = parse_metadata_capability(&tokens).expect("present");
        assert_eq!(parsed.max_subs, Some(7));
        assert_eq!(parsed.max_value_bytes, None);
        let bare = parse_metadata_capability(&strings(&["draft/metadata-2"])).expect("present");
        assert_eq!(bare, MetadataCapability::default());
    }

    #[test]
    fn bot_value_names_software_and_the_off_words_are_not_bots() {
        let mut facts = NickPresence::default();
        assert!(!facts.is_bot(), "unset is not a bot");
        facts.keys.insert(BOT_KEY, "   ".to_owned());
        assert!(facts.is_bot(), "a blank value is still set, so it is a bot");
        for off in ["0", " FALSE ", "no"] {
            facts.keys.insert(BOT_KEY, off.to_owned());
            assert!(!facts.is_bot(), "{off:?} is off");
        }
        facts.keys.insert(BOT_KEY, "packetbot/1.2".to_owned());
        assert!(facts.is_bot());
    }

    #[test]
    fn metadata_reads_resolve_the_key_case_insensitively() {
        let mut facts = NickPresence::default();
        facts.keys.insert(STATUS_KEY, "brb".to_owned());
        assert_eq!(facts.metadata("STATUS"), "brb");
        assert_eq!(facts.status(), "brb");
        assert!(facts.has_key("Status"));
        assert_eq!(facts.metadata("unknown"), "");
        assert!(!facts.has_key("unknown"));
        assert_eq!(facts.avatar(), "");
    }

    #[test]
    fn realname_hides_placeholders_and_the_nick_itself() {
        assert!(!meaningful_realname("", "fred"));
        assert!(!meaningful_realname("fred", "fred"));
        assert!(!meaningful_realname("Unknown", "fred"));
        assert!(meaningful_realname("Fred Machado", "fred"));
        assert_eq!(
            displayed_realname("  Fred Machado  ", "fred"),
            "Fred Machado"
        );
        assert_eq!(displayed_realname("FRED", "fred"), "");
        assert_eq!(displayed_realname("realname", "fred"), "");
        assert_eq!(displayed_realname("   ", "fred"), "");
    }

    #[test]
    fn fact_labels_follow_the_header_order() {
        let mut facts = NickPresence {
            account: "mira".to_owned(),
            account_known: true,
            server_operator: true,
            ..NickPresence::default()
        };
        facts.keys.insert(BOT_KEY, "bot".to_owned());
        assert_eq!(
            peer_fact_labels(&facts, false),
            vec!["mira", "server operator", "bot"]
        );
        assert_eq!(
            peer_fact_labels(&facts, true),
            vec!["server operator", "bot"],
            "an account that is the nick's own adds nothing"
        );
        let logged_out = NickPresence {
            account_known: true,
            ..NickPresence::default()
        };
        assert_eq!(
            peer_fact_labels(&logged_out, false),
            vec!["unauthenticated"]
        );
        assert!(peer_fact_labels(&NickPresence::default(), false).is_empty());
    }

    #[test]
    fn set_metadata_stores_known_keys_and_ignores_the_rest() {
        let mut store = NetworkPresence::default();
        store.set_metadata("fred", "status", "brb");
        store.set_metadata("fred", "not-a-key", "x");
        store.set_metadata("", "status", "x");
        assert_eq!(store.lookup("fred").status(), "brb");
        assert!(!store.knows(""));
        assert!(!store.lookup("fred").has_key("not-a-key"));
    }

    #[test]
    fn an_empty_value_clears_a_key_and_forgets_a_default_nick() {
        let mut store = presence_with(|s| s.set_metadata("fred", "color", "red"));
        store.set_metadata("fred", "color", "");
        assert!(
            !store.knows("fred"),
            "only metadata was held, so it is forgotten"
        );
        store.set_metadata("ghost", "color", "");
        assert!(!store.knows("ghost"));
    }

    #[test]
    fn account_records_a_fact_even_when_it_is_a_logout() {
        let mut store = NetworkPresence::default();
        store.set_account("fred", "*");
        let facts = store.lookup("fred");
        assert!(facts.account_known);
        assert!(facts.account.is_empty());
        store.set_account("fred", "fredm");
        assert_eq!(store.lookup("fred").account, "fredm");
        store.set_account("", "x");
        assert!(!store.knows(""));
    }

    #[test]
    fn realname_and_operator_forget_a_nick_only_when_it_is_default() {
        let mut store = NetworkPresence::default();
        store.set_realname("fred", "Fred");
        store.set_server_operator("fred", true);
        store.set_realname("fred", "");
        assert!(store.lookup("fred").server_operator);
        store.set_server_operator("fred", false);
        assert!(!store.knows("fred"));
        store.set_realname("fred", "");
        assert!(
            !store.knows("fred"),
            "a blank realname never creates an entry"
        );
    }

    #[test]
    fn away_clears_and_forgets_only_default_nicks() {
        let mut store = NetworkPresence::default();
        store.set_away(
            "fred",
            Some(Away {
                reason: "brb".to_owned(),
            }),
        );
        store.set_metadata("fred", "status", "x");
        store.set_away(
            "mira",
            Some(Away {
                reason: String::new(),
            }),
        );
        store.set_away("mira", None);
        assert!(!store.knows("mira"));
        store.clear_away();
        assert!(store.knows("fred"), "fred still has metadata");
        assert!(store.lookup("fred").away.is_none());
    }

    #[test]
    fn clear_metadata_forgets_nicks_left_with_nothing() {
        let mut store = NetworkPresence::default();
        store.set_metadata("fred", "status", "x");
        store.set_realname("fred", "Fred");
        store.set_metadata("mira", "status", "y");
        store.clear_metadata();
        assert!(store.knows("fred"), "the realname keeps fred");
        assert!(!store.knows("mira"));
        assert!(store.lookup("fred").keys.is_empty());
    }

    #[test]
    fn rekey_moves_facts_and_replaces_the_destination() {
        let mut store = NetworkPresence::default();
        store.set_realname("ann", "Ann");
        store.set_realname("anna", "Old Anna");
        store.rekey("ann", "anna");
        assert!(!store.knows("ann"));
        assert_eq!(store.lookup("anna").realname, "Ann");
        store.rekey("anna", "anna");
        assert_eq!(store.lookup("anna").realname, "Ann");
        store.rekey("nobody", "bob");
        assert!(!store.knows("bob"));
    }

    #[test]
    fn forget_and_clear_drop_facts() {
        let mut store = NetworkPresence::default();
        store.set_realname("fred", "Fred");
        store.set_realname("mira", "Mira");
        store.forget("fred");
        assert!(!store.knows("fred"));
        store.clear();
        assert!(!store.knows("mira"));
    }

    #[test]
    fn nick_facts_keep_the_first_account_name_and_avatar_and_a_sticky_bot() {
        let mut facts = NickFacts::default();
        facts.absorb("", "", false, None);
        facts.absorb("fredm", "Ann Lee", true, Some("qrc:/ann.png"));
        facts.absorb("other", "Other", false, Some("qrc:/two.png"));
        assert_eq!(facts.account, "fredm");
        assert_eq!(facts.realname, "Ann Lee");
        assert_eq!(facts.avatar.as_deref(), Some("qrc:/ann.png"));
        assert!(facts.bot);
    }

    #[test]
    fn a_later_record_fills_in_an_account_an_earlier_one_left_out() {
        let mut facts = NickFacts::default();
        facts.absorb("", "", false, None);
        facts.absorb("kaidev", "", false, None);
        assert_eq!(facts.account, "kaidev");
    }
}
