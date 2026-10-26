//! The Nick sheet (`Ctrl+Shift+K`, and the `ONLINE` heading): the channel's
//! members in panel order. Enter opens or creates the DM. Self is listed but
//! inert.

use gpui::{div, prelude::*, AnyElement, FontWeight};

use crate::controller::controller::Controller;
use crate::model::nick_pick::{can_open_direct, first_selectable, nick_targets, NickTarget};
use crate::model::presence::{
    member_away_chrome_visible, member_presence_dot_visible, photo_for,
};
use crate::irc::reducer::MemberView;
use crate::model::Style;
use crate::ui::sheets::filter_sheet::{match_text, FilterSource};
use crate::ui::widgets::avatar::{avatar_with_presence, AvatarPhotos};
use crate::ui::widgets::bot_mark::bot_mark;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// Qt's dimmed glyph for an away member (`client/src/ui/sheets/nick.rs` `opacity`).
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
    /// Whether away-notify is on for the selected network (`client/src/ui/sheets/nick.rs`
    /// `awayPresenceVisible`), from `Controller::has_away_presence()` when the sheet opens.
    pub away_visible: bool,
}

impl NickSource {
    /// Snapshot the shell passes when opening the nick sheet (`present_nick`).
    pub fn for_open(
        controller: &Controller,
        members: Vec<MemberView>,
        self_nick: String,
        photos: AvatarPhotos,
    ) -> Self {
        Self {
            members,
            self_nick,
            avatars: controller.preferences().avatars,
            photos,
            away_visible: controller.has_away_presence(),
        }
    }
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
        nick_row(
            style,
            item,
            &self.self_nick,
            selected,
            self.avatars,
            self.away_visible,
            &self.photos,
        )
        .into_any_element()
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
    away_visible: bool,
    photos: &AvatarPhotos,
) -> impl IntoElement {
    let member: &MemberView = &target.member;
    let is_self = member.nick == self_nick;
    let away = member.away.is_some();
    let away_chrome = member_away_chrome_visible(away, away_visible, is_self);
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
        member_presence_dot_visible(away_visible, is_self),
    );
    let label_color = if away_chrome {
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
                        .when(away_chrome, |this| this.opacity(AWAY_GLYPH_OPACITY))
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

#[cfg(test)]
mod tests {
    use super::NickSource;
    use crate::controller::controller::Controller;
    use crate::demo::seed::OMARCHY_NETWORK_ID;
    use crate::model::presence::member_presence_dot_visible;

    #[test]
    fn opening_the_nick_sheet_reads_away_presence_from_the_controller() {
        let controller = Controller::seeded_for_tests();
        let source = NickSource::for_open(
            &controller,
            vec![],
            controller.current_nick_for(OMARCHY_NETWORK_ID),
            Default::default(),
        );
        assert_eq!(source.away_visible, controller.has_away_presence());
    }

    #[test]
    fn another_members_dot_stays_off_when_away_notify_is_off() {
        let source = NickSource {
            members: vec![],
            self_nick: "fred".to_owned(),
            avatars: true,
            photos: Default::default(),
            away_visible: false,
        };
        assert!(!member_presence_dot_visible(source.away_visible, false));
    }
}
