//! The history search sheet over the seeded demo: open it with the chord, type, walk and
//! Enter, and the right conversation shows with the line in view. A log on disk answers
//! for lines the loaded transcript no longer holds.

use std::time::Duration;

use crate::irc::event::ConversationKey;
use crate::irc::reducer::{ConversationLog, TranscriptLine};
use crate::irc::casemapping::IrcCaseMapping;
use crate::model::search::{JumpTarget, SearchHit, OLDER_THAN_LOADED_NOTE};
use crate::storage::conversationlog::FileConversationLog;
use crate::ui::sheets::search::SearchSource;
use crate::util::testdir::TempDir;

use super::fixture::{SeededShell, OMARCHY};

const DEBOUNCE_AND_A_BIT: Duration = Duration::from_millis(200);

fn open_search(ui: &mut SeededShell) {
    ui.keys("ctrl-shift-f");
    assert!(search_open(ui), "Ctrl+Shift+F opens the search sheet");
}

fn search_open(ui: &SeededShell) -> bool {
    ui.shell.read_with(ui.cx, |shell, _| {
        matches!(
            shell.modal.as_ref().map(|modal| modal.sheet()),
            Some(crate::model::chords::Sheet::Search)
        )
    })
}

/// Types into the open sheet, then lets the debounce and the scan run.
fn type_query(ui: &mut SeededShell, text: &str) {
    let keys: Vec<String> = text
        .chars()
        .map(|ch| if ch == ' ' { "space".to_owned() } else { ch.to_string() })
        .collect();
    ui.keys(&keys.join(" "));
    settle_search(ui);
}

fn settle_search(ui: &mut SeededShell) {
    ui.cx.executor().advance_clock(DEBOUNCE_AND_A_BIT);
    ui.settle();
}

fn source(ui: &SeededShell) -> SearchSource {
    ui.shell.read_with(ui.cx, |shell, cx| match &shell.modal {
        Some(crate::ui::shell::modal::ActiveModal::Search { sheet, .. }) => {
            sheet.read(cx).source.clone()
        }
        _ => panic!("the search sheet is not open"),
    })
}

fn hits(ui: &SeededShell) -> Vec<SearchHit> {
    source(ui).results.hits
}

fn tagged(msgid: &str, nick: &str, target: &str, body: &str) -> String {
    format!("@msgid={msgid} :{nick}!u@h PRIVMSG {target} :{body}")
}

/// A line to find in `#help` and one in `#omarchy`, each with lines after it so it is not at
/// the end of its transcript.
fn seed_two_conversations(ui: &mut SeededShell) {
    ui.select("#help");
    ui.inject(OMARCHY, &tagged("help-zebra", "kai", "#help", "a zebra in the help room"));
    fill_below(ui, "kai", "#help");
    ui.select("#omarchy");
    ui.inject(OMARCHY, &tagged("omarchy-zebra", "dax", "#omarchy", "Zebra crossing at the omarchy desk"));
    fill_below(ui, "dax", "#omarchy");
}

/// Enough lines after the one to find that the view can sit on it with more below.
fn fill_below(ui: &mut SeededShell, nick: &str, target: &str) {
    for index in 0..60 {
        ui.inject_chat(nick, target, &format!("filler {index}"));
    }
}

#[gpui::test]
fn open_type_down_enter_lands_on_the_second_hit_in_its_conversation(cx: &mut gpui::TestAppContext) {
    let mut ui = super::fixture::open(cx);
    seed_two_conversations(&mut ui);
    ui.select("#omarchy");

    open_search(&mut ui);
    type_query(&mut ui, "zebra");
    let found = hits(&ui);
    assert_eq!(found.len(), 2, "one zebra in each conversation: {found:?}");
    assert!(!ui.composer_focused(), "the sheet holds the keyboard");
    let second = found[1].clone();

    ui.keys("down");
    ui.keys("enter");

    assert!(!search_open(&ui), "Enter closes the sheet");
    assert_eq!(
        ConversationKey::new(OMARCHY, ui.selected_key()),
        second.conversation,
        "the second hit's conversation is selected"
    );
    assert!(ui.composer_focused(), "focus returns to the composer");
    let JumpTarget::Msgid(msgid) = &second.target else {
        panic!("the injected line carries a msgid");
    };
    ui.shell.read_with(ui.cx, |shell, cx| {
        let transcript = shell.transcript.read(cx);
        let row = crate::model::transcript::row_for_msgid(transcript.rows(), msgid)
            .expect("the target line is in the transcript");
        let top = transcript.logical_scroll_top().item_ix;
        assert!(top <= row, "the target row {row} is not above the view top {top} of {} rows, at_end {}", transcript.rows().len(), transcript.at_end());
        assert!(row - top < 4, "the target row is at the top of the view, not scrolled past");
        assert!(!transcript.at_end(), "the reader is on the line, not at the end");
        assert!(shell.search_note.is_none());
    });
}

#[gpui::test]
fn escape_closes_the_sheet_and_returns_focus_to_the_composer(cx: &mut gpui::TestAppContext) {
    let mut ui = super::fixture::open(cx);
    open_search(&mut ui);
    type_query(&mut ui, "zeb");
    ui.keys("escape");
    assert!(!search_open(&ui));
    assert!(ui.composer_focused());
}

#[gpui::test]
fn the_chord_toggles_the_sheet_and_connect_blocks_it(cx: &mut gpui::TestAppContext) {
    let mut ui = super::fixture::open(cx);
    open_search(&mut ui);
    ui.keys("ctrl-shift-f");
    assert!(!search_open(&ui), "the chord closes the sheet it opened");

    ui.cx.update(|window, cx| {
        ui.shell.update(cx, |shell, cx| shell.open_connect(window, cx));
    });
    ui.settle();
    ui.keys("ctrl-shift-f");
    assert!(!search_open(&ui), "Connect holds the keyboard");
}

#[gpui::test]
fn a_blank_field_shows_only_the_hint(cx: &mut gpui::TestAppContext) {
    let mut ui = super::fixture::open(cx);
    open_search(&mut ui);
    let blank = source(&ui);
    assert!(blank.query_blank && blank.results.hits.is_empty());
    type_query(&mut ui, "   ");
    assert!(source(&ui).results.hits.is_empty());
}

#[gpui::test]
fn the_author_and_the_plain_text_match(cx: &mut gpui::TestAppContext) {
    let mut ui = super::fixture::open(cx);
    ui.select("#help");
    ui.inject(OMARCHY, &tagged("bold-1", "kai", "#help", "\u{2}bold\u{2} words"));
    open_search(&mut ui);
    type_query(&mut ui, "bold words");
    let found = hits(&ui);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].text, "bold words");
}

#[gpui::test]
fn a_newer_query_never_shows_the_older_ones_hits(cx: &mut gpui::TestAppContext) {
    let mut ui = super::fixture::open(cx);
    seed_two_conversations(&mut ui);
    open_search(&mut ui);
    type_query(&mut ui, "zebra");
    assert_eq!(hits(&ui).len(), 2);

    // Typing again restarts the search: nothing for the old text is painted once the
    // debounce for the new text has run.
    type_query(&mut ui, " crossing");
    let found = hits(&ui);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].text.contains("crossing"));

    // Keys typed inside the pause leave the old results until the new search answers.
    ui.keys("x");
    settle_search(&mut ui);
    assert!(hits(&ui).is_empty());
}

#[gpui::test]
fn a_line_older_than_the_loaded_transcript_opens_the_conversation_with_a_note(
    cx: &mut gpui::TestAppContext,
) {
    let dir = TempDir::new("search-ui-disk");
    let mut ui = super::fixture::open(cx);
    // `#help` keeps a log with a line from long before anything the demo loaded.
    ui.shell.update(ui.cx, |shell, _| shell.controller.set_transcript_root(dir.path()));
    let log = FileConversationLog::new(dir.path().to_path_buf());
    assert!(log.append(
        OMARCHY,
        "#help",
        IrcCaseMapping::default(),
        &TranscriptLine {
            timestamp: chrono::DateTime::from_timestamp(1_000_000_000, 0).unwrap(),
            author: "old-timer".to_owned(),
            kind: "message".to_owned(),
            body: "the walrus was here first".to_owned(),
            msgid: "old-1".to_owned(),
            only_visible_to_you: false,
        },
    ));
    ui.select("#omarchy");

    open_search(&mut ui);
    type_query(&mut ui, "walrus");
    let found = hits(&ui);
    assert_eq!(found.len(), 1, "the disk answered: {found:?}");
    assert_eq!(found[0].conversation.normalized_target, "#help");
    assert!(!source(&ui).scanning, "the scan finished");

    ui.keys("enter");
    assert_eq!(ui.selected_key(), "#help", "the conversation opens");
    ui.shell.read_with(ui.cx, |shell, _| {
        assert_eq!(shell.search_note, Some(OLDER_THAN_LOADED_NOTE));
    });
    assert!(search_open(&ui) == false);

    // Leaving the conversation clears the note.
    ui.select("#omarchy");
    ui.shell.read_with(ui.cx, |shell, _| assert!(shell.search_note.is_none()));
}

#[gpui::test]
fn a_credential_in_a_log_is_never_listed(cx: &mut gpui::TestAppContext) {
    let dir = TempDir::new("search-ui-secret");
    let mut ui = super::fixture::open(cx);
    ui.shell.update(ui.cx, |shell, _| shell.controller.set_transcript_root(dir.path()));
    let path = FileConversationLog::new(dir.path().to_path_buf()).path_for(
        OMARCHY,
        "#help",
        IrcCaseMapping::default(),
    );
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let line = |body: &str| {
        serde_json::json!({"timestamp": "2001-09-09T01:46:40.000Z", "author": "fred",
            "kind": "message", "body": body})
        .to_string()
    };
    std::fs::write(
        &path,
        format!(
            "{}\n{}\n",
            line("PRIVMSG NickServ :IDENTIFY hunter2"),
            line("hunter2 is a fine joke")
        ),
    )
    .unwrap();
    open_search(&mut ui);
    type_query(&mut ui, "hunter2");
    let texts: Vec<String> = hits(&ui).into_iter().map(|hit| hit.text).collect();
    assert_eq!(texts, ["hunter2 is a fine joke"]);
}

/// Thirty lines with "zebra" in `#help`, and one in `#omarchy`, which sorts last. A
/// third conversation is on screen, so Enter on the last hit visibly moves.
fn seed_many_zebras(ui: &mut SeededShell) {
    ui.select("#help");
    for index in 0..30 {
        ui.inject(
            OMARCHY,
            &tagged(&format!("zh-{index}"), "kai", "#help", &format!("zebra number {index}")),
        );
    }
    ui.select("#omarchy");
    ui.inject(OMARCHY, &tagged("zlast", "dax", "#omarchy", "zebra in the omarchy room"));
    ui.select("#ricing");
}

fn selected_hit(ui: &SeededShell) -> usize {
    ui.shell.read_with(ui.cx, |shell, cx| match &shell.modal {
        Some(crate::ui::shell::modal::ActiveModal::Search { sheet, .. }) => sheet.read(cx).selected,
        _ => panic!("the search sheet is not open"),
    })
}

#[gpui::test]
fn end_selects_the_last_hit_and_home_the_first_without_moving_the_caret(cx: &mut gpui::TestAppContext) {
    let mut ui = super::fixture::open(cx);
    seed_many_zebras(&mut ui);
    open_search(&mut ui);
    type_query(&mut ui, "zebra");
    let count = hits(&ui).len();
    assert_eq!(count, 31);
    assert_eq!(selected_hit(&ui), 0);

    ui.keys("end");
    assert_eq!(selected_hit(&ui), count - 1, "End selects the last hit");
    ui.keys("home");
    assert_eq!(selected_hit(&ui), 0, "Home selects the first hit");

    // The field keeps its caret keys: Left then typing inserts before the last char.
    ui.keys("left x");
    settle_search(&mut ui);
    assert!(hits(&ui).is_empty(), "the field now reads `zebrxa`");
}

#[gpui::test]
fn enter_after_end_opens_the_last_hits_conversation(cx: &mut gpui::TestAppContext) {
    let mut ui = super::fixture::open(cx);
    seed_many_zebras(&mut ui);
    open_search(&mut ui);
    type_query(&mut ui, "zebra");
    let last = hits(&ui).pop().expect("hits");
    assert_eq!(last.conversation.normalized_target, "#omarchy");

    ui.keys("end");
    ui.keys("enter");

    assert!(!search_open(&ui));
    assert_eq!(ConversationKey::new(OMARCHY, ui.selected_key()), last.conversation);
}
