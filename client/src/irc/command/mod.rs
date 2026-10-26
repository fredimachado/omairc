//! `tui/internal/irc/command.go`:
//! the slash catalog, the composer surface and verb scope, the parser into typed
//! `Command` values, the refusal texts, `/help`, and the pure replies of the verbs
//! that need no server state (`/pref`, the CTCP target rule).
//! A verb's argument is split the way its usage line splits it. A verb that names one
//! word first (`/query <nick> [text]`, `/kick [channel] <nick> [reason]`) carries a
//! `Words`: the first word and the trimmed text after it, as Go's `firstToken` and
//! `restAfterFirstToken`. A verb whose argument is one free text (`/away [reason]`
//! `/topic [text]`) carries a `String`. Which word is a channel, how a join list
//! splits against CHANTYPES, and what a MODE request means all need `ServerFeatures`
//! so the dispatcher reads those, not this parser.
//! Pure: no socket, file, thread or gpui. `tests/irc_portability.rs` enforces that.
mod parse;
mod verbs;

pub use parse::parse;
pub use verbs::{lookup, spec_for, visible_on, Verb, VerbSpec, VERB_TABLE};

#[cfg(test)]
use crate::irc::ctcp::{ctcp_version_reply_text, format_ctcp_reply_text};
use crate::irc::pref::{
    format_pref_list, format_pref_noise, format_pref_query, format_pref_state, PrefKind, PrefName,
    PrefRequest, PrefValue, Preferences,
};

/// Which composer a line was typed into (Go `ComposerSurface`, C++
/// `IrcComposerSurface`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComposerSurface {
    /// The channel or direct-message composer.
    Conversation,
    /// The Status console composer.
    Status,
}

/// The refusal text for a wrong scope with no verb-specific text, and for a plain
/// line typed on Status (`ircCommandOutcomeText`, WrongScope).
pub const NO_CONVERSATION_TEXT: &str = "Select a connected conversation first";

/// The refusal text for a command that was understood but cannot run.
pub const COMMAND_REFUSED_TEXT: &str = "Command was refused";

/// The text for a slash word outside the catalog that has no word to name.
const NOT_SUPPORTED_TEXT: &str = "That command is not supported";

/// A verb's argument split at its first space: the first word, and the trimmed text
/// after it (Go `firstToken` and `restAfterFirstToken`). Only a space separates
/// words, as in the C++ core, so a tab stays inside the word.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Words {
    pub first: String,
    pub rest: String,
}

impl Words {
    /// Splits `argument` at its first space.
    pub fn split(argument: &str) -> Self {
        match argument.find(' ') {
            Some(space) => Self {
                first: argument[..space].to_string(),
                rest: argument[space + 1..].trim().to_string(),
            },
            None => Self {
                first: argument.to_string(),
                rest: String::new(),
            },
        }
    }
}

/// One parsed composer line (Go `Command`, C++ `IrcCommand`), with the verb folded
/// into the variant and its argument parsed to the shape the verb's usage implies.
/// A variant that is not `Words` keeps the whole trimmed argument. Where the
/// dispatcher must refuse a stray argument (`/ignored`, `/muted`, `/highlights`
/// `/monitored`), the variant still holds that argument, so the refusal is the
/// dispatcher's decision and not the parser's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Blank input. There is nothing to send and nothing to report.
    Empty,
    /// Plain text for the selected conversation. `//x` arrives here as `/x`.
    Say(String),
    /// A slash word outside the catalog, keeping its slash: `/nope`.
    Unknown(String),
    /// `/me <text>`. The text is empty when none was typed.
    Action(String),
    /// `/join <targets> [key]`. The raw comma-separated list, read by `jointarget`
    /// against CHANTYPES. Empty joins the latest invite.
    Join(String),
    /// `/part [channel]` (`/leave`). The first word; empty means the selected channel.
    Part(Words),
    /// `/nick <nickname>`. The whole argument.
    Nick(String),
    /// `/disconnect [reason]` (`/quit`). The whole argument.
    Disconnect(String),
    /// `/clear`. Any argument is ignored.
    Clear,
    /// `/close`. Any argument is ignored.
    Close,
    Query(Words),
    Msg(Words),
    /// `/topic [text]`. The whole argument.
    Topic(String),
    Notice(Words),
    /// `/away [reason]`. The whole argument.
    Away(String),
 /// `/back`. Any argument is dropped, as in the C++ core.
    Back,
    /// `/autoaway [...]`. The raw argument; the autoaway parser reads it.
    Autoaway(String),
    /// `/pref [...]`, already parsed.
    Pref(PrefRequest),
    /// `/status [text]`. The whole argument.
    Status(String),
    /// `/avatar [url|email|clear]`. The whole argument.
    Avatar(String),
    Whois(Words),
    Ping(Words),
    Time(Words),
    Version(Words),
    /// `/mode <channel> [...]`. The raw argument, read against ServerFeatures.
    Mode(String),
    /// `/kick [channel] <nick> [reason]`. The first word may name the channel.
    Kick(Words),
    Invite(Words),
    Ignore(Words),
    Unignore(Words),
    /// `/ignored`. A non-empty argument is refused by the dispatcher.
    Ignored(String),
    Monitor(Words),
    Unmonitor(Words),
    /// `/monitored`. A non-empty argument is refused by the dispatcher.
    Monitored(String),
    Mute(Words),
    Unmute(Words),
    /// `/muted`. A non-empty argument is refused by the dispatcher.
    Muted(String),
    Highlight(Words),
    Unhighlight(Words),
    /// `/highlights`. A non-empty argument is refused by the dispatcher.
    Highlights(String),
    /// `/op <nick>`. The first word; the dispatcher turns it into a MODE change.
    Op(Words),
    Deop(Words),
    Voice(Words),
    Devoice(Words),
    Ban(Words),
    /// `/ns <text>`: a PRIVMSG to NickServ, with the text as its body.
    Ns(String),
    /// `/cs <text>`: a PRIVMSG to ChanServ.
    Cs(String),
    /// `/znc <text>`: a PRIVMSG to the ZNC `*status` service.
    Znc(String),
    /// `/raw <line>` (`/quote`). The whole argument, sent as one line.
    Raw(String),
    /// `/help`. Any argument is ignored.
    Help,
    /// `/list [mask]`. The whole argument.
    List(String),
}

impl Command {
    /// The verb this command runs. Blank input, plain text and unknown words have none.
    pub fn verb(&self) -> Option<Verb> {
        Some(match self {
            Command::Empty | Command::Say(_) | Command::Unknown(_) => return None,
            Command::Action(_) => Verb::Action,
            Command::Join(_) => Verb::Join,
            Command::Part(_) => Verb::Part,
            Command::Nick(_) => Verb::Nick,
            Command::Disconnect(_) => Verb::Disconnect,
            Command::Clear => Verb::Clear,
            Command::Close => Verb::Close,
            Command::Query(_) => Verb::Query,
            Command::Msg(_) => Verb::Msg,
            Command::Topic(_) => Verb::Topic,
            Command::Notice(_) => Verb::Notice,
            Command::Away(_) => Verb::Away,
            Command::Back => Verb::Back,
            Command::Autoaway(_) => Verb::Autoaway,
            Command::Pref(_) => Verb::Pref,
            Command::Status(_) => Verb::Status,
            Command::Avatar(_) => Verb::Avatar,
            Command::Whois(_) => Verb::Whois,
            Command::Ping(_) => Verb::Ping,
            Command::Time(_) => Verb::Time,
            Command::Version(_) => Verb::Version,
            Command::Mode(_) => Verb::Mode,
            Command::Kick(_) => Verb::Kick,
            Command::Invite(_) => Verb::Invite,
            Command::Ignore(_) => Verb::Ignore,
            Command::Unignore(_) => Verb::Unignore,
            Command::Ignored(_) => Verb::Ignored,
            Command::Monitor(_) => Verb::Monitor,
            Command::Unmonitor(_) => Verb::Unmonitor,
            Command::Monitored(_) => Verb::Monitored,
            Command::Mute(_) => Verb::Mute,
            Command::Unmute(_) => Verb::Unmute,
            Command::Muted(_) => Verb::Muted,
            Command::Highlight(_) => Verb::Highlight,
            Command::Unhighlight(_) => Verb::Unhighlight,
            Command::Highlights(_) => Verb::Highlights,
            Command::Op(_) => Verb::Op,
            Command::Deop(_) => Verb::Deop,
            Command::Voice(_) => Verb::Voice,
            Command::Devoice(_) => Verb::Devoice,
            Command::Ban(_) => Verb::Ban,
            Command::Ns(_) => Verb::Ns,
            Command::Cs(_) => Verb::Cs,
            Command::Znc(_) => Verb::Znc,
            Command::Raw(_) => Verb::Raw,
            Command::Help => Verb::Help,
            Command::List(_) => Verb::List,
        })
    }

    /// Whether the command sends channel or direct text: plain text or `/me`
    /// (`IrcCommand::isLiveMessage`).
    pub fn is_live_message(&self) -> bool {
        matches!(self, Command::Say(_) | Command::Action(_))
    }

    /// Whether the command may run on `surface` (`IrcCommand::allowedOn`). Plain
    /// text is conversation-only. Blank input and unknown words pass here, and
    /// `rejection` refuses an unknown word.
    pub fn allowed_on(&self, surface: ComposerSurface) -> bool {
        match self.verb() {
            Some(verb) => spec_for(verb).allowed_on(surface),
            None => match self {
                Command::Say(_) => surface == ComposerSurface::Conversation,
                _ => true,
            },
        }
    }

    /// Why the command cannot run on `surface`, as the outcome the dispatcher would
    /// return before it runs anything. `None` means it may run. Blank input never
    /// fails. Plain text is wrong scope on Status. An empty `/me` is refused, as
    /// `IrcSession::sendAction` refuses an empty body.
    #[cfg(test)]
    pub fn rejection(&self, surface: ComposerSurface) -> Option<CommandOutcome> {
        match self {
            Command::Empty => None,
            Command::Unknown(_) => Some(CommandOutcome::Unsupported),
            _ if !self.allowed_on(surface) => Some(CommandOutcome::WrongScope),
            Command::Action(text) if text.is_empty() => Some(CommandOutcome::Refused),
            _ => None,
        }
    }
}

/// The outcome of one dispatched command (`IrcCommandOutcome`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandOutcome {
    /// The command was written to the wire, or handled locally.
    Sent,
    /// The session was not registered.
    NotConnected,
    /// The connection layer rejected the command, or the argument did not fit.
    Refused,
    /// The word is not a catalog verb.
    Unsupported,
    /// The verb is not valid on this surface, or it has no target.
    WrongScope,
}

/// The user-facing text for an outcome (`ircCommandOutcomeText`). `Sent` has none.
pub fn command_outcome_text(outcome: CommandOutcome, command: &Command) -> String {
    match outcome {
        CommandOutcome::Sent => String::new(),
        CommandOutcome::NotConnected => "Not connected".to_string(),
        CommandOutcome::Refused => COMMAND_REFUSED_TEXT.to_string(),
        CommandOutcome::Unsupported => match command {
            Command::Unknown(word) => format!("Unknown command: {word}"),
            _ => NOT_SUPPORTED_TEXT.to_string(),
        },
        CommandOutcome::WrongScope => match command.verb() {
            Some(verb) => spec_for(verb).wrong_scope_message().to_string(),
            None => NO_CONVERSATION_TEXT.to_string(),
        },
    }
}

/// The `/help` line: every catalog name, in catalog order, then the join hint
/// (`IrcCommandDispatcher::dispatchHelp`).
pub fn help_text() -> String {
    let names: Vec<String> = VERB_TABLE
        .iter()
        .map(|spec| format!("/{}", spec.name))
        .collect();
    format!(
        "Commands: {}. Empty /join joins the latest invite.",
        names.join(", ")
    )
}

/// What the selected view is, for a CTCP verb typed without a nick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(test)]
pub enum SelectedView<'a> {
    /// A direct message with this peer. A bare `/ping`, `/time` or `/version` asks the peer.
    Direct(&'a str),
    /// A channel. A bare CTCP verb needs a nick.
    Channel,
}

/// A CTCP query that may be sent (`/ping`, `/time`, `/version`).
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
pub struct CtcpTarget {
    pub nick: String,
    /// True when the reply is a whois-style row in the asking conversation. On
    /// Status the reply stays on Status.
    pub in_conversation: bool,
}

/// Picks the target for a CTCP verb's query (`IrcReplyRouter::dispatchCtcp`).
/// - The first word is the nick. Any text after it refuses the command.
/// - With no nick: a direct message uses its peer, a channel needs a nick (a
///   wrong-scope refusal), and no selection refuses.
/// - A channel name is refused. `chantypes` lists the network's channel prefixes.
/// - With a nick but no selection on a conversation, the command is wrong scope.
#[cfg(test)]
pub fn ctcp_target(
    words: &Words,
    surface: ComposerSurface,
    selected: Option<SelectedView<'_>>,
    chantypes: &str,
) -> Result<CtcpTarget, CommandOutcome> {
    if !words.rest.is_empty() {
        return Err(CommandOutcome::Refused);
    }
    let in_conversation = surface == ComposerSurface::Conversation;
    let nick = words.first.as_str();

    if nick.is_empty() {
        return match selected {
            Some(SelectedView::Direct(peer)) => Ok(CtcpTarget {
                nick: peer.to_string(),
                in_conversation,
            }),
            Some(SelectedView::Channel) => Err(CommandOutcome::WrongScope),
            None => Err(CommandOutcome::Refused),
        };
    }
    if in_conversation && selected.is_none() {
        return Err(CommandOutcome::WrongScope);
    }
    if nick.starts_with(|ch: char| chantypes.contains(ch)) {
        return Err(CommandOutcome::Refused);
    }
    Ok(CtcpTarget {
        nick: nick.to_string(),
        in_conversation,
    })
}

/// The VERSION reply line, `VERSION reply from <nick>: https://omairc.app`
/// (`formatCtcpReplyText` with the VERSION payload).
#[cfg(test)]
pub fn version_reply_text(nick: &str) -> String {
    format_ctcp_reply_text("VERSION", nick, ctcp_version_reply_text(), 0)
}

/// The lines one `/pref` prints, and whether it changed a value. The caller saves
/// the preferences when it did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrefReply {
    /// The lines to show, joined with newlines.
    pub text: String,
    /// True when the command set a value.
    pub changed: bool,
}

/// Runs a parsed `/pref` against `prefs` and returns its reply. A set writes into
/// `prefs` at once. Usage echoes the verb's usage and changes nothing.
pub fn pref_reply(request: PrefRequest, prefs: &mut Preferences) -> PrefReply {
    match request.kind {
        PrefKind::Usage => PrefReply {
            text: spec_for(Verb::Pref).usage.to_string(),
            changed: false,
        },
        PrefKind::QueryAll => PrefReply {
            text: format_pref_list(
                prefs.directs,
                prefs.avatars,
                prefs.unread,
                prefs.colors,
                prefs.joins,
            ),
            changed: false,
        },
        PrefKind::QueryOne => PrefReply {
            text: if request.name == PrefName::Joins {
                format_pref_noise(prefs.joins)
            } else {
                format_pref_query(request.name, prefs.enabled(request.name))
            },
            changed: false,
        },
        PrefKind::Set => {
            let value = if request.name == PrefName::Joins {
                PrefValue::Noise(request.noise)
            } else {
                PrefValue::Toggle(request.enabled)
            };
            prefs.set(request.name, value);
            let text = if request.name == PrefName::Joins {
                format_pref_noise(prefs.joins)
            } else {
                format_pref_state(request.name, prefs.enabled(request.name))
            };
            PrefReply {
                text,
                changed: true,
            }
        }
    }
}

#[cfg(test)]
mod tests;
