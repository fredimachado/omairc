//! The composer's send limit. A draft holds only the bytes one send can carry, so the
//! budget comes from `irc::commandbuilder`, sized by the network's `LINELEN` (its
//! ISUPPORT frame, classic 512 bytes by default). This module keeps the insert-fit
//! rule the composer edits follow. Every number counts bytes, not characters.

use crate::irc::commandbuilder::{clamp_utf8_prefix, composer_byte_budget_for_draft};
use crate::irc::serverfeatures::IrcServerFeatures;

/// Where the composer's draft goes and the features that size its frame. `target` is
/// the channel or nick, or empty for Status.
#[derive(Clone, Debug)]
pub struct SendLimit {
    pub target: String,
    pub features: IrcServerFeatures,
}

impl SendLimit {
    /// The limit for `target` on a network with `features`. `None` is Status.
    pub fn new(target: Option<&str>, features: &IrcServerFeatures) -> Self {
        Self {
            target: target.unwrap_or_default().to_string(),
            features: features.clone(),
        }
    }

    /// How many bytes of `inserted` fit between `before` and `after`.
    pub fn insert_fit_len(&self, before: &str, inserted: &str, after: &str) -> usize {
        insert_fit_len(&self.target, &self.features, before, inserted, after)
    }
}

/// How many bytes of `inserted` may go between `before` and `after` so the whole
/// draft fits its budget. The result is a character boundary of `inserted`.
/// Text already in the draft is never cut, only the insertion. Each round
/// recomputes the budget, because a cut can drop the `/me ` that made the draft
/// an action, and the plain budget is larger.
pub fn insert_fit_len(
    target: &str,
    features: &IrcServerFeatures,
    before: &str,
    inserted: &str,
    after: &str,
) -> usize {
    let mut keep = inserted.len();
    loop {
        let draft = [before, &inserted[..keep], after].concat();
        let budget = composer_byte_budget_for_draft(target, &draft, features);
        let room = budget.saturating_sub(before.len() + after.len());
        if keep <= room {
            return keep;
        }
        // `room` is below `keep`, so each round shortens the insertion.
        keep = clamp_utf8_prefix(inserted, room).len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn features() -> IrcServerFeatures {
        IrcServerFeatures::new()
    }

    #[test]
    fn a_fitting_insertion_is_kept_whole() {
        let limit = SendLimit::new(Some("#omarchy"), &features());
        assert_eq!(limit.insert_fit_len("", "hello", ""), 5);
    }

    #[test]
    fn an_insertion_past_the_budget_is_cut_on_a_character_boundary() {
        // 492 bytes fit a channel PRIVMSG on the default frame.
        let inserted = "é".repeat(300);
        let fit = insert_fit_len("#omarchy", &features(), "", &inserted, "");
        assert!(fit <= 492);
        assert!(inserted.is_char_boundary(fit));
        assert_eq!(fit % 2, 0, "whole two-byte scalars only");
    }

    #[test]
    fn status_takes_the_whole_raw_line_budget() {
        let limit = SendLimit::new(None, &features());
        assert_eq!(limit.insert_fit_len("", &"x".repeat(600), ""), 510);
    }

    #[test]
    fn a_longer_frame_from_linelen_widens_the_budget() {
        let mut wide = features();
        wide.apply_token("LINELEN=2048");
        let inserted = "x".repeat(1500);
        assert_eq!(insert_fit_len("#omarchy", &wide, "", &inserted, ""), 1500);
        assert!(insert_fit_len("#omarchy", &features(), "", &inserted, "") < 1500);
    }
}
