use gpui::{div, prelude::*, Entity, FocusHandle, MouseButton, ScrollHandle};

use crate::irc::reducer::MemberView;
use crate::model::Style;
use crate::ui::members::member_row::{member_row, RowState};
use crate::ui::scrollbar::{div_vertical_scrollbar, ScrollbarDragCell};
use crate::ui::widgets::avatar::AvatarPhotos;
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// Panel width in logical pixels (`MembersColumn.qml`).
pub const MEMBERS_PANEL_WIDTH: f32 = 216.;

/// What the member panel shows and answers, for the current channel.
pub struct MembersData<'a> {
    /// The channel's members in panel order (`Controller::members`).
    pub members: &'a [MemberView],
    /// The people count the heading shows, which the people pill shares.
    pub people: usize,
    pub self_nick: &'a str,
    /// A member row click: opens or selects that nick's DM.
    pub on_nick: Handler<String>,
    /// The `ONLINE - N` heading's click: opens the Nick sheet.
    pub on_heading: Handler<()>,
    /// The list's keyboard focus.
    pub focus: &'a FocusHandle,
    /// The current row, drawn raised only while the list has focus.
    pub cursor: Option<usize>,
    /// The `avatars` preference: off, every row shows its initials.
    pub avatars: bool,
    /// The photos this frame draws, by avatar reference (`ui::shell::avatars`).
    pub photos: &'a AvatarPhotos,
    /// The nicks typing in the channel now (`Controller::typing_nicks`), empty when
    /// the network cannot show typing. A member whose nick is listed shows the dots.
    pub typing: &'a [String],
    /// The shell's typing pulse, which lights one dot at a time.
    pub typing_phase: u8,
    /// Whether away-notify is on for the channel's network (`HasAwayPresence`).
    pub away_visible: bool,
}

/// The right-hand member panel for a channel (`MembersColumn.qml`). `host` is
/// only notified by the scrollbar. The list is focusable with its own key
/// context (`MemberList`). The `ONLINE - N` heading is clickable and calls
/// `on_heading`, which opens the Nick sheet.
pub fn render_members<V: 'static>(
    data: MembersData,
    style: &Style,
    scroll: &ScrollHandle,
    host: Entity<V>,
    drag: &ScrollbarDragCell,
) -> impl IntoElement {
    let MembersData {
        members,
        people,
        self_nick,
        on_nick,
        on_heading,
        focus,
        cursor,
        avatars,
        photos,
        typing,
        typing_phase,
        away_visible,
    } = data;
    let heading_color = to_gpui(style.muted_color());
    let ink = to_gpui(style.ink_color());
    div()
        .w(scaled_px(style, MEMBERS_PANEL_WIDTH))
        .min_w(scaled_px(style, 196.))
        .h_full()
        .bg(to_gpui(style.panel_color()))
        .border_l_1()
        .border_color(to_gpui(style.divider_color()))
        .flex()
        .flex_col()
        .min_h_0()
        .child(
            div()
                .id("members-heading")
                .pt(scaled_px(style, 23.))
                .pb(scaled_px(style, 14.))
                .px(scaled_px(style, 20.))
                .flex_shrink_0()
                .cursor_pointer()
                .text_size(scaled_px(style, 9.))
                .font_weight(gpui::FontWeight::BOLD)
                .text_color(heading_color)
                .font_family(FONT_FAMILY)
                .hover(|s| s.text_color(ink))
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    on_heading((), window, cx);
                })
                .child(format!("ONLINE - {}", people)),
        )
        .child(
            div()
                .relative()
                .flex_grow()
                .min_h_0()
                .child(
                    div()
                        .id("member-list")
                        .size_full()
                        .pb(scaled_px(style, 10.))
                        .overflow_y_scroll()
                        .track_scroll(scroll)
                        .track_focus(focus)
                        .key_context("MemberList")
                        .children(members.iter().enumerate().map(|(ix, member)| {
                            let row = RowState {
                                self_nick,
                                current: cursor == Some(ix),
                                avatars,
                                away_visible,
                                typing: typing.contains(&member.nick),
                                typing_phase,
                            };
                            member_row(member, ix, style, on_nick.clone(), photos, row)
                        })),
                )
                .child(div_vertical_scrollbar(scroll, style, host, drag.clone())),
        )
}
