//! This file holds the timestamp and verb rules every read-marker line shares:
//! millisecond truncation, the `timestamp=` format and parse, the `*` sentinel
//! and which verb a negotiated token selects. The outbound pacing (one marker in
//! flight per target) is in `session/readmarker.rs`, and the reducer's unread
//! recount from a marker is in the reducer. Both call these helpers.
//! Pure: no clock is read here.

use chrono::{DateTime, Utc};

/// The `timestamp=` value layout: millisecond precision, trailing `Z`.
const READ_MARKER_TIME_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.3fZ";

/// The `draft/read-marker` capability token and its verb.
pub const DRAFT_READ_MARKER_TOKEN: &str = "draft/read-marker";

/// The `soju.im/read` capability token and its verb.
pub const SOJU_READ_TOKEN: &str = "soju.im/read";

/// The read-marker verb for a negotiated token (`ReadMarkerCommand`):
/// `MARKREAD` for `draft/read-marker`, `READ` for `soju.im/read`, and empty when
/// neither is enabled. `enabled` is the token that the negotiation enabled, if
/// any; when both were, the draft token wins, as in Go.
pub fn read_marker_command(enabled_token: Option<&str>) -> &'static str {
    match enabled_token {
        Some(DRAFT_READ_MARKER_TOKEN) => "MARKREAD",
        Some(SOJU_READ_TOKEN) => "READ",
        _ => "",
    }
}

/// Truncates a timestamp to whole UTC milliseconds, the precision markers compare
/// at (`ReadMarkerTimeMillis`).
pub fn read_marker_time_millis(when: DateTime<Utc>) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(when.timestamp_millis()).unwrap_or(when)
}

/// Whether `left` is strictly newer than `right` at millisecond precision
/// (`ReadMarkerTimeAfter`). A sub-millisecond tag is not newer than the
/// millisecond marker it truncates to.
pub fn read_marker_time_after(left: DateTime<Utc>, right: DateTime<Utc>) -> bool {
    read_marker_time_millis(left) > read_marker_time_millis(right)
}

/// A UTC timestamp for a `timestamp=` parameter (`FormatReadMarkerTime`).
pub fn format_read_marker_time(when: DateTime<Utc>) -> String {
    read_marker_time_millis(when)
        .format(READ_MARKER_TIME_FORMAT)
        .to_string()
}

/// Parses a `timestamp=` value (`ParseReadMarkerTime`). Fractional seconds may
/// use any precision and are truncated to milliseconds; only a trailing `Z` is
/// accepted.
pub fn parse_read_marker_time(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim();
    if value.is_empty() || !value.ends_with('Z') {
        return None;
    }
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|parsed| read_marker_time_millis(parsed.with_timezone(&Utc)))
}

/// Parses the trailing parameter of a `MARKREAD` / `READ` line
/// (`ParseReadMarkerParameter`). `Some(None)` is `*`, the no-marker sentinel
/// `Some(Some(time))` is a `timestamp=` marker; `None` is anything else.
pub fn parse_read_marker_parameter(parameter: &str) -> Option<Option<DateTime<Utc>>> {
    let parameter = parameter.trim();
    if parameter == "*" {
        return Some(None);
    }
    let value = parameter.strip_prefix("timestamp=")?;
    parse_read_marker_time(value).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeDelta, TimeZone, Timelike};

    fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32, s: u32, nanos: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, s)
            .unwrap()
            .with_nanosecond(nanos)
            .unwrap()
    }

    #[test]
    fn variable_fraction_parses_and_truncates_to_milliseconds() {
        let cases = [
            ("2024-06-01T12:00:00Z", utc(2024, 6, 1, 12, 0, 0, 0)),
            (
                "2024-06-01T12:00:00.1Z",
                utc(2024, 6, 1, 12, 0, 0, 100_000_000),
            ),
            (
                "2024-06-01T12:00:00.12Z",
                utc(2024, 6, 1, 12, 0, 0, 120_000_000),
            ),
            (
                "2024-06-01T12:00:00.123Z",
                utc(2024, 6, 1, 12, 0, 0, 123_000_000),
            ),
            (
                "2024-06-01T12:00:00.123456Z",
                utc(2024, 6, 1, 12, 0, 0, 123_000_000),
            ),
        ];
        for (input, want) in cases {
            assert_eq!(parse_read_marker_time(input), Some(want), "{input}");
        }
    }

    #[test]
    fn a_value_without_a_trailing_z_or_a_time_is_rejected() {
        assert_eq!(parse_read_marker_time(""), None);
        assert_eq!(parse_read_marker_time("2024-06-01T12:00:00+00:00"), None);
        assert_eq!(parse_read_marker_time("yesterday"), None);
    }

    #[test]
    fn after_uses_milliseconds() {
        let full = utc(2024, 6, 1, 12, 0, 0, 123_456_789);
        let trunc = utc(2024, 6, 1, 12, 0, 0, 123_000_000);
        assert!(
            !read_marker_time_after(full, trunc),
            "sub-millisecond tag must not be newer than millisecond marker"
        );
        assert!(read_marker_time_after(
            trunc + TimeDelta::milliseconds(1),
            trunc
        ));
        assert!(!read_marker_time_after(trunc, trunc), "equal is not newer");
    }

    #[test]
    fn format_is_millisecond_utc_and_round_trips() {
        let when = utc(2024, 6, 1, 12, 0, 0, 123_456_789);
        let text = format_read_marker_time(when);
        assert_eq!(text, "2024-06-01T12:00:00.123Z");
        assert_eq!(
            parse_read_marker_time(&text),
            Some(read_marker_time_millis(when))
        );
    }

    #[test]
    fn the_parameter_is_a_marker_a_sentinel_or_not_a_marker() {
        assert_eq!(parse_read_marker_parameter("*"), Some(None));
        assert_eq!(parse_read_marker_parameter(" * "), Some(None));
        assert_eq!(
            parse_read_marker_parameter("timestamp=2024-06-01T12:00:00.5Z"),
            Some(Some(utc(2024, 6, 1, 12, 0, 0, 500_000_000)))
        );
        assert_eq!(
            parse_read_marker_parameter("stamp=2024-06-01T12:00:00Z"),
            None
        );
        assert_eq!(parse_read_marker_parameter("timestamp=garbage"), None);
    }

    #[test]
    fn the_verb_follows_the_enabled_token() {
        assert_eq!(read_marker_command(Some("draft/read-marker")), "MARKREAD");
        assert_eq!(read_marker_command(Some("soju.im/read")), "READ");
        assert_eq!(read_marker_command(None), "");
        assert_eq!(read_marker_command(Some("draft/chathistory")), "");
    }
}
