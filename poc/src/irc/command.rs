//! Port of `tui/internal/irc/command.go` (Go) and `src/irc/irccommand.{h,cpp}` (C++):
//! the slash catalog, the composer surface and verb scope, the parser into typed
//! `Command` values, the refusal texts, `/help`, and the pure replies of the verbs
//! that need no server state (`/pref`, the CTCP target rule).
//!
//! A verb's argument is split the way its usage line splits it. A verb that names one
//! word first (`/query <nick> [text]`, `/kick [channel] <nick> [reason]`) carries a
//! `Words`: the first word and the trimmed text after it, as Go's `firstToken` and
//! `restAfterFirstToken`. A verb whose argument is one free text (`/away [reason]`,
//! `/topic [text]`) carries a `String`. Which word is a channel, how a join list
//! splits against CHANTYPES, and what a MODE request means all need `ServerFeatures`,
//! so the dispatcher reads those, not this parser.
//!
//! Pure: no socket, file, thread or gpui. `tests/irc_portability.rs` enforces that.

use super::ctcp::{ctcp_version_reply_text, format_ctcp_reply_text};
use super::pref::{
    format_pref_list, format_pref_noise, format_pref_query, format_pref_state, parse_pref_argument,
    PrefKind, PrefName, PrefRequest, PrefValue, Preferences,
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

/// The surfaces a verb may run on (Go `VerbScope`, C++ `IrcVerbScope`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerbScope {
    /// Only in a conversation.
    Conversation,
    /// Only on Status.
    Status,
    /// On either surface.
    Either,
}

/// One catalog verb. Each name matches its catalog row; `Say`, `Empty` and unknown
/// words are not verbs (they are `Command` variants with no `Verb`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Verb {
    Action,
    Join,
    Part,
    Nick,
    Disconnect,
    Clear,
    Close,
    Query,
    Msg,
    Topic,
    Notice,
    Away,
    Back,
    Autoaway,
    Pref,
    Status,
    Avatar,
    Whois,
    Ping,
    Time,
    Version,
    Mode,
    Kick,
    Invite,
    Ignore,
    Unignore,
    Ignored,
    Monitor,
    Unmonitor,
    Monitored,
    Mute,
    Unmute,
    Muted,
    Highlight,
    Unhighlight,
    Highlights,
    Op,
    Deop,
    Voice,
    Devoice,
    Ban,
    Ns,
    Cs,
    Znc,
    Raw,
    Help,
    List,
}

/// The refusal text for a wrong scope with no verb-specific text, and for a plain
/// line typed on Status (`ircCommandOutcomeText`, WrongScope).
pub const NO_CONVERSATION_TEXT: &str = "Select a connected conversation first";

/// The refusal text for a command that was understood but cannot run.
pub const COMMAND_REFUSED_TEXT: &str = "Command was refused";

/// The text for a slash word outside the catalog that has no word to name.
const NOT_SUPPORTED_TEXT: &str = "That command is not supported";

/// One catalog row (Go `VerbSpec`, C++ `IrcVerbSpec`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerbSpec {
    pub verb: Verb,
    /// The name, without the slash.
    pub name: &'static str,
    /// Alternate names that resolve to this verb, without the slash.
    pub aliases: &'static [&'static str],
    /// The usage line the completion popup shows beside the name.
    pub usage: &'static str,
    pub scope: VerbScope,
    /// The refusal text for a wrong scope or a missing target. Empty means
    /// `NO_CONVERSATION_TEXT`.
    pub wrong_scope_text: &'static str,
}

impl VerbSpec {
    /// Whether the verb may run on `surface` (`IrcVerbSpec::allowedOn`).
    pub fn allowed_on(&self, surface: ComposerSurface) -> bool {
        match self.scope {
            VerbScope::Conversation => surface == ComposerSurface::Conversation,
            VerbScope::Status => surface == ComposerSurface::Status,
            VerbScope::Either => true,
        }
    }

    /// The text to show when the verb cannot run here (`wrongScopeMessage`).
    pub fn wrong_scope_message(&self) -> &'static str {
        if self.wrong_scope_text.is_empty() {
            NO_CONVERSATION_TEXT
        } else {
            self.wrong_scope_text
        }
    }
}

/// The ordered catalog. The order is the completion ranking's tie order and the
/// `/help` order, so rows are never re-sorted. Mirrors `IrcVerbTable::all` and
/// `verbCatalog`.
pub static VERB_TABLE: [VerbSpec; 47] = [
    VerbSpec {
        verb: Verb::Action,
        name: "me",
        aliases: &[],
        usage: "/me <text>",
        scope: VerbScope::Conversation,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Join,
        name: "join",
        aliases: &["j"],
        usage: "/join [channel] [key][, ...]",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Part,
        name: "part",
        aliases: &["leave"],
        usage: "/part [channel]",
        scope: VerbScope::Either,
        wrong_scope_text: "Part applies to channels",
    },
    VerbSpec {
        verb: Verb::Nick,
        name: "nick",
        aliases: &[],
        usage: "/nick <nickname>",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Disconnect,
        name: "disconnect",
        aliases: &["quit"],
        usage: "/disconnect [reason]",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Clear,
        name: "clear",
        aliases: &[],
        usage: "/clear",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Close,
        name: "close",
        aliases: &[],
        usage: "/close",
        scope: VerbScope::Conversation,
        wrong_scope_text: "Close applies to direct messages and channels you have left",
    },
    VerbSpec {
        verb: Verb::Query,
        name: "query",
        aliases: &[],
        usage: "/query <nick> [text]",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Msg,
        name: "msg",
        aliases: &[],
        usage: "/msg <nick> <text>",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Topic,
        name: "topic",
        aliases: &[],
        usage: "/topic [text]",
        scope: VerbScope::Conversation,
        wrong_scope_text: "Topic applies to channels",
    },
    VerbSpec {
        verb: Verb::Notice,
        name: "notice",
        aliases: &[],
        usage: "/notice <target> <text>",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Away,
        name: "away",
        aliases: &[],
        usage: "/away [reason]",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Back,
        name: "back",
        aliases: &[],
        usage: "/back",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Autoaway,
        name: "autoaway",
        aliases: &[],
        usage: "/autoaway [off|on|duration [reason]|reason [text]]",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Pref,
        name: "pref",
        aliases: &[],
        usage: "/pref [directs|avatars|unread] [on|off] | joins [folded|every|hidden]",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Status,
        name: "status",
        aliases: &[],
        usage: "/status [text]",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Avatar,
        name: "avatar",
        aliases: &[],
        usage: "/avatar [url|email]",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Whois,
        name: "whois",
        aliases: &[],
        usage: "/whois [nick]",
        scope: VerbScope::Either,
        wrong_scope_text: "Name a nick",
    },
    VerbSpec {
        verb: Verb::Ping,
        name: "ping",
        aliases: &[],
        usage: "/ping [nick]",
        scope: VerbScope::Either,
        wrong_scope_text: "Name a nick",
    },
    VerbSpec {
        verb: Verb::Time,
        name: "time",
        aliases: &[],
        usage: "/time [nick]",
        scope: VerbScope::Either,
        wrong_scope_text: "Name a nick",
    },
    VerbSpec {
        verb: Verb::Version,
        name: "version",
        aliases: &[],
        usage: "/version [nick]",
        scope: VerbScope::Either,
        wrong_scope_text: "Name a nick",
    },
    VerbSpec {
        verb: Verb::Mode,
        name: "mode",
        aliases: &[],
        usage: "/mode <channel> [[+|-]modechars [parameters]]",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Kick,
        name: "kick",
        aliases: &[],
        usage: "/kick [channel] <nick> [reason]",
        scope: VerbScope::Either,
        wrong_scope_text: "Kick applies to channels",
    },
    VerbSpec {
        verb: Verb::Invite,
        name: "invite",
        aliases: &[],
        usage: "/invite <nick> [channel]",
        scope: VerbScope::Either,
        wrong_scope_text: "Invite applies to channels",
    },
    VerbSpec {
        verb: Verb::Ignore,
        name: "ignore",
        aliases: &[],
        usage: "/ignore <nick>",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Unignore,
        name: "unignore",
        aliases: &[],
        usage: "/unignore <nick>",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Ignored,
        name: "ignored",
        aliases: &[],
        usage: "/ignored",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Monitor,
        name: "monitor",
        aliases: &[],
        usage: "/monitor <nick>",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Unmonitor,
        name: "unmonitor",
        aliases: &[],
        usage: "/unmonitor <nick>",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Monitored,
        name: "monitored",
        aliases: &[],
        usage: "/monitored",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Mute,
        name: "mute",
        aliases: &[],
        usage: "/mute [target]",
        scope: VerbScope::Either,
        wrong_scope_text: "Mute applies to conversations",
    },
    VerbSpec {
        verb: Verb::Unmute,
        name: "unmute",
        aliases: &[],
        usage: "/unmute [target]",
        scope: VerbScope::Either,
        wrong_scope_text: "Mute applies to conversations",
    },
    VerbSpec {
        verb: Verb::Muted,
        name: "muted",
        aliases: &[],
        usage: "/muted",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Highlight,
        name: "highlight",
        aliases: &[],
        usage: "/highlight <word>",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Unhighlight,
        name: "unhighlight",
        aliases: &[],
        usage: "/unhighlight <word>",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Highlights,
        name: "highlights",
        aliases: &[],
        usage: "/highlights",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Op,
        name: "op",
        aliases: &[],
        usage: "/op <nick>",
        scope: VerbScope::Conversation,
        wrong_scope_text: "Op applies to channels",
    },
    VerbSpec {
        verb: Verb::Deop,
        name: "deop",
        aliases: &[],
        usage: "/deop <nick>",
        scope: VerbScope::Conversation,
        wrong_scope_text: "Deop applies to channels",
    },
    VerbSpec {
        verb: Verb::Voice,
        name: "voice",
        aliases: &[],
        usage: "/voice <nick>",
        scope: VerbScope::Conversation,
        wrong_scope_text: "Voice applies to channels",
    },
    VerbSpec {
        verb: Verb::Devoice,
        name: "devoice",
        aliases: &[],
        usage: "/devoice <nick>",
        scope: VerbScope::Conversation,
        wrong_scope_text: "Devoice applies to channels",
    },
    VerbSpec {
        verb: Verb::Ban,
        name: "ban",
        aliases: &[],
        usage: "/ban <mask>",
        scope: VerbScope::Conversation,
        wrong_scope_text: "Ban applies to channels",
    },
    VerbSpec {
        verb: Verb::Ns,
        name: "ns",
        aliases: &[],
        usage: "/ns <text>",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Cs,
        name: "cs",
        aliases: &[],
        usage: "/cs <text>",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Znc,
        name: "znc",
        aliases: &[],
        usage: "/znc <text>",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Raw,
        name: "raw",
        aliases: &["quote"],
        usage: "/raw <line>",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Help,
        name: "help",
        aliases: &[],
        usage: "/help",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::List,
        name: "list",
        aliases: &[],
        usage: "/list [mask]",
        scope: VerbScope::Either,
        wrong_scope_text: "",
    },
];

/// The full ordered catalog (`IrcVerbTable::all`). It is a static table, so a caller
/// cannot change a row.
pub fn verb_table() -> &'static [VerbSpec] {
    &VERB_TABLE
}

/// The catalog row for `verb`. Every verb has exactly one.
pub fn spec_for(verb: Verb) -> &'static VerbSpec {
    VERB_TABLE
        .iter()
        .find(|spec| spec.verb == verb)
        .expect("every verb has a catalog row")
}

/// The catalog row for a typed name or alias, ignoring case (`IrcVerbTable::lookup`).
/// `None` when nothing matches.
pub fn lookup(token: &str) -> Option<&'static VerbSpec> {
    let folded = token.to_lowercase();
    VERB_TABLE
        .iter()
        .find(|spec| spec.name == folded || spec.aliases.contains(&folded.as_str()))
}

/// The rows allowed on `surface`, in catalog order (`IrcVerbTable::visibleOn`).
pub fn visible_on(surface: ComposerSurface) -> Vec<&'static VerbSpec> {
    VERB_TABLE
        .iter()
        .filter(|spec| spec.allowed_on(surface))
        .collect()
}

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
///
/// A variant that is not `Words` keeps the whole trimmed argument. Where the
/// dispatcher must refuse a stray argument (`/ignored`, `/muted`, `/highlights`,
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
    /// `/query <nick> [text]`.
    Query(Words),
    /// `/msg <nick> <text>`.
    Msg(Words),
    /// `/topic [text]`. The whole argument.
    Topic(String),
    /// `/notice <target> <text>`.
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
    /// `/whois [nick]`.
    Whois(Words),
    /// `/ping [nick]`.
    Ping(Words),
    /// `/time [nick]`.
    Time(Words),
    /// `/version [nick]`.
    Version(Words),
    /// `/mode <channel> [...]`. The raw argument, read against ServerFeatures.
    Mode(String),
    /// `/kick [channel] <nick> [reason]`. The first word may name the channel.
    Kick(Words),
    /// `/invite <nick> [channel]`.
    Invite(Words),
    /// `/ignore <nick>`.
    Ignore(Words),
    /// `/unignore <nick>`.
    Unignore(Words),
    /// `/ignored`. A non-empty argument is refused by the dispatcher.
    Ignored(String),
    /// `/monitor <nick>`.
    Monitor(Words),
    /// `/unmonitor <nick>`.
    Unmonitor(Words),
    /// `/monitored`. A non-empty argument is refused by the dispatcher.
    Monitored(String),
    /// `/mute [target]`.
    Mute(Words),
    /// `/unmute [target]`.
    Unmute(Words),
    /// `/muted`. A non-empty argument is refused by the dispatcher.
    Muted(String),
    /// `/highlight <word>`.
    Highlight(Words),
    /// `/unhighlight <word>`.
    Unhighlight(Words),
    /// `/highlights`. A non-empty argument is refused by the dispatcher.
    Highlights(String),
    /// `/op <nick>`. The first word; the dispatcher turns it into a MODE change.
    Op(Words),
    /// `/deop <nick>`.
    Deop(Words),
    /// `/voice <nick>`.
    Voice(Words),
    /// `/devoice <nick>`.
    Devoice(Words),
    /// `/ban <mask>`.
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

/// Parses one composer line (`IrcCommand::parse`, Go `ParseCommand`). The line is
/// trimmed first.
///
/// - Blank input is `Empty`.
/// - Text without a leading `/` is `Say`.
/// - `//x` is `Say` with the text `/x`.
/// - `/word rest` looks `word` up in the catalog, ignoring case. A miss is `Unknown`,
///   and the word keeps its slash.
pub fn parse(input: &str) -> Command {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Command::Empty;
    }
    if !trimmed.starts_with('/') {
        return Command::Say(trimmed.to_string());
    }
    if trimmed.starts_with("//") {
        // Both slashes are ASCII, so byte 1 is a character boundary.
        return Command::Say(trimmed[1..].to_string());
    }

    let (word, argument) = match trimmed.find(' ') {
        Some(space) => (&trimmed[..space], trimmed[space + 1..].trim()),
        None => (trimmed, ""),
    };
    match lookup(&word[1..]) {
        Some(spec) => command_for(spec.verb, argument),
        None => Command::Unknown(word.to_string()),
    }
}

/// Builds the typed command for a verb from its trimmed argument.
fn command_for(verb: Verb, argument: &str) -> Command {
    match verb {
        Verb::Action => Command::Action(argument.to_string()),
        Verb::Join => Command::Join(argument.to_string()),
        Verb::Part => Command::Part(Words::split(argument)),
        Verb::Nick => Command::Nick(argument.to_string()),
        Verb::Disconnect => Command::Disconnect(argument.to_string()),
        Verb::Clear => Command::Clear,
        Verb::Close => Command::Close,
        Verb::Query => Command::Query(Words::split(argument)),
        Verb::Msg => Command::Msg(Words::split(argument)),
        Verb::Topic => Command::Topic(argument.to_string()),
        Verb::Notice => Command::Notice(Words::split(argument)),
        Verb::Away => Command::Away(argument.to_string()),
        Verb::Back => Command::Back,
        Verb::Autoaway => Command::Autoaway(argument.to_string()),
        Verb::Pref => Command::Pref(parse_pref_argument(argument)),
        Verb::Status => Command::Status(argument.to_string()),
        Verb::Avatar => Command::Avatar(argument.to_string()),
        Verb::Whois => Command::Whois(Words::split(argument)),
        Verb::Ping => Command::Ping(Words::split(argument)),
        Verb::Time => Command::Time(Words::split(argument)),
        Verb::Version => Command::Version(Words::split(argument)),
        Verb::Mode => Command::Mode(argument.to_string()),
        Verb::Kick => Command::Kick(Words::split(argument)),
        Verb::Invite => Command::Invite(Words::split(argument)),
        Verb::Ignore => Command::Ignore(Words::split(argument)),
        Verb::Unignore => Command::Unignore(Words::split(argument)),
        Verb::Ignored => Command::Ignored(argument.to_string()),
        Verb::Monitor => Command::Monitor(Words::split(argument)),
        Verb::Unmonitor => Command::Unmonitor(Words::split(argument)),
        Verb::Monitored => Command::Monitored(argument.to_string()),
        Verb::Mute => Command::Mute(Words::split(argument)),
        Verb::Unmute => Command::Unmute(Words::split(argument)),
        Verb::Muted => Command::Muted(argument.to_string()),
        Verb::Highlight => Command::Highlight(Words::split(argument)),
        Verb::Unhighlight => Command::Unhighlight(Words::split(argument)),
        Verb::Highlights => Command::Highlights(argument.to_string()),
        Verb::Op => Command::Op(Words::split(argument)),
        Verb::Deop => Command::Deop(Words::split(argument)),
        Verb::Voice => Command::Voice(Words::split(argument)),
        Verb::Devoice => Command::Devoice(Words::split(argument)),
        Verb::Ban => Command::Ban(Words::split(argument)),
        Verb::Ns => Command::Ns(argument.to_string()),
        Verb::Cs => Command::Cs(argument.to_string()),
        Verb::Znc => Command::Znc(argument.to_string()),
        Verb::Raw => Command::Raw(argument.to_string()),
        Verb::Help => Command::Help,
        Verb::List => Command::List(argument.to_string()),
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

// ---------------------------------------------------------------------------
// The CTCP target rule (`IrcReplyRouter::dispatchCtcp`)
// ---------------------------------------------------------------------------

/// What the selected view is, for a CTCP verb typed without a nick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelectedView<'a> {
    /// A direct message with this peer. A bare `/ping`, `/time` or `/version` asks the peer.
    Direct(&'a str),
    /// A channel. A bare CTCP verb needs a nick.
    Channel,
}

/// A CTCP query that may be sent (`/ping`, `/time`, `/version`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtcpTarget {
    /// The nick to ask.
    pub nick: String,
    /// True when the reply is a whois-style row in the asking conversation. On
    /// Status the reply stays on Status.
    pub in_conversation: bool,
}

/// Picks the target for a CTCP verb's query (`IrcReplyRouter::dispatchCtcp`).
///
/// - The first word is the nick. Any text after it refuses the command.
/// - With no nick: a direct message uses its peer, a channel needs a nick (a
///   wrong-scope refusal), and no selection refuses.
/// - A channel name is refused. `chantypes` lists the network's channel prefixes.
/// - With a nick but no selection on a conversation, the command is wrong scope.
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
pub fn version_reply_text(nick: &str) -> String {
    format_ctcp_reply_text("VERSION", nick, ctcp_version_reply_text(), 0)
}

// ---------------------------------------------------------------------------
// /pref replies (`IrcCommandDispatcher::dispatchPref`)
// ---------------------------------------------------------------------------

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
            text: format_pref_list(prefs.directs, prefs.avatars, prefs.unread, prefs.joins),
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
mod tests {
    use super::*;
    use crate::irc::membershipnoise::MembershipNoise;
    use crate::irc::pref::PrefValueKind;
    use std::collections::HashSet;

    fn words(first: &str, rest: &str) -> Words {
        Words {
            first: first.to_string(),
            rest: rest.to_string(),
        }
    }

    fn say(text: &str) -> Command {
        Command::Say(text.to_string())
    }

    const CONVERSATION: ComposerSurface = ComposerSurface::Conversation;
    const STATUS: ComposerSurface = ComposerSurface::Status;

    // Ported from tui/internal/irc/command_test.go.

    #[test]
    fn verb_table_has_the_catalog_shape() {
        let table = verb_table();
        assert_eq!(table.len(), 47, "the catalog has 47 rows");

        let first = table[0];
        assert_eq!(first.verb, Verb::Action);
        assert_eq!(first.name, "me");
        assert_eq!(first.usage, "/me <text>");
        assert_eq!(first.scope, VerbScope::Conversation);
        assert_eq!(first.wrong_scope_text, "");
        assert!(first.aliases.is_empty());

        let join = table[1];
        assert_eq!(join.verb, Verb::Join);
        assert_eq!(join.name, "join");
        assert_eq!(join.usage, "/join [channel] [key][, ...]");
        assert_eq!(join.scope, VerbScope::Either);
        assert_eq!(join.aliases, ["j"]);

        let part = table[2];
        assert_eq!(part.scope, VerbScope::Either);
        assert_eq!(part.wrong_scope_text, "Part applies to channels");

        let topic = table[9];
        assert_eq!(topic.verb, Verb::Topic);
        assert_eq!(topic.scope, VerbScope::Conversation);
        assert_eq!(topic.wrong_scope_text, "Topic applies to channels");

        let last = table[46];
        assert_eq!(last.verb, Verb::List);
        assert_eq!(last.name, "list");
        assert_eq!(last.usage, "/list [mask]");
    }

    #[test]
    fn every_verb_has_exactly_one_row_and_names_are_unique() {
        let mut names = HashSet::new();
        let mut verbs = HashSet::new();
        for spec in verb_table() {
            assert!(names.insert(spec.name), "duplicate name {}", spec.name);
            assert!(verbs.insert(spec.verb), "duplicate verb {:?}", spec.verb);
            assert_eq!(spec_for(spec.verb).name, spec.name);
        }
        assert_eq!(verbs.len(), 47);
    }

    #[test]
    fn the_catalog_table_is_a_static_slice() {
        // `verb_table` hands out a `'static` slice, so a caller cannot change a row.
        // Two reads see the same rows.
        assert_eq!(verb_table()[0].name, "me");
        assert!(std::ptr::eq(verb_table(), verb_table()));
    }

    #[test]
    fn lookup_is_case_insensitive_and_takes_aliases() {
        assert_eq!(lookup("J").map(|s| s.verb), Some(Verb::Join));
        assert_eq!(lookup("leave").map(|s| s.verb), Some(Verb::Part));
        assert_eq!(lookup("QUIT").map(|s| s.verb), Some(Verb::Disconnect));
        assert_eq!(lookup("quote").map(|s| s.verb), Some(Verb::Raw));
        assert!(lookup("nope").is_none());
        assert!(lookup("").is_none());
    }

    #[test]
    fn q_and_t_are_not_verbs() {
        // `/q` completes to `/query` but is not a quit alias; `/t` is no verb at all.
        assert!(lookup("q").is_none());
        assert!(lookup("t").is_none());
        assert!(lookup("quit").is_some_and(|s| s.name == "disconnect"));
    }

    #[test]
    fn parse_command_table() {
        let cases: Vec<(&str, Command)> = vec![
            ("", Command::Empty),
            ("   ", Command::Empty),
            ("hello world", say("hello world")),
            ("  spaced  ", say("spaced")),
            ("//away", say("/away")),
            ("///x", say("//x")),
            ("//", say("/")),
            ("/j #help", Command::Join("#help".into())),
            ("/join", Command::Join(String::new())),
            ("/JOIN  #help  ", Command::Join("#help".into())),
            ("/back extra", Command::Back),
            ("/bogus", Command::Unknown("/bogus".into())),
            ("/", Command::Unknown("/".into())),
        ];
        for (input, want) in cases {
            assert_eq!(parse(input), want, "parse({input:?})");
        }
    }

    #[test]
    fn command_is_live_message_for_say_and_action_only() {
        assert!(say("hello").is_live_message());
        assert!(Command::Action("waves".into()).is_live_message());
        assert!(!Command::Join("#x".into()).is_live_message());
        assert!(parse("/me").is_live_message());
    }

    #[test]
    fn command_allowed_on_follows_the_catalog_scope() {
        assert!(say("hi").allowed_on(CONVERSATION));
        assert!(!say("hi").allowed_on(STATUS));
        // Join is Either.
        assert!(Command::Join(String::new()).allowed_on(STATUS));
        // /me (Action) is conversation-only.
        let me = parse("/me waves");
        assert!(!me.allowed_on(STATUS));
        assert!(me.allowed_on(CONVERSATION));
    }

    #[test]
    fn verb_spec_allowed_on_matches_the_scope() {
        let action = lookup("me").expect("me is a verb");
        assert!(action.allowed_on(CONVERSATION));
        assert!(!action.allowed_on(STATUS));
        let join = lookup("join").expect("join is a verb");
        assert!(join.allowed_on(CONVERSATION));
        assert!(join.allowed_on(STATUS));
        // The Status-only scope exists in the catalog for parity, though no row uses it.
        let status_only = VerbSpec {
            scope: VerbScope::Status,
            ..*action
        };
        assert!(status_only.allowed_on(STATUS));
        assert!(!status_only.allowed_on(CONVERSATION));
    }

    #[test]
    fn verb_spec_wrong_scope_message_defaults_when_empty() {
        let empty = VerbSpec {
            wrong_scope_text: "",
            ..*lookup("join").expect("join")
        };
        assert_eq!(
            empty.wrong_scope_message(),
            "Select a connected conversation first"
        );
        let named = VerbSpec {
            wrong_scope_text: "Topic applies to channels",
            ..*lookup("join").expect("join")
        };
        assert_eq!(named.wrong_scope_message(), "Topic applies to channels");
    }

    #[test]
    fn visible_verbs_on_surface_drops_the_conversation_only_rows() {
        let conversation = visible_on(CONVERSATION);
        let status = visible_on(STATUS);
        assert_eq!(conversation.len(), 47);
        // Status drops me, close, topic, op, deop, voice, devoice and ban.
        assert_eq!(status.len(), 39);
        for spec in status {
            assert_ne!(
                spec.scope,
                VerbScope::Conversation,
                "{} is conversation-only",
                spec.name
            );
        }
    }

    #[test]
    fn command_outcome_text_matches_the_core() {
        let cases: Vec<(CommandOutcome, Command, &str)> = vec![
            (CommandOutcome::Sent, Command::Empty, ""),
            (
                CommandOutcome::NotConnected,
                Command::Empty,
                "Not connected",
            ),
            (
                CommandOutcome::Refused,
                Command::Empty,
                "Command was refused",
            ),
            (
                CommandOutcome::Unsupported,
                Command::Empty,
                "That command is not supported",
            ),
            (
                CommandOutcome::Unsupported,
                Command::Unknown("/bogus".into()),
                "Unknown command: /bogus",
            ),
            (
                CommandOutcome::WrongScope,
                Command::Back,
                "Select a connected conversation first",
            ),
            (
                CommandOutcome::WrongScope,
                Command::Topic("hello".into()),
                "Topic applies to channels",
            ),
            (
                CommandOutcome::WrongScope,
                Command::Unknown("/x".into()),
                "Select a connected conversation first",
            ),
        ];
        for (outcome, command, want) in cases {
            assert_eq!(
                command_outcome_text(outcome, &command),
                want,
                "{outcome:?} {command:?}"
            );
        }
    }

    // Ported from tst_command.cpp (`parseVerbsAndAliases` through `parseWrappersAndHelp`).

    #[test]
    fn verbs_keep_their_argument_and_ignore_case() {
        assert_eq!(parse("/ME waves"), Command::Action("waves".into()));
        assert!(parse("/ME waves").is_live_message());
        assert_eq!(
            parse("/LEAVE #omarchy"),
            Command::Part(words("#omarchy", ""))
        );
        assert_eq!(parse("/part"), Command::Part(Words::default()));
        assert_eq!(parse("/leave"), Command::Part(Words::default()));
        assert_eq!(parse("/join"), Command::Join(String::new()));
        assert_eq!(
            parse("/disconnect later"),
            Command::Disconnect("later".into())
        );
        assert_eq!(parse("/quit later"), Command::Disconnect("later".into()));
    }

    #[test]
    fn unknown_words_keep_their_slash_and_their_argument_is_dropped() {
        assert_eq!(parse("/nope"), Command::Unknown("/nope".into()));
        assert_eq!(parse("/nope extra"), Command::Unknown("/nope".into()));
        assert_eq!(parse("/q"), Command::Unknown("/q".into()));
        assert_eq!(parse("/t"), Command::Unknown("/t".into()));
    }

    #[test]
    fn close_and_back_drop_their_arguments() {
        assert_eq!(parse("/close"), Command::Close);
        assert!(!parse("/close").is_live_message());
        assert_eq!(parse("//close"), say("/close"));
        assert_eq!(parse("/back leftover"), Command::Back);
        assert_eq!(parse("/BACK leftover"), Command::Back);
        assert_eq!(parse("//away lunch"), say("/away lunch"));
    }

    #[test]
    fn query_msg_and_notice_split_the_first_word() {
        assert_eq!(parse("/query lena hi"), Command::Query(words("lena", "hi")));
        assert_eq!(parse("/MSG lena"), Command::Msg(words("lena", "")));
        assert_eq!(parse("/msg lena hi"), Command::Msg(words("lena", "hi")));
        assert_eq!(parse("//query"), say("/query"));
        assert_eq!(parse("//msg hi"), say("/msg hi"));
        assert_eq!(
            parse("/notice lena later"),
            Command::Notice(words("lena", "later"))
        );
        assert_eq!(
            parse("/NOTICE #omarchy hi"),
            Command::Notice(words("#omarchy", "hi"))
        );
        assert_eq!(parse("//notice hi"), say("/notice hi"));
    }

    #[test]
    fn topic_and_away_keep_the_whole_text() {
        assert_eq!(parse("/topic"), Command::Topic(String::new()));
        assert_eq!(
            parse("/TOPIC Hello world"),
            Command::Topic("Hello world".into())
        );
        assert_eq!(
            parse("/topic #omarchy is the place"),
            Command::Topic("#omarchy is the place".into())
        );
        assert_eq!(parse("/away lunch"), Command::Away("lunch".into()));
        assert_eq!(parse("/AWAY lunch"), Command::Away("lunch".into()));
        assert_eq!(parse("/away"), Command::Away(String::new()));
    }

    #[test]
    fn autoaway_keeps_its_raw_argument() {
        assert_eq!(parse("/autoaway"), Command::Autoaway(String::new()));
        assert_eq!(parse("/autoaway 15"), Command::Autoaway("15".into()));
        assert_eq!(parse("/AUTOAWAY off"), Command::Autoaway("off".into()));
        assert_eq!(parse("//autoaway 15"), say("/autoaway 15"));
    }

    #[test]
    fn status_and_avatar_keep_the_whole_argument() {
        assert_eq!(parse("/status"), Command::Status(String::new()));
        assert_eq!(
            parse("/status writing docs"),
            Command::Status("writing docs".into())
        );
        assert_eq!(parse("/STATUS clear"), Command::Status("clear".into()));
        assert_eq!(
            parse("/status clear the table"),
            Command::Status("clear the table".into())
        );
        assert_eq!(parse("//status lunch"), say("/status lunch"));
        assert_eq!(parse("/avatar"), Command::Avatar(String::new()));
        assert_eq!(
            parse("/avatar https://example.com/a.png"),
            Command::Avatar("https://example.com/a.png".into())
        );
        assert_eq!(
            parse("/avatar me@example.com"),
            Command::Avatar("me@example.com".into())
        );
        assert_eq!(parse("/AVATAR clear"), Command::Avatar("clear".into()));
    }

    #[test]
    fn whois_and_the_ctcp_queries_split_the_first_word() {
        assert_eq!(parse("/whois lena"), Command::Whois(words("lena", "")));
        assert_eq!(parse("/WHOIS"), Command::Whois(Words::default()));
        assert_eq!(parse("//whois lena"), say("/whois lena"));
        assert_eq!(parse("/ping lena"), Command::Ping(words("lena", "")));
        assert_eq!(parse("/PING"), Command::Ping(Words::default()));
        assert_eq!(parse("/TIME lena"), Command::Time(words("lena", "")));
        assert_eq!(parse("/version lena"), Command::Version(words("lena", "")));
        assert_eq!(parse("/version"), Command::Version(Words::default()));
        assert_eq!(parse("//ping lena"), say("/ping lena"));
    }

    #[test]
    fn mode_kick_and_invite() {
        assert_eq!(
            parse("/mode #omarchy +o lena"),
            Command::Mode("#omarchy +o lena".into())
        );
        assert_eq!(parse("/MODE #omarchy"), Command::Mode("#omarchy".into()));
        assert_eq!(parse("/mode"), Command::Mode(String::new()));
        assert_eq!(parse("//mode #omarchy"), say("/mode #omarchy"));
        assert_eq!(parse("/kick bob spam"), Command::Kick(words("bob", "spam")));
        assert_eq!(
            parse("/KICK #omarchy bob"),
            Command::Kick(words("#omarchy", "bob"))
        );
        assert_eq!(parse("/kick"), Command::Kick(Words::default()));
        assert_eq!(parse("//kick bob"), say("/kick bob"));
        assert_eq!(
            parse("/invite bob #lab"),
            Command::Invite(words("bob", "#lab"))
        );
        assert_eq!(parse("/INVITE bob"), Command::Invite(words("bob", "")));
        assert_eq!(parse("/invite"), Command::Invite(Words::default()));
        assert_eq!(parse("//invite bob"), say("/invite bob"));
    }

    #[test]
    fn op_family_services_raw_help_and_list() {
        assert_eq!(parse("/op alice"), Command::Op(words("alice", "")));
        assert!(!parse("/op alice").is_live_message());
        assert_eq!(parse("/DEOP alice"), Command::Deop(words("alice", "")));
        assert_eq!(parse("/voice bob"), Command::Voice(words("bob", "")));
        assert_eq!(parse("/devoice bob"), Command::Devoice(words("bob", "")));
        assert_eq!(parse("/ban eve"), Command::Ban(words("eve", "")));
        assert_eq!(
            parse("/ns identify hunter2"),
            Command::Ns("identify hunter2".into())
        );
        assert_eq!(parse("/CS help"), Command::Cs("help".into()));
        assert_eq!(parse("/znc ListMods"), Command::Znc("ListMods".into()));
        assert_eq!(parse("/raw PING :x"), Command::Raw("PING :x".into()));
        assert_eq!(parse("/quote PING :x"), Command::Raw("PING :x".into()));
        assert_eq!(parse("/help"), Command::Help);
        assert_eq!(parse("/help me"), Command::Help);
        assert_eq!(parse("/list"), Command::List(String::new()));
        assert_eq!(parse("/LIST #foo*"), Command::List("#foo*".into()));
        assert_eq!(parse("//list"), say("/list"));
        assert_eq!(parse("//op alice"), say("/op alice"));
    }

    #[test]
    fn the_remaining_catalog_verbs_parse_to_their_variant() {
        assert_eq!(parse("/nick bob"), Command::Nick("bob".into()));
        assert_eq!(parse("/clear now"), Command::Clear);
        assert_eq!(parse("/monitor lena"), Command::Monitor(words("lena", "")));
        assert_eq!(
            parse("/unmonitor lena"),
            Command::Unmonitor(words("lena", ""))
        );
        assert_eq!(parse("/monitored"), Command::Monitored(String::new()));
        assert_eq!(
            parse("/mute #omarchy"),
            Command::Mute(words("#omarchy", ""))
        );
        assert_eq!(parse("/unmute"), Command::Unmute(Words::default()));
        assert_eq!(parse("/muted"), Command::Muted(String::new()));
        assert_eq!(
            parse("/highlight rust"),
            Command::Highlight(words("rust", ""))
        );
        assert_eq!(
            parse("/unhighlight rust"),
            Command::Unhighlight(words("rust", ""))
        );
        assert_eq!(parse("/highlights"), Command::Highlights(String::new()));
        assert_eq!(parse("/ignore bob"), Command::Ignore(words("bob", "")));
        assert_eq!(parse("/unignore bob"), Command::Unignore(words("bob", "")));
        assert_eq!(parse("/ignored"), Command::Ignored(String::new()));
        assert_eq!(parse("/topic"), Command::Topic(String::new()));
    }

    #[test]
    fn every_catalog_verb_has_a_parse_arm_that_round_trips() {
        for spec in verb_table() {
            let parsed = parse(&format!("/{}", spec.name));
            assert_eq!(
                parsed.verb(),
                Some(spec.verb),
                "/{} parses to its verb",
                spec.name
            );
            if let Some(alias) = spec.aliases.first() {
                assert_eq!(parse(&format!("/{alias}")).verb(), Some(spec.verb));
            }
        }
    }

    // The catalog's usage, scope and wrong-scope rows from tst_command.cpp
    // (`catalogLookupAndScope`).

    #[test]
    fn catalog_rows_match_the_core_matrix() {
        struct Row {
            name: &'static str,
            usage: &'static str,
            scope: VerbScope,
            wrong: &'static str,
        }
        let rows = [
            Row {
                name: "part",
                usage: "/part [channel]",
                scope: VerbScope::Either,
                wrong: "Part applies to channels",
            },
            Row {
                name: "disconnect",
                usage: "/disconnect [reason]",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "close",
                usage: "/close",
                scope: VerbScope::Conversation,
                wrong: "Close applies to direct messages and channels you have left",
            },
            Row {
                name: "query",
                usage: "/query <nick> [text]",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "msg",
                usage: "/msg <nick> <text>",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "topic",
                usage: "/topic [text]",
                scope: VerbScope::Conversation,
                wrong: "Topic applies to channels",
            },
            Row {
                name: "notice",
                usage: "/notice <target> <text>",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "away",
                usage: "/away [reason]",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "back",
                usage: "/back",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "autoaway",
                usage: "/autoaway [off|on|duration [reason]|reason [text]]",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "pref",
                usage: "/pref [directs|avatars|unread] [on|off] | joins [folded|every|hidden]",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "status",
                usage: "/status [text]",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "avatar",
                usage: "/avatar [url|email]",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "whois",
                usage: "/whois [nick]",
                scope: VerbScope::Either,
                wrong: "Name a nick",
            },
            Row {
                name: "ping",
                usage: "/ping [nick]",
                scope: VerbScope::Either,
                wrong: "Name a nick",
            },
            Row {
                name: "time",
                usage: "/time [nick]",
                scope: VerbScope::Either,
                wrong: "Name a nick",
            },
            Row {
                name: "version",
                usage: "/version [nick]",
                scope: VerbScope::Either,
                wrong: "Name a nick",
            },
            Row {
                name: "mode",
                usage: "/mode <channel> [[+|-]modechars [parameters]]",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "kick",
                usage: "/kick [channel] <nick> [reason]",
                scope: VerbScope::Either,
                wrong: "Kick applies to channels",
            },
            Row {
                name: "invite",
                usage: "/invite <nick> [channel]",
                scope: VerbScope::Either,
                wrong: "Invite applies to channels",
            },
            Row {
                name: "ignore",
                usage: "/ignore <nick>",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "unignore",
                usage: "/unignore <nick>",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "ignored",
                usage: "/ignored",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "monitor",
                usage: "/monitor <nick>",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "unmonitor",
                usage: "/unmonitor <nick>",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "monitored",
                usage: "/monitored",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "mute",
                usage: "/mute [target]",
                scope: VerbScope::Either,
                wrong: "Mute applies to conversations",
            },
            Row {
                name: "unmute",
                usage: "/unmute [target]",
                scope: VerbScope::Either,
                wrong: "Mute applies to conversations",
            },
            Row {
                name: "muted",
                usage: "/muted",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "highlight",
                usage: "/highlight <word>",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "unhighlight",
                usage: "/unhighlight <word>",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "op",
                usage: "/op <nick>",
                scope: VerbScope::Conversation,
                wrong: "Op applies to channels",
            },
            Row {
                name: "deop",
                usage: "/deop <nick>",
                scope: VerbScope::Conversation,
                wrong: "Deop applies to channels",
            },
            Row {
                name: "voice",
                usage: "/voice <nick>",
                scope: VerbScope::Conversation,
                wrong: "Voice applies to channels",
            },
            Row {
                name: "devoice",
                usage: "/devoice <nick>",
                scope: VerbScope::Conversation,
                wrong: "Devoice applies to channels",
            },
            Row {
                name: "ban",
                usage: "/ban <mask>",
                scope: VerbScope::Conversation,
                wrong: "Ban applies to channels",
            },
            Row {
                name: "ns",
                usage: "/ns <text>",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "raw",
                usage: "/raw <line>",
                scope: VerbScope::Either,
                wrong: "",
            },
            Row {
                name: "list",
                usage: "/list [mask]",
                scope: VerbScope::Either,
                wrong: "",
            },
        ];
        for row in rows {
            let spec = lookup(row.name).unwrap_or_else(|| panic!("{} is a verb", row.name));
            assert_eq!(spec.usage, row.usage, "{} usage", row.name);
            assert_eq!(spec.scope, row.scope, "{} scope", row.name);
            assert_eq!(spec.wrong_scope_text, row.wrong, "{} wrong scope", row.name);
        }
    }

    #[test]
    fn the_join_and_quit_and_raw_aliases_resolve_only_their_own_verb() {
        assert_eq!(lookup("j").map(|s| s.name), Some("join"));
        assert_eq!(lookup("leave").map(|s| s.name), Some("part"));
        assert_eq!(lookup("quit").map(|s| s.name), Some("disconnect"));
        assert_eq!(lookup("quote").map(|s| s.name), Some("raw"));
        assert!(lookup("msg").is_some_and(|s| s.aliases.is_empty()));
        assert!(lookup("query").is_some_and(|s| s.aliases.is_empty()));
    }

    #[test]
    fn the_help_text_names_every_verb_then_the_join_hint() {
        let expected = [
            "/me",
            "/join",
            "/part",
            "/nick",
            "/disconnect",
            "/clear",
            "/close",
            "/query",
            "/msg",
            "/topic",
            "/notice",
            "/away",
            "/back",
            "/autoaway",
            "/pref",
            "/status",
            "/avatar",
            "/whois",
            "/ping",
            "/time",
            "/version",
            "/mode",
            "/kick",
            "/invite",
            "/ignore",
            "/unignore",
            "/ignored",
            "/monitor",
            "/unmonitor",
            "/monitored",
            "/mute",
            "/unmute",
            "/muted",
            "/highlight",
            "/unhighlight",
            "/highlights",
            "/op",
            "/deop",
            "/voice",
            "/devoice",
            "/ban",
            "/ns",
            "/cs",
            "/znc",
            "/raw",
            "/help",
            "/list",
        ];
        assert_eq!(
            help_text(),
            format!(
                "Commands: {}. Empty /join joins the latest invite.",
                expected.join(", ")
            )
        );
    }

    // Feature map `.cursor/skills/verify-omairc/features/slash-commands.md`, catalog level.

    #[test]
    fn conversation_only_verbs_are_refused_on_status_with_their_texts() {
        let cases = [
            ("/me waves", "Select a connected conversation first"),
            (
                "/close",
                "Close applies to direct messages and channels you have left",
            ),
            ("/topic hello", "Topic applies to channels"),
            ("/op alice", "Op applies to channels"),
            ("/deop alice", "Deop applies to channels"),
            ("/voice bob", "Voice applies to channels"),
            ("/devoice bob", "Devoice applies to channels"),
            ("/ban eve", "Ban applies to channels"),
        ];
        for (line, text) in cases {
            let command = parse(line);
            assert_eq!(
                command.rejection(STATUS),
                Some(CommandOutcome::WrongScope),
                "{line} on Status"
            );
            assert_eq!(
                command_outcome_text(CommandOutcome::WrongScope, &command),
                text,
                "{line} wrong-scope text"
            );
            assert_eq!(
                command.rejection(CONVERSATION),
                None,
                "{line} on a conversation"
            );
        }
    }

    #[test]
    fn ns_cs_and_znc_run_from_either_surface() {
        for line in ["/ns identify hunter2", "/cs help", "/znc ListMods"] {
            let command = parse(line);
            assert_eq!(command.rejection(CONVERSATION), None, "{line}");
            assert_eq!(command.rejection(STATUS), None, "{line}");
        }
    }

    #[test]
    fn ctcp_verbs_without_a_nick_say_name_a_nick_on_a_channel() {
        for line in ["/whois", "/ping", "/time", "/version"] {
            let command = parse(line);
            assert_eq!(
                command_outcome_text(CommandOutcome::WrongScope, &command),
                "Name a nick",
                "{line}"
            );
        }
    }

    #[test]
    fn a_double_slash_and_triple_slash_stay_plain_text() {
        assert_eq!(parse("//away"), say("/away"));
        assert_eq!(parse("///x"), say("//x"));
        assert_eq!(parse("//join"), say("/join"));
        assert!(parse("//join").allowed_on(CONVERSATION));
        assert!(!parse("//join").allowed_on(STATUS));
    }

    #[test]
    fn an_empty_me_is_refused_on_a_conversation_and_a_me_is_scoped_out_of_status() {
        assert_eq!(
            parse("/me").rejection(CONVERSATION),
            Some(CommandOutcome::Refused)
        );
        assert_eq!(
            command_outcome_text(CommandOutcome::Refused, &parse("/me")),
            "Command was refused"
        );
        assert_eq!(parse("/me hi").rejection(CONVERSATION), None);
        assert_eq!(
            command_outcome_text(CommandOutcome::WrongScope, &parse("/me hi")),
            "Select a connected conversation first"
        );
    }

    #[test]
    fn plain_text_is_wrong_scope_on_status_only_and_blank_is_never_rejected() {
        assert_eq!(parse("hello").rejection(CONVERSATION), None);
        assert_eq!(
            parse("hello").rejection(STATUS),
            Some(CommandOutcome::WrongScope)
        );
        assert_eq!(parse("").rejection(STATUS), None);
        assert_eq!(parse("   ").rejection(CONVERSATION), None);
    }

    #[test]
    fn unknown_words_are_rejected_on_both_surfaces_with_their_word() {
        let command = parse("/nope");
        for surface in [CONVERSATION, STATUS] {
            assert_eq!(
                command.rejection(surface),
                Some(CommandOutcome::Unsupported)
            );
        }
        assert_eq!(
            command_outcome_text(CommandOutcome::Unsupported, &command),
            "Unknown command: /nope"
        );
    }

    #[test]
    fn me_is_conversation_scoped_and_the_rest_run_anywhere() {
        let me = parse("/me hi");
        assert_eq!(me.rejection(CONVERSATION), None);
        assert_eq!(me.rejection(STATUS), Some(CommandOutcome::WrongScope));
        for line in [
            "/clear", "/help", "/pref", "/version", "/list", "/join #x", "/part",
        ] {
            for surface in [CONVERSATION, STATUS] {
                assert_eq!(
                    parse(line).rejection(surface),
                    None,
                    "{line} on {surface:?}"
                );
            }
        }
    }

    // Ported from the Qt pref and the reply tests in `poc/src/model/slash.rs`.

    fn pref(line: &str) -> PrefRequest {
        match parse(line) {
            Command::Pref(request) => request,
            other => panic!("{line} parsed to {other:?}"),
        }
    }

    #[test]
    fn pref_parses_its_argument_into_the_request() {
        let joins = pref("/pref joins hidden");
        assert_eq!(joins.kind, PrefKind::Set);
        assert_eq!(joins.name, PrefName::Joins);
        assert_eq!(joins.noise, MembershipNoise::Hidden);

        let avatars = pref("/pref avatars off");
        assert_eq!(avatars.kind, PrefKind::Set);
        assert_eq!(avatars.name, PrefName::Avatars);
        assert!(!avatars.enabled);

        assert_eq!(pref("/pref").kind, PrefKind::QueryAll);
    }

    #[test]
    fn pref_argument_shapes_are_query_set_and_usage() {
        assert_eq!(pref("/pref   ").kind, PrefKind::QueryAll);
        assert_eq!(pref("/pref directs").kind, PrefKind::QueryOne);
        assert_eq!(pref("/pref AVATARS").name, PrefName::Avatars);
        assert_eq!(pref("/pref joins").kind, PrefKind::QueryOne);
        assert_eq!(pref("/pref unread on").kind, PrefKind::Set);
        assert_eq!(pref("/pref avatars   off").kind, PrefKind::Set);
        assert_eq!(pref("/pref joins Every").noise, MembershipNoise::Every);
        for line in [
            "/pref directs maybe",
            "/pref joins sometimes",
            "/pref joins on",
            "/pref notify on",
            "/pref notify",
            "/pref avatars on now",
        ] {
            assert_eq!(pref(line).kind, PrefKind::Usage, "{line}");
        }
    }

    #[test]
    fn bare_pref_prints_four_lines() {
        let mut prefs = Preferences::default();
        let reply = pref_reply(pref("/pref"), &mut prefs);
        assert_eq!(
            reply.text,
            "Reopen direct messages on startup: on\n\
             Show peer avatars: on\n\
             Open conversations at unread: on\n\
             Join, part, quit, and nick lines: folded"
        );
        assert!(!reply.changed);
    }

    #[test]
    fn one_name_prints_one_line_and_avatars_adds_the_note() {
        let mut prefs = Preferences::default();
        assert_eq!(
            pref_reply(pref("/pref directs"), &mut prefs).text,
            "Reopen direct messages on startup: on"
        );
        assert_eq!(
            pref_reply(pref("/pref avatars"), &mut prefs).text,
            "Show peer avatars: on\n\
             Turn off to keep avatar hosts from seeing your IP on busy channels."
        );
        assert_eq!(
            pref_reply(pref("/pref joins"), &mut prefs).text,
            "Join, part, quit, and nick lines: folded"
        );
    }

    #[test]
    fn a_set_writes_through_and_echoes_one_state_line() {
        let mut prefs = Preferences::default();
        let reply = pref_reply(pref("/pref avatars off"), &mut prefs);
        assert_eq!(reply.text, "Show peer avatars: off");
        assert!(reply.changed);
        assert!(!prefs.avatars);

        let reply = pref_reply(pref("/pref joins every"), &mut prefs);
        assert_eq!(reply.text, "Join, part, quit, and nick lines: every");
        assert_eq!(prefs.joins, MembershipNoise::Every);
    }

    #[test]
    fn usage_echoes_the_verb_usage_and_changes_nothing() {
        let mut prefs = Preferences::default();
        let reply = pref_reply(pref("/pref joins on"), &mut prefs);
        assert_eq!(
            reply.text,
            "/pref [directs|avatars|unread] [on|off] | joins [folded|every|hidden]"
        );
        assert!(!reply.changed);
        assert_eq!(prefs, Preferences::default());
    }

    #[test]
    fn a_pref_value_kind_matches_its_catalog_row() {
        assert_eq!(
            crate::irc::pref::pref_find("joins").map(|spec| spec.value_kind),
            Some(PrefValueKind::Noise)
        );
        assert_eq!(
            crate::irc::pref::pref_find("avatars").map(|spec| spec.value_kind),
            Some(PrefValueKind::Toggle)
        );
    }

    // Ported from the CTCP target tests in `poc/src/model/slash.rs` (`version_request`).

    const CHANTYPES: &str = "#&";

    #[test]
    fn version_on_a_direct_message_asks_the_peer() {
        let target = ctcp_target(
            &Words::default(),
            CONVERSATION,
            Some(SelectedView::Direct("anna")),
            CHANTYPES,
        )
        .expect("a DM has a peer");
        assert_eq!(
            target,
            CtcpTarget {
                nick: "anna".into(),
                in_conversation: true,
            }
        );
    }

    #[test]
    fn version_from_status_stays_on_status() {
        let target = ctcp_target(
            &Words::default(),
            STATUS,
            Some(SelectedView::Direct("anna")),
            CHANTYPES,
        )
        .expect("the selected DM is the target");
        assert!(!target.in_conversation);
        assert_eq!(
            ctcp_target(&words("bob", ""), STATUS, None, CHANTYPES),
            Ok(CtcpTarget {
                nick: "bob".into(),
                in_conversation: false,
            })
        );
    }

    #[test]
    fn version_on_a_channel_needs_a_nick() {
        assert_eq!(
            ctcp_target(
                &Words::default(),
                CONVERSATION,
                Some(SelectedView::Channel),
                CHANTYPES
            ),
            Err(CommandOutcome::WrongScope)
        );
        assert_eq!(
            command_outcome_text(CommandOutcome::WrongScope, &parse("/version")),
            "Name a nick"
        );
    }

    #[test]
    fn version_with_nothing_selected_is_refused_when_empty() {
        assert_eq!(
            ctcp_target(&Words::default(), CONVERSATION, None, CHANTYPES),
            Err(CommandOutcome::Refused)
        );
    }

    #[test]
    fn a_named_nick_is_asked_in_the_selected_conversation() {
        let target = ctcp_target(
            &words("bob", ""),
            CONVERSATION,
            Some(SelectedView::Channel),
            CHANTYPES,
        )
        .expect("a nick is a valid target");
        assert_eq!(target.nick, "bob");
        assert!(target.in_conversation);
    }

    #[test]
    fn a_channel_target_is_refused() {
        assert_eq!(
            ctcp_target(
                &words("#linux", ""),
                CONVERSATION,
                Some(SelectedView::Direct("anna")),
                CHANTYPES
            ),
            Err(CommandOutcome::Refused)
        );
        assert_eq!(
            ctcp_target(&words("&local", ""), STATUS, None, CHANTYPES),
            Err(CommandOutcome::Refused)
        );
    }

    #[test]
    fn extra_words_refuse_the_command() {
        assert_eq!(
            ctcp_target(
                &words("anna", "bob"),
                CONVERSATION,
                Some(SelectedView::Channel),
                CHANTYPES
            ),
            Err(CommandOutcome::Refused)
        );
        assert_eq!(
            ctcp_target(
                &Words::split("anna  "),
                CONVERSATION,
                Some(SelectedView::Channel),
                CHANTYPES
            )
            .map(|target| target.nick),
            Ok("anna".to_string())
        );
    }

    #[test]
    fn a_nick_with_no_selection_on_a_conversation_is_wrong_scope() {
        assert_eq!(
            ctcp_target(&words("anna", ""), CONVERSATION, None, CHANTYPES),
            Err(CommandOutcome::WrongScope)
        );
    }

    #[test]
    fn the_version_reply_text_matches_qt() {
        assert_eq!(
            version_reply_text("anna"),
            "VERSION reply from anna: https://omairc.app"
        );
    }

    #[test]
    fn words_split_at_the_first_space_only() {
        assert_eq!(Words::split(""), Words::default());
        assert_eq!(Words::split("a"), words("a", ""));
        assert_eq!(Words::split("a  b  c"), words("a", "b  c"));
        assert_eq!(Words::split("a\tb"), words("a\tb", ""));
    }

    #[test]
    fn a_plain_outcome_text_has_no_text_when_sent() {
        assert_eq!(
            command_outcome_text(CommandOutcome::Sent, &Command::Empty),
            ""
        );
        assert_eq!(
            command_outcome_text(CommandOutcome::NotConnected, &Command::Empty),
            "Not connected"
        );
    }

    #[test]
    fn a_verb_argument_kind_follows_its_dispatch() {
        // The whole-argument verbs keep their text; the first-word verbs split it.
        assert!(matches!(parse("/away a b"), Command::Away(text) if text == "a b"));
        assert!(matches!(parse("/mute a b"), Command::Mute(w) if w == words("a", "b")));
    }
}
