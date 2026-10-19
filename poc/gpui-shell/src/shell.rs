use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui::{
    actions, div, list, px, App, ClipboardItem, Context, Entity, FocusHandle,
    Focusable,     KeyBinding, ListAlignment, ListState, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Render, ScrollHandle, Window, prelude::*,
};
use shared::{
    bench_channel, jump_targets, load_world, map_pointer, Conversation, MockWorld,
    Network, PointerZone, Pos, RowKind, Transcript, Style as OmStyle,
};

use crate::input::SingleLineInput;
use crate::message_row::{render_transcript_row, RowLayoutMap};
use crate::scrollbar::{
    div_vertical_scrollbar, end_scrollbar_drag, handle_scrollbar_mouse_move,
    list_vertical_scrollbar, ScrollbarDrag, ScrollbarDragSlot,
};
use crate::util::{scaled_px, style, theme_path, to_gpui, FONT_FAMILY};

actions!(
    omairc_shell,
    [
        OpenJump,
        JumpUp,
        JumpDown,
        JumpActivate,
        JumpClose,
        TranscriptSelectAll,
        TranscriptCopy,
        TranscriptClearSelection,
        ComposerSubmit,
        ToggleMembers,
    ]
);

pub struct BenchState {
    pub enabled: bool,
    pub started: Instant,
    pub first_frame: Option<Duration>,
    pub scroll_frames: u32,
    pub scroll_start: Option<Instant>,
    pub reflow_start: Option<Instant>,
    pub reflow_done: Option<Duration>,
    pub append_latency: Option<Duration>,
    pub finished: bool,
}

impl Default for BenchState {
    fn default() -> Self {
        Self {
            enabled: false,
            started: Instant::now(),
            first_frame: None,
            scroll_frames: 0,
            scroll_start: None,
            reflow_start: None,
            reflow_done: None,
            append_latency: None,
            finished: false,
        }
    }
}

pub struct Shell {
    pub world: MockWorld,
    pub network_ix: usize,
    pub conversation_ix: usize,
    pub members_open: bool,
    pub jump_open: bool,
    pub jump_selected: usize,
    pub jump_list_scroll: ScrollHandle,
    pub sidebar_scroll: ScrollHandle,
    pub members_scroll: ScrollHandle,
    pub scrollbar_drag: Option<ScrollbarDrag>,
    pub member_realnames: HashMap<String, String>,
    pub transcript: Transcript,
    pub list_state: ListState,
    pub row_layouts: RowLayoutMap,
    pub composer: Entity<SingleLineInput>,
    pub jump_filter: Entity<SingleLineInput>,
    pub focus_handle: FocusHandle,
    pub dragging: bool,
    pub drag_autoscroll: bool,
    pub drag_edge: Option<i8>,
    pub drag_pointer: Option<gpui::Point<Pixels>>,
    pub last_click: (Instant, u32, usize),
    pub typing_phase: u8,
    pub hovered_link: Option<(usize, std::ops::Range<usize>)>,
    pub bench: BenchState,
    pub lines_seed: Option<usize>,
}

impl Shell {
    pub(crate) fn set_scrollbar_drag(&mut self, _slot: ScrollbarDragSlot, drag: ScrollbarDrag) {
        self.scrollbar_drag = Some(drag);
    }

    pub fn current_network(&self) -> &Network {
        &self.world.networks[self.network_ix]
    }

    pub fn current_conversation(&self) -> &Conversation {
        &self.current_network().conversations[self.conversation_ix]
    }

    pub fn switch_conversation(&mut self, network_ix: usize, conversation_ix: usize) {
        self.network_ix = network_ix;
        self.conversation_ix = conversation_ix;
        let conv = self.current_conversation().clone();
        self.transcript
            .set_conversation(&conv.messages);
        self.member_realnames = member_map(&conv);
        self.transcript.clear_selection();
        self.list_state.reset(self.transcript.rows().len());
        self.row_layouts.borrow_mut().clear();
        if conv.is_channel() {
            // keep members panel state
        } else {
            self.members_open = false;
        }
    }

    fn switch_by_ids(&mut self, network_id: &str, conversation_key: &str) {
        for (ni, network) in self.world.networks.iter().enumerate() {
            if network.id != network_id {
                continue;
            }
            for (ci, conv) in network.conversations.iter().enumerate() {
                if conv.key() == conversation_key {
                    self.switch_conversation(ni, ci);
                    return;
                }
            }
        }
    }

    fn inbox_count(&self) -> u32 {
        self.current_network()
            .conversations
            .iter()
            .filter(|c| c.unread > 0)
            .count() as u32
    }

    fn open_jump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.jump_open = true;
        self.jump_selected = 0;
        window.focus(&self.jump_filter.read(cx).focus_handle(cx));
        cx.notify();
    }

    fn close_jump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.jump_open = false;
        window.focus(&self.composer.read(cx).focus_handle(cx));
        cx.notify();
    }

    fn jump_step(&mut self, delta: i32, cx: &mut Context<Self>) {
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

    fn jump_activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let matches = jump_targets(&self.world, self.jump_filter.read(cx).text());
        let Some(target) = matches.get(self.jump_selected) else {
            return;
        };
        if target.kind == shared::JumpKind::Conversation {
            if let Some((net, key)) = target.conversation_id.split_once('\n') {
                self.switch_by_ids(net, key);
            }
        }
        self.jump_open = false;
        window.focus(&self.composer.read(cx).focus_handle(cx));
        cx.notify();
    }

    pub fn send_message(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().trim().to_string();
        if text.is_empty() {
            return;
        }
        let append_start = self.bench.enabled.then(Instant::now);
        let author = self.current_network().nick.clone();
        let continues = self.transcript.rows().last().is_some_and(|r| {
            r.kind == RowKind::Message && r.author.as_deref() == Some(author.as_str())
        });
        let id = self.transcript.rows().len() as u64 + 1;
        let row = shared::row_from_message(
            id,
            &author,
            "now",
            &text,
            continues,
        );
        let old_len = self.transcript.rows().len();
        self.transcript.append(row);
        self.transcript.set_pinned_to_bottom(true);
        self.list_state.splice(old_len..old_len, 1);
        self.composer.update(cx, |c, cx| {
            c.clear();
            cx.notify();
        });
        if let Some(start) = append_start {
            self.bench.append_latency = Some(start.elapsed());
        }
        cx.notify();
    }

    fn handle_transcript_mouse_down(
        &mut self,
        ev: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus_handle(cx));
        let Some(row_ix) = self.row_at_position(ev.position) else {
            return;
        };
        let app_style = style(cx);
        let Some(pos) = self.pos_from_pointer_fixed(ev.position, row_ix, &app_style) else {
            return;
        };
        if ev.modifiers.shift {
            self.transcript.extend(pos);
        } else if !ev.modifiers.platform {
            self.transcript.begin(pos);
        }
        let now = Instant::now();
        if now.duration_since(self.last_click.0) < Duration::from_millis(400)
            && self.last_click.2 == row_ix
        {
            self.last_click.1 += 1;
        } else {
            self.last_click.1 = 1;
        }
        self.last_click.0 = now;
        self.last_click.2 = row_ix;
        if self.last_click.1 >= 3 {
            self.transcript.select_row(row_ix);
        } else if self.last_click.1 == 2 {
            self.transcript.select_word(pos);
        }
        self.dragging = true;
    }

    fn pos_from_pointer_fixed(
        &self,
        position: gpui::Point<Pixels>,
        row_ix: usize,
        app_style: &OmStyle,
    ) -> Option<Pos> {
        let layouts = self.row_layouts.borrow();
        let layout = layouts.get(&row_ix)?;
        let row_bounds = self.list_state.bounds_for_item(row_ix)?;
        let local_y = position.y - row_bounds.origin.y;
        if local_y < layout.avatar_top + layout.header_height {
            return Some(map_pointer(
                self.transcript.rows(),
                row_ix,
                PointerZone::HeaderOrAvatar,
            ));
        }
        if local_y
            < (layout.body_bounds.origin.y - row_bounds.origin.y).max(scaled_px(app_style, 0.))
        {
            return Some(map_pointer(
                self.transcript.rows(),
                row_ix,
                PointerZone::GapAbove,
            ));
        }
        match layout.layout.index_for_position(position) {
            Ok(offset) | Err(offset) => Some(Pos { row: row_ix, offset }),
        }
    }

    fn first_visible_row(&self) -> Option<usize> {
        (0..self.transcript.rows().len()).find(|ix| self.list_state.bounds_for_item(*ix).is_some())
    }

    fn last_visible_row(&self) -> Option<usize> {
        let viewport = self.list_state.viewport_bounds();
        (0..self.transcript.rows().len())
            .rev()
            .find(|ix| {
                self.list_state
                    .bounds_for_item(*ix)
                    .is_some_and(|bounds| bounds.intersects(&viewport))
            })
    }

    fn update_drag_autoscroll(&mut self, position: gpui::Point<Pixels>) {
        self.drag_pointer = Some(position);
        let viewport = self.list_state.viewport_bounds();
        if position.y < viewport.top() {
            self.drag_edge = Some(-1);
            self.drag_autoscroll = true;
        } else if position.y > viewport.bottom() {
            self.drag_edge = Some(1);
            self.drag_autoscroll = true;
        } else {
            self.drag_edge = None;
            self.drag_autoscroll = false;
        }
    }

    fn extend_drag_autoscroll(&mut self, position: gpui::Point<Pixels>, app_style: &OmStyle) {
        let viewport = self.list_state.viewport_bounds();
        if position.y < viewport.top() {
            if let Some(row_ix) = self.first_visible_row() {
                self.transcript.extend(Pos {
                    row: row_ix,
                    offset: 0,
                });
            }
            return;
        }
        if position.y > viewport.bottom() {
            if let Some(row_ix) = self.last_visible_row() {
                let offset = self.transcript.rows()[row_ix].body.plain.len();
                self.transcript.extend(Pos { row: row_ix, offset });
            }
            return;
        }
        if let Some(row_ix) = self.row_at_position(position) {
            if let Some(pos) = self.pos_from_pointer_fixed(position, row_ix, app_style) {
                self.transcript.extend(pos);
            }
        }
    }

    fn handle_drag_mouse_move(&mut self, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        if !self.dragging {
            return;
        }
        self.update_drag_autoscroll(position);
        if !self.drag_autoscroll {
            if let Some(row_ix) = self.row_at_position(position) {
                let app_style = style(cx);
                if let Some(pos) = self.pos_from_pointer_fixed(position, row_ix, &app_style) {
                    self.transcript.extend(pos);
                }
            }
        }
        cx.notify();
    }

    fn end_text_drag(&mut self, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        if !self.dragging {
            return;
        }
        self.dragging = false;
        self.drag_autoscroll = false;
        self.drag_edge = None;
        self.drag_pointer = None;
        if let Some((row_ix, range)) = self.link_at(position) {
            if let Some(url) = self.transcript.rows()[row_ix]
                .body
                .plain
                .get(range.clone())
            {
                println!("{url}");
            }
        }
        cx.notify();
    }

    pub fn tick_drag_autoscroll(&mut self, app_style: &OmStyle) {
        if !self.dragging || !self.drag_autoscroll {
            return;
        }
        let Some(edge) = self.drag_edge else {
            return;
        };
        let delta = px(if edge < 0 { -8. } else { 8. });
        self.transcript.set_pinned_to_bottom(false);
        self.list_state.scroll_by(delta);
        if let Some(position) = self.drag_pointer {
            self.extend_drag_autoscroll(position, app_style);
        }
    }

    fn sync_measured_heights(&mut self) {
        for ix in 0..self.transcript.rows().len() {
            if let Some(bounds) = self.list_state.bounds_for_item(ix) {
                self.transcript
                    .set_measured(ix, f32::from(bounds.size.height) as i32);
            }
        }
    }

    fn render_sidebar(&self, cx: &Context<Self>) -> impl IntoElement {
        let app_style = style(cx);
        let network = self.current_network();
        let sidebar_w = scaled_px(&app_style, 244.);

        div()
            .w(sidebar_w)
            .h_full()
            .flex_shrink_0()
            .bg(to_gpui(app_style.panel_color()))
            .border_r_1()
            .border_color(to_gpui(app_style.divider_color()))
            .flex()
            .flex_col()
            .min_h_0()
            .child(
                div()
                    .p(scaled_px(&app_style, 12.))
                    .border_b_1()
                    .border_color(to_gpui(app_style.divider_color()))
                    .flex()
                    .items_center()
                    .gap(scaled_px(&app_style, 8.))
                    .child(
                        div()
                            .size(scaled_px(&app_style, 28.))
                            .rounded_full()
                            .bg(to_gpui(app_style.accent_color()))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(scaled_px(&app_style, 11.))
                            .text_color(to_gpui(shared::Color::rgb(1., 1., 1.)))
                            .font_family(FONT_FAMILY)
                            .child(network.display_name.chars().next().unwrap_or('?').to_string()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(scaled_px(&app_style, 13.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(to_gpui(app_style.ink_color()))
                                    .font_family(FONT_FAMILY)
                                    .child(network.display_name.clone()),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(scaled_px(&app_style, 4.))
                                    .child(
                                        div()
                                            .size(scaled_px(&app_style, 6.))
                                            .rounded_full()
                                            .bg(to_gpui(app_style.presence_color("online"))),
                                    )
                                    .child(
                                        div()
                                            .text_size(scaled_px(&app_style, 10.))
                                            .text_color(to_gpui(app_style.muted_color()))
                                            .font_family(FONT_FAMILY)
                                            .child("Connected"),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .flex_grow()
                            .flex()
                            .justify_end()
                            .text_size(scaled_px(&app_style, 10.))
                            .text_color(to_gpui(app_style.muted_color()))
                            .font_family(FONT_FAMILY)
                            .child("edit"),
                    ),
            )
            .child({
                let entity = cx.entity();
                div()
                    .relative()
                    .flex_grow()
                    .min_h_0()
                    .child(
                        div()
                            .id("sidebar-conversations")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.sidebar_scroll)
                            .child(self.render_conversation_list(&app_style, cx)),
                    )
                    .child(div_vertical_scrollbar(
                        &self.sidebar_scroll,
                        &app_style,
                        entity,
                        ScrollbarDragSlot::Sidebar,
                    ))
            })
            .child(
                div()
                    .m(scaled_px(&app_style, 10.))
                    .p(scaled_px(&app_style, 10.))
                    .rounded(scaled_px(&app_style, 8.))
                    .bg(to_gpui(app_style.raised_color()))
                    .border_1()
                    .border_color(to_gpui(app_style.divider_color()))
                    .flex()
                    .justify_between()
                    .child(
                        div()
                            .text_size(scaled_px(&app_style, 12.))
                            .text_color(to_gpui(app_style.ink_color()))
                            .font_family(FONT_FAMILY)
                            .child(network.nick.clone()),
                    )
                    .child(
                        div()
                            .text_size(scaled_px(&app_style, 11.))
                            .text_color(to_gpui(app_style.muted_color()))
                            .font_family(FONT_FAMILY)
                            .child(format!("{} inbox", self.inbox_count())),
                    ),
            )
    }

    fn render_conversation_list(&self, app_style: &OmStyle, cx: &Context<Self>) -> impl IntoElement {
        let network = self.current_network();
        let channels: Vec<_> = network
            .conversations
            .iter()
            .enumerate()
            .filter(|(_, c)| c.channel)
            .collect();
        let directs: Vec<_> = network
            .conversations
            .iter()
            .enumerate()
            .filter(|(_, c)| !c.channel)
            .collect();

        div()
            .flex()
            .flex_col()
            .py(scaled_px(app_style, 8.))
            .child(section_label(app_style, "CHANNELS"))
            .children(channels.iter().map(|(ix, conv)| {
                self.conversation_row(*ix, conv, app_style, cx)
            }))
            .child(section_label(app_style, "DIRECT MESSAGES"))
            .children(directs.iter().map(|(ix, conv)| {
                self.conversation_row(*ix, conv, app_style, cx)
            }))
    }

    fn conversation_row(
        &self,
        conv_ix: usize,
        conv: &Conversation,
        app_style: &OmStyle,
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let selected = conv_ix == self.conversation_ix;
        let typing = conv.typing;
        let phase = self.typing_phase;
        div()
            .mx(scaled_px(app_style, 8.))
            .mb(scaled_px(app_style, 2.))
            .h(scaled_px(app_style, 36.))
            .rounded(scaled_px(app_style, 7.))
            .relative()
            .cursor_pointer()
            .bg(if selected {
                to_gpui(app_style.hover_color())
            } else {
                gpui::transparent_black()
            })
            .when(selected, |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(scaled_px(app_style, 9.))
                        .w(px(3.))
                        .h(scaled_px(app_style, 18.))
                        .bg(to_gpui(app_style.accent_color())),
                )
            })
            .on_mouse_down(MouseButton::Left, {
                let entity = cx.entity();
                move |_, window, cx| {
                    entity.update(cx, |shell, cx| {
                        shell.switch_conversation(shell.network_ix, conv_ix);
                        window.focus(&shell.composer.read(cx).focus_handle(cx));
                        cx.notify();
                    });
                }
            })
            .child(
                div()
                    .h_full()
                    .px(scaled_px(app_style, 10.))
                    .flex()
                    .items_center()
                    .gap(scaled_px(app_style, 8.))
                    .when(!conv.channel, |this| {
                        let peer = conv.name.as_str();
                        let fill = to_gpui(
                            app_style.nick_avatar_fills()
                                [app_style.nick_palette_index(peer)],
                        );
                        let presence = conv
                            .members
                            .first()
                            .map(|m| m.presence.as_str())
                            .unwrap_or("online");
                        this.child(avatar_with_presence(
                            app_style,
                            22.,
                            fill,
                            app_style.initials(peer),
                            to_gpui(app_style.nick_color(peer)),
                            11.,
                            presence,
                        ))
                    })
                    .child(
                        div()
                            .flex_grow()
                            .text_size(scaled_px(app_style, 13.))
                            .text_color(to_gpui(app_style.ink_color()))
                            .font_family(FONT_FAMILY)
                            .child(conv.name.clone()),
                    )
                    .when(typing, |this| {
                        this.child(typing_dots(app_style, phase))
                    })
                    .when(conv.unread > 0, |this| {
                        this.child(
                            div()
                                .min_w(scaled_px(app_style, 19.))
                                .h(scaled_px(app_style, 19.))
                                .px(scaled_px(app_style, 5.))
                                .rounded_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .text_size(scaled_px(app_style, 10.))
                                .bg(if conv.mention {
                                    to_gpui(app_style.accent_color())
                                } else {
                                    to_gpui(app_style.hover_color())
                                })
                                .text_color(to_gpui(app_style.ink_color()))
                                .font_family(FONT_FAMILY)
                                .child(conv.unread.to_string()),
                        )
                    }),
            )
    }

    fn render_members(&self, cx: &Context<Self>) -> impl IntoElement {
        let app_style = style(cx);
        let conv = self.current_conversation();
        let online = conv
            .members
            .iter()
            .filter(|m| m.presence == "online")
            .count();
        div()
            .w(scaled_px(&app_style, 216.))
            .h_full()
            .flex_shrink_0()
            .bg(to_gpui(app_style.panel_color()))
            .border_l_1()
            .border_color(to_gpui(app_style.divider_color()))
            .flex()
            .flex_col()
            .min_h_0()
            .child(
                div()
                    .p(scaled_px(&app_style, 12.))
                    .border_b_1()
                    .border_color(to_gpui(app_style.divider_color()))
                    .text_size(scaled_px(&app_style, 10.))
                    .text_color(to_gpui(app_style.muted_color()))
                    .font_family(FONT_FAMILY)
                    .child(format!("ONLINE — {}", online)),
            )
            .child({
                let entity = cx.entity();
                div()
                    .relative()
                    .flex_grow()
                    .min_h_0()
                    .child(
                        div()
                            .id("member-list")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.members_scroll)
                            .children(conv.members.iter().map(|member| {
                                let nick_color = to_gpui(app_style.nick_color(&member.nick));
                                let fill = to_gpui(
                                    app_style.nick_avatar_fills()
                                        [app_style.nick_palette_index(&member.nick)],
                                );
                                div()
                                    .px(scaled_px(&app_style, 10.))
                                    .py(scaled_px(&app_style, 6.))
                                    .flex()
                                    .gap(scaled_px(&app_style, 8.))
                                    .child(avatar_with_presence(
                                        &app_style,
                                        28.,
                                        fill,
                                        app_style.initials(&member.nick),
                                        nick_color,
                                        11.,
                                        &member.presence,
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .text_size(scaled_px(&app_style, 12.))
                                                    .text_color(nick_color)
                                                    .font_family(FONT_FAMILY)
                                                    .child(format!(
                                                        "{}{}",
                                                        member.prefix, member.nick
                                                    )),
                                            )
                                            .child(
                                                div()
                                                    .text_size(scaled_px(&app_style, 10.))
                                                    .text_color(to_gpui(app_style.muted_color()))
                                                    .font_family(FONT_FAMILY)
                                                    .child(member.realname.clone()),
                                            ),
                                    )
                            })),
                    )
                    .child(div_vertical_scrollbar(
                        &self.members_scroll,
                        &app_style,
                        entity,
                        ScrollbarDragSlot::Members,
                    ))
            })
    }

    fn render_center(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let app_style = style(cx);
        let conv = self.current_conversation().clone();
        let network = self.current_network().clone();
        let header_label = if conv.channel {
            format!(
                "{} · {} · {}",
                conv.name, network.display_name, network.nick
            )
        } else {
            conv.name.clone()
        };
        let topic = conv.topic.clone().unwrap_or_default();
        let people = conv.members.len();
        let show_people = conv.channel;

        div()
            .flex_grow()
            .h_full()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(to_gpui(app_style.page_color()))
            .child(
                div()
                    .px(scaled_px(&app_style, 16.))
                    .py(scaled_px(&app_style, 10.))
                    .border_b_1()
                    .border_color(to_gpui(app_style.divider_color()))
                    .flex()
                    .flex_col()
                    .gap(scaled_px(&app_style, 4.))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(scaled_px(&app_style, 8.))
                            .child(
                                div()
                                    .flex_grow()
                                    .text_size(scaled_px(&app_style, 14.))
                                    .font_weight(gpui::FontWeight::BOLD)
                                    .text_color(to_gpui(app_style.ink_color()))
                                    .font_family(FONT_FAMILY)
                                    .child(header_label),
                            )
                            .when(show_people, |this| {
                                this.child(
                                    div()
                                        .px(scaled_px(&app_style, 8.))
                                        .py(scaled_px(&app_style, 3.))
                                        .rounded_full()
                                        .border_1()
                                        .border_color(to_gpui(app_style.divider_color()))
                                        .cursor_pointer()
                                        .text_size(scaled_px(&app_style, 10.))
                                        .text_color(to_gpui(app_style.muted_color()))
                                        .font_family(FONT_FAMILY)
                                        .child(format!("{} PEOPLE", people))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|shell, _, _, cx| {
                                                shell.members_open = !shell.members_open;
                                                cx.notify();
                                            }),
                                        ),
                                )
                            }),
                    )
                    .child(
                        div()
                            .text_size(scaled_px(&app_style, 11.))
                            .text_color(to_gpui(app_style.muted_color()))
                            .font_family(FONT_FAMILY)
                            .child(topic),
                    ),
            )
            .child(self.render_transcript(&app_style, cx))
            .child(self.render_composer(&app_style, cx))
    }

    fn render_transcript(&mut self, app_style: &OmStyle, cx: &mut Context<Self>) -> impl IntoElement {
        let row_layouts = self.row_layouts.clone();
        let entity = cx.entity();

        self.list_state.set_scroll_handler({
            let entity = entity.clone();
            move |event, _, cx| {
                let range: Vec<usize> = event.visible_range.clone().collect();
                let entity = entity.clone();
                cx.spawn(async move |cx| {
                    cx.background_executor()
                        .timer(Duration::from_millis(0))
                        .await;
                    entity
                        .update(cx, |shell, cx| {
                            for ix in range {
                                if let Some(bounds) = shell.list_state.bounds_for_item(ix) {
                                    shell.transcript.set_measured(
                                        ix,
                                        f32::from(bounds.size.height) as i32,
                                    );
                                }
                            }
                            cx.notify();
                        })
                        .ok();
                })
                .detach();
            }
        });

        div()
            .id("transcript")
            .relative()
            .flex_grow()
            .min_h_0()
            .focusable()
            .track_focus(&self.focus_handle(cx))
            .key_context("Transcript")
            .on_action(cx.listener(|s, _: &TranscriptSelectAll, _, cx| {
                s.transcript.select_all();
                cx.notify();
            }))
            .on_action(cx.listener(|s, _: &TranscriptCopy, _, cx| {
                let text = s.transcript.copy_text();
                if !text.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }))
            .on_action(cx.listener(|s, _: &TranscriptClearSelection, _, cx| {
                s.transcript.clear_selection();
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|shell, ev: &MouseDownEvent, window, cx| {
                    shell.handle_transcript_mouse_down(ev, window, cx);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|shell, ev: &MouseMoveEvent, _, cx| {
                if shell.scrollbar_drag.is_some() {
                    let app_style = style(cx);
                    handle_scrollbar_mouse_move(shell, ev.position, &app_style, cx);
                } else if !shell.dragging {
                    shell.hovered_link = shell.link_at(ev.position);
                    cx.notify();
                }
            }))
            .child({
                let list_state = self.list_state.clone();
                let layouts = row_layouts;
                let member_realnames = self.member_realnames.clone();
                list(list_state, {
                    let entity = entity.clone();
                    move |ix, _window, app| {
                    let app_style = app.global::<crate::util::OmaircStyleGlobal>().style.clone();
                    entity.update(app, |shell, _cx| {
                        let row = shell
                            .transcript
                            .rows()
                            .get(ix)
                            .cloned()
                            .unwrap_or_else(|| shared::row_from_message(0, "", "", "", false));
                        let selection = shell.transcript.range_for_row(ix);
                        let hover = shell
                            .hovered_link
                            .as_ref()
                            .filter(|(r, _)| *r == ix)
                            .map(|(_, range)| range.clone());
                        div()
                            .id(("transcript-row-hit", ix))
                            .w_full()
                            .child(render_transcript_row(
                                ix,
                                &row,
                                &app_style,
                                &member_realnames,
                                selection,
                                hover,
                                layouts.clone(),
                            ))
                            .on_mouse_down(
                                MouseButton::Left,
                                {
                                    let entity = entity.clone();
                                    move |ev: &MouseDownEvent, window, cx| {
                                        entity.update(cx, |shell, cx| {
                                            shell.handle_transcript_mouse_down(ev, window, cx);
                                            cx.notify();
                                        });
                                    }
                                },
                            )
                            .into_any_element()
                    })
                }})
                .size_full()
                .with_sizing_behavior(gpui::ListSizingBehavior::Infer)
            })
            .child(list_vertical_scrollbar(
                self.list_state.clone(),
                app_style,
                entity,
                px(self.transcript.rows().len() as f32 * 72.),
            ))
    }

    fn row_at_position(&self, position: gpui::Point<Pixels>) -> Option<usize> {
        for ix in 0..self.transcript.rows().len() {
            if self
                .list_state
                .bounds_for_item(ix)
                .is_some_and(|b| b.contains(&position))
            {
                return Some(ix);
            }
        }
        None
    }

    fn link_at(
        &self,
        position: gpui::Point<Pixels>,
    ) -> Option<(usize, std::ops::Range<usize>)> {
        let row_ix = self.row_at_position(position)?;
        let row = self.transcript.rows().get(row_ix)?;
        let layout = self.row_layouts.borrow().get(&row_ix)?.layout.clone();
        let offset = layout.index_for_position(position).ok()?;
        row.body
            .links
            .iter()
            .find(|range| range.contains(&offset))
            .cloned()
            .map(|range| (row_ix, range))
    }

    fn render_composer(&self, app_style: &OmStyle, cx: &Context<Self>) -> impl IntoElement {
        div()
            .px(scaled_px(app_style, 16.))
            .py(scaled_px(app_style, 10.))
            .border_t_1()
            .border_color(to_gpui(app_style.divider_color()))
            .flex()
            .gap(scaled_px(app_style, 8.))
            .items_center()
            .child(
                div()
                    .flex_grow()
                    .key_context("Composer")
                    .on_action(cx.listener(|s, _: &ComposerSubmit, window, cx| {
                        s.send_message(cx);
                        window.focus(&s.composer.read(cx).focus_handle(cx));
                    }))
                    .child(self.composer.clone()),
            )
            .child(
                div()
                    .px(scaled_px(app_style, 12.))
                    .py(scaled_px(app_style, 8.))
                    .rounded(scaled_px(app_style, 7.))
                    .bg(to_gpui(app_style.accent_color()))
                    .cursor_pointer()
                    .text_size(scaled_px(app_style, 12.))
                    .text_color(to_gpui(shared::Color::rgb(1., 1., 1.)))
                    .font_family(FONT_FAMILY)
                    .child("SEND")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|s, _, window, cx| {
                            s.send_message(cx);
                            window.focus(&s.composer.read(cx).focus_handle(cx));
                        }),
                    ),
            )
    }

    fn render_jump(&self, cx: &Context<Self>) -> impl IntoElement {
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

fn avatar_with_presence(
    style: &OmStyle,
    avatar_px: f32,
    fill: gpui::Hsla,
    initials: String,
    initials_color: gpui::Hsla,
    initials_size: f32,
    presence: &str,
) -> impl IntoElement {
    let avatar_sz = scaled_px(style, avatar_px);
    let dot_sz = scaled_px(style, 7.);
    let inset = avatar_sz - dot_sz;
    div()
        .relative()
        .size(avatar_sz)
        .flex_shrink_0()
        .child(
            div()
                .size_full()
                .rounded_full()
                .bg(fill)
                .flex()
                .items_center()
                .justify_center()
                .text_size(scaled_px(style, initials_size))
                .text_color(initials_color)
                .font_family(FONT_FAMILY)
                .child(initials),
        )
        .child(
            div()
                .absolute()
                .left(inset)
                .top(inset)
                .size(dot_sz)
                .rounded_full()
                .border(scaled_px(style, 2.))
                .border_color(to_gpui(style.panel_color()))
                .bg(to_gpui(style.presence_color(presence))),
        )
}

fn section_label(style: &OmStyle, text: &str) -> impl IntoElement {
    div()
        .px(scaled_px(style, 12.))
        .pt(scaled_px(style, 8.))
        .pb(scaled_px(style, 4.))
        .text_size(scaled_px(style, 10.))
        .text_color(to_gpui(style.muted_color()))
        .font_family(FONT_FAMILY)
        .child(text.to_string())
}

fn typing_dots(style: &OmStyle, phase: u8) -> impl IntoElement {
    div()
        .flex()
        .gap(scaled_px(style, 3.))
        .children((0..3).map(|i| {
            let active = (phase as usize) % 3 == i as usize;
            div()
                .size(scaled_px(style, 4.))
                .rounded_full()
                .bg(to_gpui(if active {
                    style.accent_color()
                } else {
                    style.muted_color()
                }))
        }))
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.bench.enabled && self.bench.first_frame.is_none() {
            self.bench.first_frame = Some(self.bench.started.elapsed());
        }

        let show_members =
            self.members_open && self.current_conversation().is_channel();

        div()
            .size_full()
            .bg(to_gpui(style(cx).page_color()))
            .flex()
            .flex_row()
            .on_mouse_move(cx.listener(|shell, ev: &MouseMoveEvent, _, cx| {
                if shell.scrollbar_drag.is_some() {
                    let app_style = style(cx);
                    handle_scrollbar_mouse_move(shell, ev.position, &app_style, cx);
                } else {
                    shell.handle_drag_mouse_move(ev.position, cx);
                }
            }))
            .capture_any_mouse_up(cx.listener(|shell, ev: &MouseUpEvent, _, cx| {
                if ev.button == MouseButton::Left {
                    end_scrollbar_drag(shell);
                    shell.end_text_drag(ev.position, cx);
                }
            }))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|shell, ev: &MouseUpEvent, _, cx| {
                    shell.end_text_drag(ev.position, cx);
                }),
            )
            .on_action(cx.listener(|shell, _: &OpenJump, window, cx| {
                shell.open_jump(window, cx);
            }))
            .child(self.render_sidebar(cx))
            .child(self.render_center(cx))
            .when(show_members, |this| this.child(self.render_members(cx)))
            .when(self.jump_open, |this| this.child(self.render_jump(cx)))
    }
}

impl Focusable for Shell {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

pub fn bind_shell_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-k", OpenJump, None),
        KeyBinding::new("up", JumpUp, Some("JumpSheet")),
        KeyBinding::new("down", JumpDown, Some("JumpSheet")),
        KeyBinding::new("enter", JumpActivate, Some("JumpSheet")),
        KeyBinding::new("escape", JumpClose, Some("JumpSheet")),
        KeyBinding::new("up", JumpUp, Some("JumpFilter")),
        KeyBinding::new("down", JumpDown, Some("JumpFilter")),
        KeyBinding::new("enter", JumpActivate, Some("JumpFilter")),
        KeyBinding::new("escape", JumpClose, Some("JumpFilter")),
        KeyBinding::new("ctrl-a", TranscriptSelectAll, Some("Transcript")),
        KeyBinding::new("ctrl-c", TranscriptCopy, Some("Transcript")),
        KeyBinding::new("escape", TranscriptClearSelection, Some("Transcript")),
        KeyBinding::new("enter", ComposerSubmit, Some("Composer")),
    ]);
}

impl Shell {
    pub fn new(
        cx: &mut Context<Self>,
        lines: Option<usize>,
        bench: bool,
    ) -> Self {
        let mut world = load_world();
        if let Some(n) = lines {
            for network in &mut world.networks {
                network.conversations.retain(|c| c.name != "#bench");
                network.conversations.push(bench_channel(n));
            }
        }

        let mut shell = Self {
            world,
            network_ix: 0,
            conversation_ix: 0,
            members_open: true,
            jump_open: false,
            jump_selected: 0,
            jump_list_scroll: ScrollHandle::new(),
            sidebar_scroll: ScrollHandle::new(),
            members_scroll: ScrollHandle::new(),
            scrollbar_drag: None,
            member_realnames: HashMap::new(),
            transcript: Transcript::new(72),
            list_state: ListState::new(0, ListAlignment::Bottom, px(120.)),
            row_layouts: Rc::new(RefCell::new(Default::default())),
            composer: cx.new(|cx| {
                SingleLineInput::new("", "Composer", cx.focus_handle())
            }),
            jump_filter: cx.new(|cx| {
                SingleLineInput::new("Jump to conversation…", "JumpFilter", cx.focus_handle())
            }),
            focus_handle: cx.focus_handle(),
            dragging: false,
            drag_autoscroll: false,
            drag_edge: None,
            drag_pointer: None,
            last_click: (Instant::now(), 0, 0),
            typing_phase: 0,
            hovered_link: None,
            bench: BenchState {
                enabled: bench,
                ..Default::default()
            },
            lines_seed: lines,
        };

        if lines.is_some_and(|n| n > 0) {
            if let Some(ci) = shell.world.networks[shell.network_ix]
                .conversations
                .iter()
                .position(|c| c.name == "#bench")
            {
                shell.conversation_ix = ci;
            }
        }

        let conv = shell.current_conversation().clone();
        shell.transcript
            .set_conversation(&conv.messages);
        shell.member_realnames = member_map(&conv);
        shell
            .list_state
            .reset(shell.transcript.rows().len());
        shell
    }
}

fn member_map(conv: &Conversation) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for m in &conv.members {
        map.insert(m.nick.clone(), m.realname.clone());
    }
    if let Some(real) = &conv.peer_realname {
        if let Some(name) = conv.name.strip_prefix('#').or(Some(conv.name.as_str())) {
            map.entry(name.to_string()).or_insert_with(|| real.clone());
        }
        map.entry(conv.name.clone()).or_insert_with(|| real.clone());
    }
    map
}
