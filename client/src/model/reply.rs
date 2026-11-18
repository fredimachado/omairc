//! The line above a reply: `↪ author: text` when the parent is in the loaded
//! transcript, else `↪ reply`. Go twin: `replyLine` in `tui/internal/controller/replyline.go`.

/// The longest parent text shown, in characters, before it ends in an ellipsis.
pub const REPLY_PREVIEW_CHARS: usize = 60;

const REPLY_ARROW: &str = "\u{21AA}";

/// The text of a reply line. `parent` is the parent's author and body, when the
/// parent is loaded. The body is flattened to one line (every run of whitespace,
/// newlines included, becomes one space), trimmed and cut at `REPLY_PREVIEW_CHARS`.
pub fn reply_line(parent: Option<(&str, &str)>) -> String {
    let Some((author, text)) = parent else {
        return format!("{REPLY_ARROW} reply");
    };
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let preview = if flat.chars().count() > REPLY_PREVIEW_CHARS {
        let kept: String = flat.chars().take(REPLY_PREVIEW_CHARS - 1).collect();
        format!("{}\u{2026}", kept.trim_end())
    } else {
        flat
    };
    format!("{REPLY_ARROW} {author}: {preview}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_parent_reads_reply() {
        assert_eq!(reply_line(None), "\u{21AA} reply");
    }

    #[test]
    fn a_loaded_parent_names_its_author_and_text() {
        assert_eq!(reply_line(Some(("alice", "ship it"))), "\u{21AA} alice: ship it");
    }

    #[test]
    fn the_parent_text_is_one_trimmed_line() {
        assert_eq!(
            reply_line(Some(("alice", "  ship\n\tit   now\r\n"))),
            "\u{21AA} alice: ship it now"
        );
    }

    #[test]
    fn a_long_parent_ends_in_an_ellipsis_within_the_budget() {
        let long = "word ".repeat(40);
        let line = reply_line(Some(("alice", &long)));
        let text = line.strip_prefix("\u{21AA} alice: ").expect("prefix");
        assert!(text.ends_with('\u{2026}'));
        assert!(text.chars().count() <= REPLY_PREVIEW_CHARS);
        let exact = "a".repeat(REPLY_PREVIEW_CHARS);
        assert!(!reply_line(Some(("a", &exact))).ends_with('\u{2026}'));
        // Wide characters count once, never by byte.
        let emoji = "👍".repeat(REPLY_PREVIEW_CHARS);
        assert!(!reply_line(Some(("a", &emoji))).ends_with('\u{2026}'));
    }
}
