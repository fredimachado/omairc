//! Local calendar day and offset the transcript uses for Today/Yesterday rules.
//! The shell polls this on each tick and refreshes the open conversation when either
//! changes, as Qt did at midnight and on clock-zone changes.

use chrono::{Local, NaiveDate, Offset};

/// The local date and UTC offset that shape day separators and their captions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscriptCalendarSnapshot {
    pub local_date: NaiveDate,
    pub utc_offset_secs: i32,
}

impl TranscriptCalendarSnapshot {
    pub fn capture_now() -> Self {
        let now = Local::now();
        Self {
            local_date: now.date_naive(),
            utc_offset_secs: now.offset().fix().local_minus_utc(),
        }
    }
}

/// Returns true when `now` differs from `cached`, and stores `now` in `cached`.
pub fn advance_transcript_calendar(
    cached: &mut TranscriptCalendarSnapshot,
    now: TranscriptCalendarSnapshot,
) -> bool {
    if *cached == now {
        return false;
    }
    *cached = now;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn snapshot(date: NaiveDate, offset: i32) -> TranscriptCalendarSnapshot {
        TranscriptCalendarSnapshot {
            local_date: date,
            utc_offset_secs: offset,
        }
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn advance_transcript_calendar_ignores_unchanged_snapshot() {
        let mut cached = snapshot(day(2026, 10, 22), 36_000);
        assert!(!advance_transcript_calendar(
            &mut cached,
            snapshot(day(2026, 10, 22), 36_000)
        ));
    }

    #[test]
    fn advance_transcript_calendar_detects_local_date_change() {
        let mut cached = snapshot(day(2026, 10, 22), 36_000);
        assert!(advance_transcript_calendar(
            &mut cached,
            snapshot(day(2026, 10, 23), 36_000)
        ));
        assert_eq!(cached.local_date, day(2026, 10, 23));
    }

    #[test]
    fn advance_transcript_calendar_detects_utc_offset_change() {
        let mut cached = snapshot(day(2026, 10, 22), 36_000);
        assert!(advance_transcript_calendar(
            &mut cached,
            snapshot(day(2026, 10, 22), 39_600)
        ));
        assert_eq!(cached.utc_offset_secs, 39_600);
    }
}
