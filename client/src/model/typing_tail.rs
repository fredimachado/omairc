//! Where a DM's typing dots sit, after its last line (Qt `typingFollowsPeerChat`
//! in `client/src/ui/shell`).
//!
//! The dots ride under the peer's last line, with no avatar or header, when that
//! line is a chat line from the peer stamped with the current minute. Otherwise
//! the dots take an avatar and a nick header of their own.

use super::transcript::{Row, RowKind};

/// Whether the typing dots for `peer` continue the last row: that row is a chat
/// line from `peer` stamped `minute` (`HH:MM`, the local time now).
pub fn typing_continues(rows: &[Row], peer: &str, minute: &str) -> bool {
    if peer.is_empty() || minute.is_empty() {
        return false;
    }
    rows.last().is_some_and(|last| {
        last.kind == RowKind::Message
            && last.author.as_deref() == Some(peer)
            && last.time.as_deref() == Some(minute)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::transcript::{row_from_message, row_from_whois};

    #[test]
    fn rides_under_the_peers_line_in_the_current_minute() {
        let rows = [row_from_message(1, "anna", "10:12", "hi", false)];
        assert!(typing_continues(&rows, "anna", "10:12"));
    }

    #[test]
    fn an_older_minute_gets_a_header_of_its_own() {
        let rows = [row_from_message(1, "anna", "10:12", "hi", false)];
        assert!(!typing_continues(&rows, "anna", "10:13"));
    }

    #[test]
    fn another_authors_last_line_gets_a_header() {
        let rows = [row_from_message(1, "dax", "10:12", "hi", false)];
        assert!(!typing_continues(&rows, "anna", "10:12"));
    }

    #[test]
    fn a_whois_or_event_last_line_gets_a_header() {
        let whois = [row_from_whois(1, "VERSION reply from anna")];
        assert!(!typing_continues(&whois, "anna", "10:12"));
        let mut event = row_from_message(1, "anna", "10:12", "hi", false);
        event.kind = RowKind::Event;
        assert!(!typing_continues(&[event], "anna", "10:12"));
    }

    #[test]
    fn only_the_last_row_counts() {
        let rows = [
            row_from_message(1, "anna", "10:12", "hi", false),
            row_from_message(2, "dax", "10:12", "yo", false),
        ];
        assert!(!typing_continues(&rows, "anna", "10:12"));
    }

    #[test]
    fn an_empty_transcript_or_stamp_gets_a_header() {
        assert!(!typing_continues(&[], "anna", "10:12"));
        let rows = [row_from_message(1, "anna", "", "hi", false)];
        assert!(!typing_continues(&rows, "anna", ""));
        assert!(!typing_continues(&rows, "", ""));
    }
}
