//! The window's root view. Owns the selection, the sheets and the entities
//! for the transcript and inputs; rows and widgets only receive data and
//! `Handler`s built here.

pub mod actions;
pub mod bench;
pub mod commands;
pub mod find;
pub mod inbox;
pub mod modal;
pub mod slash;

use std::rc::Rc;

use gpui::{
    div, prelude::*, App, Context, Entity, FocusHandle, Focusable, MouseButton,
    MouseMoveEvent, MouseUpEvent, Render, ScrollHandle, Subscription, Window,
};

use crate::model::composer::{ComposerState, DraftKey};
use crate::model::find::Find;
use crate::model::inbox::{seed_inbox, Inbox};
use crate::model::prefs::Preferences;
use crate::model::sidebar::SidebarState;
use crate::model::slash_complete::SlashSession;
use crate::model::status::status_title;
use crate::model::title::{conversation_title, window_title, TitleView};
use crate::model::{bench_channel, load_world, ConversationRef, MockWorld};
use crate::ui::center::header::HeaderView;
use crate::ui::center::render_center;
use crate::ui::center::slash_popup::render_slash_popup;
use crate::ui::input::SingleLineInput;
use crate::ui::members::render_members;
use crate::ui::scrollbar::{end_scrollbar_drag, move_scrollbar_drag, ScrollbarDragCell};
use crate::ui::shell::actions::wire_shell_actions;
use crate::ui::sidebar::{render_sidebar, SidebarData};
use crate::ui::transcript::view::{InviteClicked, NickClicked};
use crate::ui::transcript::TranscriptView;
use crate::ui::Handler;
use crate::util::{style, to_gpui};

pub use actions::bind_shell_keys;
pub use bench::BenchState;

pub struct Shell {
    pub world: MockWorld,
    /// The conversation the window has open. While Status is showing it is the
    /// conversation Escape or Ctrl+` returns to.
    pub selected: ConversationRef,
    /// The network whose Status pane is showing in place of the conversation.
    /// `None` shows `selected`.
    pub status: Option<String>,
    pub sidebar: SidebarState,
    pub window_title: String,
    pub members_open: bool,
    /// The member list's keyboard cursor, an index into the channel's members.
    pub member_cursor: usize,
    pub member_focus: FocusHandle,
    /// The open sheet (Jump, Link, Nick), if any. Only one is open at a time.
    pub modal: Option<modal::ActiveModal>,
    /// Mentions and invites waiting for the user. Shown by the Inbox sheet and
    /// the footer count.
    pub inbox: Inbox,
    pub sidebar_scroll: ScrollHandle,
    pub members_scroll: ScrollHandle,
    pub scrollbar_drag: ScrollbarDragCell,
    pub transcript: Entity<TranscriptView>,
    pub composer: Entity<SingleLineInput>,
    /// The composer's drafts, sent-line history and Tab cycle, per target.
    pub composer_state: ComposerState,
    /// The slash-complete popup above the composer: its rows and highlight.
    pub slash: SlashSession,
    /// Find mode (Ctrl+F) while it is on: the last query searched and the match
    /// row. The composer holds the query, and `composer_state` holds the draft.
    pub find: Option<Find>,
    /// The saved preferences `/pref` reads and writes, loaded once at startup.
    pub preferences: Preferences,
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

        let inbox = seed_inbox(&world);
        let composer_state = ComposerState::starting_at(DraftKey::for_view(None, &selected));
        let scrollbar_drag: ScrollbarDragCell = Default::default();
        let composer = cx.new(|cx| SingleLineInput::new("", "Composer", cx.focus_handle()));
        let composer_focus = composer.read(cx).focus_handle(cx);
        let transcript = cx.new(|_| TranscriptView::new(composer_focus, scrollbar_drag.clone()));

        let transcript_events = cx.subscribe(&transcript, |shell: &mut Shell, _, event: &NickClicked, cx| {
            shell.open_direct_nick(&event.0, cx);
        });
        let invite_events = cx.subscribe(&transcript, |shell: &mut Shell, _, event: &InviteClicked, cx| {
            shell.join_status_invite(&event.0, cx);
        });
        // Every composer edit, caret move and history step re-reads the slash popup.
        // The popup only re-renders when its rows or highlight changed. A changed
        // find query searches again from the top.
        let composer_events = cx.observe(&composer, |shell: &mut Shell, _, cx| {
            shell.follow_find_query(cx);
            if shell.sync_slash_popup(cx) {
                cx.notify();
            }
        });

        let shell = Self {
            world,
            selected,
            status: None,
            sidebar: SidebarState::default(),
            window_title: String::new(),
            members_open: true,
            member_cursor: 0,
            member_focus: cx.focus_handle(),
            modal: None,
            inbox,
            sidebar_scroll: ScrollHandle::new(),
            members_scroll: ScrollHandle::new(),
            scrollbar_drag,
            transcript,
            composer,
            composer_state,
            slash: SlashSession::new(),
            find: None,
            preferences: Preferences::load(),
            focus_handle: cx.focus_handle(),
            typing_phase: 0,
            bench: BenchState {
                enabled: bench,
                ..Default::default()
            },
            lines_seed: lines,
            _subscriptions: vec![transcript_events, invite_events, composer_events],
        };

        let conv = shell.current_conversation();
        let self_nick = shell.current_network().nick.clone();
        shell.transcript.update(cx, |t, _| t.set_conversation(conv, &self_nick));
        shell.sync_send_limit(cx);
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
        // Qt gates the panel on `win.width >= scaledSize(980)`. Status hides it.
        let wide_enough = f32::from(window.viewport_size().width)
            >= app_style.scaled(980.) as f32;
        let show_members = self.members_open
            && self.current_conversation().is_channel()
            && self.status.is_none()
            && wide_enough;
        // A hidden member list cannot keep focus (a DM, a narrow window, or a
        // closed panel): hand focus to the composer, as Ctrl+Shift+M does.
        if !show_members && self.member_focus.is_focused(window) {
            self.focus_composer(window, cx);
        }
        let view = match self.status.as_deref() {
            Some(network_id) => TitleView::Status(network_id),
            None => TitleView::Conversation(Some(&self.selected)),
        };
        let wanted_window_title = window_title(&self.world, view);
        if wanted_window_title != self.window_title {
            window.set_window_title(&wanted_window_title);
            self.window_title = wanted_window_title;
        }

        // The header reads the Status pane's title and the connection line, or the
        // conversation's title, people count, and subtitle. The subtitle is the
        // network's last command error while one is set, else the topic (Status
        // shows the same error over "Connected", as Qt's `statusSubtitle` does).
        let (title, subtitle, people) = match &self.status {
            Some(network_id) => (
                status_title(&self.world, network_id).unwrap_or_else(|| "Status".to_string()),
                self.world
                    .network(network_id)
                    .and_then(|network| network.last_error.clone())
                    .unwrap_or_else(|| "Connected".to_string()),
                None,
            ),
            None => {
                let conv = self.current_conversation();
                let error = self
                    .world
                    .network(&self.selected.network_id)
                    .and_then(|network| network.last_error.clone());
                (
                    conversation_title(&self.world, &self.selected).unwrap_or_default(),
                    error.unwrap_or_else(|| conv.topic.clone().unwrap_or_default()),
                    conv.is_channel().then(|| conv.people_count()),
                )
            }
        };
        // The slash popup hangs over the composer while it shows.
        let slash_popup = self.slash_popup_showing().then(|| {
            render_slash_popup(
                self.slash.hits(),
                self.slash.selected(),
                &app_style,
                slash_pick_handler(entity.clone()),
            )
            .into_any_element()
        });

        let sidebar = render_sidebar(
            SidebarData {
                world: &self.world,
                state: &self.sidebar,
                selected: &self.selected,
                status_open: self.status.is_some(),
                inbox_count: self.inbox.len() as u32,
                typing_phase: self.typing_phase,
                on_status: status_handler(entity.clone()),
                on_inbox: inbox_handler(entity.clone()),
                on_about: about_handler(entity.clone()),
            },
            &app_style,
            &self.sidebar_scroll,
            entity.clone(),
            &self.scrollbar_drag,
            select_handler(entity.clone()),
            close_handler(entity.clone()),
            collapse_handler(entity.clone()),
        );
        let header = HeaderView {
            title: &title,
            subtitle: &subtitle,
            people,
        };
        let center = render_center(
            header,
            self.transcript.clone(),
            self.composer.clone(),
            &app_style,
            toggle_members_handler(entity.clone()),
            send_handler(entity.clone()),
            slash_popup,
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

/// A network icon or name click: opens that network's Status pane.
fn status_handler(entity: Entity<Shell>) -> Handler<String> {
    Rc::new(move |network_id: String, window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| shell.open_status(&network_id, window, cx));
    })
}

/// The footer's `inbox` link: opens the Inbox sheet, or closes it when open.
fn inbox_handler(entity: Entity<Shell>) -> Handler<()> {
    Rc::new(move |_: (), window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| shell.toggle_inbox(window, cx));
    })
}

/// A click on the footer's version: the About sheet.
fn about_handler(entity: Entity<Shell>) -> Handler<()> {
    Rc::new(move |_: (), window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| shell.open_about(window, cx));
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

/// SEND: send the draft, then focus the composer. In find mode SEND moves to the
/// next match instead, as Qt's `sendMessage` does.
fn send_handler(entity: Entity<Shell>) -> Handler<()> {
    Rc::new(move |_: (), window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| {
            if shell.find.is_some() {
                shell.begin_or_advance_find(window, cx);
            } else {
                shell.send_message(cx);
            }
            shell.focus_composer(window, cx);
        });
    })
}

/// A click on a slash-popup row: inserts that row, then focuses the composer.
fn slash_pick_handler(entity: Entity<Shell>) -> Handler<usize> {
    Rc::new(move |index: usize, window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| {
            shell.pick_slash_hit(index, window, cx);
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
