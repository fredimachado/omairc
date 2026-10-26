//! The parser: one composer line becomes a typed `Command`. Blank input, plain text,
//! `//x` and an unknown slash word are decided here. A known verb's argument is shaped
//! by `command_for`, the way that verb's usage line splits it.

use super::verbs::{lookup, Verb};
use super::{Command, Words};
use crate::irc::pref::parse_pref_argument;

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
