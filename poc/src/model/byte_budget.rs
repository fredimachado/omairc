//! The composer's send limit, ported from `IrcCommandBuilder::composerByteBudget`,
//! `actionComposerByteBudget` and `clampUtf8Prefix` (`src/irc/irccommandbuilder.cpp`).
//! A line has to fit one IRC frame, so a draft holds only the bytes one PRIVMSG
//! can carry. Every number here counts bytes, not characters.

/// The classic IRC frame, CRLF included. The PoC has no ISUPPORT `LINELEN` yet,
/// so every network uses this frame.
pub const CLASSIC_FRAME_BYTES: usize = 512;

/// `\r\n`, which the frame counts and the composer does not hold.
const CRLF_BYTES: usize = 2;
/// `\x01ACTION ` (8 bytes) plus the closing `\x01`.
const ACTION_WRAPPER_BYTES: usize = 9;
/// The `/me ` that the composer holds in front of an action body.
const ME_PREFIX_BYTES: usize = 4;

/// The bytes a draft may hold for `target` so one send fits `frame_bytes`.
/// `None` (or an empty target) is Status, a raw line that holds the whole frame
/// but for CRLF. A channel or nick is the body of `PRIVMSG <target> :<body>\r\n`.
pub fn composer_byte_budget(target: Option<&str>, frame_bytes: usize) -> usize {
    if frame_bytes < CRLF_BYTES {
        return 0;
    }
    let Some(target) = target.filter(|target| !target.is_empty()) else {
        return frame_bytes - CRLF_BYTES;
    };
    // "PRIVMSG " + target + " :"
    let prefix = "PRIVMSG ".len() + target.len() + " :".len();
    if prefix + 3 > frame_bytes {
        return 0;
    }
    frame_bytes - prefix - CRLF_BYTES
}

/// The budget for a `/me` draft to `target`. It is sent as CTCP ACTION, which
/// adds 9 bytes of wrapper to the PRIVMSG body, and the composer keeps the 4
/// bytes of `/me `. The result is 9 bytes less than the plain budget, plus 4.
pub fn action_byte_budget(target: &str, frame_bytes: usize) -> usize {
    let privmsg = composer_byte_budget(Some(target), frame_bytes);
    if privmsg < ACTION_WRAPPER_BYTES {
        return 0;
    }
    privmsg - ACTION_WRAPPER_BYTES + ME_PREFIX_BYTES
}

/// Whether `draft` is a `/me` line: optional leading spaces or tabs, then `/me`
/// in any case, then a space. A tab after `/me` does not count (Qt `draftIsAction`).
pub fn draft_is_action(draft: &str) -> bool {
    let trimmed = draft.trim_start_matches([' ', '\t']);
    if !trimmed.starts_with('/') {
        return false;
    }
    let Some(space) = trimmed.find(' ') else {
        return false;
    };
    trimmed[..space].eq_ignore_ascii_case("/me")
}

/// The budget for a whole draft. A `/me` draft to a channel or nick gets the
/// action budget. Everything else gets the plain one, and so does Status, which
/// never sends `/me` as an action (Qt `composerByteBudgetForDraft`).
pub fn composer_budget_for_draft(target: Option<&str>, draft: &str, frame_bytes: usize) -> usize {
    match target.filter(|target| !target.is_empty()) {
        Some(target) if draft_is_action(draft) => action_byte_budget(target, frame_bytes),
        _ => composer_byte_budget(target, frame_bytes),
    }
}

/// The longest prefix of `text` that is at most `max_bytes` long and ends on a
/// character boundary. A multi-byte character is kept whole or dropped.
pub fn clamp_utf8_prefix(text: &str, max_bytes: usize) -> &str {
    if max_bytes >= text.len() {
        return text;
    }
    let mut end = max_bytes;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// How many bytes of `inserted` may go between `before` and `after` so the whole
/// draft fits its budget. The result is a character boundary of `inserted`.
/// Text already in the draft is never cut, only the insertion. Each round
/// recomputes the budget, because a cut can drop the `/me ` that made the draft
/// an action, and the plain budget is larger.
pub fn insert_fit_len(
    target: Option<&str>,
    before: &str,
    inserted: &str,
    after: &str,
    frame_bytes: usize,
) -> usize {
    let mut keep = inserted.len();
    loop {
        let draft = [before, &inserted[..keep], after].concat();
        let budget = composer_budget_for_draft(target, &draft, frame_bytes);
        let room = budget.saturating_sub(before.len() + after.len());
        if keep <= room {
            return keep;
        }
        // `room` is below `keep`, so each round shortens the insertion.
        keep = clamp_utf8_prefix(inserted, room).len();
    }
}

/// Where the composer's draft goes and the frame it must fit. `target` is the
/// channel or nick, or `None` for Status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendLimit {
    pub target: Option<String>,
    pub frame_bytes: usize,
}

impl SendLimit {
    pub fn new(target: Option<&str>, frame_bytes: usize) -> Self {
        Self {
            target: target.map(str::to_string),
            frame_bytes,
        }
    }

    /// How many bytes of `inserted` fit between `before` and `after`.
    pub fn insert_fit_len(&self, before: &str, inserted: &str, after: &str) -> usize {
        insert_fit_len(
            self.target.as_deref(),
            before,
            inserted,
            after,
            self.frame_bytes,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OMARCHY: &str = "#omarchy";
    const PLACE: &str = "#place";

    #[test]
    fn plain_budgets_match_the_qt_frame_numbers() {
        assert_eq!(composer_byte_budget(None, CLASSIC_FRAME_BYTES), 510);
        assert_eq!(composer_byte_budget(Some(PLACE), CLASSIC_FRAME_BYTES), 494);
        assert_eq!(composer_byte_budget(Some(OMARCHY), CLASSIC_FRAME_BYTES), 492);
        assert_eq!(composer_byte_budget(Some(""), CLASSIC_FRAME_BYTES), 510);
    }

    #[test]
    fn a_target_too_long_for_the_frame_has_no_budget() {
        let long = "x".repeat(500);
        assert_eq!(composer_byte_budget(Some(&long), CLASSIC_FRAME_BYTES), 0);
        assert_eq!(composer_byte_budget(None, 1), 0);
    }

    #[test]
    fn budgets_follow_a_smaller_or_larger_line_length() {
        assert_eq!(composer_byte_budget(None, 300), 298);
        assert_eq!(composer_byte_budget(Some(OMARCHY), 300), 280);
        assert_eq!(composer_byte_budget(None, 2048), 2046);
        assert_eq!(composer_byte_budget(Some(OMARCHY), 2048), 2028);
    }

    #[test]
    fn action_budget_leaves_room_for_the_ctcp_wrapper() {
        assert_eq!(action_byte_budget(OMARCHY, CLASSIC_FRAME_BYTES), 487);
        // The frame is "PRIVMSG #omarchy :" (18) + "\x01ACTION " (8) + body + "\x01" (1) + CRLF (2).
        let body = action_byte_budget(OMARCHY, CLASSIC_FRAME_BYTES) - 4;
        assert_eq!(18 + 8 + body + 1 + 2, CLASSIC_FRAME_BYTES);
    }

    #[test]
    fn action_detection_follows_the_qt_rule() {
        assert!(draft_is_action("/me waves"));
        assert!(draft_is_action("/ME waves"));
        assert!(draft_is_action("  \t/Me waves"));
        assert!(!draft_is_action("/me"), "needs the space after the verb");
        assert!(!draft_is_action("/me\twaves"), "a tab does not end the verb");
        assert!(!draft_is_action("/mex waves"));
        assert!(!draft_is_action("hi /me waves"));
        assert!(!draft_is_action(""));
    }

    #[test]
    fn draft_budget_picks_the_action_budget_only_for_me_on_a_conversation() {
        assert_eq!(composer_budget_for_draft(Some(OMARCHY), "/me x", CLASSIC_FRAME_BYTES), 487);
        assert_eq!(composer_budget_for_draft(Some(OMARCHY), "x", CLASSIC_FRAME_BYTES), 492);
        assert_eq!(composer_budget_for_draft(None, "/me x", CLASSIC_FRAME_BYTES), 510);
    }

    #[test]
    fn utf8_clamp_keeps_a_character_whole_or_drops_it() {
        assert_eq!(clamp_utf8_prefix("abcd", 3), "abc");
        assert_eq!(clamp_utf8_prefix("abcd", 0), "");

        let acute = "\u{e9}";
        assert_eq!(clamp_utf8_prefix(&format!("a{acute}"), 2), "a");
        assert_eq!(clamp_utf8_prefix(&format!("a{acute}"), 3), format!("a{acute}"));

        let euro = "\u{20ac}";
        assert_eq!(clamp_utf8_prefix(&format!("a{euro}"), 3), "a");
        assert_eq!(clamp_utf8_prefix(&format!("a{euro}"), 4), format!("a{euro}"));

        let grin = "\u{1f600}";
        assert_eq!(clamp_utf8_prefix(&format!("a{grin}"), 4), "a");
        assert_eq!(clamp_utf8_prefix(&format!("a{grin}"), 5), format!("a{grin}"));
    }

    #[test]
    fn a_long_insertion_is_cut_to_the_budget() {
        let typed = "a".repeat(800);
        assert_eq!(insert_fit_len(Some(OMARCHY), "", &typed, "", CLASSIC_FRAME_BYTES), 492);
        assert_eq!(insert_fit_len(Some(PLACE), "", &typed, "", CLASSIC_FRAME_BYTES), 494);
        assert_eq!(insert_fit_len(None, "", &typed, "", CLASSIC_FRAME_BYTES), 510);
    }

    #[test]
    fn a_full_draft_takes_nothing_more_but_a_deletion_still_works() {
        let full = "a".repeat(492);
        assert_eq!(insert_fit_len(Some(OMARCHY), &full, "b", "", CLASSIC_FRAME_BYTES), 0);
        assert_eq!(insert_fit_len(Some(OMARCHY), &full, "", "", CLASSIC_FRAME_BYTES), 0);
    }

    #[test]
    fn an_insertion_in_the_middle_keeps_the_text_after_it() {
        // 4 bytes after the cursor leave 488 for the insertion on #omarchy.
        assert_eq!(
            insert_fit_len(Some(OMARCHY), "ab", &"x".repeat(600), "cd", CLASSIC_FRAME_BYTES),
            488
        );
    }

    #[test]
    fn a_multi_byte_character_that_does_not_fit_is_dropped_whole() {
        let before = "a".repeat(491);
        assert_eq!(
            insert_fit_len(Some(OMARCHY), &before, "\u{e9}", "", CLASSIC_FRAME_BYTES),
            0,
            "one byte of room cannot hold a two-byte character"
        );
        let before = "a".repeat(490);
        assert_eq!(
            insert_fit_len(Some(OMARCHY), &before, "\u{e9}\u{e9}", "", CLASSIC_FRAME_BYTES),
            2
        );
    }

    #[test]
    fn a_me_prefix_gets_the_action_budget_and_the_cut_is_rechecked() {
        let text = format!("/me {}", "a".repeat(600));
        assert_eq!(
            insert_fit_len(Some(OMARCHY), "", &text, "", CLASSIC_FRAME_BYTES),
            487
        );

        // Typing "/me b" at the start of a full line takes the "/me" away from the
        // budget once the space is cut, so the result must fit the plain budget.
        let after = "a".repeat(486);
        let keep = insert_fit_len(Some(OMARCHY), "", "/me b", &after, CLASSIC_FRAME_BYTES);
        let draft = format!("{}{}", &"/me b"[..keep], after);
        assert!(draft.len() <= composer_budget_for_draft(Some(OMARCHY), &draft, CLASSIC_FRAME_BYTES));
    }

    #[test]
    fn status_never_uses_the_action_budget() {
        let text = format!("/me {}", "a".repeat(600));
        assert_eq!(insert_fit_len(None, "", &text, "", CLASSIC_FRAME_BYTES), 510);
    }

    #[test]
    fn send_limit_applies_its_target_and_frame() {
        let limit = SendLimit::new(Some(OMARCHY), CLASSIC_FRAME_BYTES);
        assert_eq!(limit.insert_fit_len("", &"a".repeat(600), ""), 492);
        let status = SendLimit::new(None, CLASSIC_FRAME_BYTES);
        assert_eq!(status.insert_fit_len("", &"a".repeat(600), ""), 510);
    }
}
