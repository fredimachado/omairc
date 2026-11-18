//! Shell-level proofs for composer drafts that survive a restart. A restart is a second shell
//! on a fresh controller whose draft store reads the file the first one wrote. The state is
//! the demo's, so each shell is pointed at a temp `drafts.json` by hand. The Go equivalent is
//! `tui/internal/ui/drafts_test.go`.

use gpui::{AppContext as _, Entity, VisualTestContext, WindowHandle, WindowOptions};

use super::fixture::{self, SeededShell, OMARCHY};
use crate::model::ConversationRef;
use crate::storage::drafts::DraftFile;
use crate::storage::paths::DRAFTS_FILE;
use crate::storage::Backing;
use crate::ui::shell::Shell;
use crate::util::testdir::TempDir;

/// The first shell, writing its drafts to `drafts.json` in `dir`.
fn on_disk<'a>(cx: &'a mut gpui::TestAppContext, dir: &TempDir) -> SeededShell<'a> {
    let ui = fixture::open(cx);
    use_file(&ui.shell, ui.cx, dir);
    ui
}

fn use_file(shell: &Entity<Shell>, cx: &mut VisualTestContext, dir: &TempDir) {
    let file = DraftFile::new(Backing::Disk(dir.path().join(DRAFTS_FILE)));
    shell.update(cx, |shell, _| shell.controller.use_draft_file(file));
}

/// A new shell, as a relaunch builds it, reading what `dir` holds. `Shell::new` restores the
/// draft of the view it opens on, but only from the real state file, so the test points the
/// store at `dir` and then does that step itself.
fn relaunch(ui: &mut SeededShell, dir: &TempDir) -> WindowHandle<Shell> {
    let (activation_tx, _) = async_channel::unbounded();
    let file = DraftFile::new(Backing::Disk(dir.path().join(DRAFTS_FILE)));
    ui.cx.update(|_, app| {
        let second = app
            .open_window(WindowOptions::default(), |_, cx| {
                cx.new(|cx| Shell::new(cx, false, true, None, activation_tx))
            })
            .expect("a second window opens");
        second
            .update(app, |shell, _, cx| {
                shell.controller.use_draft_file(file);
                shell.restore_shown_draft(cx);
            })
            .expect("the second shell is alive");
        second
    })
}

fn type_text(ui: &mut SeededShell, text: &str) {
    ui.shell.update(ui.cx, |shell, cx| {
        shell.composer.update(cx, |c, cx| {
            c.set_text(text.to_owned());
            cx.notify();
        });
    });
    ui.settle();
}

/// The field's text and caret.
fn field(shell: &Entity<Shell>, ui: &mut SeededShell) -> (String, usize) {
    shell.read_with(ui.cx, |shell, cx| field_of(shell, cx))
}

fn field_of(shell: &Shell, cx: &gpui::App) -> (String, usize) {
    let field = shell.composer.read(cx);
    (field.text().to_owned(), field.cursor())
}

fn file_text(dir: &TempDir) -> String {
    std::fs::read_to_string(dir.path().join(DRAFTS_FILE)).unwrap_or_default()
}

#[gpui::test]
fn a_draft_left_in_a_channel_is_back_after_a_restart_with_the_caret_at_its_end(
    cx: &mut gpui::TestAppContext,
) {
    let dir = TempDir::new("ui-drafts-restart");
    let mut ui = on_disk(cx, &dir);
    ui.select("#omarchy");
    type_text(&mut ui, "half typed");
    ui.select("#help");
    assert!(file_text(&dir).contains("half typed"), "{}", file_text(&dir));

    let second = relaunch(&mut ui, &dir);
    let shown = second
        .update(ui.cx, |shell, _, cx| {
            // The demo opens on #omarchy, so leave and come back as a user would.
            shell.select(ConversationRef::new(OMARCHY, "#help"), cx);
            shell.select(ConversationRef::new(OMARCHY, "#omarchy"), cx);
            field_of(shell, cx)
        })
        .unwrap();
    assert_eq!(shown, ("half typed".to_owned(), "half typed".len()));
    assert!(
        file_text(&dir).contains("half typed"),
        "leaving a restored draft untouched must not drop it"
    );
}

#[gpui::test]
fn a_launch_that_opens_on_the_conversation_shows_its_draft(cx: &mut gpui::TestAppContext) {
    let dir = TempDir::new("ui-drafts-launch");
    let mut ui = on_disk(cx, &dir);
    let opened = ui.selected_key();
    type_text(&mut ui, "carried over");
    ui.select("anna");

    let second = relaunch(&mut ui, &dir);
    let (key, shown) = second
        .update(ui.cx, |shell, _, cx| {
            (shell.selected_ref().key, field_of(shell, cx).0)
        })
        .unwrap();
    assert_eq!(key, opened, "the demo opens where the first shell did");
    assert_eq!(shown, "carried over");
}

#[gpui::test]
fn a_secret_left_unsent_is_never_written_and_an_earlier_draft_goes(cx: &mut gpui::TestAppContext) {
    let dir = TempDir::new("ui-drafts-secret");
    let mut ui = on_disk(cx, &dir);
    ui.select("#omarchy");
    type_text(&mut ui, "half typed");
    ui.select("#help");
    ui.select("#omarchy");
    type_text(&mut ui, "/msg NickServ IDENTIFY pw");
    ui.select("#help");
    let text = file_text(&dir);
    assert!(!text.contains("pw") && !text.contains("half typed"), "{text}");
}

#[gpui::test]
fn find_mode_does_not_overwrite_the_stored_draft(cx: &mut gpui::TestAppContext) {
    let dir = TempDir::new("ui-drafts-find");
    let mut ui = on_disk(cx, &dir);
    ui.select("#omarchy");
    type_text(&mut ui, "half typed");
    ui.keys("ctrl-f");
    type_text(&mut ui, "a query");
    ui.select("#help");
    let text = file_text(&dir);
    assert!(text.contains("half typed"), "{text}");
    assert!(!text.contains("a query"), "{text}");
}

#[gpui::test]
fn an_up_down_walk_does_not_overwrite_the_stored_draft(cx: &mut gpui::TestAppContext) {
    let dir = TempDir::new("ui-drafts-walk");
    let mut ui = on_disk(cx, &dir);
    ui.select("#omarchy");
    type_text(&mut ui, "a sent line");
    ui.keys("enter");
    type_text(&mut ui, "half typed");
    ui.keys("up");
    assert_eq!(field(&ui.shell.clone(), &mut ui).0, "a sent line", "the walk shows history");
    ui.shell.update(ui.cx, |shell, cx| shell.set_window_active(false, cx));
    let text = file_text(&dir);
    assert!(text.contains("half typed"), "{text}");
    assert!(!text.contains("a sent line"), "{text}");
}

#[gpui::test]
fn losing_focus_keeps_the_draft(cx: &mut gpui::TestAppContext) {
    let dir = TempDir::new("ui-drafts-blur");
    let mut ui = on_disk(cx, &dir);
    ui.select("#omarchy");
    type_text(&mut ui, "typed then blurred");
    assert_eq!(file_text(&dir), "", "typing alone writes nothing");
    ui.shell.update(ui.cx, |shell, cx| shell.set_window_active(false, cx));
    assert!(file_text(&dir).contains("typed then blurred"), "{}", file_text(&dir));
}

#[gpui::test]
fn closing_the_window_keeps_the_draft(cx: &mut gpui::TestAppContext) {
    let dir = TempDir::new("ui-drafts-close");
    let mut ui = on_disk(cx, &dir);
    ui.select("#omarchy");
    type_text(&mut ui, "typed then closed");
    ui.shell.update(ui.cx, |shell, cx| shell.persist_current_draft(cx));
    assert!(file_text(&dir).contains("typed then closed"), "{}", file_text(&dir));
}

#[gpui::test]
fn sending_the_line_removes_the_stored_draft(cx: &mut gpui::TestAppContext) {
    let dir = TempDir::new("ui-drafts-send");
    let mut ui = on_disk(cx, &dir);
    ui.select("#omarchy");
    type_text(&mut ui, "half typed");
    ui.select("#help");
    ui.select("#omarchy");
    assert_eq!(field(&ui.shell.clone(), &mut ui).0, "half typed");
    ui.keys("enter");
    assert!(!file_text(&dir).contains("half typed"), "{}", file_text(&dir));
}

#[gpui::test]
fn a_status_draft_never_reaches_the_file(cx: &mut gpui::TestAppContext) {
    let dir = TempDir::new("ui-drafts-status");
    let mut ui = on_disk(cx, &dir);
    ui.shell.update(ui.cx, |shell, cx| {
        shell.controller.open_status(OMARCHY);
        shell.sync_view(cx);
    });
    type_text(&mut ui, "half a command");
    ui.shell.update(ui.cx, |shell, cx| shell.set_window_active(false, cx));
    ui.select("#omarchy");
    assert_eq!(file_text(&dir), "", "{}", file_text(&dir));
}
