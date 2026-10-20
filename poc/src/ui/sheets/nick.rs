//! The Nick sheet (`Ctrl+Shift+K`, and the `ONLINE` heading): the channel's
//! members in panel order. Enter opens or creates the DM. Self is listed but
//! inert.

use gpui::{div, prelude::*, AnyElement, FontWeight};

use crate::model::nick_pick::{can_open_direct, first_selectable, nick_targets};
use crate::model::{Member, Style};
use crate::ui::sheets::filter_sheet::FilterSource;
use crate::ui::widgets::avatar::avatar_with_presence;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// Qt's dimmed glyph for an away member (`NickSheet.qml` `opacity`).
const AWAY_GLYPH_OPACITY: f32 = 0.62;

pub struct NickSource {
    /// The channel's members when the sheet opened, in panel order.
    pub members: Vec<Member>,
    /// The local nick on the selected conversation's network.
    pub self_nick: String,
}

impl FilterSource for NickSource {
    type Item = Member;

    fn placeholder(&self) -> &'static str {
        "Jump to nick…"
    }

    fn items(&self, query: &str) -> Vec<Member> {
        nick_targets(&self.members, query)
            .into_iter()
            .cloned()
            .collect()
    }

    fn initial_selection(&self, items: &[Member]) -> usize {
        let refs: Vec<&Member> = items.iter().collect();
        first_selectable(&refs, &self.self_nick)
    }

    fn activatable(&self, item: &Member) -> bool {
        can_open_direct(&item.nick, &self.self_nick)
    }

    fn width(&self) -> f32 {
        348.
    }

    fn row_height(&self) -> f32 {
        43.
    }

    fn render_row(&self, item: &Member, selected: bool, style: &Style) -> AnyElement {
        nick_row(style, item, &self.self_nick, selected).into_any_element()
    }
}

/// A 43px nick row: an 8px inset with radius 7, `hover` when selected. The
/// glyph sits at x 18, the label at x 56 (bold for self, muted when away).
fn nick_row(style: &Style, member: &Member, self_nick: &str, selected: bool) -> impl IntoElement {
    let is_self = member.nick == self_nick;
    let away = member.presence == "away";
    let fill = to_gpui(style.nick_avatar_fills()[style.nick_palette_index(&member.nick)]);
    let glyph = avatar_with_presence(
        style,
        28.,
        fill,
        style.initials(&member.nick),
        to_gpui(style.nick_color(&member.nick)),
        11.,
        &member.presence,
    );
    let label_color = if away {
        to_gpui(style.muted_color())
    } else {
        to_gpui(style.ink_color())
    };
    let hover = to_gpui(style.hover_color());

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
                .child(div().when(away, |this| this.opacity(AWAY_GLYPH_OPACITY)).child(glyph))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .font_family(FONT_FAMILY)
                        .text_size(scaled_px(style, 12.))
                        .text_color(label_color)
                        .when(is_self, |this| this.font_weight(FontWeight::BOLD))
                        .child(member.nick.clone()),
                ),
        )
}
