//! The Nick sheet (`Ctrl+Shift+K`, and the `ONLINE` heading): the channel's
//! members in panel order. Enter opens or creates the DM. Self is listed but
//! inert.

use gpui::{div, prelude::*, AnyElement, FontWeight};

use crate::model::nick_pick::{can_open_direct, first_selectable, nick_targets, NickTarget};
use crate::model::presence::photo_for;
use crate::irc::reducer::MemberView;
use crate::model::Style;
use crate::ui::sheets::filter_sheet::{match_text, FilterSource};
use crate::ui::widgets::avatar::{avatar_with_presence, AvatarPhotos};
use crate::ui::widgets::bot_mark::bot_mark;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// Qt's dimmed glyph for an away member (`NickSheet.qml` `opacity`).
const AWAY_GLYPH_OPACITY: f32 = 0.62;

pub struct NickSource {
    /// The channel's members when the sheet opened, in panel order.
    pub members: Vec<MemberView>,
    /// The local nick on the selected conversation's network.
    pub self_nick: String,
    /// The `avatars` preference when the sheet opened.
    pub avatars: bool,
    /// The photos the frame draws, by avatar reference. The shell keeps them current.
    pub photos: AvatarPhotos,
}

impl FilterSource for NickSource {
    type Item = NickTarget;

    fn placeholder(&self) -> &'static str {
        "Jump to nick…"
    }

    fn items(&self, query: &str) -> Vec<NickTarget> {
        nick_targets(&self.members, query)
    }

    fn initial_selection(&self, items: &[NickTarget]) -> usize {
        first_selectable(items, &self.self_nick)
    }

    fn activatable(&self, item: &NickTarget) -> bool {
        can_open_direct(&item.member.nick, &self.self_nick)
    }

    fn width(&self) -> f32 {
        348.
    }

    fn row_height(&self) -> f32 {
        43.
    }

    fn render_row(&self, item: &NickTarget, selected: bool, style: &Style) -> AnyElement {
        nick_row(style, item, &self.self_nick, selected, self.avatars, &self.photos).into_any_element()
    }
}

/// A 43px nick row: an 8px inset with radius 7, `hover` when selected. The
/// glyph sits at x 18, the label at x 56 (bold for self, muted when away). A bot
/// gets its mark after the nick, and the status line sits under it, muted and
/// 9px. The status never dims the row.
fn nick_row(
    style: &Style,
    target: &NickTarget,
    self_nick: &str,
    selected: bool,
    avatars: bool,
    photos: &AvatarPhotos,
) -> impl IntoElement {
    let member: &MemberView = &target.member;
    let is_self = member.nick == self_nick;
    let away = member.away.is_some();
    let presence = if away { "away" } else { "online" };
    let photo = photo_for((!member.avatar.is_empty()).then_some(member.avatar.as_str()), avatars)
        .and_then(|reference| photos.get(reference));
    let glyph = avatar_with_presence(
        style,
        &member.nick,
        28.,
        11.,
        photo,
        presence,
    );
    let label_color = if away {
        to_gpui(style.muted_color())
    } else {
        to_gpui(style.ink_color())
    };
    let hover = to_gpui(style.hover_color());
    let status = (!member.status.is_empty()).then(|| member.status.clone());

    // Same as the member row: the nick sizes from its text (`flex_none`), gives
    // way when the row runs out of room (`flex_shrink`), and never wraps. A
    // `flex_1` nick has a zero basis, so inside a column that does not grow the
    // whole label collapsed to nothing and only the avatar showed.
    let label = div()
        .min_w_0()
        .flex()
        .items_center()
        .gap(scaled_px(style, 4.))
        .child(
            div()
                .flex_none()
                .flex_shrink()
                .min_w_0()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .font_family(FONT_FAMILY)
                .text_size(scaled_px(style, 12.))
                .text_color(label_color)
                .when(is_self, |this| this.font_weight(FontWeight::BOLD))
                .child(match_text(
                    member.nick.clone(),
                    &target.indices,
                    to_gpui(style.ink_color()),
                )),
        )
        .when(member.bot, |this| this.child(bot_mark(style)));

    div()
        .h(scaled_px(style, 43.))
        .w_full()
        .px(scaled_px(style, 8.))
        .when(!is_self, |this| this.cursor_pointer())
        .child(
            div()
                .size_full()
                .rounded(scaled_px(style, 7.))
                .flex()
                .items_center()
                .pl(scaled_px(style, 10.))
                .gap(scaled_px(style, 10.))
                .overflow_hidden()
                .bg(if selected {
                    hover
                } else {
                    gpui::transparent_black()
                })
                .hover(move |s| s.bg(hover))
                .child(
                    div()
                        .flex_shrink_0()
                        .when(away, |this| this.opacity(AWAY_GLYPH_OPACITY))
                        .child(glyph),
                )
                .child(
                    div()
                        .flex_grow()
                        .min_w_0()
                        .mr(scaled_px(style, 10.))
                        .flex()
                        .flex_col()
                        .child(label)
                        .when_some(status, |this, status| {
                            this.child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .font_family(FONT_FAMILY)
                                    .text_size(scaled_px(style, 9.))
                                    .text_color(to_gpui(style.muted_color()))
                                    .child(status),
                            )
                        }),
                ),
        )
}
