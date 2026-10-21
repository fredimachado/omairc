//! Slash commands for the composer. The catalog, the parser, the refusal texts and the
//! `/pref`, `/version` and `/help` replies live in `irc/command` (Go
//! `tui/internal/irc/command.go`, C++ `src/irc/irccommand.cpp`). This module re-exports
//! the names the composer shell uses, so the shell reads one path. The completion rows
//! are in `irc/slashcomplete`, and the popup's state is in `model/slash_complete`.

pub use crate::irc::command::ComposerSurface as Surface;
pub use crate::irc::command::{
    command_outcome_text, ctcp_target, help_text, parse, pref_reply, version_reply_text, Command,
    CommandOutcome, SelectedView, Words,
};
pub use crate::irc::pref::PrefRequest;
