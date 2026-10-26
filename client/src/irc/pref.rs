//! `tui/internal/irc/pref.go`:
//! the `/pref` catalog, its usage and avatar note, the query, state and list
//! formatting, the argument parser, and the in-memory preferences a session
//! holds.
//! Pure: this module reads and writes no file. The preferences are saved in the settings
//! file's `preferences` group by `storage/preferences.rs`.

use serde::{Deserialize, Serialize};

use super::membershipnoise::MembershipNoise;

/// Each `/pref` name (`IrcPrefName`). `Joins` is the one whose value is a noise
/// level. The zero value is `Directs`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PrefName {
    /// Reopens direct messages on startup.
    #[default]
    Directs,
    /// Shows peer avatars.
    Avatars,
    /// Opens conversations at their unread mark.
    Unread,
    /// Paints mIRC foreground and background colors.
    Colors,
    /// How join, part, quit and nick lines show.
    Joins,
}

impl PrefName {
    /// Catalog order, which the slash popup and the bare `/pref` list follow.
    #[cfg(test)]
    pub const ALL: [PrefName; 5] = [
        PrefName::Directs,
        PrefName::Avatars,
        PrefName::Unread,
        PrefName::Colors,
        PrefName::Joins,
    ];

    /// The word typed after `/pref`.
    #[cfg(test)]
    pub fn token(self) -> &'static str {
        self.spec().token
    }

    /// The label in every reply line, such as `Show peer avatars: on`.
    pub fn label(self) -> &'static str {
        self.spec().label
    }

    /// The name for a typed word, ignoring ASCII case (`PrefFind`).
    #[cfg(test)]
    pub fn from_token(token: &str) -> Option<Self> {
        pref_find(token).map(|spec| spec.name)
    }

    pub fn spec(self) -> &'static PrefSpec {
        PREF_CATALOG
            .iter()
            .find(|spec| spec.name == self)
            .unwrap_or(&PREF_CATALOG[0])
    }
}

/// Whether a catalog row is an on/off toggle or the membership-noise choice
/// (`IrcPrefValueKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefValueKind {
    /// On or off.
    Toggle,
    /// Folded, every or hidden.
    Noise,
}

/// One `/pref` catalog row (`IrcPrefSpec`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrefSpec {
    pub name: PrefName,
    pub token: &'static str,
    pub label: &'static str,
    pub value_kind: PrefValueKind,
}

/// The static, ordered `/pref` table (`ircPrefCatalog`).
pub const PREF_CATALOG: [PrefSpec; 5] = [
    PrefSpec {
        name: PrefName::Directs,
        token: "directs",
        label: "Reopen direct messages on startup",
        value_kind: PrefValueKind::Toggle,
    },
    PrefSpec {
        name: PrefName::Avatars,
        token: "avatars",
        label: "Show peer avatars",
        value_kind: PrefValueKind::Toggle,
    },
    PrefSpec {
        name: PrefName::Unread,
        token: "unread",
        label: "Open conversations at unread",
        value_kind: PrefValueKind::Toggle,
    },
    PrefSpec {
        name: PrefName::Colors,
        token: "colors",
        label: "Show mIRC colors",
        value_kind: PrefValueKind::Toggle,
    },
    PrefSpec {
        name: PrefName::Joins,
        token: "joins",
        label: "Join, part, quit, and nick lines",
        value_kind: PrefValueKind::Noise,
    },
];

/// The ordered catalog (`PrefCatalog`).
pub fn pref_catalog() -> &'static [PrefSpec] {
    &PREF_CATALOG
}

/// The catalog row for a typed token, ignoring ASCII case (`PrefFind`). `None`
/// when nothing matches.
pub fn pref_find(token: &str) -> Option<&'static PrefSpec> {
    let folded = token.to_ascii_lowercase();
    PREF_CATALOG.iter().find(|spec| spec.token == folded)
}

/// The `/pref` usage line (`ircPrefUsage`).
#[cfg(test)]
pub fn pref_usage() -> &'static str {
    "/pref [directs|avatars|unread|colors] [on|off] | joins [folded|every|hidden]"
}

/// The avatar privacy note (`ircPrefAvatarNote`).
pub fn pref_avatar_note() -> &'static str {
    "Turn off to keep avatar hosts from seeing your IP on busy channels."
}

/// `<label>: on` or `<label>: off` (`ircFormatPrefState`).
pub fn format_pref_state(name: PrefName, enabled: bool) -> String {
    let state = if enabled { "on" } else { "off" };
    format!("{}: {state}", name.label())
}

/// One toggle's state, with the avatar note appended for `avatars`
/// (`ircFormatPrefQuery`).
pub fn format_pref_query(name: PrefName, enabled: bool) -> String {
    let text = format_pref_state(name, enabled);
    if name == PrefName::Avatars {
        format!("{text}\n{}", pref_avatar_note())
    } else {
        text
    }
}

/// The membership-noise setting, `<label>: <token>` (`ircFormatPrefNoise`).
pub fn format_pref_noise(noise: MembershipNoise) -> String {
    format!("{}: {}", PrefName::Joins.label(), noise.token())
}

/// Every toggle and the noise setting, one per line (`ircFormatPrefList`).
pub fn format_pref_list(
    directs: bool,
    avatars: bool,
    unread: bool,
    colors: bool,
    noise: MembershipNoise,
) -> String {
    [
        format_pref_state(PrefName::Directs, directs),
        format_pref_state(PrefName::Avatars, avatars),
        format_pref_state(PrefName::Unread, unread),
        format_pref_state(PrefName::Colors, colors),
        format_pref_noise(noise),
    ]
    .join("\n")
}

/// The parsed shape of a `/pref` argument (`IrcPrefKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefKind {
    /// Lists every toggle.
    QueryAll,
    /// Queries one toggle.
    QueryOne,
    /// Sets one value.
    Set,
    /// The fallback that prints the usage line.
    Usage,
}

/// A parsed `/pref` argument (`IrcPrefRequest`). `name` is set as soon as the
/// first word names a row, even when the rest is a usage error, as in Go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrefRequest {
    pub kind: PrefKind,
    pub name: PrefName,
    pub enabled: bool,
    pub noise: MembershipNoise,
}

/// Parses a `/pref` argument (`ircParsePrefArgument`). Empty lists all; one known
/// token queries it; two tokens set it for on/off or a noise level; anything else
/// falls back to usage.
pub fn parse_pref_argument(argument: &str) -> PrefRequest {
    let mut request = PrefRequest {
        kind: PrefKind::Usage,
        name: PrefName::default(),
        enabled: false,
        noise: MembershipNoise::default(),
    };
    let parts: Vec<&str> = argument.split_whitespace().collect();
    if parts.is_empty() {
        request.kind = PrefKind::QueryAll;
        return request;
    }
    if parts.len() > 2 {
        return request;
    }

    let Some(spec) = pref_find(parts[0]) else {
        return request;
    };
    request.name = spec.name;
    if parts.len() == 1 {
        request.kind = PrefKind::QueryOne;
        return request;
    }

    let value = parts[1].to_ascii_lowercase();
    if spec.value_kind == PrefValueKind::Noise {
        if let Some(noise) = MembershipNoise::from_token(&value) {
            request.kind = PrefKind::Set;
            request.noise = noise;
        }
        return request;
    }
    match value.as_str() {
        "on" => {
            request.kind = PrefKind::Set;
            request.enabled = true;
        }
        "off" => {
            request.kind = PrefKind::Set;
            request.enabled = false;
        }
        _ => {}
    }
    request
}

/// The value a `/pref` name holds (`PrefValue`): a toggle, or a noise level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefValue {
    Toggle(bool),
    Noise(MembershipNoise),
}

impl PrefValue {
    /// The value `word` sets for `name`, or `None` if the word does not fit.
    /// Toggles take `on` or `off`. `joins` takes folded, every or hidden.
    #[cfg(test)]
    pub fn parse(name: PrefName, word: &str) -> Option<Self> {
        if name == PrefName::Joins {
            return MembershipNoise::from_token(word).map(PrefValue::Noise);
        }
        match word.to_lowercase().as_str() {
            "on" => Some(PrefValue::Toggle(true)),
            "off" => Some(PrefValue::Toggle(false)),
            _ => None,
        }
    }
}

/// The preferences a session holds. Every field is independent, and each has a
/// default so a partial record still loads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// Reopen direct messages on startup.
    pub directs: bool,
    /// Show peer avatars.
    pub avatars: bool,
    /// Open conversations at their unread mark.
    pub unread: bool,
    /// Show mIRC foreground and background colors.
    pub colors: bool,
    /// How join, part, quit and nick lines show.
    pub joins: MembershipNoise,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            directs: true,
            avatars: true,
            unread: true,
            colors: true,
            joins: MembershipNoise::Folded,
        }
    }
}

impl Preferences {
    /// Whether a toggle name is on. `Joins` is not a toggle and reads false.
    pub fn enabled(&self, name: PrefName) -> bool {
        match name {
            PrefName::Directs => self.directs,
            PrefName::Avatars => self.avatars,
            PrefName::Unread => self.unread,
            PrefName::Colors => self.colors,
            PrefName::Joins => false,
        }
    }

    /// Stores `value` under `name`. Returns false, and changes nothing, when the
    /// value's kind does not match the name.
    pub fn set(&mut self, name: PrefName, value: PrefValue) -> bool {
        match (name, value) {
            (PrefName::Directs, PrefValue::Toggle(on)) => self.directs = on,
            (PrefName::Avatars, PrefValue::Toggle(on)) => self.avatars = on,
            (PrefName::Unread, PrefValue::Toggle(on)) => self.unread = on,
            (PrefName::Colors, PrefValue::Toggle(on)) => self.colors = on,
            (PrefName::Joins, PrefValue::Noise(level)) => self.joins = level,
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(kind: PrefKind, name: PrefName) -> PrefRequest {
        PrefRequest {
            kind,
            name,
            enabled: false,
            noise: MembershipNoise::default(),
        }
    }

    #[test]
    fn the_catalog_is_ordered_and_complete() {
        let catalog = pref_catalog();
        assert_eq!(catalog.len(), 5);
        assert_eq!(
            catalog[0],
            PrefSpec {
                name: PrefName::Directs,
                token: "directs",
                label: "Reopen direct messages on startup",
                value_kind: PrefValueKind::Toggle,
            }
        );
        assert_eq!(catalog[1].token, "avatars");
        assert_eq!(catalog[2].label, "Open conversations at unread");
        assert_eq!(catalog[3].name, PrefName::Colors);
        assert_eq!(catalog[3].token, "colors");
        assert_eq!(catalog[3].label, "Show mIRC colors");
        assert_eq!(catalog[3].value_kind, PrefValueKind::Toggle);
        assert_eq!(catalog[4].name, PrefName::Joins);
        assert_eq!(catalog[4].value_kind, PrefValueKind::Noise);
    }

    #[test]
    fn find_resolves_a_token_ignoring_case() {
        assert_eq!(
            pref_find("DIRECTS").map(|s| s.name),
            Some(PrefName::Directs)
        );
        assert_eq!(
            pref_find("avatars").map(|s| s.name),
            Some(PrefName::Avatars)
        );
        assert!(pref_find("nope").is_none());
        assert_eq!(PrefName::from_token("Joins"), Some(PrefName::Joins));
        assert_eq!(PrefName::from_token(""), None);
    }

    #[test]
    fn the_usage_and_note_strings_are_fixed() {
        assert_eq!(
            pref_usage(),
            "/pref [directs|avatars|unread|colors] [on|off] | joins [folded|every|hidden]"
        );
        assert_eq!(
            pref_avatar_note(),
            "Turn off to keep avatar hosts from seeing your IP on busy channels."
        );
    }

    #[test]
    fn a_toggle_state_reads_on_or_off() {
        assert_eq!(
            format_pref_state(PrefName::Directs, true),
            "Reopen direct messages on startup: on"
        );
        assert_eq!(
            format_pref_state(PrefName::Avatars, false),
            "Show peer avatars: off"
        );
    }

    #[test]
    fn a_query_appends_the_avatar_note_only_for_avatars() {
        assert_eq!(
            format_pref_query(PrefName::Directs, true),
            "Reopen direct messages on startup: on"
        );
        assert_eq!(
            format_pref_query(PrefName::Avatars, true),
            format!("Show peer avatars: on\n{}", pref_avatar_note())
        );
    }

    #[test]
    fn the_noise_line_and_the_list_match_the_qt_reply() {
        assert_eq!(
            format_pref_noise(MembershipNoise::Every),
            "Join, part, quit, and nick lines: every"
        );
        assert_eq!(
            format_pref_list(true, false, true, true, MembershipNoise::Every),
            "Reopen direct messages on startup: on\n\
             Show peer avatars: off\n\
             Open conversations at unread: on\n\
             Show mIRC colors: on\n\
             Join, part, quit, and nick lines: every"
        );
    }

    #[test]
    fn the_argument_parser_follows_the_catalog_shapes() {
        let cases = [
            ("", request(PrefKind::QueryAll, PrefName::Directs)),
            ("   ", request(PrefKind::QueryAll, PrefName::Directs)),
            ("directs", request(PrefKind::QueryOne, PrefName::Directs)),
            ("AVATARS", request(PrefKind::QueryOne, PrefName::Avatars)),
            (
                "avatars on",
                PrefRequest {
                    enabled: true,
                    ..request(PrefKind::Set, PrefName::Avatars)
                },
            ),
            (
                "unread OFF",
                PrefRequest {
                    enabled: false,
                    ..request(PrefKind::Set, PrefName::Unread)
                },
            ),
            (
                "colors off",
                PrefRequest {
                    enabled: false,
                    ..request(PrefKind::Set, PrefName::Colors)
                },
            ),
            ("directs maybe", request(PrefKind::Usage, PrefName::Directs)),
            (
                "directs on extra",
                request(PrefKind::Usage, PrefName::Directs),
            ),
            ("bogus", request(PrefKind::Usage, PrefName::Directs)),
            ("bogus on", request(PrefKind::Usage, PrefName::Directs)),
            ("joins on", request(PrefKind::Usage, PrefName::Joins)),
            (
                "directs folded",
                request(PrefKind::Usage, PrefName::Directs),
            ),
            (
                "JOINS every",
                PrefRequest {
                    noise: MembershipNoise::Every,
                    ..request(PrefKind::Set, PrefName::Joins)
                },
            ),
            (
                "joins hidden",
                PrefRequest {
                    noise: MembershipNoise::Hidden,
                    ..request(PrefKind::Set, PrefName::Joins)
                },
            ),
        ];
        for (argument, want) in cases {
            assert_eq!(parse_pref_argument(argument), want, "argument {argument:?}");
        }
    }

    #[test]
    fn a_name_that_is_not_in_the_catalog_falls_back_to_usage_without_a_name() {
        let parsed = parse_pref_argument("bogus");
        assert_eq!(parsed.kind, PrefKind::Usage);
        assert_eq!(parsed.name, PrefName::Directs);
        assert_eq!(parse_pref_argument("a b c").kind, PrefKind::Usage);
    }

    #[test]
    fn values_parse_by_name_kind() {
        assert_eq!(
            PrefValue::parse(PrefName::Unread, "On"),
            Some(PrefValue::Toggle(true))
        );
        assert_eq!(
            PrefValue::parse(PrefName::Avatars, "off"),
            Some(PrefValue::Toggle(false))
        );
        assert_eq!(PrefValue::parse(PrefName::Directs, "folded"), None);
        assert_eq!(
            PrefValue::parse(PrefName::Joins, "hidden"),
            Some(PrefValue::Noise(MembershipNoise::Hidden))
        );
        assert_eq!(PrefValue::parse(PrefName::Joins, "on"), None);
    }

    #[test]
    fn defaults_are_all_on_with_folded_joins() {
        let prefs = Preferences::default();
        assert!(prefs.directs && prefs.avatars && prefs.unread && prefs.colors);
        assert_eq!(prefs.joins, MembershipNoise::Folded);
    }

    #[test]
    fn set_writes_only_a_matching_kind() {
        let mut prefs = Preferences::default();
        assert!(prefs.set(PrefName::Avatars, PrefValue::Toggle(false)));
        assert!(!prefs.avatars);
        assert!(prefs.set(PrefName::Colors, PrefValue::Toggle(false)));
        assert!(!prefs.colors);
        assert!(prefs.set(PrefName::Joins, PrefValue::Noise(MembershipNoise::Every)));
        assert_eq!(prefs.joins, MembershipNoise::Every);
        assert!(!prefs.set(PrefName::Joins, PrefValue::Toggle(true)));
        assert!(!prefs.set(PrefName::Directs, PrefValue::Noise(MembershipNoise::Hidden)));
        assert_eq!(prefs.joins, MembershipNoise::Every);
    }

    #[test]
    fn enabled_reads_toggles_and_is_false_for_joins() {
        let mut prefs = Preferences::default();
        assert!(prefs.enabled(PrefName::Directs));
        assert!(prefs.enabled(PrefName::Colors));
        prefs.set(PrefName::Directs, PrefValue::Toggle(false));
        prefs.set(PrefName::Colors, PrefValue::Toggle(false));
        assert!(!prefs.enabled(PrefName::Directs));
        assert!(!prefs.enabled(PrefName::Colors));
        assert!(!prefs.enabled(PrefName::Joins));
    }

    #[test]
    fn the_catalog_rows_answer_their_own_name() {
        for spec in pref_catalog() {
            assert_eq!(spec.name.spec(), spec);
            assert_eq!(PrefName::from_token(spec.token), Some(spec.name));
        }
        let tokens: Vec<&str> = PrefName::ALL.iter().map(|name| name.token()).collect();
        assert_eq!(tokens, ["directs", "avatars", "unread", "colors", "joins"]);
    }
}
