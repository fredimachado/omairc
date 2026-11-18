//! Replies and reactions as the reader sees them: the muted `↪ author: text` line above a
//! reply, the `emoji count` chips under a reacted line, and the click that scrolls a reply
//! to its parent. Each proof runs once in a dark and once in a light Omarchy theme.

use crate::model::theme::Theme;
use crate::model::transcript::{row_for_msgid, RowKind};
use crate::model::Style;
use crate::ui::message_row::chip_colors;

use super::fixture::{self, SeededShell, OMARCHY};

const ARROW: &str = "\u{21AA}";

/// `#omarchy` with a parent far above the end, a reply to it, a reply to a line that is not
/// loaded, and two reactions on the reply.
fn seed(ui: &mut SeededShell<'_>) {
    ui.select("#omarchy");
    ui.inject(OMARCHY, "@msgid=p1 :anna!u@h PRIVMSG #omarchy :parent line to answer");
    ui.fill_until_scrollable("anna", "#omarchy", "filler");
    // More than a viewport after the parent, so landing on it is not clamped to the end.
    for index in 0..40 {
        ui.inject_chat("anna", "#omarchy", &format!("more filler {index}"));
    }
    ui.inject(
        OMARCHY,
        "@msgid=r1;+draft/reply=p1 :ben!u@h PRIVMSG #omarchy :agreed with that",
    );
    ui.inject(
        OMARCHY,
        "@msgid=r2;+draft/reply=gone :ben!u@h PRIVMSG #omarchy :orphan reply",
    );
    ui.inject(OMARCHY, "@+draft/reply=r1;+draft/react=\u{1F44D} :anna!u@h TAGMSG #omarchy");
    ui.inject(OMARCHY, "@+draft/reply=r1;+draft/react=\u{1F44D} :carl!u@h TAGMSG #omarchy");
    ui.inject(OMARCHY, "@+draft/reply=r1;+draft/react=\u{1F389} :carl!u@h TAGMSG #omarchy");
}

/// The bounds of a debug selector. The window wants a `'static` name, so a test's own name
/// is leaked: a few bytes for the life of the test process.
fn bounds_of(ui: &mut SeededShell<'_>, name: &str) -> Option<gpui::Bounds<gpui::Pixels>> {
    ui.cx.debug_bounds(Box::leak(name.to_owned().into_boxed_str()))
}

fn row_of(ui: &SeededShell<'_>, msgid: &str) -> usize {
    ui.shell.read_with(ui.cx, |shell, cx| {
        row_for_msgid(shell.transcript.read(cx).rows(), msgid).expect("the row is loaded")
    })
}

fn rendered_reply_and_chips(theme: Theme, cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open_themed(cx, theme.clone());
    seed(&mut ui);
    let reply = row_of(&ui, "r1");
    let orphan = row_of(&ui, "r2");

    let (preview, orphan_preview, chips) = ui.shell.read_with(ui.cx, |shell, cx| {
        let rows = shell.transcript.read(cx).rows();
        assert_eq!(rows[reply].kind, RowKind::Message);
        (
            rows[reply].reply_preview.clone(),
            rows[orphan].reply_preview.clone(),
            rows[reply].reactions.clone(),
        )
    });
    assert_eq!(preview, format!("{ARROW} anna: parent line to answer"));
    assert_eq!(orphan_preview, format!("{ARROW} reply"));
    assert_eq!(chips.len(), 2);
    assert_eq!((chips[0].emoji.as_str(), chips[0].count), ("\u{1F44D}", 2));
    assert_eq!(chips[0].nicks, vec!["anna", "carl"]);
    assert_eq!((chips[1].emoji.as_str(), chips[1].count), ("\u{1F389}", 1));

    assert!(
        bounds_of(&mut ui, &format!("reply-line-{reply}")).is_some(),
        "the reply line is drawn above the reply"
    );
    assert!(bounds_of(&mut ui, &format!("reply-line-{orphan}")).is_some());
    assert!(
        bounds_of(&mut ui, &format!("reaction-chip-{reply}-0")).is_some()
            && bounds_of(&mut ui, &format!("reaction-chip-{reply}-1")).is_some(),
        "one chip per emoji is drawn under the reacted line"
    );
    assert!(
        bounds_of(&mut ui, &format!("reaction-chip-{orphan}-0")).is_none(),
        "a line nobody reacted to has no chips"
    );
    let line = bounds_of(&mut ui, &format!("reply-line-{reply}")).unwrap();
    let chip = bounds_of(&mut ui, &format!("reaction-chip-{reply}-0")).unwrap();
    assert!(chip.origin.y > line.origin.y, "chips sit under the reply line and its body");
}

#[gpui::test]
fn reply_line_and_chips_render_in_a_dark_theme(cx: &mut gpui::TestAppContext) {
    rendered_reply_and_chips(Theme::defaults_dark(), cx);
}

#[gpui::test]
fn reply_line_and_chips_render_in_a_light_theme(cx: &mut gpui::TestAppContext) {
    rendered_reply_and_chips(Theme::defaults_light(), cx);
}

#[test]
fn the_reply_line_and_chips_take_their_colors_from_the_style_helpers() {
    let dark = Style::new(Theme::defaults_dark(), 1.0);
    let light = Style::new(Theme::defaults_light(), 1.0);
    for style in [&dark, &light] {
        let colors = chip_colors(style);
        assert_eq!(colors.ink, style.muted_color());
        assert_eq!(colors.fill, style.raised_color());
        assert_eq!(colors.edge, style.divider_color());
    }
    assert_ne!(chip_colors(&dark), chip_colors(&light), "each theme derives its own");
}

#[gpui::test]
fn clicking_a_reply_line_scrolls_to_its_loaded_parent(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    seed(&mut ui);
    let parent = row_of(&ui, "p1");
    let reply = row_of(&ui, "r1");
    assert!(ui.at_end());
    assert!(ui.scroll_top().item_ix > parent, "the parent starts above the viewport");

    let bounds = bounds_of(&mut ui, &format!("reply-line-{reply}")).expect("reply line");
    ui.click(f32::from(bounds.center().x), f32::from(bounds.center().y));
    assert_eq!(ui.scroll_top().item_ix, parent, "the parent row lands at the top");
    assert!(!ui.at_end(), "the reader left the end");
}

#[gpui::test]
fn clicking_a_reply_line_whose_parent_is_not_loaded_does_nothing(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    seed(&mut ui);
    let orphan = row_of(&ui, "r2");
    let before = ui.scroll_top();

    let bounds = bounds_of(&mut ui, &format!("reply-line-{orphan}")).expect("reply line");
    ui.click(f32::from(bounds.center().x), f32::from(bounds.center().y));
    let after = ui.scroll_top();
    assert_eq!(
        (after.item_ix, after.offset_in_item),
        (before.item_ix, before.offset_in_item),
        "an unloaded parent is not an error and moves nothing"
    );
    assert!(ui.at_end());
}

#[gpui::test]
fn the_demo_seed_shows_one_reply_and_its_reactions(cx: &mut gpui::TestAppContext) {
    let mut ui = fixture::open(cx);
    ui.select("#omarchy");
    let question = row_of(&ui, "demo-question");
    let answer = row_of(&ui, "demo-answer");
    ui.shell.read_with(ui.cx, |shell, cx| {
        let rows = shell.transcript.read(cx).rows();
        assert_eq!(
            rows[answer].reply_preview,
            format!("{ARROW} anna: Morning! Has anyone tried the new minimal install flow yet?")
        );
        let chips: Vec<(&str, usize)> = rows[question]
            .reactions
            .iter()
            .map(|chip| (chip.emoji.as_str(), chip.count))
            .collect();
        assert_eq!(chips, vec![("\u{1F44D}", 2)]);
        assert_eq!(rows[answer].reactions[0].emoji, "\u{1F389}");
    });
}
