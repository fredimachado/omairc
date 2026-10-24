use gpui::{div, prelude::*, ImageSource, MouseButton};

use crate::controller::snapshots::ConversationRow;
use crate::model::{ConversationRef, Style};
use crate::ui::widgets::avatar::avatar_with_presence;
use crate::ui::widgets::badge::unread_badge;
use crate::ui::widgets::bot_mark::bot_mark;
use crate::ui::widgets::typing_dots::typing_dots;
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// What a row shows beyond its conversation: whether it is selected, the typing
/// dots' phase, and for a DM its peer's photo and bot flag. A channel passes no
/// photo and no bot.
pub struct RowLook {
    pub selected: bool,
    pub typing_phase: u8,
    /// The peer's photo, when the `avatars` preference allows it and its image is ready.
    pub photo: Option<ImageSource>,
    /// The peer is a bot, so its row gets the bot mark.
    pub bot: bool,
}

/// One channel or direct message row, a port of `ConversationRow.qml`: 36 tall,
/// inset 8. A click reports `target` to `on_select`; a middle click, on a channel
/// or a direct message alike, reports it to `on_close`. The controller decides
/// whether the row closes: a joined channel does not.
pub fn conversation_row(
    target: ConversationRef,
    conv: &ConversationRow,
    look: RowLook,
    style: &Style,
    on_select: Handler<ConversationRef>,
    on_close: Handler<ConversationRef>,
) -> impl IntoElement {
    let ink = to_gpui(style.ink_color());
    let muted = to_gpui(style.muted_color());
    let unread = conv.unread > 0;
    let label_emphasized = sidebar_label_emphasized(look.selected, conv.muted, conv.unread);
    let label_color = if label_emphasized { ink } else { muted };
    let label = if !conv.direct {
        format!("#  {}", conv.target.replace('#', ""))
    } else {
        conv.target.clone()
    };
    let label_left = if !conv.direct { 20. } else { 49. };
    // Keep the label clear of the badge and typing dots at the right edge.
    let label_right = if unread || (conv.typing && conv.direct) {
        40.
    } else {
        8.
    };
    let typing = conv.typing && conv.direct;
    let bot = look.bot && conv.direct;

    div()
        .w_full()
        .h(scaled_px(style, 36.))
        .flex_shrink_0()
        .relative()
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, {
            let target = target.clone();
            move |_, window, cx| on_select(target.clone(), window, cx)
        })
        .on_mouse_down(MouseButton::Middle, move |_, window, cx| {
            on_close(target.clone(), window, cx);
        })
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(scaled_px(style, 8.))
                .right(scaled_px(style, 8.))
                .rounded(scaled_px(style, 7.))
                .when(look.selected, |this| this.bg(to_gpui(style.raised_color())))
                .when(!look.selected, |this| {
                    this.hover(|s| s.bg(to_gpui(style.hover_color())))
                }),
        )
        .when(look.selected, |this| {
            this.child(
                div()
                    .absolute()
                    .left(scaled_px(style, 8.))
                    .top(scaled_px(style, 9.))
                    .w(scaled_px(style, 3.))
                    .h(scaled_px(style, 18.))
                    .rounded_full()
                    .bg(to_gpui(style.accent_color())),
            )
        })
        .when(conv.direct, |this| {
            let presence = if conv.presence.is_empty() {
                "offline"
            } else {
                conv.presence
            };
            this.child(
                div()
                    .absolute()
                    .left(scaled_px(style, 18.))
                    .top(scaled_px(style, 7.))
                    .child(avatar_with_presence(
                        style,
                        &conv.target,
                        22.,
                        11.,
                        look.photo,
                        presence,
                    )),
            )
        })
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(scaled_px(style, label_left))
                .right(scaled_px(style, label_right))
                .flex()
                .items_center()
                .gap(scaled_px(style, 4.))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .text_size(scaled_px(style, 13.))
                        .when(label_emphasized, |this| {
                            this.font_weight(gpui::FontWeight::BOLD)
                        })
                        .text_color(label_color)
                        .font_family(FONT_FAMILY)
                        .child(label),
                )
                .when(bot, |this| this.child(bot_mark(style))),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right(scaled_px(style, 17.))
                .flex()
                .items_center()
                .gap(scaled_px(style, 6.))
                .when(typing, |this| this.child(typing_dots(style, look.typing_phase)))
                .when(unread, |this| {
                    this.child(unread_badge(
                        style,
                        conv.unread,
                        sidebar_badge_mention(conv.mention, conv.muted),
                    ))
                }),
        )
}

/// Ink and bold when selected, or when the conversation is not muted and has unread — Qt
/// `ConversationRow.qml` label `color` and `font.bold`.
pub(crate) fn sidebar_label_emphasized(selected: bool, muted: bool, unread: u32) -> bool {
    selected || (!muted && unread > 0)
}

/// Accent mention badge only when the conversation is not muted — Qt `unreadBadge` colors.
pub(crate) fn sidebar_badge_mention(mention: bool, muted: bool) -> bool {
    mention && !muted
}

#[cfg(test)]
mod tests {
    use super::{sidebar_badge_mention, sidebar_label_emphasized};

    #[test]
    fn muted_unread_row_is_dim_and_not_bold() {
        assert!(!sidebar_label_emphasized(false, true, 3));
    }

    #[test]
    fn unmuted_unread_row_is_emphasized() {
        assert!(sidebar_label_emphasized(false, false, 1));
    }

    #[test]
    fn selected_muted_row_stays_emphasized() {
        assert!(sidebar_label_emphasized(true, true, 0));
    }

    #[test]
    fn muted_demotes_mention_badge() {
        assert!(!sidebar_badge_mention(true, true));
        assert!(sidebar_badge_mention(true, false));
    }
}
