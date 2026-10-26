//! The refusal and outcome texts a command reports back.

use super::*;

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
