use gpui::{div, prelude::*, Context, Focusable, MouseButton, Window};

use crate::model::{jump_targets, JumpKind};
use crate::ui::shell::actions::{JumpActivate, JumpClose, JumpDown, JumpUp};
use crate::ui::shell::Shell;
use crate::util::{scaled_px, style, to_gpui, FONT_FAMILY};

/// The Jump (`Ctrl+K`) sheet. It is the only modal so far; the rest follow
/// the same open/close pattern.
impl Shell {
    pub fn open_jump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.jump_open = true;
        self.jump_selected = 0;
        self.jump_filter.update(cx, |filter, cx| {
            filter.clear();
            cx.notify();
        });
        window.focus(&self.jump_filter.read(cx).focus_handle(cx));
        cx.notify();
    }

    pub fn close_jump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.jump_open = false;
        self.focus_composer(window, cx);
        cx.notify();
    }

    pub fn jump_step(&mut self, delta: i32, cx: &mut Context<Self>) {
        let matches = jump_targets(&self.world, self.jump_filter.read(cx).text());
        if matches.is_empty() {
            return;
        }
        let len = matches.len();
        let next = (self.jump_selected as i32 + delta).rem_euclid(len as i32) as usize;
        self.jump_selected = next;
        self.jump_list_scroll.scroll_to_item(self.jump_selected);
        cx.notify();
    }

    pub fn jump_activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let matches = jump_targets(&self.world, self.jump_filter.read(cx).text());
        let Some(target) = matches.get(self.jump_selected) else {
            return;
        };
        if target.kind == JumpKind::Conversation {
            if let Some((net, key)) = target.conversation_id.split_once('\n') {
                self.select_by_ids(net, key, cx);
            }
        }
        self.jump_open = false;
        self.focus_composer(window, cx);
        cx.notify();
    }

    pub fn render_jump(&self, cx: &Context<Self>) -> impl IntoElement {
        let app_style = style(cx);
        let matches = jump_targets(&self.world, self.jump_filter.read(cx).text());
        let selected = self.jump_selected.min(matches.len().saturating_sub(1));
        self.jump_list_scroll.scroll_to_item(selected);

        div()
            .absolute()
            .size_full()
            .bg(to_gpui(app_style.overlay_veil_color()))
            .occlude()
            .flex()
            .items_start()
            .justify_center()
            .pt(scaled_px(&app_style, 80.))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, _, window, cx| {
                    shell.close_jump(window, cx);
                }),
            )
            .child(
                div()
                    .w(scaled_px(&app_style, 348.))
                    .p(scaled_px(&app_style, 16.))
                    .bg(to_gpui(app_style.raised_color()))
                    .border_1()
                    .border_color(to_gpui(app_style.divider_color()))
                    .rounded(scaled_px(&app_style, 9.))
                    .flex()
                    .flex_col()
                    .gap(scaled_px(&app_style, 8.))
                    .on_mouse_down(MouseButton::Left, |_, _, _| {})
                    .child(
                        div()
                            .key_context("JumpSheet")
                            .on_action(cx.listener(|s, _: &JumpUp, _, cx| s.jump_step(-1, cx)))
                            .on_action(cx.listener(|s, _: &JumpDown, _, cx| {
                                s.jump_step(1, cx);
                            }))
                            .on_action(cx.listener(|s, _: &JumpActivate, window, cx| {
                                s.jump_activate(window, cx);
                            }))
                            .on_action(cx.listener(|s, _: &JumpClose, window, cx| {
                                s.close_jump(window, cx);
                            }))
                            .child(
                                div()
                                    .key_context("JumpFilter")
                                    .on_action(cx.listener(|s, _: &JumpUp, _, cx| {
                                        s.jump_step(-1, cx);
                                    }))
                                    .on_action(cx.listener(|s, _: &JumpDown, _, cx| {
                                        s.jump_step(1, cx);
                                    }))
                                    .on_action(cx.listener(|s, _: &JumpActivate, window, cx| {
                                        s.jump_activate(window, cx);
                                    }))
                                    .on_action(cx.listener(|s, _: &JumpClose, window, cx| {
                                        s.close_jump(window, cx);
                                    }))
                                    .child(self.jump_filter.clone()),
                            ),
                    )
                    .child(
                        div()
                            .id("jump-list")
                            .h(scaled_px(&app_style, 252.))
                            .overflow_y_scroll()
                            .track_scroll(&self.jump_list_scroll)
                            .flex()
                            .flex_col()
                            .children(matches.iter().enumerate().map(|(ix, target)| {
                                div()
                                    .h(scaled_px(&app_style, 28.))
                                    .px(scaled_px(&app_style, 8.))
                                    .rounded(scaled_px(&app_style, 4.))
                                    .bg(if ix == selected {
                                        to_gpui(app_style.hover_color())
                                    } else {
                                        gpui::transparent_black()
                                    })
                                    .cursor_pointer()
                                    .text_size(scaled_px(&app_style, 13.))
                                    .text_color(to_gpui(app_style.ink_color()))
                                    .font_family(FONT_FAMILY)
                                    .child(target.label.clone())
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |shell, _, window, cx| {
                                            shell.jump_selected = ix;
                                            shell.jump_activate(window, cx);
                                        }),
                                    )
                            })),
                    ),
            )
    }
}
