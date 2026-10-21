//! The About sheet (`Ctrl+Shift+/`, or a click on the footer's version): a
//! port of `AboutSheet.qml` on the veil look. OK has focus on open. Tab moves
//! between `Check for Updates` and OK. Enter or Space activates the focused
//! button. Check for Updates asks GitHub on the background executor and shows
//! one status line. A newer version opens its release page. The GitHub link
//! opens the repo through the same http(s) policy as the transcript.

use gpui::{
    actions, div, img, prelude::*, App, Context, Entity, EventEmitter, FocusHandle, KeyBinding,
    MouseButton, ObjectFit, Subscription, Window,
};

use crate::model::update_check::{StatusAction, UpdateCheck, REPO_URL};
use crate::model::Style;
use crate::ui::sheets::frame::veil_frame;
use crate::ui::update::{activate_status, start_check};
use crate::ui::widgets::text_button::text_button;
use crate::util::{open_web_link, scaled_px, style, to_gpui, FONT_FAMILY};
use crate::version::version;

const DESCRIPTION: &str = "Omairc is an Internet Relay Chat client for Omarchy. People use it to communicate, share, play, and work with each other on IRC networks around the world.";

actions!(about_sheet, [AboutNextButton, AboutActivate]);

/// Tab and Shift+Tab move between the two buttons; Enter and Space activate.
/// Scoped to the sheet's key context, so the composer keeps its own keys.
pub fn bind_about_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", AboutNextButton, Some("AboutSheet")),
        KeyBinding::new("shift-tab", AboutNextButton, Some("AboutSheet")),
        KeyBinding::new("enter", AboutActivate, Some("AboutSheet")),
        KeyBinding::new("space", AboutActivate, Some("AboutSheet")),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AboutEvent {
    Dismissed,
}

pub struct AboutSheet {
    check_focus: FocusHandle,
    ok_focus: FocusHandle,
    /// The window's update state. The sheet re-renders when it changes.
    update: Entity<UpdateCheck>,
    _observe: Subscription,
}

impl AboutSheet {
    pub fn new(update: Entity<UpdateCheck>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&update, |_, _, cx| cx.notify());
        Self {
            check_focus: cx.focus_handle(),
            ok_focus: cx.focus_handle(),
            update,
            _observe: observe,
        }
    }

    /// Focus goes to OK when the sheet opens.
    pub fn initial_focus(&self) -> FocusHandle {
        self.ok_focus.clone()
    }

    fn toggle_focus(&self, window: &mut Window) {
        let target = if self.check_focus.is_focused(window) {
            &self.ok_focus
        } else {
            &self.check_focus
        };
        window.focus(target);
    }

    fn activate(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.ok_focus.is_focused(window) {
            cx.emit(AboutEvent::Dismissed);
        } else if self.check_focus.is_focused(window) {
            start_check(&self.update, cx);
        }
    }

    /// The update status line under the GitHub link, hidden until a check runs.
    /// Its click does what the state offers: a release page, a download, or a restart.
    fn status_line(
        &self,
        message: Option<String>,
        action: StatusAction,
        app_style: &Style,
        muted: gpui::Hsla,
        accent: gpui::Hsla,
    ) -> Option<gpui::Div> {
        let message = message?;
        let line = div()
            .text_size(scaled_px(app_style, 12.))
            .text_color(muted)
            .font_family(FONT_FAMILY)
            .child(message);
        let clickable = action != StatusAction::None;
        let update = self.update.clone();
        Some(line.when(clickable, |line| {
            line.underline()
                .cursor_pointer()
                .hover(move |s| s.text_color(accent))
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    activate_status(&update, cx);
                })
        }))
    }
}

impl EventEmitter<AboutEvent> for AboutSheet {}

impl Render for AboutSheet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_style = style(cx);
        let ink = to_gpui(app_style.ink_color());
        let muted = to_gpui(app_style.muted_color());
        let accent = to_gpui(app_style.accent_color());
        let check_focused = self.check_focus.is_focused(window);
        let ok_focused = self.ok_focus.is_focused(window);
        let message = self.update.read(cx).message();
        let action = self.update.read(cx).action();
        let update = self.update.clone();
        let entity = cx.entity();
        let ok_entity = entity.clone();
        let dismiss_entity = entity;

        let brand = div()
            .flex_grow()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(scaled_px(&app_style, 8.))
            .child(
                div()
                    .text_size(scaled_px(&app_style, 28.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(ink)
                    .font_family(FONT_FAMILY)
                    .child("Omairc"),
            )
            .child(
                div()
                    .text_size(scaled_px(&app_style, 13.))
                    .text_color(muted)
                    .font_family(FONT_FAMILY)
                    .child(version()),
            )
            .child(text_button(
                &app_style,
                "Check for Updates",
                None,
                28.,
                &self.check_focus,
                check_focused,
                move |_, cx| {
                    start_check(&update, cx);
                },
            ));

        let logo = img("icons/omairc.svg")
            .size(scaled_px(&app_style, 88.))
            .flex_shrink_0()
            .object_fit(ObjectFit::Contain);

        let github = div()
            .text_size(scaled_px(&app_style, 12.))
            .text_color(ink)
            .font_family(FONT_FAMILY)
            .underline()
            .cursor_pointer()
            .hover(|s| s.text_color(accent))
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                open_web_link(cx, REPO_URL);
            })
            .child("View the source on GitHub");

        let body = div()
            .flex()
            .flex_col()
            .gap(scaled_px(&app_style, 14.))
            .px(scaled_px(&app_style, 22.))
            .pt(scaled_px(&app_style, 18.))
            .pb(scaled_px(&app_style, 16.))
            .child(
                div()
                    .text_size(scaled_px(&app_style, 11.))
                    .text_color(muted)
                    .font_family(FONT_FAMILY)
                    .child("About Omairc"),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(scaled_px(&app_style, 16.))
                    .child(brand)
                    .child(logo),
            )
            .child(
                div()
                    .text_size(scaled_px(&app_style, 12.))
                    .text_color(ink)
                    .font_family(FONT_FAMILY)
                    .child(DESCRIPTION),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(scaled_px(&app_style, 6.))
                    .child(
                        div()
                            .text_size(scaled_px(&app_style, 12.))
                            .text_color(ink)
                            .font_family(FONT_FAMILY)
                            .child("This project is open-source."),
                    )
                    .child(github)
                    .children(self.status_line(message, action, &app_style, muted, accent)),
            )
            .child(
                div()
                    .h(scaled_px(&app_style, 30.))
                    .w_full()
                    .flex()
                    .justify_center()
                    .child(text_button(
                        &app_style,
                        "OK",
                        Some(88.),
                        30.,
                        &self.ok_focus,
                        ok_focused,
                        move |_, cx| {
                            ok_entity.update(cx, |_, cx| cx.emit(AboutEvent::Dismissed));
                        },
                    )),
            );

        let footer = div()
            .w_full()
            .h(scaled_px(&app_style, 40.))
            .border_t_1()
            .border_color(to_gpui(app_style.divider_color()))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .text_size(scaled_px(&app_style, 10.))
                    .text_color(muted)
                    .font_family(FONT_FAMILY)
                    .child("Copyright © 2026 Fredi Machado"),
            );

        let card = div()
            .w(scaled_px(&app_style, 440.))
            .rounded(scaled_px(&app_style, 9.))
            .bg(to_gpui(app_style.raised_color()))
            .border_1()
            .border_color(to_gpui(app_style.divider_color()))
            .overflow_hidden()
            .flex()
            .flex_col()
            .key_context("AboutSheet")
            .on_action(cx.listener(|sheet, _: &AboutNextButton, window, _| {
                sheet.toggle_focus(window);
            }))
            .on_action(cx.listener(|sheet, _: &AboutActivate, window, cx| {
                sheet.activate(window, cx);
            }))
            .child(body)
            .child(footer);

        veil_frame(&app_style, card, move |_, cx| {
            dismiss_entity.update(cx, |_, cx| cx.emit(AboutEvent::Dismissed));
        })
    }
}
