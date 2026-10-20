//! Slash commands for the composer: a port of `IrcCommand::parse` and
//! `IrcVerbTable` (`src/irc/irccommand.cpp`), cut to the six verbs the PoC
//! supports: `me`, `clear`, `help`, `pref`, `version` and `list`. It also ports
//! the `/pref` argument parser and reply text (`src/irc/ircpref.cpp`), and the
//! `/version` target rule and reply text (`IrcReplyRouter::dispatchCtcp`,
//! `formatCtcpReplyText`).
//!
//! Toolkit-free. The shell runs what this module parses: it clears the
//! composer, records history, writes the transcript or a Status line, and opens
//! the `/list` sheet. This module only says what the command is and whether it
//! may run on the surface it was typed on.

use super::prefs::{MembershipNoise, PrefName, PrefValue, Preferences};

/// Where a command was typed: the conversation view, or a network's Status pane
/// (Qt `IrcComposerSurface`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Conversation,
    Status,
}

/// Where a verb may run (Qt `IrcVerbScope`, cut to the two scopes the six verbs use).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Only in a conversation. Plain text and `/me` have this scope.
    Conversation,
    /// On either surface.
    Either,
}

/// The six verbs the PoC supports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verb {
    Me,
    Clear,
    Help,
    Pref,
    Version,
    List,
}

/// One catalog row (Qt `IrcVerbSpec`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerbSpec {
    pub verb: Verb,
    pub name: &'static str,
    pub usage: &'static str,
    pub scope: Scope,
    /// The text for a wrong-scope or missing-target rejection. Empty means
    /// the default, `Select a connected conversation first`.
    pub wrong_scope_text: &'static str,
}

impl VerbSpec {
    /// Whether the verb may run on `surface` (`IrcVerbSpec::allowedOn`).
    pub fn allowed_on(&self, surface: Surface) -> bool {
        match self.scope {
            Scope::Conversation => surface == Surface::Conversation,
            Scope::Either => true,
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

/// Qt's default wrong-scope text, used for plain text on Status and for `/me` there.
pub const NO_CONVERSATION_TEXT: &str = "Select a connected conversation first";

/// Qt's text for a command that was understood but cannot run.
pub const COMMAND_REFUSED_TEXT: &str = "Command was refused";

/// The `/help` line. It lists the catalog names in this order.
pub const HELP_TEXT: &str = "Commands: /me, /clear, /help, /pref, /list, /version.";

/// The text a `/version` needs when it has no target.
const NAME_A_NICK: &str = "Name a nick";

/// The URL in the VERSION reply (`ctcpVersionReplyText`).
const VERSION_URL: &str = "https://omairc.app";

/// The catalog. Usage and scope are copied from `irccommand.cpp`.
pub static CATALOG: [VerbSpec; 6] = [
    VerbSpec {
        verb: Verb::Me,
        name: "me",
        usage: "/me <text>",
        scope: Scope::Conversation,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Clear,
        name: "clear",
        usage: "/clear",
        scope: Scope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Help,
        name: "help",
        usage: "/help",
        scope: Scope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Pref,
        name: "pref",
        usage: "/pref [directs|avatars|unread] [on|off] | joins [folded|every|hidden]",
        scope: Scope::Either,
        wrong_scope_text: "",
    },
    VerbSpec {
        verb: Verb::Version,
        name: "version",
        usage: "/version [nick]",
        scope: Scope::Either,
        wrong_scope_text: NAME_A_NICK,
    },
    VerbSpec {
        verb: Verb::List,
        name: "list",
        usage: "/list [mask]",
        scope: Scope::Either,
        wrong_scope_text: "",
    },
];

/// The catalog row for `verb`. Every verb has one.
pub fn spec_for(verb: Verb) -> &'static VerbSpec {
    CATALOG
        .iter()
        .find(|spec| spec.verb == verb)
        .expect("every verb has a catalog row")
}

/// The catalog row for a typed word, ignoring case (`IrcVerbTable::lookup`).
pub fn lookup(word: &str) -> Option<&'static VerbSpec> {
    let folded = word.to_lowercase();
    CATALOG.iter().find(|spec| spec.name == folded)
}

/// A parsed composer line (Qt `IrcCommand`, with the verb folded into the variant).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Blank input. There is nothing to send and nothing to report.
    Empty,
    /// Plain text to the conversation. `//x` also lands here, with the text `/x`.
    Say(String),
    /// `/me <text>`. The text is empty when none was typed.
    Action(String),
    Clear,
    Help,
    /// `/pref`, with its argument already parsed.
    Pref(PrefRequest),
    /// `/version [nick]`. The argument is the typed text, possibly empty.
    Version(String),
    /// `/list [mask]`. The mask is the typed text, possibly empty.
    List(String),
    /// A slash word outside the catalog, with its slash: `/nope`.
    Unknown(String),
}

impl Command {
    /// The verb this command runs. Plain text, blank input and unknown words have none.
    pub fn verb(&self) -> Option<Verb> {
        match self {
            Command::Empty | Command::Say(_) | Command::Unknown(_) => None,
            Command::Action(_) => Some(Verb::Me),
            Command::Clear => Some(Verb::Clear),
            Command::Help => Some(Verb::Help),
            Command::Pref(_) => Some(Verb::Pref),
            Command::Version(_) => Some(Verb::Version),
            Command::List(_) => Some(Verb::List),
        }
    }

    /// Why this command cannot run on `surface`, or `None` if it may run.
    /// Blank input always passes. Plain text is wrong scope on Status. An
    /// empty `/me` is refused, as `IrcSession::sendAction` refuses an empty body.
    pub fn rejection(&self, surface: Surface) -> Option<Rejection> {
        match self {
            Command::Empty => None,
            Command::Unknown(word) => Some(Rejection::Unknown(word.clone())),
            Command::Say(_) => (surface == Surface::Status)
                .then(|| Rejection::WrongScope(NO_CONVERSATION_TEXT.to_string())),
            other => {
                let spec = spec_for(other.verb()?);
                if !spec.allowed_on(surface) {
                    return Some(Rejection::WrongScope(
                        spec.wrong_scope_message().to_string(),
                    ));
                }
                matches!(other, Command::Action(text) if text.is_empty())
                    .then_some(Rejection::Refused)
            }
        }
    }
}

/// Why a command did not run. `text` is the line the shell shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rejection {
    /// A word outside the catalog: `Unknown command: /nope`.
    Unknown(String),
    /// The verb does not run here, or it has no target. Holds the text to show.
    WrongScope(String),
    /// The command was understood but cannot run, such as `/version a b`.
    Refused,
}

impl Rejection {
    /// The message, as `ircCommandOutcomeText` builds it.
    pub fn text(&self) -> String {
        match self {
            Rejection::Unknown(word) => format!("Unknown command: {word}"),
            Rejection::WrongScope(text) => text.clone(),
            Rejection::Refused => COMMAND_REFUSED_TEXT.to_string(),
        }
    }
}

/// Parses one composer line (`IrcCommand::parse`). The line is trimmed first.
///
/// - Blank input is `Empty`.
/// - Text without a leading `/` is `Say`.
/// - `//x` is `Say` with the text `/x`.
/// - `/word rest` looks `word` up in the catalog, ignoring case. A miss is
///   `Unknown`, and the word keeps its slash.
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

fn command_for(verb: Verb, argument: &str) -> Command {
    match verb {
        Verb::Me => Command::Action(argument.to_string()),
        Verb::Clear => Command::Clear,
        Verb::Help => Command::Help,
        Verb::Pref => Command::Pref(parse_pref_argument(argument)),
        Verb::Version => Command::Version(argument.to_string()),
        Verb::List => Command::List(argument.to_string()),
    }
}

/// The text of the `/help` line. It mirrors `HELP_TEXT` and the catalog names.
pub fn help_text() -> &'static str {
    HELP_TEXT
}

// ---------------------------------------------------------------------------
// /pref
// ---------------------------------------------------------------------------

/// What a `/pref` argument asks for (Qt `IrcPrefRequest`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrefRequest {
    /// Bare `/pref`: the four lines.
    QueryAll,
    /// One name: its line. `avatars` adds the avatar note.
    Query(PrefName),
    /// `<name> on|off`, or `joins <level>`.
    Set(PrefName, PrefValue),
    /// Anything else. The reply is the verb usage.
    Usage,
}

/// Parses the text after `/pref` (`ircParsePrefArgument`). Words split on
/// spaces only. More than two words, an unknown name, or a value that does
/// not fit the name gives `Usage`.
pub fn parse_pref_argument(argument: &str) -> PrefRequest {
    let words: Vec<&str> = argument
        .trim()
        .split(' ')
        .filter(|word| !word.is_empty())
        .collect();
    match words.as_slice() {
        [] => PrefRequest::QueryAll,
        [name] => match PrefName::from_token(name) {
            Some(name) => PrefRequest::Query(name),
            None => PrefRequest::Usage,
        },
        [name, value] => {
            let Some(name) = PrefName::from_token(name) else {
                return PrefRequest::Usage;
            };
            match PrefValue::parse(name, value) {
                Some(value) => PrefRequest::Set(name, value),
                None => PrefRequest::Usage,
            }
        }
        _ => PrefRequest::Usage,
    }
}

/// One reply line: `label: on` or `label: off`.
pub fn format_pref_state(name: PrefName, enabled: bool) -> String {
    let word = if enabled { "on" } else { "off" };
    format!("{}: {word}", name.label())
}

/// The reply line for the joins level: `Join, part, quit, and nick lines: folded`.
pub fn format_pref_noise(level: MembershipNoise) -> String {
    format!("{}: {}", PrefName::Joins.label(), level.token())
}

/// The reply to a single name. `avatars` adds the note about avatar hosts.
pub fn format_pref_query(name: PrefName, prefs: &Preferences) -> String {
    match name {
        PrefName::Joins => format_pref_noise(prefs.joins),
        PrefName::Avatars => format!(
            "{}\n{}",
            format_pref_state(name, prefs.enabled(name)),
            PREF_AVATAR_NOTE
        ),
        _ => format_pref_state(name, prefs.enabled(name)),
    }
}

/// The reply to bare `/pref`: one line per name, in catalog order.
pub fn format_pref_list(prefs: &Preferences) -> String {
    [
        format_pref_state(PrefName::Directs, prefs.directs),
        format_pref_state(PrefName::Avatars, prefs.avatars),
        format_pref_state(PrefName::Unread, prefs.unread),
        format_pref_noise(prefs.joins),
    ]
    .join("\n")
}

/// The note under `avatars` on a query (`ircPrefAvatarNote`).
pub const PREF_AVATAR_NOTE: &str =
    "Turn off to keep avatar hosts from seeing your IP on busy channels.";

/// The result of one `/pref` command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrefReply {
    /// The lines to show, joined with newlines.
    pub text: String,
    /// True when the command set a value. The shell then saves the preferences.
    pub changed: bool,
}

/// Runs a parsed `/pref` against `prefs` and returns the reply
/// (`IrcCommandDispatcher::dispatchPref`). A set writes into `prefs` at once.
/// The caller saves when `changed` is true.
pub fn pref_reply(request: &PrefRequest, prefs: &mut Preferences) -> PrefReply {
    let plain = |text: String| PrefReply {
        text,
        changed: false,
    };
    match request {
        PrefRequest::Usage => plain(spec_for(Verb::Pref).usage.to_string()),
        PrefRequest::QueryAll => plain(format_pref_list(prefs)),
        PrefRequest::Query(name) => plain(format_pref_query(*name, prefs)),
        PrefRequest::Set(name, value) => {
            prefs.set(*name, *value);
            let text = match name {
                PrefName::Joins => format_pref_noise(prefs.joins),
                _ => format_pref_state(*name, prefs.enabled(*name)),
            };
            PrefReply {
                text,
                changed: true,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// /version
// ---------------------------------------------------------------------------

/// What the selected view is, for `/version` with no nick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selected<'a> {
    /// A direct message with this peer. An empty `/version` asks the peer.
    Direct(&'a str),
    /// A channel. An empty `/version` needs a nick.
    Channel,
}

/// A `/version` that may be sent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionRequest {
    /// The nick to ask.
    pub nick: String,
    /// True when the reply is a whois-style row in the asking conversation.
    /// On Status the reply stays on Status.
    pub in_conversation: bool,
}

/// Picks the target for `/version` (`IrcReplyRouter::dispatchCtcp`).
///
/// - The first word is the nick. Anything after it refuses the command.
/// - With no nick: a DM uses its peer, a channel needs a nick (`Name a nick`),
///   and no selection refuses.
/// - A channel name is refused. `chantypes` lists the network's channel prefix
///   characters, such as `#&`.
/// - With a nick but no selection on a conversation, the command is wrong scope.
pub fn version_request(
    argument: &str,
    surface: Surface,
    selected: Option<Selected>,
    chantypes: &str,
) -> Result<VersionRequest, Rejection> {
    let (nick, rest) = first_token(argument);
    if !rest.is_empty() {
        return Err(Rejection::Refused);
    }
    let in_conversation = surface == Surface::Conversation;

    if nick.is_empty() {
        return match selected {
            Some(Selected::Direct(peer)) => Ok(VersionRequest {
                nick: peer.to_string(),
                in_conversation,
            }),
            Some(Selected::Channel) => Err(Rejection::WrongScope(NAME_A_NICK.to_string())),
            None => Err(Rejection::Refused),
        };
    }
    if in_conversation && selected.is_none() {
        return Err(Rejection::WrongScope(NAME_A_NICK.to_string()));
    }
    if nick.starts_with(|ch: char| chantypes.contains(ch)) {
        return Err(Rejection::Refused);
    }
    Ok(VersionRequest {
        nick: nick.to_string(),
        in_conversation,
    })
}

/// The VERSION reply text, `VERSION reply from <nick>: https://omairc.app`
/// (`formatCtcpReplyText` with the VERSION payload).
pub fn version_reply_text(nick: &str) -> String {
    format!("VERSION reply from {nick}: {VERSION_URL}")
}

/// The first space-separated word, and the rest after it with spaces trimmed.
/// Mirrors `firstToken` and `restAfterFirstToken`.
fn first_token(argument: &str) -> (&str, &str) {
    match argument.find(' ') {
        Some(space) => (&argument[..space], argument[space + 1..].trim()),
        None => (argument, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHANTYPES: &str = "#&";

    fn say(text: &str) -> Command {
        Command::Say(text.to_string())
    }

    #[test]
    fn plain_text_is_a_say_and_blank_input_is_empty() {
        assert_eq!(parse("  hello there  "), say("hello there"));
        assert_eq!(parse(""), Command::Empty);
        assert_eq!(parse("   \t "), Command::Empty);
    }

    #[test]
    fn a_double_slash_sends_the_single_slash_text() {
        assert_eq!(parse("//me waves"), say("/me waves"));
        assert_eq!(parse("//"), say("/"));
        assert_eq!(parse("  //clear"), say("/clear"));
    }

    #[test]
    fn verbs_parse_ignoring_case_with_their_argument() {
        assert_eq!(parse("/me waves  "), Command::Action("waves".to_string()));
        assert_eq!(
            parse("/ME   spaced out "),
            Command::Action("spaced out".to_string())
        );
        assert_eq!(parse("/me"), Command::Action(String::new()));
        assert_eq!(parse("/CLEAR"), Command::Clear);
        assert_eq!(parse("/help"), Command::Help);
        assert_eq!(parse("/version anna"), Command::Version("anna".to_string()));
        assert_eq!(parse("/Version"), Command::Version(String::new()));
        assert_eq!(parse("/list #linux"), Command::List("#linux".to_string()));
        assert_eq!(parse("/list"), Command::List(String::new()));
    }

    #[test]
    fn clear_and_help_ignore_any_argument() {
        assert_eq!(parse("/clear now"), Command::Clear);
        assert_eq!(parse("/help me"), Command::Help);
    }

    #[test]
    fn pref_parses_its_argument() {
        assert_eq!(parse("/pref"), Command::Pref(PrefRequest::QueryAll));
        assert_eq!(
            parse("/pref avatars off"),
            Command::Pref(PrefRequest::Set(
                PrefName::Avatars,
                PrefValue::Toggle(false)
            ))
        );
        assert_eq!(
            parse("/pref joins hidden"),
            Command::Pref(PrefRequest::Set(
                PrefName::Joins,
                PrefValue::Noise(MembershipNoise::Hidden)
            ))
        );
    }

    #[test]
    fn an_unknown_word_keeps_its_slash_and_its_case() {
        assert_eq!(parse("/Nope now"), Command::Unknown("/Nope".to_string()));
        assert_eq!(parse("/"), Command::Unknown("/".to_string()));
        assert_eq!(parse("/join #linux"), Command::Unknown("/join".to_string()));
        assert_eq!(
            parse("/me\ttext"),
            Command::Unknown("/me\ttext".to_string())
        );
    }

    #[test]
    fn unknown_words_are_rejected_on_both_surfaces() {
        let command = parse("/nope");
        for surface in [Surface::Conversation, Surface::Status] {
            assert_eq!(
                command.rejection(surface).map(|r| r.text()),
                Some("Unknown command: /nope".to_string())
            );
        }
    }

    #[test]
    fn plain_text_is_wrong_scope_on_status_only() {
        assert_eq!(parse("hello").rejection(Surface::Conversation), None);
        assert_eq!(
            parse("hello").rejection(Surface::Status).map(|r| r.text()),
            Some("Select a connected conversation first".to_string())
        );
        assert_eq!(parse("//me").rejection(Surface::Conversation), None);
    }

    #[test]
    fn me_is_conversation_scoped_and_the_rest_run_anywhere() {
        let me = parse("/me hi");
        assert_eq!(me.rejection(Surface::Conversation), None);
        assert_eq!(
            me.rejection(Surface::Status).map(|r| r.text()),
            Some("Select a connected conversation first".to_string())
        );
        for line in ["/clear", "/help", "/pref", "/version", "/list"] {
            for surface in [Surface::Conversation, Surface::Status] {
                assert_eq!(
                    parse(line).rejection(surface),
                    None,
                    "{line} on {surface:?}"
                );
            }
        }
    }

    #[test]
    fn an_empty_me_is_refused_on_a_conversation() {
        assert_eq!(
            parse("/me").rejection(Surface::Conversation),
            Some(Rejection::Refused)
        );
        assert_eq!(
            parse("/me")
                .rejection(Surface::Conversation)
                .map(|r| r.text()),
            Some("Command was refused".to_string())
        );
        assert_eq!(parse("/me hi").rejection(Surface::Conversation), None);
    }

    #[test]
    fn blank_input_is_never_rejected() {
        assert_eq!(parse("").rejection(Surface::Status), None);
        assert_eq!(parse("   ").rejection(Surface::Conversation), None);
    }

    #[test]
    fn the_catalog_matches_the_spec_table() {
        let rows: Vec<(&str, &str, Scope)> = CATALOG
            .iter()
            .map(|spec| (spec.name, spec.usage, spec.scope))
            .collect();
        assert_eq!(
            rows,
            vec![
                ("me", "/me <text>", Scope::Conversation),
                ("clear", "/clear", Scope::Either),
                ("help", "/help", Scope::Either),
                (
                    "pref",
                    "/pref [directs|avatars|unread] [on|off] | joins [folded|every|hidden]",
                    Scope::Either
                ),
                ("version", "/version [nick]", Scope::Either),
                ("list", "/list [mask]", Scope::Either),
            ]
        );
        assert_eq!(spec_for(Verb::Version).wrong_scope_message(), "Name a nick");
        assert_eq!(
            spec_for(Verb::Clear).wrong_scope_message(),
            "Select a connected conversation first"
        );
    }

    #[test]
    fn the_help_line_names_every_verb_in_the_spec_order() {
        assert_eq!(
            help_text(),
            "Commands: /me, /clear, /help, /pref, /list, /version."
        );
        for spec in CATALOG.iter() {
            assert!(help_text().contains(&format!("/{}", spec.name)));
        }
    }

    #[test]
    fn pref_argument_shapes() {
        assert_eq!(parse_pref_argument(""), PrefRequest::QueryAll);
        assert_eq!(parse_pref_argument("   "), PrefRequest::QueryAll);
        assert_eq!(
            parse_pref_argument("directs"),
            PrefRequest::Query(PrefName::Directs)
        );
        assert_eq!(
            parse_pref_argument("AVATARS"),
            PrefRequest::Query(PrefName::Avatars)
        );
        assert_eq!(
            parse_pref_argument("joins"),
            PrefRequest::Query(PrefName::Joins)
        );
        assert_eq!(
            parse_pref_argument("unread on"),
            PrefRequest::Set(PrefName::Unread, PrefValue::Toggle(true))
        );
        assert_eq!(
            parse_pref_argument("avatars   off"),
            PrefRequest::Set(PrefName::Avatars, PrefValue::Toggle(false))
        );
        assert_eq!(
            parse_pref_argument("joins Every"),
            PrefRequest::Set(PrefName::Joins, PrefValue::Noise(MembershipNoise::Every))
        );
    }

    #[test]
    fn bad_pref_arguments_are_usage() {
        assert_eq!(parse_pref_argument("directs maybe"), PrefRequest::Usage);
        assert_eq!(parse_pref_argument("joins sometimes"), PrefRequest::Usage);
        assert_eq!(parse_pref_argument("joins on"), PrefRequest::Usage);
        assert_eq!(parse_pref_argument("notify on"), PrefRequest::Usage);
        assert_eq!(parse_pref_argument("notify"), PrefRequest::Usage);
        assert_eq!(parse_pref_argument("avatars on now"), PrefRequest::Usage);
    }

    #[test]
    fn bare_pref_prints_four_lines() {
        let mut prefs = Preferences::default();
        let reply = pref_reply(&PrefRequest::QueryAll, &mut prefs);
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
            pref_reply(&PrefRequest::Query(PrefName::Directs), &mut prefs).text,
            "Reopen direct messages on startup: on"
        );
        assert_eq!(
            pref_reply(&PrefRequest::Query(PrefName::Avatars), &mut prefs).text,
            "Show peer avatars: on\n\
             Turn off to keep avatar hosts from seeing your IP on busy channels."
        );
        assert_eq!(
            pref_reply(&PrefRequest::Query(PrefName::Joins), &mut prefs).text,
            "Join, part, quit, and nick lines: folded"
        );
    }

    #[test]
    fn a_set_writes_through_and_echoes_one_state_line() {
        let mut prefs = Preferences::default();
        let reply = pref_reply(
            &PrefRequest::Set(PrefName::Avatars, PrefValue::Toggle(false)),
            &mut prefs,
        );
        assert_eq!(reply.text, "Show peer avatars: off");
        assert!(reply.changed);
        assert!(!prefs.avatars);

        let reply = pref_reply(
            &PrefRequest::Set(PrefName::Joins, PrefValue::Noise(MembershipNoise::Every)),
            &mut prefs,
        );
        assert_eq!(reply.text, "Join, part, quit, and nick lines: every");
        assert_eq!(prefs.joins, MembershipNoise::Every);
    }

    #[test]
    fn usage_echoes_the_verb_usage_and_changes_nothing() {
        let mut prefs = Preferences::default();
        let reply = pref_reply(&PrefRequest::Usage, &mut prefs);
        assert_eq!(
            reply.text,
            "/pref [directs|avatars|unread] [on|off] | joins [folded|every|hidden]"
        );
        assert!(!reply.changed);
        assert_eq!(prefs, Preferences::default());
    }

    #[test]
    fn version_on_a_direct_message_asks_the_peer() {
        let request = version_request(
            "",
            Surface::Conversation,
            Some(Selected::Direct("anna")),
            CHANTYPES,
        )
        .expect("a DM has a peer");
        assert_eq!(
            request,
            VersionRequest {
                nick: "anna".to_string(),
                in_conversation: true,
            }
        );
    }

    #[test]
    fn version_from_status_stays_on_status() {
        let request = version_request(
            "",
            Surface::Status,
            Some(Selected::Direct("anna")),
            CHANTYPES,
        )
        .expect("the selected DM is the target");
        assert!(!request.in_conversation);
        let named = version_request("bob", Surface::Status, None, CHANTYPES);
        assert_eq!(
            named,
            Ok(VersionRequest {
                nick: "bob".to_string(),
                in_conversation: false,
            })
        );
    }

    #[test]
    fn version_on_a_channel_needs_a_nick() {
        assert_eq!(
            version_request(
                "",
                Surface::Conversation,
                Some(Selected::Channel),
                CHANTYPES
            ),
            Err(Rejection::WrongScope("Name a nick".to_string()))
        );
        assert_eq!(
            version_request("", Surface::Status, Some(Selected::Channel), CHANTYPES)
                .map_err(|r| r.text()),
            Err("Name a nick".to_string())
        );
    }

    #[test]
    fn version_with_nothing_selected_is_refused_when_empty() {
        assert_eq!(
            version_request("", Surface::Conversation, None, CHANTYPES),
            Err(Rejection::Refused)
        );
    }

    #[test]
    fn a_named_nick_is_asked_in_the_selected_conversation() {
        let request = version_request(
            "bob",
            Surface::Conversation,
            Some(Selected::Channel),
            CHANTYPES,
        )
        .expect("a nick is a valid target");
        assert_eq!(request.nick, "bob");
        assert!(request.in_conversation);
    }

    #[test]
    fn a_channel_target_is_refused() {
        assert_eq!(
            version_request(
                "#linux",
                Surface::Conversation,
                Some(Selected::Direct("anna")),
                CHANTYPES
            ),
            Err(Rejection::Refused)
        );
        assert_eq!(
            version_request("&local", Surface::Status, None, CHANTYPES),
            Err(Rejection::Refused)
        );
    }

    #[test]
    fn extra_words_refuse_the_command() {
        assert_eq!(
            version_request(
                "anna bob",
                Surface::Conversation,
                Some(Selected::Channel),
                CHANTYPES
            ),
            Err(Rejection::Refused)
        );
        assert_eq!(
            version_request(
                "anna  ",
                Surface::Conversation,
                Some(Selected::Channel),
                CHANTYPES
            )
            .map(|r| r.nick),
            Ok("anna".to_string())
        );
    }

    #[test]
    fn a_nick_with_no_selection_on_a_conversation_is_wrong_scope() {
        assert_eq!(
            version_request("anna", Surface::Conversation, None, CHANTYPES),
            Err(Rejection::WrongScope("Name a nick".to_string()))
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
    fn refused_and_unknown_texts() {
        assert_eq!(Rejection::Refused.text(), "Command was refused");
        assert_eq!(
            Rejection::WrongScope("Name a nick".to_string()).text(),
            "Name a nick"
        );
    }
}
