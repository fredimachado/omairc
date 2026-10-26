//! The `/autoaway` argument parser, the duration parser and formatter, and
//! the query, confirmation and Status strings. Pure: no clock, no session.
//! The controller's auto-away runtime (`controller/autoawayruntime.rs`) holds the
//! state and the timers; this module only reads the argument and words the reply.

/// The shortest accepted auto-away timeout, in seconds.
pub const AUTOAWAY_MIN_TIMEOUT_SECONDS: i64 = 30;

/// The longest accepted auto-away timeout, in seconds (`numeric_limits<int>::max/1000`).
pub const AUTOAWAY_MAX_TIMEOUT_SECONDS: i64 = 2_147_483;

/// The parsed shape of a `/autoaway` argument (`IrcAutoawayKind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AutoawayKind {
    /// Reports the current configuration.
    Query,
    /// Turns auto-away off.
    Disable,
    /// Turns auto-away on with the current timeout.
    EnableOn,
    /// Sets the timeout and an optional one-shot reason.
    SetTimeout,
    /// Sets the persistent away reason.
    SetDefaultReason,
    /// Clears the persistent away reason.
    ClearDefaultReason,
    /// The argument did not parse: print the usage line.
    Usage,
}

/// A parsed `/autoaway` argument (`IrcAutoawayRequest`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoawayRequest {
    pub kind: AutoawayKind,
    pub timeout_seconds: i64,
    pub text: String,
}

impl AutoawayRequest {
    fn of(kind: AutoawayKind) -> Self {
        Self {
            kind,
            timeout_seconds: 0,
            text: String::new(),
        }
    }
}

/// The auto-away state the query and the confirmations render (`IrcAutoawayConfig`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AutoawayConfig {
    pub enabled: bool,
    pub timeout_seconds: i64,
    pub default_reason: String,
    pub one_shot_reason: String,
}

/// Parses a duration token: a bare number or an `m` suffix is minutes, `s` is
/// seconds, and `h` is hours. At most one decimal place. The result must round
/// into the accepted range (`ircParseAutoawayDuration`).
pub fn parse_autoaway_duration(token: &str) -> Option<i64> {
    let bytes = token.as_bytes();
    let count = bytes.len();
    let mut index = 0;
    while index < count && bytes[index].is_ascii_digit() {
        index += 1;
    }
    if index == 0 {
        return None;
    }
    if index < count && bytes[index] == b'.' {
        index += 1;
        let fraction_start = index;
        while index < count && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == fraction_start {
            return None;
        }
    }

    let number = &token[..index];
    let mut suffix = None;
    if index < count {
        // The suffix is exactly one character after the number.
        if index + 1 != count {
            return None;
        }
        let letter = bytes[index].to_ascii_lowercase();
        if !matches!(letter, b's' | b'm' | b'h') {
            return None;
        }
        suffix = Some(letter);
    }

    let value: f64 = number.parse().ok()?;
    if value < 0.0 {
        return None;
    }
    let seconds = match suffix {
        None | Some(b'm') => value * 60.0,
        Some(b'h') => value * 3600.0,
        _ => value,
    };

    if seconds > AUTOAWAY_MAX_TIMEOUT_SECONDS as f64 || seconds < AUTOAWAY_MIN_TIMEOUT_SECONDS as f64
    {
        return None;
    }
    let rounded = seconds.round() as i64;
    if !(0..=AUTOAWAY_MAX_TIMEOUT_SECONDS).contains(&rounded) {
        return None;
    }
    Some(rounded)
}

/// Parses a `/autoaway` argument (`ircParseAutoawayArgument`).
pub fn parse_autoaway_argument(argument: &str) -> AutoawayRequest {
    let trimmed = argument.trim();
    if trimmed.is_empty() {
        return AutoawayRequest::of(AutoawayKind::Query);
    }

    let token = first_space_token(trimmed);
    let rest = rest_after_first_space_token(trimmed);
    match token.to_lowercase().as_str() {
        "off" | "0" => {
            if !rest.is_empty() {
                return AutoawayRequest::of(AutoawayKind::Usage);
            }
            return AutoawayRequest::of(AutoawayKind::Disable);
        }
        "on" => {
            if !rest.is_empty() {
                return AutoawayRequest::of(AutoawayKind::Usage);
            }
            return AutoawayRequest::of(AutoawayKind::EnableOn);
        }
        "reason" => {
            if rest.is_empty() {
                return AutoawayRequest::of(AutoawayKind::ClearDefaultReason);
            }
            return AutoawayRequest {
                kind: AutoawayKind::SetDefaultReason,
                timeout_seconds: 0,
                text: rest,
            };
        }
        _ => {}
    }

    match parse_autoaway_duration(token) {
        Some(seconds) if seconds >= AUTOAWAY_MIN_TIMEOUT_SECONDS => AutoawayRequest {
            kind: AutoawayKind::SetTimeout,
            timeout_seconds: seconds,
            text: rest,
        },
        _ => AutoawayRequest::of(AutoawayKind::Usage),
    }
}

/// Renders a whole number of hours, minutes, or seconds (`ircFormatAutoawayDuration`).
pub fn format_autoaway_duration(seconds: i64) -> String {
    if seconds >= 3600 && seconds % 3600 == 0 {
        let hours = seconds / 3600;
        return if hours == 1 {
            "1 hour".to_owned()
        } else {
            format!("{hours} hours")
        };
    }
    if seconds >= 60 && seconds % 60 == 0 {
        let minutes = seconds / 60;
        return if minutes == 1 {
            "1 minute".to_owned()
        } else {
            format!("{minutes} minutes")
        };
    }
    if seconds == 1 {
        return "1 second".to_owned();
    }
    format!("{seconds} seconds")
}

/// The current configuration, for a query (`ircFormatAutoawayQuery`).
pub fn format_autoaway_query(config: &AutoawayConfig) -> String {
    if !config.enabled {
        return format_disabled(config, true);
    }
    format_enabled(config, true)
}

/// The confirmation after a change (`ircFormatAutoawayConfirmation`).
pub fn format_autoaway_confirmation(config: &AutoawayConfig) -> String {
    if !config.enabled {
        return format_disabled(config, false);
    }
    format_enabled(config, false)
}

/// The Status line for a fired auto-away (`ircFormatAutoawayTrippedStatus`).
pub fn format_autoaway_tripped_status(reason: &str) -> String {
    let trimmed = reason.trim();
    if trimmed.is_empty() {
        return "Auto-away triggered.".to_owned();
    }
    format!("Auto-away triggered: {trimmed}")
}

/// The Status line when auto-away clears (`ircFormatAutoawayClearedStatus`).
pub fn format_autoaway_cleared_status() -> String {
    "Auto-away cleared — back online.".to_owned()
}

/// The grace period before auto-away fires: a twentieth of the timeout, clamped to
/// 5 to 10 seconds, and 0 for a non-positive timeout (`ircAutoawayGraceSeconds`).
pub fn autoaway_grace_seconds(timeout_seconds: i64) -> i64 {
    if timeout_seconds <= 0 {
        return 0;
    }
    (timeout_seconds / 20).clamp(5, 10)
}

/// An enabled configuration. The default reason is left out of a confirmation while
/// a one-shot reason is set, as the C++ confirmation does.
fn format_enabled(config: &AutoawayConfig, include_default_with_one_shot: bool) -> String {
    if !config.one_shot_reason.is_empty() {
        let mut text = format!(
            "Auto-away in {}: {} (this time only)",
            format_autoaway_duration(config.timeout_seconds),
            config.one_shot_reason
        );
        if include_default_with_one_shot && !config.default_reason.is_empty() {
            text.push_str(&format!(", reason: {}", config.default_reason));
        }
        return text;
    }
    let mut text = format!(
        "Auto-away {}",
        format_autoaway_duration(config.timeout_seconds)
    );
    if !config.default_reason.is_empty() {
        text.push_str(&format!(", reason: {}", config.default_reason));
    }
    text
}

/// A disabled configuration. `include_timeout` echoes the current timeout back.
fn format_disabled(config: &AutoawayConfig, include_timeout: bool) -> String {
    let mut text = "Auto-away off".to_owned();
    if include_timeout && config.timeout_seconds > 0 {
        text.push_str(&format!(
            ", {}",
            format_autoaway_duration(config.timeout_seconds)
        ));
    }
    if !config.one_shot_reason.is_empty() {
        text.push_str(&format!(": {} (this time only)", config.one_shot_reason));
    }
    if !config.default_reason.is_empty() {
        text.push_str(&format!(", reason: {}", config.default_reason));
    }
    text
}

/// The text before the first ASCII space, or the whole string when there is none.
fn first_space_token(argument: &str) -> &str {
    argument.split_once(' ').map_or(argument, |(token, _)| token)
}

/// The trimmed text after the first ASCII space, or "" when there is none.
fn rest_after_first_space_token(argument: &str) -> String {
    argument
        .split_once(' ')
        .map_or_else(String::new, |(_, rest)| rest.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_accepts_the_units_and_rejects_the_rest() {
        let cases: &[(&str, Option<i64>)] = &[
            ("", None),
            ("abc", None),
            (".5", None),
            ("1.", None),
            ("1.5.5", None),
            ("1h30", None),
            ("1 s", None),
            ("0", None),
            ("29", Some(1740)),
            ("29s", None),
            ("30", Some(1800)),
            ("30s", Some(30)),
            ("30S", Some(30)),
            ("1m", Some(60)),
            ("1.5m", Some(90)),
            ("0.5m", Some(30)),
            ("1h", Some(3600)),
            ("1.5h", Some(5400)),
            ("35791", Some(2_147_460)),
            ("35792", None),
            ("2147483", None),
        ];
        for (token, want) in cases {
            assert_eq!(parse_autoaway_duration(token), *want, "token {token:?}");
        }
    }

    #[test]
    fn parse_argument_maps_each_shape() {
        let req = |kind| AutoawayRequest {
            kind,
            timeout_seconds: 0,
            text: String::new(),
        };
        let timed = |seconds: i64, text: &str| AutoawayRequest {
            kind: AutoawayKind::SetTimeout,
            timeout_seconds: seconds,
            text: text.to_owned(),
        };
        let reason = |text: &str| AutoawayRequest {
            kind: AutoawayKind::SetDefaultReason,
            timeout_seconds: 0,
            text: text.to_owned(),
        };
        let cases: Vec<(&str, AutoawayRequest)> = vec![
            ("", req(AutoawayKind::Query)),
            ("   ", req(AutoawayKind::Query)),
            ("off", req(AutoawayKind::Disable)),
            ("OFF", req(AutoawayKind::Disable)),
            ("0", req(AutoawayKind::Disable)),
            ("off now", req(AutoawayKind::Usage)),
            ("on", req(AutoawayKind::EnableOn)),
            ("on now", req(AutoawayKind::Usage)),
            ("reason", req(AutoawayKind::ClearDefaultReason)),
            ("reason busy", reason("busy")),
            ("reason  busy now", reason("busy now")),
            ("30s", timed(30, "")),
            ("5", timed(300, "")),
            ("1h", timed(3600, "")),
            ("30s lunch", timed(30, "lunch")),
            ("30s  meeting now", timed(30, "meeting now")),
            ("abc", req(AutoawayKind::Usage)),
            ("29s", req(AutoawayKind::Usage)),
        ];
        for (argument, want) in cases {
            assert_eq!(parse_autoaway_argument(argument), want, "argument {argument:?}");
        }
    }

    #[test]
    fn format_duration_picks_the_largest_whole_unit() {
        let cases = [
            (1, "1 second"),
            (30, "30 seconds"),
            (60, "1 minute"),
            (1800, "30 minutes"),
            (3600, "1 hour"),
            (7200, "2 hours"),
            (5400, "90 minutes"),
        ];
        for (seconds, want) in cases {
            assert_eq!(format_autoaway_duration(seconds), want);
        }
    }

    #[test]
    fn query_and_confirmation_shapes() {
        let off = AutoawayConfig {
            timeout_seconds: 1800,
            ..Default::default()
        };
        assert_eq!(format_autoaway_query(&off), "Auto-away off, 30 minutes");
        assert_eq!(format_autoaway_confirmation(&off), "Auto-away off");

        let on = AutoawayConfig {
            enabled: true,
            timeout_seconds: 1800,
            default_reason: "lunch".to_owned(),
            ..Default::default()
        };
        assert_eq!(
            format_autoaway_query(&on),
            "Auto-away 30 minutes, reason: lunch"
        );
        assert_eq!(
            format_autoaway_confirmation(&on),
            "Auto-away 30 minutes, reason: lunch"
        );

        let one_shot = AutoawayConfig {
            enabled: true,
            timeout_seconds: 1800,
            default_reason: "lunch".to_owned(),
            one_shot_reason: "brb".to_owned(),
        };
        assert_eq!(
            format_autoaway_query(&one_shot),
            "Auto-away in 30 minutes: brb (this time only), reason: lunch"
        );
        assert_eq!(
            format_autoaway_confirmation(&one_shot),
            "Auto-away in 30 minutes: brb (this time only)"
        );

        let disabled_one_shot = AutoawayConfig {
            one_shot_reason: "brb".to_owned(),
            default_reason: "lunch".to_owned(),
            ..Default::default()
        };
        assert_eq!(
            format_autoaway_confirmation(&disabled_one_shot),
            "Auto-away off: brb (this time only), reason: lunch"
        );
    }

    #[test]
    fn status_strings() {
        assert_eq!(format_autoaway_tripped_status(""), "Auto-away triggered.");
        assert_eq!(
            format_autoaway_tripped_status("  brb  "),
            "Auto-away triggered: brb"
        );
        assert_eq!(
            format_autoaway_cleared_status(),
            "Auto-away cleared — back online."
        );
    }

    #[test]
    fn grace_is_a_twentieth_clamped_to_five_and_ten() {
        let cases = [(0, 0), (-30, 0), (30, 5), (100, 5), (150, 7), (200, 10), (600, 10)];
        for (timeout, want) in cases {
            assert_eq!(autoaway_grace_seconds(timeout), want, "timeout {timeout}");
        }
    }
}
