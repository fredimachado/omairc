//! Port of `tui/internal/irc/jointarget.go`.
//!
//! A validated JOIN target: a channel name and an optional channel key. The
//! `/join` argument parser turns user input into these, and the session sends
//! them. Go has no test file for this module, so the tests are the C++
//! `parseJoinTargets` cases from `tests/session/tst_command.cpp`.

use super::serverfeatures::IrcServerFeatures;

/// A channel and an optional key, both validated (`IrcJoinTarget`). Build one with
/// `make` or `parse_join_targets`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrcJoinTarget {
    channel: String,
    key: Option<String>,
}

impl IrcJoinTarget {
    /// Validates one channel and an optional key. A name without an advertised
    /// channel type gets `#` in front. The result must be at least two UTF-16 units
    /// with no field breakers. A key is trimmed, must not start with `:`, and must
    /// hold no field breakers. `None` means the target is invalid.
    pub fn make(channel: &str, key: Option<&str>, features: &IrcServerFeatures) -> Option<Self> {
        let trimmed_channel = channel.trim();
        if trimmed_channel.is_empty() {
            return None;
        }
        let name = if features.is_channel(trimmed_channel) {
            trimmed_channel.to_owned()
        } else {
            format!("#{trimmed_channel}")
        };
        if name.encode_utf16().count() < 2 || has_field_breakers(&name) {
            return None;
        }

        let key = match key {
            None => None,
            Some(raw) => {
                let trimmed = raw.trim();
                if !is_safe_join_key(trimmed) {
                    return None;
                }
                Some(trimmed.to_owned())
            }
        };
        Some(Self { channel: name, key })
    }

    pub fn channel(&self) -> &str {
        &self.channel
    }

    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    pub fn has_key(&self) -> bool {
        self.key.is_some()
    }
}

/// Splits a `/join` argument into validated targets. Commas separate entries, and
/// each entry may carry one key after whitespace. `None` when the argument is
/// empty or any entry is invalid.
pub fn parse_join_targets(argument: &str, features: &IrcServerFeatures) -> Option<Vec<IrcJoinTarget>> {
    let mut targets = Vec::new();
    for entry in argument.split(',') {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            continue;
        }
        let tokens: Vec<&str> = split_ascii_whitespace_runs(trimmed).collect();
        if tokens.is_empty() || tokens.len() > 2 {
            return None;
        }
        let key = tokens.get(1).copied();
        targets.push(IrcJoinTarget::make(tokens[0], key, features)?);
    }
    if targets.is_empty() {
        None
    } else {
        Some(targets)
    }
}

fn has_field_breakers(token: &str) -> bool {
    token
        .chars()
        .any(|character| character.is_whitespace() || character == ',' || character == '\0')
}

fn is_safe_join_key(key: &str) -> bool {
    !key.is_empty() && !key.starts_with(':') && !has_field_breakers(key)
}

/// Splits on runs of ASCII whitespace. That set includes vertical tab, which
/// `u8::is_ascii_whitespace` leaves out, to match the Go and C++ tokenizers.
fn split_ascii_whitespace_runs(text: &str) -> impl Iterator<Item = &str> {
    text.split(|character: char| matches!(character, ' ' | '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r'))
        .filter(|token| !token.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn features_with(token: &str) -> IrcServerFeatures {
        let mut features = IrcServerFeatures::new();
        features.apply_token(token);
        features
    }

    fn channels(targets: &[IrcJoinTarget]) -> Vec<&str> {
        targets.iter().map(IrcJoinTarget::channel).collect()
    }

    #[test]
    fn parses_keys_and_packed_lists() {
        let features = IrcServerFeatures::new();

        let keyed = parse_join_targets("#a pword, #b, c", &features).expect("valid");
        assert_eq!(keyed.len(), 3);
        assert_eq!(keyed[0].channel(), "#a");
        assert_eq!(keyed[0].key(), Some("pword"));
        assert_eq!(keyed[1].channel(), "#b");
        assert!(!keyed[1].has_key());
        assert_eq!(keyed[2].channel(), "#c");
        assert!(!keyed[2].has_key());

        let last_keyed = parse_join_targets("#a, #b, #c pworddd", &features).expect("valid");
        assert_eq!(last_keyed.len(), 3);
        assert!(!last_keyed[0].has_key());
        assert!(!last_keyed[1].has_key());
        assert_eq!(last_keyed[2].channel(), "#c");
        assert_eq!(last_keyed[2].key(), Some("pworddd"));

        let packed = parse_join_targets("#a,b,#c", &features).expect("valid");
        assert_eq!(channels(&packed), ["#a", "#b", "#c"]);
        assert!(packed.iter().all(|target| !target.has_key()));

        let local = parse_join_targets("&local secret", &features).expect("valid");
        assert_eq!(local.len(), 1);
        assert_eq!(local[0].channel(), "&local");
        assert_eq!(local[0].key(), Some("secret"));

        let trailing_comma = parse_join_targets("#a,#b,", &features).expect("valid");
        assert_eq!(channels(&trailing_comma), ["#a", "#b"]);
    }

    #[test]
    fn rejects_invalid_arguments() {
        let features = IrcServerFeatures::new();
        assert!(parse_join_targets("#a b c", &features).is_none());
        assert!(parse_join_targets("", &features).is_none());
        assert!(parse_join_targets(" , ", &features).is_none());
        assert!(parse_join_targets("#", &features).is_none(), "one character is too short");
    }

    #[test]
    fn prefixes_names_without_an_advertised_type() {
        let features = IrcServerFeatures::new();
        let plus = parse_join_targets("+foo", &features).expect("valid");
        assert_eq!(channels(&plus), ["#+foo"]);

        let custom = parse_join_targets("$serv", &features_with("CHANTYPES=$")).expect("valid");
        assert_eq!(channels(&custom), ["$serv"]);
    }

    #[test]
    fn make_validates_channel_and_key() {
        let features = IrcServerFeatures::new();

        assert!(IrcJoinTarget::make("", None, &features).is_none());
        assert!(IrcJoinTarget::make("#a b", None, &features).is_none());
        assert!(IrcJoinTarget::make("#a,b", None, &features).is_none());
        assert!(IrcJoinTarget::make("#a", Some("p word"), &features).is_none());
        assert!(IrcJoinTarget::make("#a", Some("x,y"), &features).is_none());
        assert!(IrcJoinTarget::make("#a", Some(""), &features).is_none(), "an empty key is not a key");
        assert!(IrcJoinTarget::make("#ok", Some(":key"), &features).is_none());
        // No channel type, so `#` goes in front: the leading colon is not a field breaker.
        assert_eq!(
            IrcJoinTarget::make(":chan", None, &features).map(|target| target.channel().to_owned()).as_deref(),
            Some("#:chan")
        );
        // Ends are trimmed, so a trailing CR or LF is dropped; an interior break or NUL is not.
        assert!(IrcJoinTarget::make("#a\rb", None, &features).is_none());
        assert!(IrcJoinTarget::make("#a\nb", None, &features).is_none());
        assert!(IrcJoinTarget::make("#a\0", None, &features).is_none());
        assert_eq!(IrcJoinTarget::make("#a\r", None, &features).map(|target| target.channel().to_owned()).as_deref(), Some("#a"));
        assert!(IrcJoinTarget::make("#a", Some("k\0ey"), &features).is_none());
        assert!(IrcJoinTarget::make(&"A".repeat(508), None, &features).is_some_and(|target| target.channel().len() == 509));

        let unkeyed = IrcJoinTarget::make("#a", None, &features).expect("valid");
        assert_eq!(unkeyed.channel(), "#a");
        assert_eq!(unkeyed.key(), None);

        let padded = IrcJoinTarget::make("  #a  ", Some("  pword  "), &features).expect("valid");
        assert_eq!(padded.channel(), "#a");
        assert_eq!(padded.key(), Some("pword"));
    }
}
