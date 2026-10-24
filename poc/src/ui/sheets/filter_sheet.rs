//! The filter sheet shared by Jump, Link and Nick. It is an entity that owns
//! the filter field, the selection and the scroll position, and emits
//! `SheetEvent`s. The shell subscribes and applies them. Rendering is a free
//! function because the sheet's rows need the sheet's own handle for clicks.

use std::ops::Range;

use gpui::{
    actions, div, prelude::*, px, AnyElement, App, Context, Entity, EventEmitter, FocusHandle,
    FontWeight, HighlightStyle, Hsla, KeyBinding, MouseButton, ScrollHandle, SharedString,
    StyledText, Subscription,
};

use crate::model::filter_list::{clamp_after_filter, step};
use crate::model::fuzzy::char_ranges;
use crate::model::Style;
use crate::ui::input::SingleLineInput;
use crate::ui::sheets::frame::popup_frame;
use crate::util::{scaled_px, style, to_gpui, FONT_FAMILY};

actions!(filter_sheet, [FilterUp, FilterDown, FilterActivate]);

/// Up / Down / Enter inside the sheet. Escape is the shell's `CloseSheet`.
pub fn bind_filter_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", FilterUp, Some("FilterSheet")),
        KeyBinding::new("down", FilterDown, Some("FilterSheet")),
        KeyBinding::new("enter", FilterActivate, Some("FilterSheet")),
    ]);
}

/// The visible list is capped at this height in logical pixels (Qt `252`).
const LIST_CAP: f32 = 252.;

/// What a filter sheet lists. A source holds the snapshot it was opened with,
/// so the sheet never reads the shell's state.
pub trait FilterSource: 'static {
    type Item: Clone + 'static;

    fn placeholder(&self) -> &'static str;
    /// The rows for `query`, in display order.
    fn items(&self, query: &str) -> Vec<Self::Item>;
    /// The row selected when the sheet opens.
    fn initial_selection(&self, _items: &[Self::Item]) -> usize {
        0
    }
    /// Whether Enter or a click on this row activates it. Inert rows stay listed.
    fn activatable(&self, _item: &Self::Item) -> bool {
        true
    }
    /// The sheet's width in logical pixels.
    fn width(&self) -> f32;
    /// One row's height in logical pixels.
    fn row_height(&self) -> f32;
    /// The list's height cap in logical pixels. Longer lists scroll.
    fn list_cap(&self) -> f32 {
        LIST_CAP
    }
    /// A muted line under the list, given the filter and how many rows it kept.
    fn footer(&self, _query: &str, _visible: usize) -> Option<String> {
        None
    }
    fn render_row(&self, item: &Self::Item, selected: bool, style: &Style) -> AnyElement;
}

#[derive(Clone, Debug)]
pub enum SheetEvent<I> {
    /// The selection moved to this row (Link uses it to reveal the source line).
    Highlighted(I),
    /// Enter or a click on an activatable row.
    Activated(I),
    /// A press outside the card.
    Dismissed,
}

pub struct FilterSheet<S: FilterSource> {
    pub source: S,
    pub filter: Entity<SingleLineInput>,
    pub selected: usize,
    pub scroll: ScrollHandle,
    _filter_observer: Subscription,
}

impl<S: FilterSource> EventEmitter<SheetEvent<S::Item>> for FilterSheet<S> {}

impl<S: FilterSource> FilterSheet<S> {
    pub fn new(source: S, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| {
            SingleLineInput::new(source.placeholder(), "SheetFilter", cx.focus_handle())
        });
        let selected = source.initial_selection(&source.items(""));
        let filter_observer = cx.observe(&filter, |_, _, cx| cx.notify());
        Self {
            source,
            filter,
            selected,
            scroll: ScrollHandle::new(),
            _filter_observer: filter_observer,
        }
    }

    pub fn filter_focus(&self, cx: &App) -> FocusHandle {
        use gpui::Focusable;
        self.filter.read(cx).focus_handle(cx)
    }

    fn query(&self, cx: &App) -> String {
        self.filter.read(cx).text().to_string()
    }

    /// The rows for the current filter, and the selection clamped to them.
    fn rows(&self, cx: &App) -> (Vec<S::Item>, usize) {
        let items = self.source.items(&self.query(cx));
        let selected = clamp_after_filter(self.selected, items.len());
        (items, selected)
    }

    /// Up / Down: moves the selection with wrap and reports the new row.
    pub fn step(&mut self, delta: isize, cx: &mut Context<Self>) {
        let (items, current) = self.rows(cx);
        if items.is_empty() {
            return;
        }
        self.selected = step(current, items.len(), delta);
        self.scroll.scroll_to_item(self.selected);
        cx.emit(SheetEvent::Highlighted(items[self.selected].clone()));
        cx.notify();
    }

    /// A click on row `ix`: select it, then activate it.
    pub fn activate_at(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.selected = ix;
        self.activate(cx);
    }

    /// Enter: activates the selected row if the source allows it.
    pub fn activate(&mut self, cx: &mut Context<Self>) {
        let (items, selected) = self.rows(cx);
        self.selected = selected;
        let Some(item) = items.get(selected) else {
            return;
        };
        if self.source.activatable(item) {
            cx.emit(SheetEvent::Activated(item.clone()));
        }
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        cx.emit(SheetEvent::Dismissed);
    }

    /// Replaces the listing snapshot after the controller's overlay changed.
    pub fn sync_source(&mut self, source: S, cx: &mut Context<Self>) {
        self.source = source;
        cx.notify();
    }
}

/// Renders the sheet as a popup over the window. Rows and the card need the
/// sheet's handle, so they take it from `sheet`.
pub fn render_filter_sheet<S: FilterSource>(sheet: &Entity<FilterSheet<S>>, cx: &App) -> AnyElement {
    let app_style = style(cx);
    let state = sheet.read(cx);
    let query = state.query(cx);
    let items = state.source.items(&query);
    let selected = clamp_after_filter(state.selected, items.len());
    state.scroll.scroll_to_item(selected);

    let row_h = app_style.scaled(state.source.row_height()) as f32;
    let cap = app_style.scaled(state.source.list_cap()) as f32;
    let list_h = px((items.len() as f32 * row_h).min(cap));
    let footer = state.source.footer(&query, items.len()).map(|text| {
        div()
            .w_full()
            .font_family(FONT_FAMILY)
            .text_size(scaled_px(&app_style, 11.))
            .text_color(to_gpui(app_style.muted_color()))
            .child(text)
    });

    let rows = items.iter().enumerate().map(|(ix, item)| {
        let entity = sheet.clone();
        div()
            .cursor_pointer()
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                entity.update(cx, |sheet, cx| sheet.activate_at(ix, cx));
            })
            .child(state.source.render_row(item, ix == selected, &app_style))
    });

    let up = sheet.clone();
    let down = sheet.clone();
    let enter = sheet.clone();
    let card = div()
        .w(scaled_px(&app_style, state.source.width()))
        .p(scaled_px(&app_style, 16.))
        .bg(to_gpui(app_style.raised_color()))
        .border_1()
        .border_color(to_gpui(app_style.divider_color()))
        .rounded(scaled_px(&app_style, 9.))
        .flex()
        .flex_col()
        .gap(scaled_px(&app_style, 8.))
        .key_context("FilterSheet")
        .on_action(move |_: &FilterUp, _, cx| {
            up.update(cx, |sheet, cx| sheet.step(-1, cx));
        })
        .on_action(move |_: &FilterDown, _, cx| {
            down.update(cx, |sheet, cx| sheet.step(1, cx));
        })
        .on_action(move |_: &FilterActivate, _, cx| {
            enter.update(cx, |sheet, cx| sheet.activate(cx));
        })
        .child(state.filter.clone())
        .child(
            div()
                .id("filter-list")
                .h(list_h)
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&state.scroll)
                .flex()
                .flex_col()
                .children(rows),
        )
        .when_some(footer, |card, footer| card.child(footer));

    let dismiss = sheet.clone();
    popup_frame(&app_style, card, move |_, cx| {
        dismiss.update(cx, |sheet, cx| sheet.dismiss(cx));
    })
    .into_any_element()
}

/// A label with the fuzzy-matched characters drawn bold in `ink`. `highlights`
/// are ascending byte offsets into `text`, as `FuzzyHit::indices` gives them.
/// The text takes its size and colour from the parent, so the unmatched part
/// keeps the row's own muted or ink colour.
pub fn match_text(text: String, highlights: &[usize], ink: Hsla) -> StyledText {
    let marked = match_highlights(&text, highlights, ink);
    StyledText::new(SharedString::from(text)).with_highlights(marked)
}

/// The bold, ink-coloured ranges of the matched characters in `text`, for
/// `StyledText::with_highlights`. Combine them with other highlights through
/// `gpui::combine_highlights`, since overlapping ranges are not merged here.
pub fn match_highlights(
    text: &str,
    highlights: &[usize],
    ink: Hsla,
) -> Vec<(Range<usize>, HighlightStyle)> {
    char_ranges(text, highlights)
        .into_iter()
        .map(|range| {
            (
                range,
                HighlightStyle {
                    color: Some(ink),
                    font_weight: Some(FontWeight::BOLD),
                    ..Default::default()
                },
            )
        })
        .collect()
}

/// One text row of a filter list (Jump and Link): 12px, radius 7, `hover`
/// when selected and muted otherwise, as Qt's `jumpList` delegate. The matched
/// characters in `highlights` are drawn bold, see [`match_text`].
pub fn text_row(
    style: &Style,
    height: f32,
    label: String,
    highlights: &[usize],
    selected: bool,
) -> impl IntoElement {
    div()
        .h(scaled_px(style, height))
        .px(scaled_px(style, 8.))
        .rounded(scaled_px(style, 7.))
        .bg(if selected {
            to_gpui(style.hover_color())
        } else {
            gpui::transparent_black()
        })
        .flex()
        .items_center()
        .overflow_hidden()
        .text_ellipsis()
        .font_family(FONT_FAMILY)
        .text_size(scaled_px(style, 12.))
        .text_color(if selected {
            to_gpui(style.ink_color())
        } else {
            to_gpui(style.muted_color())
        })
        .child(match_text(label, highlights, to_gpui(style.ink_color())))
}
