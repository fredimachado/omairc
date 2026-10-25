//! WHOIS replies. One table row per known numeric gives its layout, and the formatter
//! turns the reply into a labelled line or falls back to the plain text.

use super::incoming::{command_of, parameter_at, parameters_text};
use super::WhoisProgress;
use crate::irc::message::IrcMessage;

/// How a WHOIS numeric lays out its parameters (`statusWhoisLayout`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum WhoisLayout {
    User,
    Channels,
    Server,
    Away,
    Idle,
    End,
    NickRest,
    Account,
    NoSuchNick,
    NoSuchServer,
}

/// One WHOIS numeric the Status pane formats (`statusWhoisSpec`). An empty label
/// means the command itself.
pub(super) struct WhoisSpec {
    code: u32,
    label: &'static str,
    layout: WhoisLayout,
}

/// The WHOIS numerics Status formats (`statusWhoisNumerics`).
static WHOIS_NUMERICS: [WhoisSpec; 17] = [
    WhoisSpec {
        code: 311,
        label: "whois",
        layout: WhoisLayout::User,
    },
    WhoisSpec {
        code: 319,
        label: "whois",
        layout: WhoisLayout::Channels,
    },
    WhoisSpec {
        code: 312,
        label: "whois",
        layout: WhoisLayout::Server,
    },
    WhoisSpec {
        code: 301,
        label: "whois",
        layout: WhoisLayout::Away,
    },
    WhoisSpec {
        code: 317,
        label: "whois",
        layout: WhoisLayout::Idle,
    },
    WhoisSpec {
        code: 318,
        label: "whois",
        layout: WhoisLayout::End,
    },
    WhoisSpec {
        code: 276,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 307,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 313,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 320,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 330,
        label: "whois",
        layout: WhoisLayout::Account,
    },
    WhoisSpec {
        code: 338,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 378,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 379,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 671,
        label: "whois",
        layout: WhoisLayout::NickRest,
    },
    WhoisSpec {
        code: 401,
        label: "",
        layout: WhoisLayout::NoSuchNick,
    },
    WhoisSpec {
        code: 402,
        label: "",
        layout: WhoisLayout::NoSuchServer,
    },
];

/// The WHOIS spec for a numeric, or `None` (`statusWhoisSpecFor`).
pub(super) fn whois_spec(code: u32) -> Option<&'static WhoisSpec> {
    WHOIS_NUMERICS.iter().find(|spec| spec.code == code)
}

/// A WHOIS line, formatted (`statusFormattedWhois`).
pub(super) struct FormattedWhois {
    pub(super) label: String,
    pub(super) text: String,
    pub(super) nick: String,
    pub(super) progress: WhoisProgress,
}

/// Formats a WHOIS numeric (`statusFormatWhois`). `None` when the numeric is not
/// one Status formats, or its parameters are too few for its layout.
pub(super) fn format_whois(message: &IrcMessage) -> Option<FormattedWhois> {
    let command = command_of(message);
    if command.len() != 3 {
        return None;
    }
    let code: u32 = command.parse().ok()?;
    let spec = whois_spec(code)?;
    let parameters = parameters_text(message);
    let at = |index: usize| parameter_at(&parameters, index).to_owned();
    let short = |needed: usize| parameters.len() < needed;

    let text = match spec.layout {
        WhoisLayout::User => {
            if short(6) {
                return None;
            }
            format!("{} is {}@{} ({})", at(1), at(2), at(3), at(5))
        }
        WhoisLayout::Channels => {
            if short(3) {
                return None;
            }
            format!("{} is on {}", at(1), at(2))
        }
        WhoisLayout::Server => {
            if short(4) {
                return None;
            }
            format!("{} using {} ({})", at(1), at(2), at(3))
        }
        WhoisLayout::Away => {
            if short(3) {
                return None;
            }
            format!("{} is away: {}", at(1), at(2))
        }
        WhoisLayout::Idle => {
            if short(3) {
                return None;
            }
            let mut text = format!("{} idle {}s", at(1), at(2));
            if parameters.len() >= 5 && !at(3).is_empty() {
                text.push_str(&format!(", signon {}", at(3)));
            }
            text
        }
        WhoisLayout::End => {
            if short(2) {
                return None;
            }
            format!("End of WHOIS for {}", at(1))
        }
        WhoisLayout::NickRest => {
            if short(3) {
                return None;
            }
            format!("{} {}", at(1), parameters[2..].join(" "))
        }
        WhoisLayout::Account => {
            if short(3) {
                return None;
            }
            format!("{} is logged in as {}", at(1), at(2))
        }
        WhoisLayout::NoSuchNick => {
            if short(2) {
                return None;
            }
            format!("No such nick: {}", at(1))
        }
        WhoisLayout::NoSuchServer => {
            if short(2) {
                return None;
            }
            format!("No such server: {}", at(1))
        }
    };

    let progress = match code {
        318 => WhoisProgress::Terminal,
        401 | 402 => WhoisProgress::Failed,
        _ => WhoisProgress::Detail,
    };
    let label = if spec.label.is_empty() {
        command
    } else {
        spec.label.to_owned()
    };
    Some(FormattedWhois {
        label,
        text,
        nick: at(1),
        progress,
    })
}
