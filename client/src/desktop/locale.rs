//! The short date a day separator shows. Windows reads the
//! user's regional format. Other systems read the POSIX locale environment.

use chrono::NaiveDate;

/// The pattern for a locale the table does not name.
const FALLBACK: &str = "%d/%m/%Y";

/// The short date of `day` in the user's locale, such as `10/20/2026`.
#[cfg(windows)]
pub fn short_date(day: NaiveDate) -> String {
    windows_short_date(day).unwrap_or_else(|| day.format(FALLBACK).to_string())
}

/// The short date of `day` in the user's locale, read from `LC_ALL`, `LC_TIME` or `LANG`.
#[cfg(not(windows))]
pub fn short_date(day: NaiveDate) -> String {
    let tag = posix_locale_tag().unwrap_or_default();
    day.format(pattern_for(&tag)).to_string()
}

/// `GetDateFormatEx` with the user's default locale and the short-date style. `None` when
/// the call fails, so the caller uses the fixed pattern.
#[cfg(windows)]
fn windows_short_date(day: NaiveDate) -> Option<String> {
    use chrono::Datelike;
    use std::ptr::null;
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::Globalization::{GetDateFormatEx, DATE_SHORTDATE};

    let when = SYSTEMTIME {
        wYear: u16::try_from(day.year()).ok()?,
        wMonth: u16::try_from(day.month()).ok()?,
        wDayOfWeek: 0,
        wDay: u16::try_from(day.day()).ok()?,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    let mut buffer = [0u16; 64];
    // SAFETY: `when` outlives the call. The null locale is LOCALE_NAME_USER_DEFAULT, and the
    // null format and calendar take the defaults. The buffer length is passed as its capacity.
    let written = unsafe {
        GetDateFormatEx(
            null(),
            DATE_SHORTDATE,
            &when,
            null(),
            buffer.as_mut_ptr(),
            buffer.len() as i32,
            null(),
        )
    };
    // The count includes the terminating NUL, so zero or one means nothing was written.
    let units = usize::try_from(written).ok().filter(|count| *count > 1)?;
    Some(String::from_utf16_lossy(&buffer[..units - 1]))
}

/// The locale tag the environment names: `LC_ALL`, then `LC_TIME`, then `LANG`, the first
/// that is set and not empty, as POSIX ranks them.
#[cfg(not(windows))]
fn posix_locale_tag() -> Option<String> {
    ["LC_ALL", "LC_TIME", "LANG"]
        .into_iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
}

/// The short-date pattern for a POSIX locale tag such as `en_US.UTF-8`. Only the language
/// and territory decide it. The encoding and any `@modifier` are dropped.
#[cfg(any(not(windows), test))]
fn pattern_for(tag: &str) -> &'static str {
    let base = tag.split(['.', '@']).next().unwrap_or_default();
    let mut parts = base.split(['_', '-']);
    let language = parts.next().unwrap_or_default().to_ascii_lowercase();
    let territory = parts.next().unwrap_or_default().to_ascii_uppercase();
    match (language.as_str(), territory.as_str()) {
        ("en", "US") => "%-m/%-d/%y",
        // ISO 8601 short dates: Swedish, Lithuanian, and Canada's English and French.
        ("sv", _) | ("lt", _) | ("en", "CA") | ("fr", "CA") => "%Y-%m-%d",
        ("de", _) | ("pl", _) | ("ru", _) | ("uk", _) | ("tr", _) => "%d.%m.%y",
        _ => FALLBACK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn october(day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, day).unwrap()
    }

    #[test]
    fn pattern_for_names_the_regional_orders() {
        assert_eq!(pattern_for("en_US.UTF-8"), "%-m/%-d/%y");
        assert_eq!(pattern_for("de_DE.UTF-8"), "%d.%m.%y");
        assert_eq!(pattern_for("ru_RU"), "%d.%m.%y");
        assert_eq!(pattern_for("pl_PL.UTF-8@euro"), "%d.%m.%y");
        assert_eq!(pattern_for("sv_SE.UTF-8"), "%Y-%m-%d");
        assert_eq!(pattern_for("lt_LT.UTF-8"), "%Y-%m-%d");
        assert_eq!(pattern_for("en_CA.UTF-8"), "%Y-%m-%d");
    }

    #[test]
    fn pattern_for_falls_back_for_every_other_locale() {
        assert_eq!(pattern_for("en_GB.UTF-8"), FALLBACK);
        assert_eq!(pattern_for("fr_FR.UTF-8"), FALLBACK);
        assert_eq!(pattern_for("C.UTF-8"), FALLBACK);
        assert_eq!(pattern_for(""), FALLBACK);
    }

    #[test]
    fn a_pattern_renders_the_day_without_padding_for_en_us() {
        assert_eq!(
            october(5).format(pattern_for("en_US")).to_string(),
            "10/5/26"
        );
        assert_eq!(
            october(20).format(pattern_for("de_DE")).to_string(),
            "20.10.26"
        );
        assert_eq!(
            october(20).format(pattern_for("sv_SE")).to_string(),
            "2026-10-20"
        );
        assert_eq!(
            october(20).format(pattern_for("fr_FR")).to_string(),
            "20/10/2026"
        );
    }

    #[test]
    fn short_date_shows_the_year() {
        // The exact text depends on the machine's regional format, but the year always shows.
        let text = short_date(october(20));
        assert!(text.contains("26"), "{text}");
    }
}
