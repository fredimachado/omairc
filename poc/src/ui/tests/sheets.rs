//! Focus and sheet proofs: Connect, About, member page.

use gpui::MouseButton;

use crate::model::chords::Chord;
use crate::model::cursor::page_amount;
use crate::util::{scaled_px, style};

use super::fixture;

/// Matches `Shell::member_page` (`MembersColumn.qml` row height).
const MEMBER_ROW_HEIGHT: f32 = 43.;

#[gpui::test]
fn connection_sheet_close_restores_composer_focus(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    assert!(ui.composer_focused(), "demo launch focuses the composer");

    ui.cx.update(|window, cx| {
        ui.shell.update(cx, |shell, cx| shell.open_connect(window, cx));
    });
    ui.settle();
    assert!(ui.connect_open());
    assert!(
        !ui.composer_focused(),
        "Connect takes focus off the composer"
    );

    ui.cx.update(|window, cx| {
        ui.shell.update(cx, |shell, cx| shell.close_connect(window, cx));
    });
    ui.settle();
    assert!(!ui.connect_open());
    assert!(
        ui.composer_focused(),
        "closing Connect restores composer focus"
    );
}

#[gpui::test]
fn connection_sheet_blocks_sidebar_and_members(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("#omarchy");
    ui.cx.update(|window, cx| {
        ui.shell.update(cx, |shell, cx| {
            shell.open_connect(window, cx);
            assert!(shell.connect_open);
            assert!(
                !shell.chord_live(Chord::WalkConversationDown, window, cx),
                "Connect blocks sidebar walk"
            );
            assert!(
                !shell.chord_live(Chord::MembersFocus, window, cx),
                "Connect blocks the member list"
            );
            assert!(
                !shell.chord_live(Chord::MembersPage, window, cx),
                "Connect blocks member page"
            );
        });
    });
}

#[gpui::test]
fn about_sheet_left_click_dimmer_dismisses(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.cx.update(|window, cx| {
        ui.shell.update(cx, |shell, cx| shell.open_about(window, cx));
    });
    ui.settle();
    assert!(ui.about_open());

    // The veil fills the window; a corner click is outside the centred card.
    ui.click(10., 10.);
    assert!(!ui.about_open(), "left click on the dimmer dismisses About");
}

#[gpui::test]
fn about_sheet_ignores_right_click_and_card_click(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.cx.update(|window, cx| {
        ui.shell.update(cx, |shell, cx| shell.open_about(window, cx));
    });
    ui.settle();
    assert!(ui.about_open());

    ui.mouse_down(10., 10., MouseButton::Right);
    ui.mouse_up(10., 10., MouseButton::Right);
    assert!(ui.about_open(), "right click on the dimmer does nothing");

    // Centre of the window lands on the card; card presses must not dismiss.
    ui.click(super::fixture::WINDOW_WIDTH / 2., super::fixture::WINDOW_HEIGHT / 2.);
    assert!(ui.about_open(), "click on the About card does not dismiss");
}

#[gpui::test]
fn member_list_page_down_moves_by_visible_page(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("#omarchy");
    ui.cx.update(|window, cx| {
        ui.shell.update(cx, |shell, cx| {
            shell.members_open = true;
            window.focus(&shell.member_focus);
            cx.notify();
        });
    });
    ui.settle();

    let (before, expected) = ui.cx.update(|window, cx| {
        ui.shell.update(cx, |shell, cx| {
            assert!(shell.chord_live(Chord::MembersPage, window, cx));
            let row = f32::from(scaled_px(&style(cx), MEMBER_ROW_HEIGHT));
            let visible =
                (f32::from(shell.members_scroll.bounds().size.height) / row).floor();
            let step = page_amount(visible, false).max(1.) as usize;
            let before = shell.member_cursor;
            let len = shell.controller.members().len();
            let expected = (before + step).min(len.saturating_sub(1));
            shell.member_page(true, false, window, cx);
            (before, expected)
        })
    });
    let after = ui.shell.read_with(ui.cx, |shell, _| shell.member_cursor);
    assert!(after > before, "Page Down moves the member cursor down");
    assert_eq!(
        after, expected,
        "Page Down moves by round(visible × 0.8) rows, clamped to the list"
    );
}
