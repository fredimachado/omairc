//! The Shortcuts sheet (`Ctrl+/`): the grouped chord table from
//! `client/src/ui/sheets/shortcuts.rs`, read-only. Escape, a press outside the card, or
//! `Ctrl+/` again closes it. The card holds keyboard focus so typing does not
//! reach the composer behind it, as the Qt popup does.

use gpui::{div, prelude::*, Context, EventEmitter, FocusHandle, Window};

use crate::model::shortcuts::{chord_label, Platform, ShortcutGroup, ShortcutRow, GROUPS};
use crate::model::Style;
use crate::ui::sheets::frame::popup_frame;
use crate::util::{scaled_px, style, to_gpui, FONT_FAMILY};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutsEvent {
    Dismissed,
}

pub struct ShortcutsSheet {
    focus: FocusHandle,
}

impl ShortcutsSheet {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }
}

impl EventEmitter<ShortcutsEvent> for ShortcutsSheet {}

/// The platform the labels are written for: Windows adds the terminal
/// aliases, macOS relabels Ctrl and Alt.
pub fn host_platform() -> Platform {
    if cfg!(target_os = "windows") {
        Platform::Windows
    } else if cfg!(target_os = "macos") {
        Platform::MacOs
    } else {
        Platform::Other
    }
}

impl Render for ShortcutsSheet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_style = style(cx);
        let entity = cx.entity();
        // The frame keeps the card inside the window; the chord list scrolls inside
        // it when the window is too short for all five groups.
        let list = div()
            .id("shortcuts-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap(scaled_px(&app_style, 12.))
            .children(GROUPS.iter().map(|group| group_block(&app_style, group)));
        let card = div()
            .w(scaled_px(&app_style, 460.))
            .p(scaled_px(&app_style, 16.))
            .bg(to_gpui(app_style.raised_color()))
            .border_1()
            .border_color(to_gpui(app_style.divider_color()))
            .rounded(scaled_px(&app_style, 9.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .track_focus(&self.focus)
            .child(list);
        popup_frame(&app_style, card, move |_, cx| {
            entity.update(cx, |_, cx| cx.emit(ShortcutsEvent::Dismissed));
        })
    }
}

/// One group: its caps title, then the chord rows (4px apart).
fn group_block(style: &Style, group: &ShortcutGroup) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(scaled_px(style, 4.))
        .child(
            div()
                .text_size(scaled_px(style, 9.))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(to_gpui(style.muted_color()))
                .font_family(FONT_FAMILY)
                .child(group.title),
        )
        .children(
            group
                .rows
                .iter()
                .map(|row| shortcut_row(style, row, host_platform())),
        )
}

/// A chord and its action, 12px apart. The key column is 220 wide; a long
/// chord list wraps inside it instead of running into the action column.
fn shortcut_row(style: &Style, row: &ShortcutRow, platform: Platform) -> impl IntoElement {
    div()
        .w_full()
        .flex()
        .gap(scaled_px(style, 12.))
        .font_family(FONT_FAMILY)
        .text_size(scaled_px(style, 11.))
        .child(
            div()
                .w(scaled_px(style, 220.))
                .flex_shrink_0()
                .text_color(to_gpui(style.ink_color()))
                .child(chord_label(row, platform)),
        )
        .child(
            div()
                .text_color(to_gpui(style.muted_color()))
                .child(row.action),
        )
}
