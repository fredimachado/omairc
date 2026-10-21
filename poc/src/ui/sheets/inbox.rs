//! The Inbox sheet (`Ctrl+Shift+A`, and the footer `inbox` link). Port of
//! `InboxSheet.qml`: no filter, `Nothing waiting` when empty, 36px rows with a
//! label, an optional muted preview, and a `delete` button. Keys are Up, Down,
//! Enter and Delete; Escape is the shell's `CloseSheet`.
//!
//! The sheet keeps a snapshot of the shell's inbox for its rows and keys. It
//! never edits the inbox: it emits `InboxEvent`s, the shell applies them to the
//! real inbox, and then pushes the new snapshot back through `sync`.

use std::rc::Rc;

use gpui::{
    actions, div, prelude::*, px, AnyElement, App, Context, Entity, EventEmitter, FocusHandle,
    KeyBinding, MouseButton, ScrollHandle,
};

use crate::model::filter_list::step;
use crate::irc::inbox::InboxItem;
use crate::model::Style;
use crate::ui::sheets::frame::popup_frame;
use crate::ui::Handler;
use crate::util::{scaled_px, style, to_gpui, FONT_FAMILY};

actions!(inbox_sheet, [InboxUp, InboxDown, InboxActivate, InboxDelete]);

/// Up / Down / Enter / Delete inside the sheet.
pub fn bind_inbox_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", InboxUp, Some("InboxSheet")),
        KeyBinding::new("down", InboxDown, Some("InboxSheet")),
        KeyBinding::new("enter", InboxActivate, Some("InboxSheet")),
        KeyBinding::new("delete", InboxDelete, Some("InboxSheet")),
    ]);
}

/// Sheet width in logical pixels (Qt `404`).
const WIDTH: f32 = 404.;
/// One row's height in logical pixels (Qt `36`).
const ROW_HEIGHT: f32 = 36.;
/// The list is capped at this height (Qt `252`).
const LIST_CAP: f32 = 252.;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InboxEvent {
    /// Enter or a click on a row: act on it, then close.
    Activated(usize),
    /// The delete button, or Delete on the selected row.
    Dismissed(usize),
    /// A press outside the card.
    Closed,
}

pub struct InboxSheet {
    items: Vec<InboxItem>,
    selected: usize,
    scroll: ScrollHandle,
    focus: FocusHandle,
}

impl EventEmitter<InboxEvent> for InboxSheet {}

impl InboxSheet {
    pub fn new(items: Vec<InboxItem>, cx: &mut Context<Self>) -> Self {
        Self {
            items,
            selected: 0,
            scroll: ScrollHandle::new(),
            focus: cx.focus_handle(),
        }
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    /// Replaces the snapshot after the shell changed the inbox. `removed` is
    /// the row that went away, if any, so the selection keeps its place as Qt
    /// does: a removal above the selection moves it up one row.
    pub fn sync(&mut self, items: Vec<InboxItem>, removed: Option<usize>, cx: &mut Context<Self>) {
        let previous = self.selected;
        self.items = items;
        self.selected = match removed {
            _ if self.items.is_empty() => 0,
            Some(row) if row < previous => previous - 1,
            _ => previous.min(self.items.len() - 1),
        };
        cx.notify();
    }

    fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.items.is_empty() {
            return;
        }
        self.selected = step(self.selected, self.items.len(), delta);
        self.scroll.scroll_to_item(self.selected);
        cx.notify();
    }

    fn activate_at(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix < self.items.len() {
            self.selected = ix;
            cx.emit(InboxEvent::Activated(ix));
        }
    }

    /// Delete on the selected row, or the delete button on any row. The
    /// selection does not move here; `sync` settles it once the shell has acted.
    fn dismiss_at(&mut self, ix: usize, cx: &mut Context<Self>) {
        if ix < self.items.len() {
            cx.emit(InboxEvent::Dismissed(ix));
        }
    }

    fn close(&mut self, cx: &mut Context<Self>) {
        cx.emit(InboxEvent::Closed);
    }
}

/// Renders the sheet as a popup over the window.
pub fn render_inbox_sheet(sheet: &Entity<InboxSheet>, cx: &App) -> AnyElement {
    let app_style = style(cx);
    let state = sheet.read(cx);
    let count = state.items.len();
    let selected = state.selected;
    state.scroll.scroll_to_item(selected);

    let row_h = app_style.scaled(ROW_HEIGHT) as f32;
    let cap = app_style.scaled(LIST_CAP) as f32;
    let list_h = px((count as f32 * row_h).min(cap));

    let rows = state.items.iter().enumerate().map(|(ix, item)| {
        let open: Handler<()> = {
            let entity = sheet.clone();
            Rc::new(move |_, _, cx| {
                entity.update(cx, |s, cx| s.activate_at(ix, cx));
            })
        };
        let delete: Handler<()> = {
            let entity = sheet.clone();
            Rc::new(move |_, _, cx| {
                entity.update(cx, |s, cx| s.dismiss_at(ix, cx));
            })
        };
        inbox_row(&app_style, item, ix == selected, open, delete)
    });

    let up = sheet.clone();
    let down = sheet.clone();
    let enter = sheet.clone();
    let delete_key = sheet.clone();
    let dismiss = sheet.clone();
    let focus = state.focus.clone();

    let body = if count == 0 {
        div()
            .w_full()
            .flex()
            .justify_center()
            .font_family(FONT_FAMILY)
            .text_size(scaled_px(&app_style, 12.))
            .text_color(to_gpui(app_style.muted_color()))
            .child("Nothing waiting")
            .into_any_element()
    } else {
        div()
            .id("inbox-list")
            .h(list_h)
            .overflow_y_scroll()
            .track_scroll(&state.scroll)
            .flex()
            .flex_col()
            .children(rows)
            .into_any_element()
    };

    let card = div()
        .w(scaled_px(&app_style, WIDTH))
        .p(scaled_px(&app_style, 16.))
        .bg(to_gpui(app_style.raised_color()))
        .border_1()
        .border_color(to_gpui(app_style.divider_color()))
        .rounded(scaled_px(&app_style, 9.))
        .flex()
        .flex_col()
        .gap(scaled_px(&app_style, 8.))
        .key_context("InboxSheet")
        .track_focus(&focus)
        .on_action(move |_: &InboxUp, _, cx| {
            up.update(cx, |s, cx| s.step(-1, cx));
        })
        .on_action(move |_: &InboxDown, _, cx| {
            down.update(cx, |s, cx| s.step(1, cx));
        })
        .on_action(move |_: &InboxActivate, _, cx| {
            enter.update(cx, |s, cx| {
                let ix = s.selected;
                s.activate_at(ix, cx);
            });
        })
        .on_action(move |_: &InboxDelete, _, cx| {
            delete_key.update(cx, |s, cx| {
                let ix = s.selected;
                s.dismiss_at(ix, cx);
            });
        })
        .child(body);

    popup_frame(&app_style, card, move |_, cx| {
        dismiss.update(cx, |s, cx| s.close(cx));
    })
    .into_any_element()
}

/// Qt's `delete` button: 44×24, radius 5, a divider border, a 10px muted label,
/// and `hover` behind an ink label while the pointer is over it. It is not
/// focusable, so pressing it leaves the keyboard on the sheet.
fn delete_button(style: &Style) -> gpui::Div {
    let hover = to_gpui(style.hover_color());
    let ink = to_gpui(style.ink_color());
    div()
        .w(scaled_px(style, 44.))
        .h(scaled_px(style, 24.))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .rounded(scaled_px(style, 5.))
        .border_1()
        .border_color(to_gpui(style.divider_color()))
        .cursor_pointer()
        .font_family(FONT_FAMILY)
        .text_size(scaled_px(style, 10.))
        .text_color(to_gpui(style.muted_color()))
        .hover(move |s| s.bg(hover).text_color(ink))
        .child("delete")
}

/// One 36px row: the label (12px) over an optional preview (10px muted), then
/// the `delete` button. The row fill is `hover` when selected. The button stops
/// its press so it does not also activate the row.
fn inbox_row(
    style: &Style,
    item: &InboxItem,
    selected: bool,
    open: Handler<()>,
    delete: Handler<()>,
) -> AnyElement {
    let ink = to_gpui(style.ink_color());
    let muted = to_gpui(style.muted_color());
    let label_color = if selected { ink } else { muted };
    let preview = item.preview.clone();
    let has_preview = !preview.is_empty();

    let text = div()
        .flex_grow()
        .min_w_0()
        .pl(scaled_px(style, 8.))
        .flex()
        .flex_col()
        .justify_center()
        .gap(scaled_px(style, 1.))
        .overflow_hidden()
        .child(
            div()
                .overflow_hidden()
                .text_ellipsis()
                .font_family(FONT_FAMILY)
                .text_size(scaled_px(style, 12.))
                .text_color(label_color)
                .child(item.label()),
        )
        .when(has_preview, |this| {
            this.child(
                div()
                    .overflow_hidden()
                    .text_ellipsis()
                    .font_family(FONT_FAMILY)
                    .text_size(scaled_px(style, 10.))
                    .text_color(muted)
                    .child(preview),
            )
        });

    let button = delete_button(style).on_mouse_down(MouseButton::Left, move |_, window, cx| {
        cx.stop_propagation();
        delete((), window, cx);
    });

    let hover = to_gpui(style.hover_color());
    div()
        .h(scaled_px(style, ROW_HEIGHT))
        .w_full()
        .flex_shrink_0()
        .rounded(scaled_px(style, 7.))
        .flex()
        .items_center()
        .gap(scaled_px(style, 4.))
        .pr(scaled_px(style, 4.))
        .cursor_pointer()
        .bg(if selected {
            hover
        } else {
            gpui::transparent_black()
        })
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            open((), window, cx);
        })
        .child(text)
        .child(button)
        .into_any_element()
}
