//! `draft/multiline` receive (`tui/internal/irc/multiline.go`). A batch of
//! PRIVMSG or NOTICE lines is one message: the lines are joined with `\n`, except
//! where a line carries `draft/multiline-concat`, which joins it to the one before
//! with no separator. The sender, time, message id and account come from the
//! opening `BATCH` line, as the spec says. `assemble_multiline` is pure: the
//! session buffers the lines and calls it when the batch closes, and the result
//! is one ordinary PRIVMSG or NOTICE that the translator and reducer already
//! handle. Raw CR and LF cannot arrive from the wire (the parser rejects them),
//! so the only `\n` in an assembled body is one this module inserted.

use super::message::{IrcMessage, IrcTag};
use super::wiretext::wire_text;

/// The batch type that carries one multi-line message.
pub const MULTILINE_TYPE: &str = "draft/multiline";
/// The tag that joins a line to the previous one with no line break.
pub const MULTILINE_CONCAT_TAG: &str = "draft/multiline-concat";
/// The most lines one batch buffers before the rest arrive one by one.
pub const MULTILINE_MAX_LINES: usize = 100;
/// The most raw parameter bytes one batch buffers before the rest arrive one by one.
pub const MULTILINE_MAX_BYTES: usize = 64 * 1024;

/// The limits a server advertises in the CAP value `max-bytes=4096,max-lines=100`
/// (`MultilineLimits`). Stored for later: receiving needs neither, and the buffer
/// ceilings above are ours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MultilineLimits {
    pub max_bytes: Option<u64>,
    pub max_lines: Option<u64>,
}

/// Reads `max-bytes` and `max-lines` from a CAP value. Keys are case-sensitive;
/// anything else, or a value that is not a number, leaves the field `None`.
pub fn parse_multiline_limits(value: &str) -> MultilineLimits {
    let mut limits = MultilineLimits::default();
    for entry in value.split(',') {
        let Some((key, number)) = entry.split_once('=') else {
            continue;
        };
        let slot = match key {
            "max-bytes" => &mut limits.max_bytes,
            "max-lines" => &mut limits.max_lines,
            _ => continue,
        };
        if slot.is_none() {
            *slot = number.parse().ok();
        }
    }
    limits
}

/// Joins the PRIVMSG and NOTICE lines of one batch into one message
/// (`AssembleMultiline`). `None` means there is nothing to show as one message
/// (no such line, or only blank ones) or a line is CTCP, which the spec does not
/// allow in a batch; the caller then delivers the lines one by one.
pub fn assemble_multiline(open: &IrcMessage, lines: &[IrcMessage]) -> Option<IrcMessage> {
    let mut first: Option<&IrcMessage> = None;
    let mut command = String::new();
    let mut target: &[u8] = &[];
    let mut body = String::new();
    for line in lines {
        let line_command = line.command.to_ascii_uppercase();
        if line_command != "PRIVMSG" && line_command != "NOTICE" {
            continue;
        }
        let text = line.param(1).unwrap_or_default();
        if text.first() == Some(&1) {
            return None;
        }
        let line_target = line.param(0).unwrap_or_default();
        if first.is_none() {
            first = Some(line);
            command = line_command;
            target = line_target;
        } else if line_command != command || !line_target.eq_ignore_ascii_case(target) {
            // The spec calls a batch that mixes commands or targets invalid.
            continue;
        } else if !line.has_tag(MULTILINE_CONCAT_TAG) {
            body.push('\n');
        }
        body.push_str(&wire_text(text));
    }
    let first = first?;
    if body.chars().all(char::is_whitespace) {
        return None;
    }

    let target = open
        .param(2)
        .filter(|target| !target.is_empty())
        .unwrap_or(target);
    let tags: Vec<IrcTag> = open
        .tags
        .iter()
        .filter(|tag| tag.name != "batch")
        .cloned()
        .collect();
    Some(IrcMessage {
        tags,
        prefix: open.prefix.clone().or_else(|| first.prefix.clone()),
        command,
        params: vec![target.to_vec(), body.into_bytes()],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::parser::parse;

    // tui/internal/irc/multiline_test.go. Lines are parsed from wire text, as the
    // session sees them.

    fn line(text: &str) -> IrcMessage {
        parse(text.as_bytes()).unwrap()
    }

    fn open() -> IrcMessage {
        line("@msgid=open1;time=2026-01-02T03:04:05.000Z;account=mira :mira!u@h BATCH +ml draft/multiline #omarchy")
    }

    fn body_of(message: &IrcMessage) -> String {
        message.text_param(1)
    }

    #[test]
    fn limits_read_both_keys() {
        assert_eq!(
            parse_multiline_limits("max-bytes=4096,max-lines=100"),
            MultilineLimits { max_bytes: Some(4096), max_lines: Some(100) }
        );
    }

    #[test]
    fn limits_read_one_key() {
        assert_eq!(
            parse_multiline_limits("max-lines=7"),
            MultilineLimits { max_bytes: None, max_lines: Some(7) }
        );
    }

    #[test]
    fn limits_ignore_empty_junk_unknown_and_non_numeric() {
        for value in ["", "junk", ",,", "max-bytes", "max-bytes=", "max-bytes=abc", "max-bytes=-1", "MAX-BYTES=5", "other=5"] {
            assert_eq!(parse_multiline_limits(value), MultilineLimits::default(), "{value:?}");
        }
        assert_eq!(
            parse_multiline_limits("other=1,max-bytes=9"),
            MultilineLimits { max_bytes: Some(9), max_lines: None }
        );
    }

    #[test]
    fn three_lines_join_with_two_line_breaks() {
        let lines = [
            line("@batch=ml :mira!u@h PRIVMSG #omarchy :one"),
            line("@batch=ml :mira!u@h PRIVMSG #omarchy :two"),
            line("@batch=ml :mira!u@h PRIVMSG #omarchy :three"),
        ];
        let message = assemble_multiline(&open(), &lines).unwrap();
        assert_eq!(message.command, "PRIVMSG");
        assert_eq!(body_of(&message), "one\ntwo\nthree");
        assert_eq!(message.text_param(0), "#omarchy");
    }

    #[test]
    fn a_concat_line_joins_with_no_separator() {
        let lines = [
            line("@batch=ml PRIVMSG #omarchy :a"),
            line("@batch=ml;draft/multiline-concat PRIVMSG #omarchy :b"),
            line("@batch=ml PRIVMSG #omarchy :c"),
        ];
        assert_eq!(body_of(&assemble_multiline(&open(), &lines).unwrap()), "ab\nc");
    }

    #[test]
    fn a_concat_tag_on_the_first_line_is_ignored() {
        let lines = [
            line("@batch=ml;draft/multiline-concat PRIVMSG #omarchy :a"),
            line("@batch=ml PRIVMSG #omarchy :b"),
        ];
        assert_eq!(body_of(&assemble_multiline(&open(), &lines).unwrap()), "a\nb");
    }

    #[test]
    fn the_batch_line_supplies_the_tags_and_the_lines_do_not() {
        let lines = [
            line("@batch=ml;msgid=line1;time=1999-01-01T00:00:00.000Z :other!u@h PRIVMSG #omarchy :one"),
            line("@batch=ml;msgid=line2 PRIVMSG #omarchy :two"),
        ];
        let message = assemble_multiline(&open(), &lines).unwrap();
        assert_eq!(message.msgid(), "open1");
        assert_eq!(message.tag_text("time").as_deref(), Some("2026-01-02T03:04:05.000Z"));
        assert_eq!(message.tag_text("account").as_deref(), Some("mira"));
        assert!(!message.has_tag("batch"));
        assert_eq!(message.prefix.as_ref().unwrap().raw, b"mira!u@h");
    }

    #[test]
    fn the_prefix_falls_back_to_the_first_line() {
        let open = line("@msgid=open1 BATCH +ml draft/multiline #omarchy");
        let lines = [line("@batch=ml :bob!b@h PRIVMSG #omarchy :hi")];
        let message = assemble_multiline(&open, &lines).unwrap();
        assert_eq!(message.prefix.as_ref().unwrap().raw, b"bob!b@h");
    }

    #[test]
    fn the_target_falls_back_to_the_first_line() {
        let open = line("BATCH +ml draft/multiline");
        let lines = [line("@batch=ml PRIVMSG #omarchy :hi")];
        assert_eq!(assemble_multiline(&open, &lines).unwrap().text_param(0), "#omarchy");
    }

    #[test]
    fn a_notice_batch_stays_a_notice() {
        let lines = [
            line("@batch=ml NOTICE #omarchy :one"),
            line("@batch=ml NOTICE #omarchy :two"),
        ];
        let message = assemble_multiline(&open(), &lines).unwrap();
        assert_eq!(message.command, "NOTICE");
        assert_eq!(body_of(&message), "one\ntwo");
    }

    #[test]
    fn a_blank_line_between_text_is_kept() {
        let lines = [
            line("@batch=ml PRIVMSG #omarchy :one"),
            line("@batch=ml PRIVMSG #omarchy :"),
            line("@batch=ml PRIVMSG #omarchy :three"),
        ];
        assert_eq!(body_of(&assemble_multiline(&open(), &lines).unwrap()), "one\n\nthree");
    }

    #[test]
    fn only_blank_lines_assemble_to_nothing() {
        let lines = [
            line("@batch=ml PRIVMSG #omarchy :"),
            line("@batch=ml PRIVMSG #omarchy :  "),
        ];
        assert_eq!(assemble_multiline(&open(), &lines), None);
        assert_eq!(assemble_multiline(&open(), &[]), None);
    }

    #[test]
    fn a_ctcp_line_refuses_the_whole_batch() {
        let lines = [
            line("@batch=ml PRIVMSG #omarchy :one"),
            line("@batch=ml PRIVMSG #omarchy :\u{1}ACTION waves\u{1}"),
        ];
        assert_eq!(assemble_multiline(&open(), &lines), None);
    }

    #[test]
    fn lines_decode_one_by_one() {
        let mut latin = line("@batch=ml PRIVMSG #omarchy :x");
        latin.params[1] = vec![b'c', b'a', b'f', 0xe9];
        let lines = [line("@batch=ml PRIVMSG #omarchy :na\u{ef}ve \u{1F954}"), latin];
        let message = assemble_multiline(&open(), &lines).unwrap();
        assert_eq!(body_of(&message), "na\u{ef}ve \u{1F954}\ncaf\u{e9}");
    }

    #[test]
    fn a_line_of_another_command_or_target_is_skipped() {
        let lines = [
            line("@batch=ml PRIVMSG #omarchy :one"),
            line("@batch=ml NOTICE #omarchy :notice"),
            line("@batch=ml PRIVMSG #other :elsewhere"),
            line("@batch=ml PRIVMSG #OMARCHY :two"),
            line("@batch=ml TOPIC #omarchy :nope"),
        ];
        assert_eq!(body_of(&assemble_multiline(&open(), &lines).unwrap()), "one\ntwo");
    }
}
