//! The slash catalog: one row per verb, with its names, aliases, usage line, scope and
//! wrong-scope text. `VERB_TABLE` is the ordered catalog. Its order is the completion
//! ranking's tie order and the `/help` order, so its rows are never re-sorted. The
//! lookups below read that table and nothing else.

use super::{ComposerSurface, NO_CONVERSATION_TEXT};

/// The surfaces a verb may run on (Go `VerbScope`, C++ `IrcVerbScope`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerbScope {
    /// Only in a conversation.
    Conversation,
    /// Only on Status.
    #[cfg(test)]
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

/// One catalog row (Go `VerbSpec`, C++ `IrcVerbSpec`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerbSpec {
    pub verb: Verb,
    pub name: &'static str,
    /// Alternate names that resolve to this verb, without the slash.
    pub aliases: &'static [&'static str],
    /// The usage line the completion popup shows beside the name.
    pub usage: &'static str,
    pub scope: VerbScope,
    /// The refusal text for a wrong scope or a missing target. Empty means
    pub wrong_scope_text: &'static str,
}

impl VerbSpec {
    /// Whether the verb may run on `surface` (`IrcVerbSpec::allowedOn`).
    pub fn allowed_on(&self, surface: ComposerSurface) -> bool {
        match self.scope {
            VerbScope::Conversation => surface == ComposerSurface::Conversation,
            #[cfg(test)]
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
        usage: "/pref [directs|avatars|unread|colors] [on|off] | joins [folded|every|hidden]",
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
#[cfg(test)]
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
