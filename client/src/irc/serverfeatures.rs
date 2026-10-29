//! `IrcServerFeatures` holds one network's ISUPPORT state. PREFIX, CHANTYPES
//! and CHANMODES are parsed, never hard-coded by callers. It also applies
//! NAMES tokens, MODE rank changes and member labels, and it resolves the
//! draft/ICON and FILEHOST advertisements.
//! One deliberate difference from Go: PREFIX modes and symbols must be ASCII. A
//! PREFIX value with a non-ASCII byte is ignored, because a rank label is one
//! ASCII glyph here. Every other rule follows the Go file.

use super::casemapping::IrcCaseMapping;
use super::message::MAX_CLASSIC_FRAME_BYTES;

/// A set of rank letters, one bit per `a..z` (`IrcPrefixSet`). The default is empty.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IrcPrefixSet(u32);

/// One mode-driven rank grant or revoke (`IrcPrefixChange`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrcPrefixChange {
    nick: String,
    letter: u8,
    grant: bool,
}

impl IrcPrefixChange {
    pub fn nick(&self) -> &str {
        &self.nick
    }
}

/// One 353 NAMES token split into its rank set and nick (`IrcParsedName`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrcParsedName {
    pub nick: String,
    pub ranks: IrcPrefixSet,
}

/// Whether a channel-mode letter consumes a parameter (`IrcServerFeatures::ModeParamRule`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ModeParamRule {
    Never,
    Always,
    SetOnly,
    Prefix,
}

/// One PREFIX entry: the lowercase mode letter and its symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PrefixPair {
    mode: u8,
    symbol: u8,
}

/// The default PREFIX, highest rank first: `(qaohv)~&@%+`.
const DEFAULT_PREFIX: &[(u8, u8)] = &[(b'q', b'~'), (b'a', b'&'), (b'o', b'@'), (b'h', b'%'), (b'v', b'+')];

/// ISUPPORT state for one network (`IrcServerFeatures`). Build one with `new`.
#[derive(Clone, Debug)]
pub struct IrcServerFeatures {
    case_mapping: IrcCaseMapping,
    case_mapping_known: bool,
    channel_types: String,
    prefix_pairs: Vec<PrefixPair>,
    prefix_modes: String,
    prefix_symbols: String,
    nick_length: Option<usize>,
    line_length: usize,
    monitor_advertised: bool,
    monitor_limit: Option<usize>,
    /// CHANMODES groups A, B, C and D.
    chan_modes: [String; 4],
    icon_url: Option<String>,
    file_hosts: Vec<String>,
    mode_rules: [ModeParamRule; 256],
}

impl Default for IrcServerFeatures {
    fn default() -> Self {
        Self::new()
    }
}

impl IrcServerFeatures {
 /// Features with the C++ constructor's defaults: PREFIX `(qaohv)~&@%+`
    /// CHANTYPES `#&`, CHANMODES `b,k,l,imnpst`, and LINELEN 512.
    pub fn new() -> Self {
        let mut features = Self {
            case_mapping: IrcCaseMapping::default(),
            case_mapping_known: false,
            channel_types: "#&".to_owned(),
            prefix_pairs: DEFAULT_PREFIX
                .iter()
                .map(|&(mode, symbol)| PrefixPair { mode, symbol })
                .collect(),
            prefix_modes: String::new(),
            prefix_symbols: String::new(),
            nick_length: None,
            line_length: MAX_CLASSIC_FRAME_BYTES,
            monitor_advertised: false,
            monitor_limit: None,
            chan_modes: ["b", "k", "l", "imnpst"].map(String::from),
            icon_url: None,
            file_hosts: Vec::new(),
            mode_rules: [ModeParamRule::Never; 256],
        };
        features.rebuild_prefix_dumps();
        features
    }

    /// Consumes one ISUPPORT token, or a `-NAME` removal form.
    pub fn apply_token(&mut self, token: &str) {
        if token.is_empty() {
            return;
        }

        if let Some(removed) = token.strip_prefix('-') {
            let name = removed.split_once('=').map_or(removed, |(name, _)| name);
            match name {
                "draft/ICON" => self.icon_url = None,
                "soju.im/FILEHOST" | "draft/FILEHOST" => self.file_hosts.clear(),
                "MONITOR" => {
                    self.monitor_advertised = false;
                    self.monitor_limit = None;
                }
                "LINELEN" => self.line_length = MAX_CLASSIC_FRAME_BYTES,
                _ => {}
            }
            return;
        }

        let (name, value) = token.split_once('=').unwrap_or((token, ""));
        match name {
            "CASEMAPPING" => {
                if let Some(mapping) = IrcCaseMapping::from_name(value) {
                    self.case_mapping = mapping;
                    self.case_mapping_known = true;
                }
            }
            "CHANTYPES" => self.channel_types = value.to_owned(),
            "PREFIX" => self.apply_prefix(value),
            "CHANMODES" => {
                if let Some(groups) = split_chan_modes(value) {
                    self.chan_modes = groups;
                    self.rebuild_mode_rules();
                }
            }
            "NICKLEN" => {
                if let Some(length) = positive_decimal(value) {
                    self.nick_length = Some(length);
                }
            }
            "LINELEN" => {
                if let Some(length) = positive_decimal(value) {
                    self.line_length = length;
                }
            }
            "MONITOR" => self.apply_monitor(value),
            "draft/ICON" => {
                if !value.is_empty() {
                    self.icon_url = Some(value.to_owned());
                }
            }
            "soju.im/FILEHOST" | "draft/FILEHOST" => self.file_hosts = parse_file_host_list(value),
            _ => {}
        }
    }

    /// Consumes a batch of ISUPPORT tokens in order.
    pub fn apply_tokens<S: AsRef<str>>(&mut self, tokens: &[S]) {
        for token in tokens {
            self.apply_token(token.as_ref());
        }
    }

    /// The active case-mapping rule.
    pub fn case_mapping(&self) -> IrcCaseMapping {
        self.case_mapping
    }

    /// True once CASEMAPPING arrived, or registration marked the mapping known.
    pub fn case_mapping_known(&self) -> bool {
        self.case_mapping_known
    }

    /// Records that registration implies the default mapping.
    pub fn mark_case_mapping_known(&mut self) {
        self.case_mapping_known = true;
    }

    /// The advertised CHANTYPES, or `#&` by default.
    pub fn channel_types(&self) -> &str {
        &self.channel_types
    }

    /// True when `target` starts with an advertised channel type.
    pub fn is_channel(&self, target: &str) -> bool {
        target
            .as_bytes()
            .first()
            .is_some_and(|first| self.channel_types.as_bytes().contains(first))
    }

    /// The advertised NICKLEN, if any.
    pub fn nick_length(&self) -> Option<usize> {
        self.nick_length
    }

    /// Client-to-server line length in octets, CRLF included and tags excluded.
    /// The default is 512. A positive LINELEN replaces it, and `-LINELEN` restores 512.
    pub fn line_length(&self) -> usize {
        self.line_length
    }

    /// True when MONITOR is advertised, with or without a limit.
    pub fn monitor_advertised(&self) -> bool {
        self.monitor_advertised
    }

    /// The advertised MONITOR limit, if one was given.
    pub fn monitor_limit(&self) -> Option<usize> {
        self.monitor_limit
    }

    /// The advertised PREFIX mode letters, highest rank first.
    pub fn prefix_modes(&self) -> &str {
        &self.prefix_modes
    }

    /// The advertised PREFIX symbols, highest rank first.
    pub fn prefix_symbols(&self) -> &str {
        &self.prefix_symbols
    }

    /// CHANMODES group A, the list modes.
    pub fn chan_modes_a(&self) -> &str {
        &self.chan_modes[0]
    }

    /// CHANMODES group B, the modes that always take a parameter.
    #[cfg(test)]
    pub fn chan_modes_b(&self) -> &str {
        &self.chan_modes[1]
    }

    /// CHANMODES group C, the modes that take a parameter only when set.
    #[cfg(test)]
    pub fn chan_modes_c(&self) -> &str {
        &self.chan_modes[2]
    }

    /// CHANMODES group D, the modes that take no parameter.
    #[cfg(test)]
    pub fn chan_modes_d(&self) -> &str {
        &self.chan_modes[3]
    }

    /// The advertised draft/ICON template, if any.
    pub fn icon_url(&self) -> Option<&str> {
        self.icon_url.as_deref()
    }

    /// The upload URI this connection may use. An encrypted connection refuses a
    /// plain `http://` URI, and a server may list several.
    pub fn file_host(&self, encrypted: bool) -> Option<&str> {
        let mut cleartext = None;
        for uri in &self.file_hosts {
            if scheme_is(uri, "https://") {
                return Some(uri.as_str());
            }
            if cleartext.is_none() && scheme_is(uri, "http://") {
                cleartext = Some(uri.as_str());
            }
        }
        if encrypted {
            None
        } else {
            cleartext
        }
    }

    /// Splits one 353 NAMES token into its rank set and nick. `None` when the
    /// token is only rank glyphs.
    pub fn parse_names_token(&self, token: &str) -> Option<IrcParsedName> {
        let bytes = token.as_bytes();
        let mut bits = 0;
        let mut index = 0;
        while index < bytes.len() {
            let letter = self.letter_for_symbol(bytes[index]);
            if letter == 0 {
                break;
            }
            bits |= bit_for(letter);
            index += 1;
        }
        let nick = token.get(index..).filter(|nick| !nick.is_empty())?;
        Some(IrcParsedName {
            nick: nick.to_owned(),
            ranks: IrcPrefixSet(bits),
        })
    }

    /// Derives the rank grants and revokes from one MODE line, consuming the
    /// parameters that non-rank modes take as it walks the letters.
    pub fn prefix_changes<S: AsRef<str>>(&self, mode: &str, arguments: &[S]) -> Vec<IrcPrefixChange> {
        let mut changes = Vec::new();
        let mut grant = true;
        let mut argument = 0;
        for &raw in mode.as_bytes() {
            match raw {
                b'+' => {
                    grant = true;
                    continue;
                }
                b'-' => {
                    grant = false;
                    continue;
                }
                _ => {}
            }

            let letter = raw.to_ascii_lowercase();
            let rule = self.rule_for(raw);
            if rule == ModeParamRule::Prefix {
                let Some(nick) = arguments.get(argument) else {
                    continue;
                };
                changes.push(IrcPrefixChange {
                    nick: nick.as_ref().to_owned(),
                    letter,
                    grant,
                });
                argument += 1;
                continue;
            }

            let consumes = rule == ModeParamRule::Always
                || (rule == ModeParamRule::SetOnly && grant);
            if consumes && argument < arguments.len() {
                argument += 1;
            }
        }
        changes
    }

    /// Returns `ranks` with `change` applied. Applying a change twice is the same as once.
    pub fn apply(&self, ranks: IrcPrefixSet, change: &IrcPrefixChange) -> IrcPrefixSet {
        let bit = bit_for(change.letter);
        if bit == 0 {
            return ranks;
        }
        if change.grant {
            IrcPrefixSet(ranks.0 | bit)
        } else {
            IrcPrefixSet(ranks.0 & !bit)
        }
    }

    /// The index of the highest rank in `ranks`, or the number of PREFIX entries when none is set.
    pub fn rank_priority(&self, ranks: IrcPrefixSet) -> usize {
        self.prefix_pairs
            .iter()
            .position(|pair| ranks.0 & bit_for(pair.mode) != 0)
            .unwrap_or(self.prefix_pairs.len())
    }

    /// Prefixes `nick` with the symbol of its highest rank.
    pub fn member_label(&self, ranks: IrcPrefixSet, nick: &str) -> String {
        match self.prefix_pairs.get(self.rank_priority(ranks)) {
            Some(pair) => format!("{}{nick}", char::from(pair.symbol)),
            None => nick.to_owned(),
        }
    }

    fn apply_prefix(&mut self, value: &str) {
        let bytes = value.as_bytes();
        if bytes.len() < 4 || bytes[0] != b'(' {
            return;
        }
        let Some(close) = bytes.iter().position(|&byte| byte == b')') else {
            return;
        };
        if close < 2 {
            return;
        }
        let modes = &bytes[1..close];
        let symbols = &bytes[close + 1..];
        if modes.len() != symbols.len() || !modes.is_ascii() || !symbols.is_ascii() {
            return;
        }
        self.prefix_pairs = modes
            .iter()
            .zip(symbols)
            .map(|(&mode, &symbol)| PrefixPair {
                mode: mode.to_ascii_lowercase(),
                symbol,
            })
            .collect();
        self.rebuild_prefix_dumps();
    }

    fn apply_monitor(&mut self, value: &str) {
        if value.is_empty() {
            self.monitor_advertised = true;
            self.monitor_limit = None;
        } else if let Some(limit) = positive_decimal(value) {
            self.monitor_advertised = true;
            self.monitor_limit = Some(limit);
        }
    }

    fn rebuild_prefix_dumps(&mut self) {
        self.prefix_modes = self.prefix_pairs.iter().map(|pair| char::from(pair.mode)).collect();
        self.prefix_symbols = self.prefix_pairs.iter().map(|pair| char::from(pair.symbol)).collect();
        self.rebuild_mode_rules();
    }

    fn rebuild_mode_rules(&mut self) {
        self.mode_rules = [ModeParamRule::Never; 256];
        let groups = [
            (&self.chan_modes[0], ModeParamRule::Always),
            (&self.chan_modes[1], ModeParamRule::Always),
            (&self.chan_modes[2], ModeParamRule::SetOnly),
            (&self.chan_modes[3], ModeParamRule::Never),
        ];
        for (letters, rule) in groups {
            for &letter in letters.as_bytes() {
                self.mode_rules[usize::from(letter)] = rule;
            }
        }
        for pair in &self.prefix_pairs {
            self.mode_rules[usize::from(pair.mode)] = ModeParamRule::Prefix;
        }
    }

    fn letter_for_symbol(&self, symbol: u8) -> u8 {
        self.prefix_pairs
            .iter()
            .find(|pair| pair.symbol == symbol)
            .map_or(0, |pair| pair.mode)
    }

    /// The rule for `raw`. A rank letter wins over any CHANMODES group, in either case.
    fn rule_for(&self, raw: u8) -> ModeParamRule {
        if self.mode_rules[usize::from(raw.to_ascii_lowercase())] == ModeParamRule::Prefix {
            return ModeParamRule::Prefix;
        }
        self.mode_rules[usize::from(raw)]
    }
}

fn bit_for(letter: u8) -> u32 {
    if letter.is_ascii_lowercase() {
        1 << (letter - b'a')
    } else {
        0
    }
}

/// CHANMODES must carry exactly four comma-separated groups.
fn split_chan_modes(value: &str) -> Option<[String; 4]> {
    let parts: Vec<&str> = value.split(',').collect();
    if parts.len() != 4 {
        return None;
    }
    Some([parts[0], parts[1], parts[2], parts[3]].map(String::from))
}

/// A decimal above zero, digits only: no sign, no whitespace, no overflow.
fn positive_decimal(value: &str) -> Option<usize> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse::<usize>().ok().filter(|&length| length > 0)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Decodes `\x20` and `\\` inside one ISUPPORT value. Other escapes stay as written.
fn unescape_isupport_value(value: &str) -> Vec<u8> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte != b'\\' || index + 1 >= bytes.len() {
            decoded.push(byte);
            index += 1;
            continue;
        }
        let next = bytes[index + 1];
        if next == b'\\' {
            decoded.push(b'\\');
            index += 2;
            continue;
        }
        if next == b'x' && index + 3 < bytes.len() {
            if let (Some(high), Some(low)) = (hex_value(bytes[index + 2]), hex_value(bytes[index + 3])) {
                decoded.push((high << 4) | low);
                index += 4;
                continue;
            }
        }
        decoded.push(byte);
        index += 1;
    }
    decoded
}

/// True when `uri` begins with `scheme`, compared ASCII-case-insensitively.
fn scheme_is(uri: &str, scheme: &str) -> bool {
    uri.len() >= scheme.len() && uri.as_bytes()[..scheme.len()].eq_ignore_ascii_case(scheme.as_bytes())
}

/// Accepts an `http` or `https` URI with a host and no userinfo, printable ASCII only.
fn usable_file_host_uri(uri: &str) -> bool {
    if uri.is_empty() || !uri.bytes().all(|byte| byte > 0x20 && byte < 0x7F) {
        return false;
    }
    let rest = if scheme_is(uri, "https://") {
        &uri["https://".len()..]
    } else if scheme_is(uri, "http://") {
        &uri["http://".len()..]
    } else {
        return false;
    };
    if rest.is_empty() {
        return false;
    }
    let authority = rest.split('/').next().unwrap_or(rest);
    if authority.is_empty() || authority.contains('@') {
        return false;
    }
    let host = authority.split(':').next().unwrap_or(authority);
    !host.is_empty()
}

fn parse_file_host_list(value: &str) -> Vec<String> {
    let decoded = unescape_isupport_value(value);
    decoded
        .split(|&byte| byte == b' ' || byte == b'\t')
        .filter(|token| !token.is_empty())
        .filter_map(|token| std::str::from_utf8(token).ok())
        .filter(|token| usable_file_host_uri(token))
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn features_with(tokens: &[&str]) -> IrcServerFeatures {
        let mut features = IrcServerFeatures::new();
        features.apply_tokens(tokens);
        features
    }

    #[test]
    fn parses_server_features() {
        let mut features = features_with(&["CASEMAPPING=ascii", "CHANTYPES=&", "PREFIX=(qaohv)~&@%+", "NICKLEN=31"]);

        assert_eq!(features.case_mapping().kind(), crate::irc::casemapping::IrcCaseMappingKind::Ascii);
        assert!(features.is_channel("&local"));
        assert!(!features.is_channel("#channel"));
        assert_eq!(features.nick_length(), Some(31));
        assert_eq!(features.line_length(), MAX_CLASSIC_FRAME_BYTES);

        features.apply_token("LINELEN=2048");
        assert_eq!(features.line_length(), 2048);
        features.apply_token("LINELEN=0");
        features.apply_token("LINELEN=nope");
        assert_eq!(features.line_length(), 2048, "a zero or junk LINELEN is ignored");
        features.apply_token("-LINELEN");
        assert_eq!(features.line_length(), MAX_CLASSIC_FRAME_BYTES);
        features.apply_token("LINELEN=300");
        assert_eq!(features.line_length(), 300);

        features.apply_token("MONITOR=100");
        assert!(features.monitor_advertised());
        assert_eq!(features.monitor_limit(), Some(100));
        assert_eq!(features.prefix_modes(), "qaohv");
        assert_eq!(features.prefix_symbols(), "~&@%+");

        let op = features.parse_names_token("@alice").expect("@alice parses");
        assert_eq!(op.nick, "alice");
        assert_eq!(features.member_label(op.ranks, "alice"), "@alice");

        let owner = features.parse_names_token("~alice").expect("~alice parses");
        assert_eq!(features.member_label(owner.ranks, "alice"), "~alice");
    }

    #[test]
    fn case_mapping_known_after_token_or_registration() {
        let mut features = IrcServerFeatures::new();
        assert!(!features.case_mapping_known(), "fresh features must not know the case mapping");

        features.apply_token("CASEMAPPING=ascii");
        assert!(features.case_mapping_known(), "CASEMAPPING must mark the mapping known");

        let mut fresh = IrcServerFeatures::new();
        fresh.mark_case_mapping_known();
        assert!(fresh.case_mapping_known(), "mark_case_mapping_known must mark the mapping known");
    }

    #[test]
    fn invalid_case_mapping_token_does_not_mark_known() {
        let mut features = IrcServerFeatures::new();
        features.apply_token("CASEMAPPING=unknown");
        assert!(!features.case_mapping_known());

        let mut kept = IrcServerFeatures::new();
        kept.apply_token("CASEMAPPING=rfc1459");
        assert!(kept.case_mapping_known());
        kept.apply_token("CASEMAPPING=unknown");
        assert!(kept.case_mapping_known(), "a later invalid CASEMAPPING must not clear the flag");
        assert_eq!(kept.case_mapping().kind(), crate::irc::casemapping::IrcCaseMappingKind::Rfc1459);
    }

    #[test]
    fn default_prefix_strips_owner_before_005() {
        let features = IrcServerFeatures::new();
        assert_eq!(features.prefix_modes(), "qaohv");
        assert_eq!(features.prefix_symbols(), "~&@%+");
        let parsed = features.parse_names_token("~alice").expect("~alice parses");
        assert_eq!(parsed.nick, "alice");
        assert_eq!(features.member_label(parsed.ranks, "alice"), "~alice");

        let mut kept = features.clone();
        kept.apply_token("PREFIX=bad");
        kept.apply_token("-PREFIX");
        kept.apply_token("PREFIX");
        assert_eq!(kept.prefix_modes(), "qaohv", "malformed PREFIX tokens change nothing");
    }

    #[test]
    fn stacked_names_converge_and_paint_highest() {
        let mut features = IrcServerFeatures::new();
        let plus_at = features.parse_names_token("+@alice").expect("+@alice parses");
        let at_plus = features.parse_names_token("@+alice").expect("@+alice parses");
        assert_eq!(plus_at.nick, at_plus.nick);
        assert_eq!(plus_at.ranks, at_plus.ranks);
        assert_eq!(features.member_label(plus_at.ranks, "alice"), "@alice");

        features.apply_token("PREFIX=(ov)@+");
        assert_eq!(features.member_label(plus_at.ranks, "alice"), "@alice", "label after PREFIX remap");
    }

    #[test]
    fn empty_leftover_nick_is_not_a_parse() {
        let features = IrcServerFeatures::new();
        for token in ["@", "@+", "+", ""] {
            assert!(features.parse_names_token(token).is_none(), "parse_names_token({token:?}) succeeded");
        }
    }

    #[test]
    fn prefix_changes_walk_letters_and_apply_idempotently() {
        let features = IrcServerFeatures::new();
        let changes = features.prefix_changes("+ov-o", &["alice", "bob", "alice"]);
        assert_eq!(changes.len(), 3);
        assert_eq!(changes[0].nick(), "alice");
        assert_eq!(changes[1].nick(), "bob");
        assert_eq!(changes[2].nick(), "alice");

        let named = features.parse_names_token("alice").expect("alice parses");
        let mut ranks = named.ranks;
        ranks = features.apply(ranks, &changes[0]);
        ranks = features.apply(ranks, &changes[0]);
        assert_eq!(features.member_label(ranks, "alice"), "@alice");

        ranks = features.apply(ranks, &features.prefix_changes("+v", &["alice"])[0]);
        assert_eq!(features.member_label(ranks, "alice"), "@alice");

        ranks = features.apply(ranks, &features.prefix_changes("-o", &["alice"])[0]);
        assert_eq!(features.member_label(ranks, "alice"), "+alice");

        ranks = features.apply(ranks, &features.prefix_changes("-v", &["alice"])[0]);
        assert_eq!(features.member_label(ranks, "alice"), "alice");
    }

    #[test]
    fn later_prefix_remaps_glyphs_without_rewriting_sets() {
        let mut features = IrcServerFeatures::new();
        let stacked = features.parse_names_token("~@alice").expect("~@alice parses");
        assert_eq!(features.member_label(stacked.ranks, "alice"), "~alice");

        features.apply_token("PREFIX=(ov)@+");
        assert_eq!(features.member_label(stacked.ranks, "alice"), "@alice");

        features.apply_token("PREFIX=(ov)!+");
        assert_eq!(features.member_label(stacked.ranks, "alice"), "!alice");

        features.apply_token("PREFIX=broken");
        assert_eq!(features.prefix_modes(), "ov");
        assert_eq!(features.member_label(stacked.ranks, "alice"), "!alice");
    }

    #[test]
    fn non_ascii_prefix_is_ignored() {
        let mut features = IrcServerFeatures::new();
        // Two modes and one two-byte symbol: the lengths agree, so only the ASCII rule rejects it.
        features.apply_token("PREFIX=(ov)\u{e9}");
        assert_eq!(features.prefix_modes(), "qaohv", "a multi-byte symbol must not be accepted");
    }

    #[test]
    fn parses_chan_modes_from_isupport() {
        let mut features = IrcServerFeatures::new();
        assert_eq!(
            [features.chan_modes_a(), features.chan_modes_b(), features.chan_modes_c(), features.chan_modes_d()],
            ["b", "k", "l", "imnpst"]
        );

        features.apply_token("CHANMODES=eIbq,k,l,imnpst");
        assert_eq!(
            [features.chan_modes_a(), features.chan_modes_b(), features.chan_modes_c(), features.chan_modes_d()],
            ["eIbq", "k", "l", "imnpst"]
        );

        features.apply_token("CHANMODES=bad");
        features.apply_token("-CHANMODES");
        features.apply_token("CHANMODES");
        assert_eq!(features.chan_modes_a(), "eIbq", "malformed CHANMODES tokens change nothing");
    }

    #[test]
    fn prefix_changes_consume_non_prefix_parameters() {
        let mut features = IrcServerFeatures::new();

        let mixed = features.prefix_changes("+k+o", &["secret", "alice"]);
        assert_eq!(mixed.len(), 1);
        assert_eq!(mixed[0].nick(), "alice");
        let named = features.parse_names_token("alice").expect("alice parses");
        let ranks = features.apply(named.ranks, &mixed[0]);
        assert_eq!(features.member_label(ranks, "alice"), "@alice");

        let plus_o = features.prefix_changes("+o", &["alice"]);
        assert_eq!(plus_o.len(), 1);
        assert_eq!(plus_o[0].nick(), "alice");

        assert!(features.prefix_changes("+b", &["mask"]).is_empty());
        let ban_then_op = features.prefix_changes("+b+o", &["mask", "alice"]);
        assert_eq!(ban_then_op.len(), 1);
        assert_eq!(ban_then_op[0].nick(), "alice");

        features.apply_token("CHANMODES=eIbq,k,l,imnpst");
        let advertised = features.prefix_changes("+k+o", &["secret", "alice"]);
        assert_eq!(advertised.len(), 1);
        assert_eq!(advertised[0].nick(), "alice");
        assert!(features.prefix_changes("+b", &["mask"]).is_empty());
        let limit_then_op = features.prefix_changes("+l+o", &["10", "alice"]);
        assert_eq!(limit_then_op.len(), 1);
        assert_eq!(limit_then_op[0].nick(), "alice");
    }

    #[test]
    fn monitor_tokens_advertise_and_limit() {
        let mut features = IrcServerFeatures::new();
        features.apply_token("MONITOR");
        assert!(features.monitor_advertised());
        assert_eq!(features.monitor_limit(), None);
        features.apply_token("MONITOR=0");
        assert_eq!(features.monitor_limit(), None, "a zero limit is ignored");
        features.apply_token("-MONITOR");
        assert!(!features.monitor_advertised());
    }

    #[test]
    fn parses_draft_icon_from_isupport() {
        let mut features = IrcServerFeatures::new();
        assert_eq!(features.icon_url(), None);

        features.apply_token("draft/ICON=https://example.org/icon.svg");
        assert_eq!(features.icon_url(), Some("https://example.org/icon.svg"));

        features.apply_token("draft/ICON=https://example.net/icon.png?size={size}");
        assert_eq!(features.icon_url(), Some("https://example.net/icon.png?size={size}"));

        let mut ignored = IrcServerFeatures::new();
        ignored.apply_token("ICON=https://example.org/x.png");
        assert_eq!(ignored.icon_url(), None, "a bare ICON token is not the draft/ICON advertisement");

        let mut empty = IrcServerFeatures::new();
        empty.apply_token("draft/ICON");
        empty.apply_token("draft/ICON=");
        assert_eq!(empty.icon_url(), None);

        let mut kept = IrcServerFeatures::new();
        kept.apply_token("draft/ICON=https://example.org/icon.svg");
        kept.apply_token("CHANTYPES=#");
        kept.apply_token("NICKLEN=16");
        assert_eq!(kept.icon_url(), Some("https://example.org/icon.svg"));
    }

    #[test]
    fn parses_file_host_from_isupport() {
        let empty = IrcServerFeatures::new();
        assert_eq!(empty.file_host(true), None);
        assert_eq!(empty.file_host(false), None);

        let mut https = IrcServerFeatures::new();
        https.apply_token("soju.im/FILEHOST=https://uploads.example/upload");
        assert_eq!(https.file_host(true), Some("https://uploads.example/upload"));
        assert_eq!(https.file_host(false), Some("https://uploads.example/upload"));

        let mut http_host = IrcServerFeatures::new();
        http_host.apply_token("soju.im/FILEHOST=http://uploads.example/upload");
        assert_eq!(http_host.file_host(true), None, "an encrypted connection refuses http");
        assert_eq!(http_host.file_host(false), Some("http://uploads.example/upload"));

        let mut listed = IrcServerFeatures::new();
        listed.apply_token("draft/FILEHOST=http://plain.example/up\\x20https://safe.example/up");
        assert_eq!(listed.file_host(true), Some("https://safe.example/up"));
        assert_eq!(listed.file_host(false), Some("https://safe.example/up"));

        let mut replaced = IrcServerFeatures::new();
        replaced.apply_token("soju.im/FILEHOST=https://uploads.example/upload");
        replaced.apply_token("soju.im/FILEHOST=ftp://files.example/up");
        assert_eq!(replaced.file_host(false), None);

        let mut icon = IrcServerFeatures::new();
        icon.apply_token("ICON=https://example.org/x.png");
        icon.apply_token("draft/ICON=https://example.org/icon.svg");
        assert_eq!(icon.file_host(true), None);
        assert_eq!(icon.icon_url(), Some("https://example.org/icon.svg"));

        let mut removed = IrcServerFeatures::new();
        removed.apply_token("soju.im/FILEHOST=https://uploads.example/upload");
        removed.apply_token("-soju.im/FILEHOST");
        assert_eq!(removed.file_host(false), None);
        removed.apply_token("draft/FILEHOST=https://uploads.example/upload");
        removed.apply_token("-draft/FILEHOST");
        assert_eq!(removed.file_host(false), None);

        let mut kept = IrcServerFeatures::new();
        kept.apply_token("soju.im/FILEHOST=https://uploads.example/upload");
        kept.apply_token("-FILEHOST");
        kept.apply_token("-ICON");
        kept.apply_token("CHANTYPES=#");
        assert_eq!(kept.file_host(true), Some("https://uploads.example/upload"));

        let mut userinfo = IrcServerFeatures::new();
        userinfo.apply_token("soju.im/FILEHOST=https://user:pw@uploads.example/upload");
        assert_eq!(userinfo.file_host(false), None, "userinfo is refused");

        let mut cleared = IrcServerFeatures::new();
        cleared.apply_token("soju.im/FILEHOST=https://uploads.example/upload");
        cleared.apply_token("soju.im/FILEHOST=");
        assert_eq!(cleared.file_host(false), None);
    }

    #[test]
    fn draft_icon_removal_clears_advertised_url() {
        let mut features = IrcServerFeatures::new();
        features.apply_token("draft/ICON=https://example.org/icon.svg");
        assert_eq!(features.icon_url(), Some("https://example.org/icon.svg"));

        features.apply_token("-draft/ICON");
        assert_eq!(features.icon_url(), None);

        features.apply_token("draft/ICON=https://example.org/icon.svg");
        features.apply_token("-ICON");
        assert_eq!(features.icon_url(), Some("https://example.org/icon.svg"), "-ICON keeps the url");

        features.apply_token("-PREFIX");
        assert_eq!(features.prefix_modes(), "qaohv");
        assert_eq!(features.icon_url(), Some("https://example.org/icon.svg"));
    }

    #[test]
    fn rank_priority_and_labels_follow_prefix_order() {
        let features = IrcServerFeatures::new();
        let ranks = features.parse_names_token("~@%alice").expect("parses").ranks;
        assert_eq!(features.rank_priority(ranks), 0);
        assert_eq!(features.rank_priority(IrcPrefixSet::default()), 5);
        assert_eq!(features.member_label(IrcPrefixSet::default(), "alice"), "alice");
    }
}

#[cfg(test)]
impl IrcServerFeatures {
    /// Features that advertise only `CHANTYPES=<chantypes>`, for the presentation tests
    /// that need a channel prefix set without a whole ISUPPORT burst.
    pub fn with_chantypes(chantypes: &str) -> Self {
        let mut features = Self::new();
        features.apply_token(&format!("CHANTYPES={chantypes}"));
        features
    }
}
