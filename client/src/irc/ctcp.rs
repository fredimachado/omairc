//! The VERSION reply is the desktop reply: the project URL alone, no build number
//! as in `ctcpVersionReplyText` in the C++ core. The Go TUI names itself in its
//! reply; the GPUI app does not.

/// The `\x01` that brackets a CTCP payload.
const DELIMITER: char = '\u{1}';

/// Milliseconds in a day: PING lag past this is not reported as a lag.
const DAY_MILLIS: i64 = 24 * 60 * 60 * 1000;

/// A parsed CTCP payload. `command` is upper-cased; `argument` keeps its spaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtcpRequest {
    pub command: String,
    pub argument: String,
}

/// Parses a body wrapped in `\x01` delimiters. `None` when it is not delimited or
/// carries no command.
pub fn parse_ctcp_request(body: &str) -> Option<CtcpRequest> {
    let payload = body.strip_prefix(DELIMITER)?.strip_suffix(DELIMITER)?;
    let (raw_command, argument) = match payload.split_once(' ') {
        Some((command, argument)) => (command, argument),
        None => (payload, ""),
    };
    let command = raw_command.trim().to_uppercase();
    if command.is_empty() {
        return None;
    }
    Some(CtcpRequest {
        command,
        argument: argument.to_string(),
    })
}

/// Wraps a request back into its `\x01`-delimited payload.
pub fn ctcp_payload(request: &CtcpRequest) -> String {
    let body = if request.argument.is_empty() {
        request.command.clone()
    } else {
        format!("{} {}", request.command, request.argument)
    };
    format!("{DELIMITER}{body}{DELIMITER}")
}

/// The argument of our CTCP VERSION reply on the desktop.
pub fn ctcp_version_reply_text() -> &'static str {
    "https://omairc.app"
}

/// The local Status line for one CTCP reply. `now_unix_millis` is the clock
/// reading; a PING whose argument is a millisecond timestamp reports its lag when
/// the lag is within a day.
pub fn format_ctcp_reply_text(
    command: &str,
    nick: &str,
    argument: &str,
    now_unix_millis: i64,
) -> String {
    if command == "PING" {
        if let Ok(sent_millis) = argument.parse::<i64>() {
            // checked_sub: an extreme timestamp must not wrap into the window.
            if let Some(lag) = now_unix_millis.checked_sub(sent_millis) {
                if (0..DAY_MILLIS).contains(&lag) {
                    return format!("PING reply from {nick}: {lag} ms");
                }
            }
        }
        if argument.is_empty() {
            return format!("PING reply from {nick}");
        }
        return format!("PING reply from {nick}: {argument}");
    }
    if argument.is_empty() {
        return format!("{command} reply from {nick}");
    }
    format!("{command} reply from {nick}: {argument}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // tui/internal/irc/ctcp_test.go.

    /// Mirrors TestCtcpVersionReplyNamesTheTui. The Go reply names the TUI; the
    /// desktop reply is the URL alone (`ctcpVersionReplyText` in `tui/internal/irc/ctcp.go`).
    #[test]
    fn version_reply_is_the_project_url_on_the_desktop() {
        assert_eq!(ctcp_version_reply_text(), "https://omairc.app");
    }

    #[test]
    fn parses_and_formats_ctcp_requests() {
        let version = parse_ctcp_request("\u{1}VERSION\u{1}").expect("VERSION parses");
        assert_eq!(version.command, "VERSION");
        assert_eq!(version.argument, "");
        assert_eq!(ctcp_payload(&version), "\u{1}VERSION\u{1}");

        let ping = parse_ctcp_request("\u{1}ping 42\u{1}").expect("ping parses");
        assert_eq!(ping.command, "PING");
        assert_eq!(ping.argument, "42");
        assert_eq!(ctcp_payload(&ping), "\u{1}PING 42\u{1}");
    }

    #[test]
    fn undelimited_and_empty_payloads_do_not_parse() {
        assert!(parse_ctcp_request("VERSION").is_none());
        assert!(parse_ctcp_request("\u{1}\u{1}").is_none());
        assert!(parse_ctcp_request("\u{1}").is_none());
    }

    #[test]
    fn a_ctcp_argument_keeps_its_inner_spaces() {
        let time = parse_ctcp_request("\u{1}time  Tue, 15 Sep\u{1}").expect("TIME parses");
        assert_eq!(time.argument, " Tue, 15 Sep");
    }

    #[test]
    fn reply_lines_report_ping_lag_against_the_clock() {
        let now = 1_700_000_000_042;
        let cases = [
            (
                "PING",
                "lena",
                "1700000000000",
                "PING reply from lena: 42 ms",
            ),
            ("PING", "lena", "", "PING reply from lena"),
            (
                "TIME",
                "lena",
                "Tue, 15 Sep 2026 12:00:00 +0000",
                "TIME reply from lena: Tue, 15 Sep 2026 12:00:00 +0000",
            ),
            (
                "VERSION",
                "lena",
                "Omairc 0.4.0",
                "VERSION reply from lena: Omairc 0.4.0",
            ),
        ];
        for (command, nick, argument, want) in cases {
            assert_eq!(
                format_ctcp_reply_text(command, nick, argument, now),
                want,
                "{command} {argument:?}"
            );
        }
    }

    #[test]
    fn a_ping_outside_the_day_window_shows_its_argument_not_a_lag() {
        // Future timestamp: lag is negative.
        assert_eq!(
            format_ctcp_reply_text("PING", "lena", "2000", 1000),
            "PING reply from lena: 2000"
        );
        // Exactly a day old is out of the window.
        assert_eq!(
            format_ctcp_reply_text("PING", "lena", "0", DAY_MILLIS),
            "PING reply from lena: 0"
        );
        // A timestamp whose difference would overflow is not a lag.
        assert_eq!(
            format_ctcp_reply_text("PING", "lena", &i64::MIN.to_string(), 0),
            format!("PING reply from lena: {}", i64::MIN)
        );
    }
}
