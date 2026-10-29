//! Ban and quiet list numerics (`367`/`368`, `728`/`729`). Status formats them
//! and the reply router copies an armed inquiry into the asking channel as
//! whois-style rows.

use super::incoming::{command_of, parameter_at, parameters_text};
use super::WhoisProgress;
use crate::irc::listmask::ListMaskKind;
use crate::irc::message::IrcMessage;

/// The list-mode detail a server Status entry carries (`ListModeLine`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListModeLine {
    pub channel: String,
    pub text: String,
    pub kind: ListMaskKind,
    pub progress: WhoisProgress,
}

impl ListModeLine {
    /// Whether the line ends the list exchange.
    pub fn is_terminal(&self) -> bool {
        self.progress != WhoisProgress::Detail
    }
}

/// A formatted list-mode numeric (`statusFormattedListMode`).
pub(super) struct FormattedListMode {
    pub(super) label: &'static str,
    pub(super) line: ListModeLine,
}

/// Formats a ban or quiet list numeric. `None` when the numeric is not one of
/// `367`/`368`/`728`/`729`, or its parameters are too few.
pub(super) fn format_list_mode(message: &IrcMessage) -> Option<FormattedListMode> {
    let command = command_of(message);
    if command.len() != 3 {
        return None;
    }
    let code: u32 = command.parse().ok()?;
    let parameters = parameters_text(message);
    let at = |index: usize| parameter_at(&parameters, index).to_owned();

    let (label, kind, progress, text, channel) = match code {
        367 => {
            if parameters.len() < 3 {
                return None;
            }
            let channel = at(1);
            let mask = at(2);
            let mut text = format!("ban {mask} on {channel}");
            if parameters.len() >= 4 && !at(3).is_empty() {
                text.push_str(&format!(" by {}", at(3)));
            }
            ("banlist", ListMaskKind::Ban, WhoisProgress::Detail, text, channel)
        }
        368 => {
            if parameters.len() < 2 {
                return None;
            }
            let channel = at(1);
            (
                "banlist",
                ListMaskKind::Ban,
                WhoisProgress::Terminal,
                format!("End of ban list for {channel}"),
                channel,
            )
        }
        728 => {
            if parameters.len() < 4 {
                return None;
            }
            let channel = at(1);
            let mask = at(3);
            let mut text = format!("quiet {mask} on {channel}");
            if parameters.len() >= 5 && !at(4).is_empty() {
                text.push_str(&format!(" by {}", at(4)));
            }
            ("quietlist", ListMaskKind::Quiet, WhoisProgress::Detail, text, channel)
        }
        729 => {
            if parameters.len() < 2 {
                return None;
            }
            let channel = at(1);
            (
                "quietlist",
                ListMaskKind::Quiet,
                WhoisProgress::Terminal,
                format!("End of quiet list for {channel}"),
                channel,
            )
        }
        _ => return None,
    };

    if channel.is_empty() || text.is_empty() {
        return None;
    }
    Some(FormattedListMode {
        label,
        line: ListModeLine {
            channel,
            text,
            kind,
            progress,
        },
    })
}

/// Whether Status keeps this list-mode numeric.
pub(super) fn keeps_list_mode_numeric(code: u32) -> bool {
    matches!(code, 367 | 368 | 728 | 729)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::irc::parser::parse;

    fn msg(line: &str) -> IrcMessage {
        parse(line.as_bytes()).expect("parse")
    }

    #[test]
    fn formats_ban_and_quiet_list_numerics() {
        let ban = format_list_mode(&msg(":irc 367 me #room *!*@evil.com alice 1")).expect("367");
        assert_eq!(ban.label, "banlist");
        assert_eq!(ban.line.kind, ListMaskKind::Ban);
        assert_eq!(ban.line.progress, WhoisProgress::Detail);
        assert_eq!(ban.line.channel, "#room");
        assert_eq!(ban.line.text, "ban *!*@evil.com on #room by alice");

        let end = format_list_mode(&msg(":irc 368 me #room :End of Channel Ban List")).expect("368");
        assert!(end.line.is_terminal());
        assert_eq!(end.line.text, "End of ban list for #room");

        let quiet = format_list_mode(&msg(":irc 728 me #room q *!*@evil.com bob 2")).expect("728");
        assert_eq!(quiet.label, "quietlist");
        assert_eq!(quiet.line.kind, ListMaskKind::Quiet);
        assert_eq!(quiet.line.text, "quiet *!*@evil.com on #room by bob");

        let quiet_end = format_list_mode(&msg(":irc 729 me #room q :End of Channel Quiet List")).expect("729");
        assert!(quiet_end.line.is_terminal());
        assert_eq!(quiet_end.line.text, "End of quiet list for #room");
    }
}
