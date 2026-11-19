//! A multi-line message from another client (`draft/multiline`, #478) is one
//! transcript row whose body breaks at the embedded newlines. The batch goes in as
//! wire lines through the demo session, so the proof runs from the wire to the
//! painted row.

use gpui::{px, Modifiers, MouseButton, Point};

use super::fixture::{self, SeededShell, OMARCHY};

/// One closed multiline batch of mira's lines into `#omarchy`.
fn inject_batch(ui: &mut SeededShell, lines: &[&str]) {
    let mut wire = vec![":mira!u@h BATCH +ml draft/multiline #omarchy".to_owned()];
    for line in lines {
        wire.push(format!("@batch=ml :mira!u@h PRIVMSG #omarchy :{line}"));
    }
    wire.push(":mira!u@h BATCH -ml".to_owned());
    ui.inject(OMARCHY, &wire.join("\r\n"));
}

fn row_with(ui: &SeededShell, text: &str) -> usize {
    ui.shell.read_with(ui.cx, |shell, cx| {
        shell
            .transcript
            .read(cx)
            .rows()
            .iter()
            .position(|row| row.body.plain == text)
            .unwrap_or_else(|| panic!("no row with {text:?}"))
    })
}

fn painted(ui: &SeededShell, row: usize) -> (gpui::Bounds<gpui::Pixels>, gpui::Pixels) {
    ui.shell.read_with(ui.cx, |shell, cx| {
        shell
            .transcript
            .read(cx)
            .painted_body(row)
            .expect("the row was painted")
    })
}

#[gpui::test]
fn a_three_line_message_is_one_row_three_lines_tall_that_copies_with_its_breaks(
    cx: &mut gpui::TestAppContext,
) {
    let mut ui = fixture::open(cx);
    ui.select("#omarchy");
    ui.inject_chat("mira", "#omarchy", "a single line");
    inject_batch(&mut ui, &["one", "two", "three"]);
    let bodies: Vec<String> = ui.shell.read_with(ui.cx, |shell, cx| {
        let rows = shell.transcript.read(cx).rows();
        rows.iter().map(|row| row.body.plain.clone()).collect()
    });
    for line in ["one", "two", "three"] {
        assert!(
            !bodies.iter().any(|body| body == line),
            "{line:?} must not be a row of its own"
        );
    }

    let single = row_with(&ui, "a single line");
    let multi = row_with(&ui, "one\ntwo\nthree");
    let (single_bounds, single_line) = painted(&ui, single);
    let (multi_bounds, line_height) = painted(&ui, multi);
    assert_eq!(single_line, line_height);
    assert_eq!(single_bounds.size.height, line_height, "the control is one line");
    assert!(
        multi_bounds.size.height >= line_height * 3.0,
        "three lines tall: {:?} for a line of {:?}",
        multi_bounds.size.height,
        line_height
    );

    // Drag from the first character to past the last one, then copy.
    let from = Point::new(multi_bounds.origin.x + px(1.), multi_bounds.origin.y + px(1.));
    let to = Point::new(
        multi_bounds.origin.x + multi_bounds.size.width - px(1.),
        multi_bounds.origin.y + multi_bounds.size.height - px(1.),
    );
    ui.cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    ui.cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
    ui.cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    ui.settle();
    let copied = ui
        .shell
        .read_with(ui.cx, |shell, cx| shell.transcript.read(cx).copy_text());
    assert_eq!(copied, "one\ntwo\nthree", "copying the row gives the breaks back");
}

#[gpui::test]
fn a_multiline_message_with_a_mention_on_its_last_line_is_one_unread(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("#help");
    let before = ui.shell.read_with(ui.cx, |shell, _| {
        shell.controller.unread_count_for(OMARCHY)
    });
    inject_batch(&mut ui, &["hello", "anyone", "fred, ping"]);
    let after = ui.shell.read_with(ui.cx, |shell, _| {
        shell.controller.unread_count_for(OMARCHY)
    });
    assert_eq!(after, before + 1, "one message is one unread");
}
