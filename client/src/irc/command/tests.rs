//! Fixtures and the command's tests. The tests are grouped by what they pin, one
//! child module each under `tests/`, and the fixtures here are shared by all of them.

use super::verbs::{verb_table, VerbScope};
use super::*;
use crate::irc::membershipnoise::MembershipNoise;
use crate::irc::pref::PrefValueKind;
use std::collections::HashSet;

mod catalog;
mod ctcp;
mod outcome;
mod parse;
mod pref;

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

// Ported from tst_command.cpp (`parseVerbsAndAliases` through `parseWrappersAndHelp`).

// The catalog's usage, scope and wrong-scope rows from tst_command.cpp
// (`catalogLookupAndScope`).

// Feature map `.cursor/skills/verify-omairc/features/slash-commands.md`, catalog level.

// Ported from the Qt pref and the reply tests in `client/src/model/slash.rs`.

fn pref(line: &str) -> PrefRequest {
    match parse(line) {
        Command::Pref(request) => request,
        other => panic!("{line} parsed to {other:?}"),
    }
}

// Ported from the CTCP target tests in `client/src/model/slash.rs` (`version_request`).

const CHANTYPES: &str = "#&";
