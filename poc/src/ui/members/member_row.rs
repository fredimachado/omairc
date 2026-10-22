use gpui::{div, prelude::*, relative, MouseButton};

use crate::irc::plaintext::plain_irc_text;
use crate::irc::reducer::MemberView;
use crate::model::presence::photo_for;
use crate::model::Style;
use crate::ui::widgets::avatar::{avatar_with_presence, AvatarPhotos};
use crate::ui::widgets::bot_mark::bot_mark;
use crate::ui::widgets::tooltip::text_tooltip;
use crate::ui::widgets::typing_dots::typing_dots;
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// Qt's dimmed glyph for an away member.
const AWAY_GLYPH_OPACITY: f32 = 0.62;

/// The account's cap as a share of the row's width. Qt caps it at
/// `max(48, 40%)`. The 48 floor only binds on a row under 120 px, and the
/// panel's 196 px minimum keeps every row wider, so the share alone is the cap.
const ACCOUNT_MAX_SHARE: f32 = 0.4;

/// What the panel derives for one row, beyond the member itself.
#[derive(Clone, Copy)]
pub struct RowState<'a> {
    /// The local nick: its row is bold and is not a click target.
    pub self_nick: &'a str,
    /// The keyboard cursor's row, drawn raised.
    pub current: bool,
    /// The `avatars` preference: off, the glyph shows initials.
    pub avatars: bool,
    /// Whether away-notify is on, so other members' away state shows.
    pub away_visible: bool,
    /// Whether this member is typing right now.
    pub typing: bool,
    /// The shell's typing pulse, which lights one dot at a time.
    pub typing_phase: u8,
}

/// The hover lines for a member, in Qt's `memberTip` order: the presence word
/// (only when away-notify shows it, or for the local user), the real name as
/// plain text, then the query-header labels. Empty when there is nothing to say.
pub fn member_tip_lines(member: &MemberView, is_self: bool, away_visible: bool) -> Vec<String> {
    let mut lines = Vec::new();
    if away_visible || is_self {
        lines.push(if member.is_away() { "away" } else { "online" }.to_owned());
    }
    let realname = plain_irc_text(&member.realname);
    if !realname.is_empty() {
        lines.push(realname);
    }
    lines.extend(
        member
            .labels
            .iter()
            .filter(|label| !label.is_empty())
            .cloned(),
    );
    lines
}

/// One 43px member row: inset 8 with a radius of 7, `hover` on hover and
/// `raised` for the keyboard cursor. The glyph sits at x 18 and the label at
/// x 56, bold for self and muted when away. The label line reads: the nick,
/// then the account in muted text, the bot mark when the member is a bot, and
/// the typing dots while the member types. The status line sits under the
/// label, muted and 9px. It never dims the row: only the glyph and the label
/// take the away treatment. A click on anyone but `self_nick` reports the nick
/// to `on_nick`, which also gets the pointer cursor. Hovering the row shows
/// `member_tip_lines` as a tooltip.
pub fn member_row(
    member: &MemberView,
    ix: usize,
    style: &Style,
    on_nick: Handler<String>,
    photos: &AvatarPhotos,
    row: RowState<'_>,
) -> impl IntoElement {
    let is_self = member.nick == row.self_nick;
    let away = member.away.is_some();
    let presence = if away { "away" } else { "online" };
    let clickable = !is_self;
    let nick = member.nick.clone();
    let photo = photo_for((!member.avatar.is_empty()).then_some(member.avatar.as_str()), row.avatars)
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
    let raised = to_gpui(style.raised_color());
    let tip = member_tip_lines(member, is_self, row.away_visible);

    // The label sizes from its text (`flex_none`) and gives way when the row runs
    // out of room (`flex_shrink`). It must not wrap: a wrapping text's min-content
    // width is about one glyph, which showed the nick as `~…`. `whitespace_nowrap`
    // keeps the whole nick on one line, and `text_ellipsis` marks the cut.
    let label = div()
        .flex_none()
        .flex_shrink()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(scaled_px(style, 12.))
        .text_color(label_color)
        .font_family(FONT_FAMILY)
        .when(is_self, |this| this.font_weight(gpui::FontWeight::BOLD))
        .child(member.label.clone());
    // The account keeps its width up to its cap, so the label elides first.
    let account = (!member.account.is_empty()).then(|| {
        div()
            .flex_shrink_0()
            .min_w_0()
            .max_w(relative(ACCOUNT_MAX_SHARE))
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_size(scaled_px(style, 12.))
            .text_color(to_gpui(style.muted_color()))
            .font_family(FONT_FAMILY)
            .child(member.account.clone())
    });
    let label_row = div()
        .flex()
        .items_center()
        .gap(scaled_px(style, 4.))
        .min_w_0()
        .child(label)
        .when_some(account, |this, account| this.child(account))
        .when(member.bot, |this| this.child(bot_mark(style)))
        .when(row.typing, |this| {
            this.child(
                div()
                    .flex_shrink_0()
                    .child(typing_dots(style, row.typing_phase)),
            )
        });
    let status = (!member.status.is_empty()).then(|| member.status.clone());

    div()
        .id(("member", ix))
        .h(scaled_px(style, 43.))
        .w_full()
        .px(scaled_px(style, 8.))
        .when(clickable, |this| {
            this.cursor_pointer()
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    on_nick(nick.clone(), window, cx);
                })
        })
        .when(!tip.is_empty(), |this| this.tooltip(text_tooltip(tip)))
        .child(
            div()
                .size_full()
                .rounded(scaled_px(style, 7.))
                .flex()
                .items_center()
                .pl(scaled_px(style, 10.))
                .when(row.current, |this| this.bg(raised))
                .hover(|s| s.bg(hover))
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
                        .ml(scaled_px(style, 10.))
                        .mr(scaled_px(style, 10.))
                        .flex()
                        .flex_col()
                        .child(label_row)
                        .when_some(status, |this, status| {
                            this.child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .text_size(scaled_px(style, 9.))
                                    .text_color(to_gpui(style.muted_color()))
                                    .font_family(FONT_FAMILY)
                                    .child(status),
                            )
                        }),
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::member_tip_lines;
    use crate::irc::presence::Away;
    use crate::irc::reducer::MemberView;

    fn member(nick: &str) -> MemberView {
        MemberView {
            nick: nick.to_owned(),
            label: nick.to_owned(),
            ..Default::default()
        }
    }

    #[test]
    fn a_plain_member_has_no_tip_until_presence_shows() {
        assert!(member_tip_lines(&member("ann"), false, false).is_empty());
        assert_eq!(
            member_tip_lines(&member("ann"), false, true),
            vec!["online"]
        );
    }

    #[test]
    fn presence_shows_for_self_or_when_away_notify_is_on() {
        let mut away = member("ann");
        away.away = Some(Away::default());
        assert_eq!(member_tip_lines(&away, false, false), Vec::<String>::new());
        assert_eq!(member_tip_lines(&away, false, true), vec!["away"]);
        assert_eq!(
            member_tip_lines(&member("ann"), false, true),
            vec!["online"]
        );
        assert_eq!(member_tip_lines(&member("me"), true, false), vec!["online"]);
    }

    #[test]
    fn the_real_name_is_plain_text_and_the_labels_follow_it() {
        let mut ann = member("ann");
        ann.realname = "\u{2}Ann Lee\u{2}".to_owned();
        ann.labels = vec![
            String::new(),
            "unauthenticated".to_owned(),
            "bot".to_owned(),
        ];
        assert_eq!(
            member_tip_lines(&ann, false, false),
            vec!["Ann Lee", "unauthenticated", "bot"]
        );
        ann.away = Some(Away::default());
        assert_eq!(
            member_tip_lines(&ann, false, true),
            vec!["away", "Ann Lee", "unauthenticated", "bot"]
        );
    }
}
