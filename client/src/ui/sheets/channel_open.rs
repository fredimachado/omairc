//! The Channel open sheet: a click on a channel name the network does not have
//! yet asks before it joins (`ChannelOpenSheet`). Enter joins the channel
//! Escape cancels (the shell's `CloseSheet`), and a press outside the card
//! cancels too. The card holds focus, so the composer does not take the keys.

use gpui::{actions, div, prelude::*, App, Context, EventEmitter, FocusHandle, Window};

use crate::ui::key_bindings::{bind_return, RETURN_KEYS};
use crate::ui::sheets::frame::popup_frame;
use crate::util::{scaled_px, style, to_gpui, FONT_FAMILY};

actions!(channel_open_sheet, [ChannelOpenConfirm]);

/// Enter confirms while the sheet's card has focus.
pub fn bind_channel_open_keys(cx: &mut App) {
    cx.bind_keys(bind_return(
        RETURN_KEYS,
        ChannelOpenConfirm,
        Some("ChannelOpenSheet"),
    ));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChannelOpenEvent {
    /// Enter: the shell joins the channel.
    Confirmed,
    /// Escape or a press outside the card: nothing happens.
    Dismissed,
}

pub struct ChannelOpenSheet {
    channel: String,
    focus: FocusHandle,
}

impl ChannelOpenSheet {
    pub fn new(channel: String, cx: &mut Context<Self>) -> Self {
        Self {
            channel,
            focus: cx.focus_handle(),
        }
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }
}

impl EventEmitter<ChannelOpenEvent> for ChannelOpenSheet {}

impl Render for ChannelOpenSheet {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_style = style(cx);
        let entity = cx.entity();
        let confirm = entity.clone();
        let dismiss = entity;
        let ink = to_gpui(app_style.ink_color());
        let muted = to_gpui(app_style.muted_color());

        let card = div()
            .w(scaled_px(&app_style, 360.))
            .p(scaled_px(&app_style, 16.))
            .bg(to_gpui(app_style.raised_color()))
            .border_1()
            .border_color(to_gpui(app_style.divider_color()))
            .rounded(scaled_px(&app_style, 9.))
            .flex()
            .flex_col()
            .gap(scaled_px(&app_style, 8.))
            .key_context("ChannelOpenSheet")
            .track_focus(&self.focus)
            .on_action(move |_: &ChannelOpenConfirm, _, cx| {
                confirm.update(cx, |_, cx| cx.emit(ChannelOpenEvent::Confirmed));
            })
            .child(
                div()
                    .text_size(scaled_px(&app_style, 14.))
                    .text_color(ink)
                    .font_family(FONT_FAMILY)
                    .child(format!("Open {}?", self.channel)),
            )
            .child(
                div()
                    .text_size(scaled_px(&app_style, 12.))
                    .text_color(muted)
                    .font_family(FONT_FAMILY)
                    .child("Enter opens it. Escape cancels."),
            );

        popup_frame(&app_style, card, move |_, cx| {
            dismiss.update(cx, |_, cx| cx.emit(ChannelOpenEvent::Dismissed));
        })
    }
}
