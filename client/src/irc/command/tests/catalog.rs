//! The slash catalog: its shape, lookups, scopes, /help, and the parse round trip for every row.

use super::*;

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
    // me (Action) is conversation-only.
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
            usage: "/pref [directs|avatars|unread|colors] [on|off] | joins [folded|every|hidden]",
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
