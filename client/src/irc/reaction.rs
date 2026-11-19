//! The vocabulary of the IRCv3 `+draft/reply`, `+draft/react` and
//! `+draft/unreact` client tags: what a wire value must look like to be
//! believed, and the per-line aggregate the reducer keeps for reactions
//! (`tui/internal/irc/reaction.go`). Pure: nothing here reads a clock, a socket
//! or the case mapping. The caller hands in the normalized nick.

use std::collections::HashMap;
use std::sync::OnceLock;

/// The longest react value kept, in bytes. A longer value is not an emoji.
pub const MAX_REACTION_BYTES: usize = 64;

/// The longest `+draft/reply` target kept, in bytes. A msgid is opaque to the
/// client, so the cap only stops a hostile tag from riding along.
pub const MAX_REPLY_TARGET_BYTES: usize = 128;

/// How many distinct emoji one line keeps. A reaction past it is dropped.
pub const MAX_REACTION_EMOJIS: usize = 32;

/// How many nicks one emoji keeps. A reaction past it is dropped.
pub const MAX_REACTION_NICKS: usize = 256;

/// The reactions of a conversation that has none, for a read with nothing selected.
pub fn no_reactions() -> &'static HashMap<String, ReactionSet> {
    static NONE: OnceLock<HashMap<String, ReactionSet>> = OnceLock::new();
    NONE.get_or_init(HashMap::new)
}

/// Whether `value` may be a react or unreact value: non-empty, at most
/// `MAX_REACTION_BYTES`, and free of any control character (C0, DEL, C1, and so
/// every line break) (`validReactionValue`).
pub fn valid_reaction_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REACTION_BYTES
        && !value.chars().any(char::is_control)
}

/// Whether `value` may be a `+draft/reply` target: non-empty, at most
/// `MAX_REPLY_TARGET_BYTES`, and free of control characters and whitespace
/// (`validReplyTarget`).
pub fn valid_reply_target(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_REPLY_TARGET_BYTES
        && !value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
}

/// One nick that reacted: the case-mapped form decides identity, the display form
/// is what the tooltip shows.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Reactor {
    normalized: String,
    display: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Reaction {
    emoji: String,
    reactors: Vec<Reactor>,
}

/// The reactions on one line: emoji in first-seen order, each with the nicks that
/// sent it (`ReactionSet`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReactionSet {
    reactions: Vec<Reaction>,
}

/// One chip under a line: the emoji, how many nicks sent it and who they are, in
/// first-seen order (`ReactionChip`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReactionChip {
    pub emoji: String,
    pub count: usize,
    pub nicks: Vec<String>,
}

impl ReactionSet {
    /// Adds `normalized` to `emoji`. The same nick twice counts once. A reaction
    /// past the emoji or nick cap is dropped. Returns whether the set changed
    /// (`Add`).
    pub fn add(&mut self, emoji: &str, normalized: &str, display: &str) -> bool {
        let reactor = Reactor {
            normalized: normalized.to_owned(),
            display: display.to_owned(),
        };
        if let Some(existing) = self.reactions.iter_mut().find(|r| r.emoji == emoji) {
            if existing.reactors.iter().any(|r| r.normalized == normalized)
                || existing.reactors.len() >= MAX_REACTION_NICKS
            {
                return false;
            }
            existing.reactors.push(reactor);
            return true;
        }
        if self.reactions.len() >= MAX_REACTION_EMOJIS {
            return false;
        }
        self.reactions.push(Reaction {
            emoji: emoji.to_owned(),
            reactors: vec![reactor],
        });
        true
    }

    /// Removes `normalized` from `emoji`. The last nick takes the chip with it.
    /// An absent nick is a no-op. Returns whether the set changed (`Remove`).
    pub fn remove(&mut self, emoji: &str, normalized: &str) -> bool {
        let Some(at) = self.reactions.iter().position(|r| r.emoji == emoji) else {
            return false;
        };
        let reactors = &mut self.reactions[at].reactors;
        let Some(nick) = reactors.iter().position(|r| r.normalized == normalized) else {
            return false;
        };
        reactors.remove(nick);
        if reactors.is_empty() {
            self.reactions.remove(at);
        }
        true
    }

    pub fn is_empty(&self) -> bool {
        self.reactions.is_empty()
    }

    /// The chips to draw, in first-seen emoji order (`Chips`).
    pub fn chips(&self) -> Vec<ReactionChip> {
        self.reactions
            .iter()
            .map(|reaction| ReactionChip {
                emoji: reaction.emoji.clone(),
                count: reaction.reactors.len(),
                nicks: reaction
                    .reactors
                    .iter()
                    .map(|reactor| reactor.display.clone())
                    .collect(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_react_value_is_short_visible_text() {
        assert!(valid_reaction_value("👍"));
        assert!(valid_reaction_value("+1"));
        assert!(valid_reaction_value(&"a".repeat(MAX_REACTION_BYTES)));
        assert!(!valid_reaction_value(&"a".repeat(MAX_REACTION_BYTES + 1)));
        assert!(!valid_reaction_value(""));
        for bad in ["a\nb", "a\rb", "\x07", "a\x7fb", "a\u{85}b", "\u{1b}[31m", "a\0b"] {
            assert!(!valid_reaction_value(bad), "{bad:?} must be refused");
        }
    }

    #[test]
    fn a_reply_target_is_one_opaque_token() {
        assert!(valid_reply_target("abc123"));
        assert!(valid_reply_target(&"m".repeat(MAX_REPLY_TARGET_BYTES)));
        assert!(!valid_reply_target(&"m".repeat(MAX_REPLY_TARGET_BYTES + 1)));
        assert!(!valid_reply_target(""));
        for bad in ["a b", "a\tb", "a\nb", "a\x7fb", "a\u{85}b"] {
            assert!(!valid_reply_target(bad), "{bad:?} must be refused");
        }
    }

    #[test]
    fn a_nick_counts_once_per_emoji_and_the_last_one_takes_the_chip() {
        let mut set = ReactionSet::default();
        assert!(set.add("👍", "bob", "Bob"));
        assert!(!set.add("👍", "bob", "bob"), "the same nick adds nothing");
        assert!(set.add("👍", "carol", "carol"));
        assert!(set.add("🎉", "bob", "Bob"));
        let chips = set.chips();
        assert_eq!(chips.len(), 2);
        assert_eq!(chips[0].emoji, "👍");
        assert_eq!(chips[0].count, 2);
        assert_eq!(chips[0].nicks, vec!["Bob", "carol"]);
        assert_eq!(chips[1].emoji, "🎉");

        assert!(!set.remove("👍", "dave"), "an absent nick is a no-op");
        assert!(!set.remove("🔥", "bob"), "an absent emoji is a no-op");
        assert!(set.remove("🎉", "bob"));
        assert_eq!(set.chips().len(), 1, "the last nick takes the chip");
        assert!(set.remove("👍", "bob"));
        assert!(set.remove("👍", "carol"));
        assert!(set.is_empty());
    }

    #[test]
    fn the_caps_drop_a_flood_without_growing() {
        let mut set = ReactionSet::default();
        for index in 0..MAX_REACTION_EMOJIS + 5 {
            set.add(&format!("e{index}"), "bob", "bob");
        }
        assert_eq!(set.chips().len(), MAX_REACTION_EMOJIS);
        for index in 0..MAX_REACTION_NICKS + 5 {
            set.add("e0", &format!("n{index}"), "n");
        }
        assert_eq!(set.chips()[0].count, MAX_REACTION_NICKS);
    }
}
