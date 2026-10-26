use std::ops::Range;

use gpui::{
    div, prelude::*, AnyElement, FontWeight, InteractiveText, MouseButton, SharedString, StyledText,
};

use crate::model::format::{FormattedText, TextHit};
use crate::model::topic_line::{clipped_hit_ranges, ellipsis_width};
use crate::model::Style;
use crate::ui::center::header_actions::{render_header_actions, HeaderActions, HeaderOffers};
use crate::ui::format_ui::{gpui_text_style, irc_runs_to_gpui, IrcColors};
use crate::ui::Handler;
use crate::util::{scaled_px, to_gpui, FONT_FAMILY};

/// The header's second line.
pub enum Subtitle<'a> {
    /// Text with nothing to click. Status passes its network's connection line
    /// (`model::status::connection_line`): the last error, or the connection word. A
    /// direct message passes the peer's real name, and an empty line when it has none.
    Plain(&'a str),
    /// A conversation's topic. It is formatted, its URLs and channel names are
    /// clickable, and `on_hit` gets the hit that was clicked. `colors` follows
    /// `/pref colors`.
    Topic {
        text: &'a FormattedText,
        hits: &'a [(Range<usize>, TextHit)],
        colors: IrcColors,
        on_hit: Handler<TextHit>,
    },
}

/// The identity a direct message's title row adds to the nick (Qt's `queryIdentity`):
/// a presence dot in front of the nick, and the account, operator and bot labels after it.
pub struct QueryIdentity<'a> {
    /// `online`, `away` or `offline`. The dot takes its colour from `Style::presence_color`.
    pub presence: &'a str,
    /// The labels after the nick, in muted 11px.
    pub labels: &'a [String],
    /// Whether the dot shows. It shows only where the network sends away-notify
    /// (`awayPresenceVisible`).
    pub away_visible: bool,
}

/// What the header shows. A conversation passes its topic as the subtitle, its people
/// count and its LEAVE, JOIN and CLOSE buttons; Status passes its connection line and no
/// count and no buttons. A direct message also passes its `query` identity.
pub struct HeaderView<'a> {
    pub title: &'a str,
    pub subtitle: Subtitle<'a>,
    /// `Some` for a direct message: the title row shows its presence dot and labels.
    pub query: Option<QueryIdentity<'a>>,
    /// `Some` shows the `N PEOPLE` pill. See `people_pill`.
    pub people: Option<usize>,
    /// The LEAVE, JOIN and CLOSE buttons, left of the pill. `None` on Status.
    pub actions: Option<HeaderActions>,
    /// Whether the member panel shows. The people pill is lit while it does.
    pub members_visible: bool,
    /// Width available to the subtitle row, for topic hit clipping (`TopicLine`).
    pub subtitle_width: f32,
}

/// The count the `N PEOPLE` pill shows for the selection. Only a channel the user is in
/// has one: a direct message, a channel the user has left, and Status hide the pill and
/// the member panel (Qt's `peopleButton` visibility).
pub fn people_pill(channel: bool, joined: bool, count: usize) -> Option<usize> {
    (channel && joined).then_some(count)
}

/// The header's fixed height, the same for a channel, a direct message and Status (Qt's
/// `conversationHeader` and `consoleHeader`).
const HEADER_HEIGHT: f32 = 72.;
/// The text's left edge, and the right edge when nothing sits at the right.
const EDGE: f32 = 24.;
/// The right edge when the buttons and the pill sit at the right (Qt's `headerChrome`).
const CHROME_EDGE: f32 = 19.;
/// The gap between the title column and the buttons and pill.
const CHROME_GAP: f32 = 18.;
const ACTION_WIDTH_LEAVE_JOIN: f32 = 58.;
const ACTION_WIDTH_CLOSE: f32 = 64.;
const CHROME_INNER_GAP: f32 = 8.;
const PEOPLE_PILL_WIDTH: f32 = 74.;

/// Trailing chrome width: action buttons, gaps, and the people pill (`headerChrome`).
pub fn header_chrome_trailing_width(style: &Style, offers: HeaderOffers, has_people: bool) -> f32 {
    let gap = f32::from(scaled_px(style, CHROME_INNER_GAP));
    let mut width = 0.;
    let mut buttons = 0u32;
    if offers.leave {
        width += f32::from(scaled_px(style, ACTION_WIDTH_LEAVE_JOIN));
        buttons += 1;
    }
    if offers.join {
        width += f32::from(scaled_px(style, ACTION_WIDTH_LEAVE_JOIN));
        buttons += 1;
    }
    if offers.close {
        width += f32::from(scaled_px(style, ACTION_WIDTH_CLOSE));
        buttons += 1;
    }
    if buttons > 1 {
        width += gap * (buttons as f32 - 1.);
    }
    if has_people {
        if buttons > 0 {
            width += gap;
        }
        width += f32::from(scaled_px(style, PEOPLE_PILL_WIDTH));
    }
    width
}

/// Width for topic hit clipping: the subtitle column inside the header (`TopicLine`).
pub fn header_subtitle_clip_width(
    style: &Style,
    center_column_width: f32,
    chrome: bool,
    chrome_trailing_width: f32,
) -> f32 {
    let left = f32::from(scaled_px(style, EDGE));
    let right = f32::from(scaled_px(style, if chrome { CHROME_EDGE } else { EDGE }));
    let gap = if chrome {
        f32::from(scaled_px(style, CHROME_GAP))
    } else {
        0.
    };
    (center_column_width - left - right - gap - chrome_trailing_width).max(0.)
}

/// The title, the subtitle line, the action buttons, and the `N PEOPLE` pill when there is
/// a count. A direct message's title row also carries its presence dot and labels.
pub fn render_header(
    view: HeaderView<'_>,
    style: &Style,
    on_toggle_members: Handler<()>,
) -> impl IntoElement {
    let HeaderView {
        title,
        subtitle,
        query,
        people,
        actions,
        members_visible,
        subtitle_width,
    } = view;
    let offers = actions.as_ref().is_some_and(|actions| actions.offers.any());
    let chrome = offers || people.is_some();

    div()
        .h(scaled_px(style, HEADER_HEIGHT))
        .flex_shrink_0()
        .pl(scaled_px(style, EDGE))
        .pr(scaled_px(style, if chrome { CHROME_EDGE } else { EDGE }))
        .border_b_1()
        .border_color(to_gpui(style.divider_color()))
        .flex()
        .items_center()
        .gap(scaled_px(style, CHROME_GAP))
        .child(
            div()
                .flex_grow()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(scaled_px(style, 3.))
                .child(title_line(style, title, query))
                .child(subtitle_line(style, subtitle, subtitle_width)),
        )
        .when(chrome, |this| {
            this.child(
                div()
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(scaled_px(style, 8.))
                    .children(actions.and_then(|actions| render_header_actions(actions, style)))
                    .when_some(people, |this, people| {
                        this.child(render_people_pill(
                            style,
                            people,
                            members_visible,
                            on_toggle_members,
                        ))
                    }),
            )
        })
}

/// The `N PEOPLE` pill (Qt's `peopleButton`): 74 by 30, lit while the member panel shows,
/// and raised on hover. Its border and fill are transparent until then.
fn render_people_pill(
    style: &Style,
    people: usize,
    visible: bool,
    on_toggle: Handler<()>,
) -> impl IntoElement {
    let raised = to_gpui(style.raised_color());
    let divider = to_gpui(style.divider_color());
    let ink = if visible {
        style.ink_color()
    } else {
        style.muted_color()
    };
    div()
        .w(scaled_px(style, 74.))
        .h(scaled_px(style, 30.))
        .rounded(scaled_px(style, 7.))
        .border_1()
        .border_color(if visible {
            divider
        } else {
            gpui::transparent_black()
        })
        .bg(if visible {
            raised
        } else {
            gpui::transparent_black()
        })
        .hover(move |this| this.bg(raised))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .font_family(FONT_FAMILY)
        .font_weight(FontWeight::BOLD)
        .text_size(scaled_px(style, 9.))
        .text_color(to_gpui(ink))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            on_toggle((), window, cx);
        })
        .child(format!("{} PEOPLE", people))
}

/// The name line: the title alone, or for a direct message the presence dot, the nick and
/// the labels in a row. The nick elides first, so the labels keep their width.
fn title_line(style: &Style, title: &str, query: Option<QueryIdentity<'_>>) -> AnyElement {
    let Some(query) = query else {
        return title_text(style, title).w_full().into_any_element();
    };
    let muted = to_gpui(style.muted_color());
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap(scaled_px(style, 8.))
        .when(query.away_visible, |this| {
            this.child(
                div()
                    .size(scaled_px(style, 8.))
                    .flex_shrink_0()
                    .rounded_full()
                    .bg(to_gpui(style.presence_color(query.presence))),
            )
        })
        .child(title_text(style, title).flex_shrink().min_w_0())
        .children(query.labels.iter().map(|label| {
            div()
                .flex_shrink_0()
                .whitespace_nowrap()
                .text_size(scaled_px(style, 11.))
                .text_color(muted)
                .font_family(FONT_FAMILY)
                .child(label.clone())
        }))
        .into_any_element()
}

/// The nick or title in bold 17px, on one line that elides.
fn title_text(style: &Style, title: &str) -> gpui::Div {
    div()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(scaled_px(style, 17.))
        .font_weight(FontWeight::BOLD)
        .text_color(to_gpui(style.ink_color()))
        .font_family(FONT_FAMILY)
        .child(title.to_string())
}

/// The subtitle row, one line that elides (Qt's `TopicLine`). A topic is drawn formatted,
/// and each of its hits is a clickable range of the text.
fn subtitle_line(style: &Style, subtitle: Subtitle<'_>, subtitle_width: f32) -> impl IntoElement {
    let line = div()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(scaled_px(style, 11.))
        .font_family(FONT_FAMILY);
    match subtitle {
        Subtitle::Plain(text) => line
            .text_color(to_gpui(style.muted_color()))
            .child(text.to_string()),
        Subtitle::Topic {
            text,
            hits,
            colors,
            on_hit,
        } => {
            let mut base = gpui_text_style(style, 11., false);
            base.color = to_gpui(style.muted_color());
            let runs = irc_runs_to_gpui(&text.plain, &text.runs, style, &base, colors, None);
            let styled = StyledText::new(SharedString::from(text.plain.clone())).with_runs(runs);
            let font_px = f32::from(scaled_px(style, 11.));
            let char_width = font_px * 0.55;
            let ew = ellipsis_width(char_width);
            let mut clickable = Vec::new();
            let mut targets = Vec::new();
            for (range, hit) in hits.iter() {
                let clips = clipped_hit_ranges(&[range.clone()], &text.plain, subtitle_width, char_width, ew);
                if let Some(clip) = clips.first() {
                    clickable.push(clip.clone());
                    targets.push(hit.clone());
                }
            }
            line.child(InteractiveText::new("header-topic", styled).on_click(
                clickable,
                move |index, window, cx| {
                    if let Some(hit) = targets.get(index) {
                        on_hit(hit.clone(), window, cx);
                    }
                },
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use chrono::{TimeZone, Utc};

    use super::{
        header_chrome_trailing_width, header_subtitle_clip_width, people_pill, EDGE, CHROME_EDGE,
        CHROME_GAP,
    };
    use crate::controller::controller::Controller;
    use crate::model::theme::Theme;
    use crate::model::Style;
    use crate::session::clock::FakeClock;
    use crate::ui::center::header_actions::HeaderOffers;
    use crate::ui::members::column::MEMBERS_PANEL_WIDTH;
    use crate::ui::sidebar::column::SIDEBAR_RAIL_WIDTH;
    use crate::util::scaled_px;

    #[test]
    fn subtitle_clip_width_matches_header_chrome_not_viewport_fraction() {
        let style = Style::new(Theme::defaults_dark(), 1.0);
        let viewport = 1000.;
        let sidebar = f32::from(scaled_px(&style, SIDEBAR_RAIL_WIDTH));
        let members = f32::from(scaled_px(&style, MEMBERS_PANEL_WIDTH));
        let center = viewport - sidebar - members;
        let offers = HeaderOffers {
            leave: true,
            join: false,
            close: false,
        };
        let chrome = true;
        let trailing = header_chrome_trailing_width(&style, offers, true);
        let width = header_subtitle_clip_width(&style, center, chrome, trailing);

        let viewport_guess = viewport * 0.42;
        assert_ne!(
            width,
            viewport_guess,
            "topic hits must not clip against a viewport fraction"
        );

        let left = f32::from(scaled_px(&style, EDGE));
        let right = f32::from(scaled_px(&style, CHROME_EDGE));
        let gap = f32::from(scaled_px(&style, CHROME_GAP));
        let expected = center - left - right - gap - trailing;
        assert_eq!(width, expected.max(0.));
    }

    #[test]
    fn the_pill_shows_the_count_only_on_a_joined_channel() {
        assert_eq!(people_pill(true, true, 12), Some(12));
        assert_eq!(people_pill(true, false, 0), None, "a left channel hides the pill");
        assert_eq!(people_pill(false, false, 3), None, "a direct message hides it");
        assert_eq!(people_pill(false, true, 3), None, "a direct message is never joined");
    }

    #[test]
    fn a_part_hides_the_pill_and_a_join_shows_it_again() {
        let clock = Rc::new(FakeClock::new(
            Utc.with_ymd_and_hms(2026, 9, 12, 12, 0, 0).unwrap(),
        ));
        let mut controller =
            Controller::new_demo_with(clock).expect("the demo starts");
        controller.select_conversation("omarchy", "#omarchy");
        let pill = |controller: &Controller| {
            people_pill(
                controller.is_channel(),
                controller.channel_joined(),
                controller.people_count(),
            )
        };
        assert!(pill(&controller).is_some(), "a joined channel shows its count");

        assert!(controller.leave_selected_channel());
        assert_eq!(pill(&controller), None, "a parted channel hides the pill");

        assert!(controller.join_selected_channel());
        assert!(pill(&controller).is_some(), "the rejoined channel shows its count again");
    }
}
