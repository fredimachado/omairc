//! The window's root view. Owns the selection, the sheets and the entities
//! for the transcript and inputs; rows and widgets only receive data and
//! `Handler`s built here.

pub mod actions;
pub mod bench;
pub mod commands;
pub mod modal;

use std::rc::Rc;

use gpui::{
    div, prelude::*, App, ClipboardItem, Context, Entity, FocusHandle, Focusable, MouseButton,
    MouseMoveEvent, MouseUpEvent, Render, ScrollHandle, Window,
};

use crate::model::{bench_channel, load_world, ConversationRef, MockWorld};
use crate::ui::center::render_center;
use crate::ui::input::SingleLineInput;
use crate::ui::members::render_members;
use crate::ui::scrollbar::{end_scrollbar_drag, move_scrollbar_drag, ScrollbarDragCell};
use crate::ui::shell::actions::{OpenJump, TranscriptCopy};
use crate::ui::sidebar::{render_sidebar, SidebarData};
use crate::ui::transcript::TranscriptView;
use crate::ui::Handler;
use crate::util::{style, to_gpui};

pub use actions::bind_shell_keys;
pub use bench::BenchState;

pub struct Shell {
    pub world: MockWorld,
    pub selected: ConversationRef,
    pub members_open: bool,
    pub jump_open: bool,
    pub jump_selected: usize,
    pub jump_list_scroll: ScrollHandle,
    pub sidebar_scroll: ScrollHandle,
    pub members_scroll: ScrollHandle,
    pub scrollbar_drag: ScrollbarDragCell,
    pub transcript: Entity<TranscriptView>,
    pub composer: Entity<SingleLineInput>,
    pub jump_filter: Entity<SingleLineInput>,
    pub focus_handle: FocusHandle,
    pub typing_phase: u8,
    pub bench: BenchState,
    pub lines_seed: Option<usize>,
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
        let jump_filter = cx.new(|cx| {
            SingleLineInput::new("Jump to conversation…", "JumpFilter", cx.focus_handle())
        });

        let shell = Self {
            world,
            selected,
            members_open: true,
            jump_open: false,
            jump_selected: 0,
            jump_list_scroll: ScrollHandle::new(),
            sidebar_scroll: ScrollHandle::new(),
            members_scroll: ScrollHandle::new(),
            scrollbar_drag,
            transcript,
            composer,
            jump_filter,
            focus_handle: cx.focus_handle(),
            typing_phase: 0,
            bench: BenchState {
                enabled: bench,
                ..Default::default()
            },
            lines_seed: lines,
        };

        let conv = shell.current_conversation();
        shell.transcript.update(cx, |t, _| t.set_conversation(conv));
        shell
    }
}

impl Render for Shell {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.bench.enabled && self.bench.first_frame.is_none() {
            self.bench.first_frame = Some(self.bench.started.elapsed());
        }

        let app_style = style(cx);
        let entity = cx.entity();
        let show_members = self.members_open && self.current_conversation().is_channel();

        let sidebar = render_sidebar(
            SidebarData {
                network: self.current_network(),
                selected: &self.selected,
                inbox_count: self.inbox_count(),
                typing_phase: self.typing_phase,
            },
            &app_style,
            &self.sidebar_scroll,
            entity.clone(),
            &self.scrollbar_drag,
            select_handler(entity.clone()),
        );
        let center = render_center(
            self.current_conversation(),
            self.current_network(),
            self.transcript.clone(),
            self.composer.clone(),
            &app_style,
            toggle_members_handler(entity.clone()),
            send_handler(entity.clone()),
        );

        div()
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
            .on_action(cx.listener(|shell, _: &OpenJump, window, cx| {
                shell.open_jump(window, cx);
            }))
            .on_action(cx.listener(|shell, _: &TranscriptCopy, _, cx| {
                // With the composer focused, Ctrl+C copies the transcript
                // selection unless the draft has its own selection.
                if shell.composer.read(cx).has_selection() {
                    cx.propagate();
                    return;
                }
                let text = shell.transcript.read(cx).copy_text();
                if !text.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
            }))
            .child(sidebar)
            .child(center)
            .when(show_members, |this| {
                this.child(render_members(
                    self.current_conversation(),
                    &app_style,
                    &self.members_scroll,
                    entity.clone(),
                    &self.scrollbar_drag,
                ))
            })
            .when(self.jump_open, |this| this.child(self.render_jump(cx)))
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
