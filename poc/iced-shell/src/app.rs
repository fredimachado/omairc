use crate::fonts::{bold, IA_WRITER};
use crate::transcript_widget::{SelectKind, TranscriptMessage, TranscriptWidget};
use crate::convert::to_iced;
use iced::keyboard::{self, key::Named, Key};
use iced::widget::{
    button, column, container, mouse_area, row, scrollable, text, text_input, Space,
};
use iced::futures::SinkExt;
use iced::widget::operation::{self, AbsoluteOffset};
use iced::{clipboard, Element, Length, Size, Subscription, Task};
use shared::{
    jump_targets, row_from_message, Conversation, JumpKind, JumpTarget, MockWorld, Network,
    Style, Theme, ThemeWatcher, Transcript,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const SIDEBAR_W: f32 = 244.0;
const MEMBERS_W: f32 = 216.0;

fn px(style: &Style, value: f32) -> f32 {
    style.scaled(value) as f32
}

pub struct App {
    world: MockWorld,
    style: Style,
    theme_path: Option<PathBuf>,
    theme_watcher: Arc<Mutex<ThemeWatcher>>,
    theme_generation: u64,
    transcript: Transcript,
    conversation_id: String,
    network_id: String,
    members_open: bool,
    composer: String,
    jump_open: bool,
    jump_filter: String,
    jump_selected: usize,
    jump_matches: Vec<JumpTarget>,
    member_realnames: HashMap<String, String>,
    transcript_focused: bool,
    drag_scroll: bool,
    drag_edge: Option<i8>,
    shift_held: bool,
    typing_phase: u8,
    bench: bool,
    bench_state: BenchState,
    bench_started: Instant,
    first_frame_ms: Option<f64>,
    composer_id: iced::widget::Id,
    jump_filter_id: iced::widget::Id,
    jump_list_id: iced::widget::Id,
    next_message_id: u64,
}

struct BenchState {
    phase: BenchPhase,
    scroll_step: usize,
    frame_times: Vec<Instant>,
    reflow_start: Option<Instant>,
    reflow_ms: Option<f64>,
    append_at: Option<Instant>,
    append_latency_ms: Option<f64>,
}

enum BenchPhase {
    Idle,
    WaitFirstFrame,
    Scrolling,
    Resize,
    Append,
    Done,
}

impl App {
    pub fn new(
        world: MockWorld,
        theme: Theme,
        scale: f32,
        theme_path: Option<PathBuf>,
        theme_watcher: Arc<Mutex<ThemeWatcher>>,
        bench: bool,
    ) -> Self {
        let style = Style::new(theme, scale);
        let mut transcript = Transcript::new(72);
        let (network_id, conversation_id) = default_conversation(&world);
        let conv = find_conversation(&world, &network_id, &conversation_id);
        let next_message_id = conv.messages.len() as u64 + 1000;
        transcript.set_conversation(&conv.messages);
        let member_realnames = member_map(conv);

        let mut app = Self {
            jump_matches: jump_targets(&world, ""),
            world,
            style,
            theme_path,
            theme_watcher,
            theme_generation: 0,
            transcript,
            conversation_id,
            network_id,
            members_open: true,
            composer: String::new(),
            jump_open: false,
            jump_filter: String::new(),
            jump_selected: 0,
            member_realnames,
            transcript_focused: false,
            drag_scroll: false,
            shift_held: false,
            typing_phase: 0,
            bench,
            bench_state: BenchState {
                phase: if bench {
                    BenchPhase::WaitFirstFrame
                } else {
                    BenchPhase::Idle
                },
                scroll_step: 0,
                frame_times: Vec::new(),
                reflow_start: None,
                reflow_ms: None,
                append_at: None,
                append_latency_ms: None,
            },
            bench_started: Instant::now(),
            first_frame_ms: None,
            drag_edge: None,
            composer_id: iced::widget::Id::unique(),
            jump_filter_id: iced::widget::Id::unique(),
            jump_list_id: iced::widget::Id::unique(),
            next_message_id,
        };
        app.sync_jump_matches();
        app
    }

    fn current_conversation(&self) -> &Conversation {
        find_conversation(&self.world, &self.network_id, &self.conversation_id)
    }

    fn current_network(&self) -> &Network {
        self.world
            .networks
            .iter()
            .find(|n| n.id == self.network_id)
            .expect("network")
    }

    fn sync_jump_matches(&mut self) {
        self.jump_matches = jump_targets(&self.world, &self.jump_filter);
        if self.jump_selected >= self.jump_matches.len() {
            self.jump_selected = self.jump_matches.len().saturating_sub(1);
        }
    }

    fn switch_conversation(&mut self, network_id: String, conversation_id: String) {
        if conversation_id.is_empty() {
            return;
        }
        self.network_id = network_id;
        self.conversation_id = conversation_id;
        let conv = find_conversation(&self.world, &self.network_id, &self.conversation_id);
        self.transcript.set_conversation(&conv.messages);
        self.member_realnames = member_map(conv);
        self.members_open = conv.channel && self.members_open;
        self.next_message_id = conv.messages.len() as u64 + 1000;
    }

    pub fn update(app: &mut Self, message: Message) -> Task<Message> {
        match message {
            Message::TickTyping => {
                app.typing_phase = app.typing_phase.wrapping_add(1);
            }
            Message::ThemeReloaded => {
                let path = app.theme_path.clone();
                if let Ok(theme) = Theme::load(path.as_deref()) {
                    app.style = Style::new(theme, app.style.text_scale);
                    app.theme_generation = app.theme_generation.wrapping_add(1);
                    app.transcript.invalidate_all();
                }
            }
            Message::ComposerChanged(s) => app.composer = s,
            Message::Send => {
                let body = app.composer.trim();
                if body.is_empty() {
                    return Task::none();
                }
                let continues = app.transcript.rows().last().is_some_and(|r| {
                    r.author.as_deref() == Some("fred")
                });
                let row = row_from_message(
                    app.next_message_id,
                    "fred",
                    "now",
                    body,
                    continues,
                );
                app.next_message_id += 1;
                let bench_append = app.bench && matches!(app.bench_state.phase, BenchPhase::Append);
                if bench_append {
                    app.bench_state.append_at = Some(Instant::now());
                }
                app.transcript.append(row);
                app.composer.clear();
                if bench_append {
                    return Task::done(Message::BenchAppendDone);
                }
            }
            Message::ToggleMembers => {
                app.members_open = !app.members_open;
            }
            Message::SelectConversation { network_id, key } => {
                let id = format!("{network_id}\n{key}");
                app.switch_conversation(network_id, id);
            }
            Message::OpenJump => {
                app.jump_open = true;
                app.jump_filter.clear();
                app.jump_selected = 0;
                app.sync_jump_matches();
                return focus_widget(app.jump_filter_id.clone());
            }
            Message::CloseJump => {
                app.jump_open = false;
                return focus_widget(app.composer_id.clone());
            }
            Message::JumpFilter(s) => {
                app.jump_filter = s;
                app.jump_selected = 0;
                app.sync_jump_matches();
                return jump_list_scroll(app);
            }
            Message::JumpStep(delta) => {
                if app.jump_matches.is_empty() {
                    return Task::none();
                }
                let len = app.jump_matches.len();
                let next = (app.jump_selected as i32 + delta).rem_euclid(len as i32) as usize;
                app.jump_selected = next;
                return jump_list_scroll(app);
            }
            Message::JumpActivate => {
                if let Some(target) = app.jump_matches.get(app.jump_selected).cloned() {
                    if target.kind == JumpKind::Conversation {
                        app.switch_conversation(target.network_id, target.conversation_id);
                    }
                    app.jump_open = false;
                    return focus_widget(app.composer_id.clone());
                }
            }
            Message::Transcript(m) => {
                if matches!(m, TranscriptMessage::Copy) {
                    let text = app.transcript.copy_text();
                    if !text.is_empty() {
                        return clipboard::write::<Message>(text);
                    }
                }
                app.handle_transcript(m);
            }
            Message::Keyboard(event) => {
                if let Some(task) = app.handle_keyboard(event) {
                    return task;
                }
            }
            Message::Modifiers(m) => {
                app.shift_held = m.shift();
            }
            Message::FirstFrame => {
                app.first_frame_ms = Some(app.bench_started.elapsed().as_secs_f64() * 1000.0);
                if app.bench && matches!(app.bench_state.phase, BenchPhase::WaitFirstFrame) {
                    app.bench_state.phase = BenchPhase::Scrolling;
                    app.bench_state.frame_times.push(Instant::now());
                }
            }
            Message::BenchScrollStep => {
                if !app.bench || !matches!(app.bench_state.phase, BenchPhase::Scrolling) {
                    return Task::none();
                }
                app.bench_state.frame_times.push(Instant::now());
                let rows = app.transcript.rows().len();
                if rows == 0 {
                    return Task::none();
                }
                let step = app.bench_state.scroll_step;
                app.transcript.set_pinned_to_bottom(false);
                let row = (step * rows / 200).min(rows.saturating_sub(1));
                app.transcript.set_scroll_anchor(row, 0);
                app.bench_state.scroll_step += 1;
                if app.bench_state.scroll_step >= 200 {
                    app.bench_state.phase = BenchPhase::Resize;
                    app.bench_state.reflow_start = Some(Instant::now());
                    app.style = Style::new(app.style.theme.clone(), app.style.text_scale * 1.05);
                    app.transcript.invalidate_all();
                    return iced::window::latest().then(|id| {
                        if let Some(id) = id {
                            iced::window::resize::<Message>(id, Size::new(1020.0, 680.0))
                        } else {
                            Task::none()
                        }
                    });
                }
            }
            Message::WindowResized(_) => {
                if app.bench && matches!(app.bench_state.phase, BenchPhase::Resize) {
                    if let Some(start) = app.bench_state.reflow_start.take() {
                        app.bench_state.reflow_ms =
                            Some(start.elapsed().as_secs_f64() * 1000.0);
                    }
                    app.bench_state.phase = BenchPhase::Append;
                    app.transcript.set_pinned_to_bottom(true);
                    app.composer = "bench append".to_string();
                    return Task::done(Message::Send);
                }
            }
            Message::BenchAppendDone => {
                if let Some(start) = app.bench_state.append_at.take() {
                    app.bench_state.append_latency_ms =
                        Some(start.elapsed().as_secs_f64() * 1000.0);
                }
                return Task::done(Message::ExitBench);
            }
            Message::DragScrollTick => {
                if app.drag_scroll {
                    if let Some(edge) = app.drag_edge {
                        app.nudge_transcript(edge as i32 * 16);
                    }
                }
            }
            Message::ExitBench => {
                app.print_bench();
                std::process::exit(0);
            }
        }
        Task::none()
    }

    fn handle_transcript(&mut self, message: TranscriptMessage) {
        match message {
            TranscriptMessage::Select(kind) => match kind {
                SelectKind::Begin(p) => self.transcript.begin(p),
                SelectKind::Extend(p) => self.transcript.extend(p),
                SelectKind::Word(p) => self.transcript.select_word(p),
                SelectKind::Row(r) => self.transcript.select_row(r),
                SelectKind::Clear => self.transcript.clear(),
                SelectKind::All => self.transcript.select_all(),
            },
            TranscriptMessage::ScrollTo(anchor) => {
                self.transcript.set_pinned_to_bottom(anchor.pinned_to_bottom);
                if !anchor.pinned_to_bottom {
                    self.transcript
                        .set_scroll_anchor(anchor.row, anchor.offset_px);
                }
            }
            TranscriptMessage::Measured(row, h) => self.transcript.set_measured(row, h),
            TranscriptMessage::Copy => {}
            TranscriptMessage::LinkClick(url) => println!("{url}"),
            TranscriptMessage::FocusChanged(f) => self.transcript_focused = f,
            TranscriptMessage::DragScrollActive(active) => {
                self.drag_scroll = active;
                if !active {
                    self.drag_edge = None;
                }
            }
            TranscriptMessage::DragEdge(edge) => self.drag_edge = edge,
        }
    }

    fn nudge_transcript(&mut self, delta_px: i32) {
        self.transcript.set_pinned_to_bottom(false);
        let scroll = self.transcript.scroll();
        let mut row = scroll.row;
        let mut off = scroll.offset_px + delta_px;
        while off < 0 && row > 0 {
            row -= 1;
            off += 72;
        }
        while off > 72 && row + 1 < self.transcript.rows().len() {
            off -= 72;
            row += 1;
        }
        self.transcript.set_scroll_anchor(row, off);
    }

    fn handle_keyboard(&mut self, event: keyboard::Event) -> Option<Task<Message>> {
        if let keyboard::Event::ModifiersChanged(m) = &event {
            self.shift_held = m.shift();
            return None;
        }
        if self.jump_open {
            if let keyboard::Event::KeyPressed { key, .. } = event {
                match key.as_ref() {
                    Key::Named(Named::Escape) => {
                        self.jump_open = false;
                        return Some(focus_widget(self.composer_id.clone()));
                    }
                    Key::Named(Named::ArrowDown) => {
                        self.update_step(1);
                        return Some(jump_list_scroll(self));
                    }
                    Key::Named(Named::ArrowUp) => {
                        self.update_step(-1);
                        return Some(jump_list_scroll(self));
                    }
                    Key::Named(Named::Enter) => {
                        if let Some(target) = self.jump_matches.get(self.jump_selected).cloned() {
                            if target.kind == JumpKind::Conversation {
                                self.switch_conversation(target.network_id, target.conversation_id);
                            }
                            self.jump_open = false;
                            return Some(focus_widget(self.composer_id.clone()));
                        }
                    }
                    _ => {}
                }
            }
            return None;
        }
        if let keyboard::Event::KeyPressed { key, modifiers, .. } = event {
            if modifiers.control() && matches!(key.as_ref(), Key::Character("k") | Key::Character("K")) {
                self.jump_open = true;
                self.jump_filter.clear();
                self.jump_selected = 0;
                self.sync_jump_matches();
                return Some(focus_widget(self.jump_filter_id.clone()));
            }
        }
        None
    }

    fn update_step(&mut self, delta: i32) {
        if self.jump_matches.is_empty() {
            return;
        }
        let len = self.jump_matches.len();
        self.jump_selected = (self.jump_selected as i32 + delta).rem_euclid(len as i32) as usize;
    }

    fn print_bench(&self) {
        let first = self.first_frame_ms.unwrap_or(0.0);
        let frames = self.bench_state.frame_times.len().max(1);
        let duration = self
            .bench_state
            .frame_times
            .last()
            .zip(self.bench_state.frame_times.first())
            .map(|(a, b)| (*a - *b).as_secs_f64())
            .unwrap_or(1.0);
        let fps = (frames as f64 - 1.0) / duration.max(0.001);
        println!("first frame: {first:.2} ms");
        println!("scroll fps: {fps:.1}");
        if let Some(ms) = self.bench_state.reflow_ms {
            println!("reflow resize: {ms:.2} ms");
        } else {
            println!("reflow resize: n/a");
        }
        if let Some(ms) = self.bench_state.append_latency_ms {
            println!("append latency: {ms:.2} ms");
        } else {
            println!("append latency: n/a");
        }
    }

    pub fn view(app: &Self) -> Element<'_, Message> {
        let s = &app.style;
        let sidebar_w = s.scaled(SIDEBAR_W) as f32;
        let members_w = s.scaled(MEMBERS_W) as f32;

        let sidebar = container(app.sidebar())
            .width(Length::Fixed(sidebar_w))
            .height(Length::Fill)
            .style(move |_| iced::widget::container::Style {
                background: Some(to_iced(s.panel_color()).into()),
                ..Default::default()
            });

        let center = container(app.center_column())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| iced::widget::container::Style {
                background: Some(to_iced(s.page_color()).into()),
                ..Default::default()
            });

        let mut main = row![sidebar, center].spacing(0);

        if app.current_conversation().channel && app.members_open {
            let members = container(app.members_column())
                .width(Length::Fixed(members_w))
                .height(Length::Fill)
                .style(move |_| iced::widget::container::Style {
                    background: Some(to_iced(s.panel_color()).into()),
                    ..Default::default()
                });
            main = main.push(members);
        }

        let mut stack = iced::widget::Stack::new().push(main);

        if app.jump_open {
            let dimmer = mouse_area(
                container(Space::new().width(Length::Fill).height(Length::Fill))
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(move |_| iced::widget::container::Style {
                        background: Some(to_iced(s.overlay_veil_color()).into()),
                        ..Default::default()
                    }),
            )
            .on_press(Message::CloseJump);
            stack = stack.push(dimmer).push(app.jump_sheet());
        }

        stack.into()
    }

    fn sidebar(&self) -> Element<'_, Message> {
        let s = &self.style;
        let network = self.current_network();
        let mut col = column![].spacing(px(s, 8.0)).padding(px(s, 12.0));

        col = col.push(
            row![
                text("O").size(px(s, 14.0)).font(bold()),
                column![
                    text(&network.display_name).size(px(s, 13.0)),
                    text("Connected").size(px(s, 10.0)).color(to_iced(s.muted_color())),
                ]
                .spacing(2),
                Space::new().width(Length::Fill),
                text("edit").size(px(s, 11.0)).color(to_iced(s.muted_color())),
            ]
            .align_y(iced::Alignment::Center)
            .spacing(px(s, 8.0)),
        );

        col = col.push(text("CHANNELS").size(px(s, 10.0)).color(to_iced(s.muted_color())));
        for conv in network
            .conversations
            .iter()
            .filter(|c| c.channel)
        {
            col = col.push(self.conversation_row(conv, &network.id));
        }

        col = col.push(
            text("DIRECT MESSAGES")
                .size(px(s, 10.0))
                .color(to_iced(s.muted_color())),
        );
        for conv in network
            .conversations
            .iter()
            .filter(|c| !c.channel)
        {
            col = col.push(self.conversation_row(conv, &network.id));
        }

        let inbox = self
            .world
            .networks
            .iter()
            .flat_map(|n| n.conversations.iter())
            .filter(|c| !c.channel && c.unread > 0)
            .count();
        col = col.push(Space::new().width(Length::Fill).height(Length::Fill));
        col = col.push(
            container(
                column![
                    text("fred").size(px(s, 13.0)).font(bold()),
                    text(format!("{inbox} inbox")).size(px(s, 11.0)).color(to_iced(s.muted_color())),
                ]
                .spacing(4),
            )
            .padding(px(s, 10.0))
            .style(move |_| iced::widget::container::Style {
                background: Some(to_iced(s.raised_color()).into()),
                border: iced::Border {
                    width: 1.0,
                    color: to_iced(s.divider_color()),
                    radius: (s.scaled(8.0) as f32).into(),
                },
                ..Default::default()
            }),
        );

        scrollable(col).into()
    }

    fn conversation_row(&self, conv: &Conversation, network_id: &str) -> Element<'_, Message> {
        let s = &self.style;
        let id = format!("{network_id}\n{}", conv.key());
        let selected = self.conversation_id == id;
        let row_h = s.scaled(36.0) as f32;
        let accent_w = s.scaled(3.0) as f32;
        let accent_h = s.scaled(18.0) as f32;

        let label = if conv.channel {
            conv.name.clone()
        } else {
            conv.name.clone()
        };

        let mut content = row![].align_y(iced::Alignment::Center).spacing(px(s, 6.0));
        if !conv.channel {
            content = content.push(
                text("●")
                    .size(px(s, 8.0))
                    .color(to_iced(s.presence_color("online"))),
            );
        }
        content = content
            .push(text(label).size(px(s, 12.0)))
            .push(Space::new().width(Length::Fill));

        if conv.unread > 0 {
            let badge = if conv.mention {
                container(text(format!("{}", conv.unread)).size(px(s, 10.0)))
                    .padding([2, 6])
                    .style(move |_| iced::widget::container::Style {
                        background: Some(to_iced(s.accent_color()).into()),
                        border: iced::Border {
                            radius: (s.scaled(19.0) as f32 / 2.0).into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
            } else {
                container(text(format!("{}", conv.unread)).size(px(s, 10.0)))
                    .padding([2, 6])
                    .style(move |_| iced::widget::container::Style {
                        background: Some(to_iced(s.hover_color()).into()),
                        border: iced::Border {
                            radius: (s.scaled(19.0) as f32 / 2.0).into(),
                            ..Default::default()
                        },
                        ..Default::default()
                    })
            };
            content = content.push(badge);
        }

        if conv.typing {
            let phase = self.typing_phase % 3;
            let dots = ".".repeat((phase as usize) + 1);
            content = content.push(
                text(dots)
                    .size(px(s, 10.0))
                    .color(to_iced(s.muted_color())),
            );
        }

        let bg = if selected {
            to_iced(s.hover_color())
        } else {
            iced::Color::TRANSPARENT
        };

        let mut accent_row = row![].height(Length::Fixed(row_h));
        if selected {
            accent_row = accent_row.push(
                container(
                    Space::new()
                        .width(Length::Fixed(accent_w))
                        .height(Length::Fixed(accent_h)),
                )
                .style(move |_| iced::widget::container::Style {
                    background: Some(to_iced(s.accent_color()).into()),
                    ..Default::default()
                }),
            );
        } else {
            accent_row = accent_row.push(Space::new().width(Length::Fixed(accent_w)));
        }
        accent_row = accent_row.push(content);

        button(
            container(accent_row)
            .style(move |_| iced::widget::container::Style {
                background: Some(bg.into()),
                border: iced::Border {
                    radius: (s.scaled(7.0) as f32).into(),
                    ..Default::default()
                },
                ..Default::default()
            }),
        )
        .on_press(Message::SelectConversation {
            network_id: network_id.to_string(),
            key: conv.key(),
        })
        .style(|_, _| iced::widget::button::Style {
            background: None,
            ..Default::default()
        })
        .into()
    }

    fn center_column(&self) -> Element<'_, Message> {
        let s = &self.style;
        let conv = self.current_conversation();
        let network = self.current_network();

        let title = if conv.channel {
            format!("{} · {} · {}", conv.name, network.display_name, network.nick)
        } else {
            conv.name.clone()
        };

        let mut header = column![
            text(title).size(px(s, 14.0)).font(bold()),
            text(conv.topic.clone().unwrap_or_default())
                .size(px(s, 11.0))
                .color(to_iced(s.muted_color())),
        ]
        .spacing(4);

        if conv.channel {
            let count = conv.members.len();
            header = header.push(
                button(text(format!("{count} PEOPLE")).size(px(s, 10.0)))
                    .on_press(Message::ToggleMembers)
                    .style(|_, _| iced::widget::button::Style {
                        background: None,
                        ..Default::default()
                    }),
            );
        }

        let transcript = TranscriptWidget::new(
            &self.transcript,
            s,
            self.theme_generation,
            &self.member_realnames,
            self.shift_held,
        );

        let composer = row![
            text_input("Message…", &self.composer)
                .id(self.composer_id.clone())
                .on_input(Message::ComposerChanged)
                .on_submit(Message::Send)
                .size(px(s, 13.0))
                .font(IA_WRITER)
                .width(Length::Fill),
            button(text("SEND").size(px(s, 12.0)))
                .on_press(Message::Send)
                .padding([px(s, 8.0), px(s, 12.0)]),
        ]
        .spacing(px(s, 8.0))
        .align_y(iced::Alignment::Center);

        column![
            container(header).padding([px(s, 12.0), px(s, 16.0)]),
            container(transcript)
                .width(Length::Fill)
                .height(Length::Fill),
            container(composer).padding(px(s, 12.0)),
        ]
        .into()
    }

    fn members_column(&self) -> Element<'_, Message> {
        let s = &self.style;
        let conv = self.current_conversation();
        let mut col = column![text(format!("ONLINE - {}", conv.members.len()))
            .size(px(s, 10.0))
            .color(to_iced(s.muted_color()))]
        .spacing(px(s, 6.0))
        .padding(px(s, 12.0));

        for member in &conv.members {
            col = col.push(
                row![
                    text("●")
                        .size(px(s, 8.0))
                        .color(to_iced(s.presence_color(&member.presence))),
                    text(format!("{}{}", member.prefix, member.nick)).size(px(s, 12.0)),
                    text(&member.realname)
                        .size(px(s, 11.0))
                        .color(to_iced(s.muted_color())),
                ]
                .spacing(px(s, 6.0))
                .align_y(iced::Alignment::Center),
            );
        }
        scrollable(col).into()
    }

    fn jump_sheet(&self) -> Element<'_, Message> {
        let s = &self.style;
        let card_w = s.scaled(348.0) as f32;
        let card = container(
            column![
                text_input("Jump to conversation…", &self.jump_filter)
                    .id(self.jump_filter_id.clone())
                    .on_input(Message::JumpFilter)
                    .size(px(s, 13.0))
                    .font(IA_WRITER)
                    .width(Length::Fill),
                {
                    let mut list = column![].spacing(2);
                    for (i, target) in self.jump_matches.iter().enumerate() {
                        let current = i == self.jump_selected;
                        list = list.push(
                            container(text(&target.label).size(px(s, 12.0)).color(if current {
                                to_iced(s.ink_color())
                            } else {
                                to_iced(s.muted_color())
                            }))
                            .width(Length::Fill)
                            .height(Length::Fixed(s.scaled(28.0) as f32))
                            .padding([0.0, px(s, 8.0)])
                            .align_y(iced::alignment::Vertical::Center)
                            .style(move |_| iced::widget::container::Style {
                                background: Some(
                                    if current {
                                        to_iced(s.hover_color())
                                    } else {
                                        iced::Color::TRANSPARENT
                                    }
                                    .into(),
                                ),
                                border: iced::Border {
                                    radius: (s.scaled(7.0) as f32).into(),
                                    ..Default::default()
                                },
                                ..Default::default()
                            }),
                        );
                    }
                    scrollable(list)
                        .id(self.jump_list_id.clone())
                        .height(Length::Fixed(s.scaled(252.0) as f32))
                },
            ]
            .spacing(px(s, 8.0)),
        )
        .width(Length::Fixed(card_w))
        .padding(px(s, 16.0))
        .style(move |_| iced::widget::container::Style {
            background: Some(to_iced(s.raised_color()).into()),
            border: iced::Border {
                width: 1.0,
                color: to_iced(s.divider_color()),
                radius: (s.scaled(9.0) as f32).into(),
            },
            ..Default::default()
        });

        container(card)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Center)
            .into()
    }

    pub fn subscription(app: &Self) -> Subscription<Message> {
        let mut subs = vec![
            iced::time::every(Duration::from_millis(320)).map(|_| Message::TickTyping),
            theme_subscription(app.theme_path.clone()),
            keyboard::listen().map(Message::Keyboard),
        ];

        if app.jump_open {
            subs.push(
                keyboard::listen().map(Message::Keyboard),
            );
        }

        if app.drag_scroll {
            subs.push(
                iced::time::every(Duration::from_millis(16)).map(|_| Message::DragScrollTick),
            );
        }

        if app.bench {
            subs.push(iced::time::every(Duration::from_millis(16)).map(|_| {
                Message::BenchScrollStep
            }));
            subs.push(
                iced::window::resize_events().map(|(_, size)| Message::WindowResized(size)),
            );
        }

        Subscription::batch(subs)
    }
}

fn theme_subscription(path: Option<PathBuf>) -> Subscription<Message> {
    Subscription::run_with(path, theme_watch_stream)
}

fn theme_watch_stream(
    path: &Option<PathBuf>,
) -> impl iced::futures::Stream<Item = Message> {
    let path = path.clone();
    iced::stream::channel(8, move |mut output: iced::futures::channel::mpsc::Sender<Message>| async move {
        let watcher = ThemeWatcher::start(path.as_deref()).expect("theme watcher");
        loop {
            if watcher.receiver().try_recv().is_ok() {
                let _ = output.send(Message::ThemeReloaded).await;
            }
            smol::Timer::after(Duration::from_millis(200)).await;
        }
    })
}

impl From<TranscriptMessage> for Message {
    fn from(value: TranscriptMessage) -> Self {
        Message::Transcript(value)
    }
}

#[derive(Debug, Clone)]
pub enum Message {
    TickTyping,
    ThemeReloaded,
    ComposerChanged(String),
    Send,
    ToggleMembers,
    SelectConversation { network_id: String, key: String },
    OpenJump,
    CloseJump,
    JumpFilter(String),
    JumpStep(i32),
    JumpActivate,
    Transcript(TranscriptMessage),
    Keyboard(keyboard::Event),
    Modifiers(keyboard::Modifiers),
    FirstFrame,
    BenchScrollStep,
    WindowResized(Size),
    BenchAppendDone,
    DragScrollTick,
    ExitBench,
}

fn focus_widget(id: iced::widget::Id) -> Task<Message> {
    operation::focus(id)
}

fn jump_list_scroll(app: &App) -> Task<Message> {
    let row_h = app.style.scaled(28.0) as f32 + 2.0;
    let max_scroll = (app.jump_matches.len().saturating_sub(1) as f32) * row_h;
    let y = (app.jump_selected as f32 * row_h).min(max_scroll);
    operation::scroll_to(
        app.jump_list_id.clone(),
        AbsoluteOffset {
            x: None,
            y: Some(y),
        },
    )
}

fn default_conversation(world: &MockWorld) -> (String, String) {
    let network = world.networks.first().expect("network");
    let conv = network
        .conversations
        .iter()
        .find(|c| c.channel)
        .expect("channel");
    (
        network.id.clone(),
        format!("{}\n{}", network.id, conv.key()),
    )
}

fn find_conversation<'a>(world: &'a MockWorld, network_id: &str, conversation_id: &str) -> &'a Conversation {
    let (nid, key) = conversation_id.split_once('\n').unwrap_or((network_id, conversation_id));
    let network = world.networks.iter().find(|n| n.id == nid).expect("network");
    network
        .conversations
        .iter()
        .find(|c| c.key() == key)
        .expect("conversation")
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
