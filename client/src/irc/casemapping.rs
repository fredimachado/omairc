//! Identifier comparison under the CASEMAPPING rule a server advertises. The
//! default is RFC 1459, where `[]\` fold to `{}|` and `^` folds to `~`.

/// The rule a mapping applies (`IrcCaseMapping::Kind`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IrcCaseMappingKind {
    /// `[]\` fold to `{}|`, and `^` folds to `~`.
    #[default]
    Rfc1459,
    /// ASCII letters only; the special characters stay distinct.
    Ascii,
    /// `[]\` fold to `{}|`, but `^` stays distinct from `~`.
    Rfc1459Strict,
}

/// Normalizes identifiers for comparison (`IrcCaseMapping`). The default is RFC 1459.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct IrcCaseMapping {
    kind: IrcCaseMappingKind,
}

impl IrcCaseMapping {
    pub const fn new(kind: IrcCaseMappingKind) -> Self {
        Self { kind }
    }

    /// Resolves an advertised CASEMAPPING value, matched ASCII-case-insensitively.
    pub fn from_name(name: &str) -> Option<Self> {
        let kind = match name.to_ascii_lowercase().as_str() {
            "rfc1459" => IrcCaseMappingKind::Rfc1459,
            "ascii" => IrcCaseMappingKind::Ascii,
            "rfc1459-strict" | "strict-rfc1459" => IrcCaseMappingKind::Rfc1459Strict,
            _ => return None,
        };
        Some(Self::new(kind))
    }

    pub fn kind(self) -> IrcCaseMappingKind {
        self.kind
    }

    /// Folds `identifier` to its comparison form.
    pub fn normalize(self, identifier: &str) -> String {
        identifier
            .chars()
            .map(|character| normalized_character(character, self.kind))
            .collect()
    }

    /// True when the two identifiers are equal under this mapping.
    pub fn equals(self, left: &str, right: &str) -> bool {
        self.normalize(left) == self.normalize(right)
    }
}

fn normalized_character(character: char, kind: IrcCaseMappingKind) -> char {
    if character.is_ascii_uppercase() {
        return character.to_ascii_lowercase();
    }
    if kind == IrcCaseMappingKind::Ascii {
        return character;
    }
    match character {
        '[' => '{',
        ']' => '}',
        '\\' => '|',
        '^' if kind == IrcCaseMappingKind::Rfc1459 => '~',
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_advertised_mappings() {
        let rfc1459 = IrcCaseMapping::new(IrcCaseMappingKind::Rfc1459);
        assert!(rfc1459.equals("Nick[]\\", "nick{}|"), "rfc1459 must fold []\\ to {{}}|");
        assert!(rfc1459.equals("nick^", "NICK~"), "rfc1459 must fold ^ to ~");

        let strict = IrcCaseMapping::new(IrcCaseMappingKind::Rfc1459Strict);
        assert!(strict.equals("Nick[]\\", "nick{}|"), "strict rfc1459 must fold []\\ to {{}}|");
        assert!(!strict.equals("nick^", "nick~"), "strict rfc1459 must keep ^ distinct from ~");

        let ascii = IrcCaseMapping::new(IrcCaseMappingKind::Ascii);
        assert!(ascii.equals("Nick", "nick"), "ascii mapping must fold case");
        assert!(!ascii.equals("[]\\", "{}|"), "ascii mapping must keep []\\ distinct from {{}}|");

        assert!(IrcCaseMapping::from_name("rfc1459-strict").is_some());
        assert!(IrcCaseMapping::from_name("strict-rfc1459").is_some());
        assert!(IrcCaseMapping::from_name("ASCII").is_some(), "names fold ASCII case");
        assert!(IrcCaseMapping::from_name("unknown").is_none());
    }

    #[test]
    fn default_is_rfc1459() {
        assert_eq!(IrcCaseMapping::default().kind(), IrcCaseMappingKind::Rfc1459);
    }

    #[test]
    fn non_ascii_identifiers_compare_unchanged() {
        let rfc1459 = IrcCaseMapping::default();
        assert!(rfc1459.equals("\u{e9}t\u{e9}", "\u{e9}t\u{e9}"));
        assert!(!rfc1459.equals("\u{c9}T\u{c9}", "\u{e9}t\u{e9}"), "only ASCII letters fold");
    }
}
