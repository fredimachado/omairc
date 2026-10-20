//! The window's root view. Owns the selection, the sheets and the entities
//! for the transcript and inputs; rows and widgets only receive data and
//! `Handler`s built here.

pub mod actions;
pub mod bench;
pub mod commands;
pub mod modal;

use std::rc::Rc;

use gpui::{
    div, prelude::*, App, Context, Entity, FocusHandle, Focusable, MouseButton,
    MouseMoveEvent, MouseUpEvent, Render, ScrollHandle, Subscription, Window,
};

use crate::model::sidebar::SidebarState;
use crate::model::title::{conversation_title, window_title};
use crate::model::{bench_channel, load_world, ConversationRef, MockWorld};
use crate::ui::center::render_center;
use crate::ui::input::SingleLineInput;
use crate::ui::members::render_members;
use crate::ui::scrollbar::{end_scrollbar_drag, move_scrollbar_drag, ScrollbarDragCell};
use crate::ui::shell::actions::wire_shell_actions;
use crate::ui::sidebar::{render_sidebar, SidebarData};
use crate::ui::transcript::view::NickClicked;
use crate::ui::transcript::TranscriptView;
use crate::ui::Handler;
use crate::util::{style, to_gpui};

pub use actions::bind_shell_keys;
pub use bench::BenchState;

pub struct Shell {
    pub world: MockWorld,
    pub selected: ConversationRef,
    pub sidebar: SidebarState,
    pub window_title: String,
    pub members_open: bool,
    /// The member list's keyboard cursor, an index into the channel's members.
    pub member_cursor: usize,
    pub member_focus: FocusHandle,
    /// The open sheet (Jump, Link, Nick), if any. Only one is open at a time.
    pub modal: Option<modal::ActiveModal>,
    pub sidebar_scroll: ScrollHandle,
    pub members_scroll: ScrollHandle,
    pub scrollbar_drag: ScrollbarDragCell,
    pub transcript: Entity<TranscriptView>,
    pub composer: Entity<SingleLineInput>,
    pub focus_handle: FocusHandle,
    pub typing_phase: u8,
    pub bench: BenchState,
    pub lines_seed: Option<usize>,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    pub fn new(cx: &mut Context<Self>, lines: Option<usize>, bench: bool) -> Self {
        let mut world = load_world();
        if let Some(n) = lines {
            for network in &mut world.networks {
                network.conversations.retain(|c| c.name != "#bench");
                network.conversations.push(bench_channel(n));
            }
        }

        let selected = {
            let first = &world.networks[0];
            let bench_key = lines
                .filter(|n| *n > 0)
                .and_then(|_| first.conversations.iter().find(|c| c.name == "#bench"))
                .map(|c| c.key());
            let key = bench_key.unwrap_or_else(|| first.conversations[0].key());
            ConversationRef::new(first.id.clone(), key)
        };

        let scrollbar_drag: ScrollbarDragCell = Default::default();
        let composer = cx.new(|cx| SingleLineInput::new("", "Composer", cx.focus_handle()));
        let composer_focus = composer.read(cx).focus_handle(cx);
        let transcript = cx.new(|_| TranscriptView::new(composer_focus, scrollbar_drag.clone()));

        let transcript_events = cx.subscribe(&transcript, |shell: &mut Shell, _, event: &NickClicked, cx| {
            shell.open_direct_nick(&event.0, cx);
        });

        let shell = Self {
            world,
            selected,
            sidebar: SidebarState::default(),
            window_title: String::new(),
            members_open: true,
            member_cursor: 0,
            member_focus: cx.focus_handle(),
            modal: None,
            sidebar_scroll: ScrollHandle::new(),
            members_scroll: ScrollHandle::new(),
            scrollbar_drag,
            transcript,
            composer,
            focus_handle: cx.focus_handle(),
            typing_phase: 0,
            bench: BenchState {
                enabled: bench,
                ..Default::default()
            },
            lines_seed: lines,
            _subscriptions: vec![transcript_events],
        };

        let conv = shell.current_conversation();
        let self_nick = shell.current_network().nick.clone();
        shell.transcript.update(cx, |t, _| t.set_conversation(conv, &self_nick));
        shell
    }
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.bench.enabled && self.bench.first_frame.is_none() {
            self.bench.first_frame = Some(self.bench.started.elapsed());
        }

        let app_style = style(cx);
        let entity = cx.entity();
        // Qt gates the panel on `win.width >= scaledSize(980)`.
        let wide_enough = f32::from(window.viewport_size().width)
            >= app_style.scaled(980.) as f32;
        let show_members = self.members_open
            && self.current_conversation().is_channel()
            && wide_enough;
        let title = conversation_title(&self.world, &self.selected).unwrap_or_default();
        let wanted_window_title = window_title(&self.world, Some(&self.selected));
        if wanted_window_title != self.window_title {
            window.set_window_title(&wanted_window_title);
            self.window_title = wanted_window_title;
        }

        let sidebar = render_sidebar(
            SidebarData {
                world: &self.world,
                state: &self.sidebar,
                selected: &self.selected,
                inbox_count: self.inbox_count(),
                typing_phase: self.typing_phase,
            },
            &app_style,
            &self.sidebar_scroll,
            entity.clone(),
            &self.scrollbar_drag,
            select_handler(entity.clone()),
            close_handler(entity.clone()),
            collapse_handler(entity.clone()),
        );
        let center = render_center(
            self.current_conversation(),
            &title,
            self.transcript.clone(),
            self.composer.clone(),
            &app_style,
            toggle_members_handler(entity.clone()),
            send_handler(entity.clone()),
        );

        // The member cursor is drawn only while the list holds keyboard focus.
        let members_focused = self.member_focus.is_focused(window);
        let members_cursor = members_focused.then_some(self.member_cursor);

        let root = div()
            .size_full()
            .bg(to_gpui(app_style.page_color()))
            .flex()
            .flex_row()
            .on_mouse_move(cx.listener(|shell, ev: &MouseMoveEvent, _, cx| {
                let app_style = style(cx);
                if shell.scrollbar_drag.borrow().is_some() {
                    move_scrollbar_drag(&shell.scrollbar_drag, ev.position, &app_style);
                    cx.notify();
                } else {
                    shell.transcript.update(cx, |t, cx| {
                        if t.drag_move(ev.position, &app_style) {
                            cx.notify();
                        }
                    });
                }
            }))
            .capture_any_mouse_up(cx.listener(|shell, ev: &MouseUpEvent, _, cx| {
                if ev.button == MouseButton::Left {
                    end_scrollbar_drag(&shell.scrollbar_drag);
                    shell.end_text_drag(ev.position, cx);
                }
            }))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|shell, ev: &MouseUpEvent, _, cx| {
                    shell.end_text_drag(ev.position, cx);
                }),
            )
            .child(sidebar)
            .child(center)
            .when(show_members, |this| {
                this.child(render_members(
                    self.current_conversation(),
                    &app_style,
                    &self.members_scroll,
                    entity.clone(),
                    &self.scrollbar_drag,
                    &self.current_network().nick,
                    nick_handler(entity.clone()),
                    heading_handler(entity.clone()),
                    &self.member_focus,
                    members_cursor,
                ))
            })
            .when_some(self.render_modal(cx), |this, modal| this.child(modal));

        wire_shell_actions(root, cx)
    }
}

impl Focusable for Shell {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// A sidebar row click: select, then focus the composer, as Qt does.
fn select_handler(entity: Entity<Shell>) -> Handler<ConversationRef> {
    Rc::new(move |target: ConversationRef, window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| {
            shell.select(target, cx);
            shell.focus_composer(window, cx);
            cx.notify();
        });
    })
}

/// A network header's chevron: flip that network's collapse state.
fn collapse_handler(entity: Entity<Shell>) -> Handler<String> {
    Rc::new(move |network_id: String, _: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| {
            shell.toggle_collapsed(&network_id, cx);
        });
    })
}

/// SEND or Enter: send the draft, then focus the composer.
fn send_handler(entity: Entity<Shell>) -> Handler<()> {
    Rc::new(move |_: (), window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| {
            shell.send_message(cx);
            shell.focus_composer(window, cx);
        });
    })
}

fn toggle_members_handler(entity: Entity<Shell>) -> Handler<()> {
    Rc::new(move |_: (), _: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| {
            shell.members_open = !shell.members_open;
            cx.notify();
        });
    })
}

/// Ctrl+W, or a middle click on a DM row: close that DM.
fn close_handler(entity: Entity<Shell>) -> Handler<ConversationRef> {
    Rc::new(move |target: ConversationRef, window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| {
            shell.close_direct_message(&target, window, cx);
        });
    })
}

/// A click on the `ONLINE - N` heading: the Nick sheet.
fn heading_handler(entity: Entity<Shell>) -> Handler<()> {
    Rc::new(move |_: (), window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| shell.open_nick_sheet(window, cx));
    })
}

/// A member row click: open or select that nick's DM, then focus the composer.
fn nick_handler(entity: Entity<Shell>) -> Handler<String> {
    Rc::new(move |nick: String, window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| {
            shell.open_direct_nick(&nick, cx);
            shell.focus_composer(window, cx);
        });
    })
}
