use gpui::{
    div, point, px, Context, Entity, MouseButton, MouseDownEvent, Pixels, Point, ScrollHandle,
    prelude::*,
};
use shared::Style;

use crate::shell::Shell;
use crate::util::{scaled_px, to_gpui};

const MIN_THUMB: f32 = 28.;

pub fn div_vertical_scrollbar(
    scroll_handle: &ScrollHandle,
    style: &Style,
    entity: Entity<Shell>,
    drag_slot: ScrollbarDragSlot,
) -> impl IntoElement {
    let viewport = scroll_handle.bounds();
    let viewport_h = viewport.size.height;
    let max_scroll = scroll_handle.max_offset().height;
    if viewport_h <= px(0.) || max_scroll <= px(0.) {
        return div().into_any_element();
    }

    let scrolled = (-scroll_handle.offset().y).max(px(0.));
    let metrics = thumb_metrics(viewport_h, max_scroll, scrolled, style);
    let bar_w = scaled_px(style, 6.);
    let track = to_gpui(style.divider_color());
    let thumb = to_gpui(style.muted_color());
    let min_thumb = scaled_px(style, MIN_THUMB);
    let entity = entity.clone();
    let handle_thumb = scroll_handle.clone();
    let handle_track = scroll_handle.clone();
    let track_top = viewport.top();

    div()
        .absolute()
        .top_0()
        .right_0()
        .h(viewport_h)
        .w(bar_w)
        .bg(track)
        .child(
            div()
                .absolute()
                .top(metrics.top)
                .w_full()
                .h(metrics.height)
                .bg(thumb)
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    {
                        let entity = entity.clone();
                        move |ev: &MouseDownEvent, _, cx| {
                            let handle = handle_thumb.clone();
                            entity.update(cx, |shell, _| {
                                shell.set_scrollbar_drag(
                                    drag_slot,
                                    ScrollbarDrag {
                                        kind: ScrollbarDragKind::Div(handle),
                                        pointer_offset: ev.position.y - (track_top + metrics.top),
                                    },
                                );
                            });
                        }
                    },
                ),
        )
        .on_mouse_down(
            MouseButton::Left,
            {
                let entity = entity.clone();
                let handle = handle_track;
                move |ev: &MouseDownEvent, _, cx| {
                    let local_y = ev.position.y - track_top;
                    if local_y >= metrics.top && local_y <= metrics.top + metrics.height {
                        return;
                    }
                    entity.update(cx, |_shell, cx| {
                        jump_div_scroll(&handle, viewport_h, max_scroll, local_y, min_thumb);
                        cx.notify();
                    });
                }
            },
        )
        .into_any_element()
}

pub fn list_vertical_scrollbar(
    list_state: gpui::ListState,
    style: &Style,
    entity: Entity<Shell>,
    content_height: Pixels,
) -> impl IntoElement {
    let viewport = list_state.viewport_bounds();
    let viewport_h = viewport.size.height;
    let max_scroll = list_state
        .max_offset_for_scrollbar()
        .height
        .max((content_height - viewport_h).max(px(0.)));
    if viewport_h <= px(0.) || max_scroll <= px(0.) {
        return div().into_any_element();
    }

    let scrolled = (-list_state.scroll_px_offset_for_scrollbar().y).max(px(0.));
    let metrics = thumb_metrics(viewport_h, max_scroll, scrolled, style);
    let bar_w = scaled_px(style, 6.);
    let track = to_gpui(style.divider_color());
    let thumb = to_gpui(style.muted_color());
    let min_thumb = scaled_px(style, MIN_THUMB);
    let entity = entity.clone();
    let list_thumb = list_state.clone();
    let list_track = list_state.clone();
    let track_top = viewport.top();

    div()
        .absolute()
        .top_0()
        .right_0()
        .h(viewport_h)
        .w(bar_w)
        .bg(track)
        .child(
            div()
                .absolute()
                .top(metrics.top)
                .w_full()
                .h(metrics.height)
                .bg(thumb)
                .cursor_pointer()
                .on_mouse_down(
                    MouseButton::Left,
                    {
                        let entity = entity.clone();
                        move |ev: &MouseDownEvent, _, cx| {
                            let list = list_thumb.clone();
                            list.scrollbar_drag_started();
                            entity.update(cx, |shell, _| {
                                shell.set_scrollbar_drag(
                                    ScrollbarDragSlot::Transcript,
                                    ScrollbarDrag {
                                        kind: ScrollbarDragKind::List(list.clone()),
                                        pointer_offset: ev.position.y - (track_top + metrics.top),
                                    },
                                );
                            });
                        }
                    },
                ),
        )
        .on_mouse_down(
            MouseButton::Left,
            {
                let entity = entity.clone();
                let list = list_track;
                move |ev: &MouseDownEvent, _, cx| {
                    let local_y = ev.position.y - track_top;
                    if local_y >= metrics.top && local_y <= metrics.top + metrics.height {
                        return;
                    }
                    entity.update(cx, |_shell, cx| {
                        jump_list_scroll(&list, max_scroll, local_y, min_thumb);
                        cx.notify();
                    });
                }
            },
        )
        .into_any_element()
}

struct ThumbMetrics {
    top: Pixels,
    height: Pixels,
}

fn thumb_metrics(
    viewport_h: Pixels,
    max_scroll: Pixels,
    scrolled: Pixels,
    style: &Style,
) -> ThumbMetrics {
    let viewport = px_f32(viewport_h);
    let max = px_f32(max_scroll);
    let content = viewport + max;
    let mut height = px(viewport * viewport / content.max(1.));
    height = height.max(scaled_px(style, MIN_THUMB)).min(viewport_h);
    let travel = (viewport_h - height).max(px(0.));
    let top = if max_scroll > px(0.) {
        px(px_f32(travel) * (px_f32(scrolled) / max.max(1.)))
    } else {
        px(0.)
    };
    ThumbMetrics { top, height }
}

fn jump_div_scroll(
    handle: &ScrollHandle,
    viewport_h: Pixels,
    max_scroll: Pixels,
    local_y: Pixels,
    min_thumb: Pixels,
) {
    let metrics = thumb_metrics_raw(viewport_h, max_scroll, min_thumb);
    let travel = px_f32((viewport_h - metrics.height).max(px(1.)));
    let thumb_top = px_f32((local_y - metrics.height / 2.).max(px(0.)).min(px(travel)));
    let scroll_top = px(px_f32(max_scroll) * (thumb_top / travel.max(1.)));
    handle.set_offset(point(px(0.), -scroll_top));
}

fn jump_list_scroll(
    list: &gpui::ListState,
    max_scroll: Pixels,
    local_y: Pixels,
    min_thumb: Pixels,
) {
    let viewport_h = list.viewport_bounds().size.height;
    let metrics = thumb_metrics_raw(viewport_h, max_scroll, min_thumb);
    let travel = px_f32((viewport_h - metrics.height).max(px(1.)));
    let thumb_top = px_f32((local_y - metrics.height / 2.).max(px(0.)).min(px(travel)));
    let scroll_top = px(px_f32(max_scroll) * (thumb_top / travel.max(1.)));
    list.set_offset_from_scrollbar(point(px(0.), scroll_top));
}

fn thumb_metrics_raw(viewport_h: Pixels, max_scroll: Pixels, min_thumb: Pixels) -> ThumbMetrics {
    let viewport = px_f32(viewport_h);
    let content = viewport + px_f32(max_scroll);
    let mut height = px(viewport * viewport / content.max(1.));
    height = height.max(min_thumb).min(viewport_h);
    ThumbMetrics {
        top: px(0.),
        height,
    }
}

fn px_f32(value: Pixels) -> f32 {
    f32::from(value)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarDragSlot {
    Transcript,
    Sidebar,
    Members,
}

#[derive(Clone)]
pub enum ScrollbarDragKind {
    Div(ScrollHandle),
    List(gpui::ListState),
}

#[derive(Clone)]
pub struct ScrollbarDrag {
    pub kind: ScrollbarDragKind,
    pub pointer_offset: Pixels,
}

pub fn handle_scrollbar_mouse_move(
    shell: &mut Shell,
    position: Point<Pixels>,
    style: &Style,
    cx: &mut Context<Shell>,
) {
    let Some(drag) = shell.scrollbar_drag.clone() else {
        return;
    };
    match drag.kind {
        ScrollbarDragKind::Div(handle) => {
            let viewport = handle.bounds();
            let viewport_h = viewport.size.height;
            let max_scroll = handle.max_offset().height;
            if max_scroll <= px(0.) {
                return;
            }
            let local_y = position.y - viewport.top();
            let min_thumb = scaled_px(style, MIN_THUMB);
            let metrics = thumb_metrics_raw(viewport_h, max_scroll, min_thumb);
            let travel = px_f32((viewport_h - metrics.height).max(px(1.)));
            let thumb_top = px_f32((local_y - drag.pointer_offset).max(px(0.)).min(px(travel)));
            let scroll_top = px(px_f32(max_scroll) * (thumb_top / travel.max(1.)));
            handle.set_offset(point(px(0.), -scroll_top));
        }
        ScrollbarDragKind::List(list) => {
            let viewport = list.viewport_bounds();
            let viewport_h = viewport.size.height;
            let max_scroll = list.max_offset_for_scrollbar().height;
            if max_scroll <= px(0.) {
                return;
            }
            let local_y = position.y - viewport.top();
            let min_thumb = scaled_px(style, MIN_THUMB);
            let metrics = thumb_metrics_raw(viewport_h, max_scroll, min_thumb);
            let travel = px_f32((viewport_h - metrics.height).max(px(1.)));
            let thumb_top = px_f32((local_y - drag.pointer_offset).max(px(0.)).min(px(travel)));
            let scroll_top = px(px_f32(max_scroll) * (thumb_top / travel.max(1.)));
            list.set_offset_from_scrollbar(point(px(0.), scroll_top));
        }
    }
    cx.notify();
}

pub fn end_scrollbar_drag(shell: &mut Shell) {
    if let Some(drag) = shell.scrollbar_drag.take() {
        if let ScrollbarDragKind::List(list) = drag.kind {
            list.scrollbar_drag_ended();
        }
    }
}
