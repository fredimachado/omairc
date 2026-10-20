//! The one open sheet at a time. Each open builds the sheet entity from a
//! snapshot of the shell's state, subscribes to its events, and focuses its
//! filter. Closing (by Escape, a backdrop press, or an activation) returns focus
//! to the composer, as Qt does. Inbox, Shortcuts and About join in step 7.

use gpui::{prelude::*, AnyElement, App, Context, Entity, Subscription, Window};

use crate::model::chords::{Chord, Sheet};
use crate::model::jump::{JumpKind, JumpTarget};
use crate::model::links::{LinkKind, LinkTarget};
use crate::model::nick_pick::NickTarget;
use crate::model::sidebar::set_collapsed;
use crate::ui::sheets::about::{AboutEvent, AboutSheet};
use crate::ui::sheets::filter_sheet::{render_filter_sheet, FilterSheet, FilterSource, SheetEvent};
use crate::ui::sheets::inbox::{render_inbox_sheet, InboxEvent, InboxSheet};
use crate::ui::sheets::jump::JumpSource;
use crate::ui::sheets::link::LinkSource;
use crate::ui::sheets::nick::NickSource;
use crate::ui::sheets::shortcuts::{ShortcutsEvent, ShortcutsSheet};
use crate::ui::shell::Shell;
use crate::util::open_web_link;

/// The open sheet and the subscriptions that keep it wired to the shell. The
/// subscriptions are never read; they are held so that dropping the sheet
/// unhooks them.
#[allow(dead_code)]
pub enum ActiveModal {
    Jump(Entity<FilterSheet<JumpSource>>, Vec<Subscription>),
    Link(Entity<FilterSheet<LinkSource>>, Vec<Subscription>),
    Nick(Entity<FilterSheet<NickSource>>, Vec<Subscription>),
    Inbox(Entity<InboxSheet>, Vec<Subscription>),
    Shortcuts(Entity<ShortcutsSheet>, Vec<Subscription>),
    About(Entity<AboutSheet>, Vec<Subscription>),
}

impl ActiveModal {
    pub fn sheet(&self) -> Sheet {
        match self {
            Self::Jump(..) => Sheet::Jump,
            Self::Link(..) => Sheet::Link,
            Self::Nick(..) => Sheet::Nick,
            Self::Inbox(..) => Sheet::Inbox,
            Self::Shortcuts(..) => Sheet::Shortcuts,
            Self::About(..) => Sheet::About,
        }
    }
}

impl Shell {
    /// Ctrl+K: opens Jump, or closes it when it is already open.
    pub fn toggle_jump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::Jump, window, cx) {
            return;
        }
        if matches!(self.modal, Some(ActiveModal::Jump(..))) {
            self.close_modal(window, cx);
        } else {
            self.present_jump(window, cx);
        }
    }

    /// Ctrl+Shift+O: opens the Link sheet, or closes it when it is already open.
    pub fn toggle_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::Link, window, cx) {
            return;
        }
        if matches!(self.modal, Some(ActiveModal::Link(..))) {
            self.close_modal(window, cx);
        } else {
            self.present_link(window, cx);
        }
    }

    /// Ctrl+Shift+K and a click on the `ONLINE` heading: the Nick sheet, on a channel.
    pub fn open_nick_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::NickSheet, window, cx) {
            return;
        }
        self.present_nick(window, cx);
    }

    /// Ctrl+Shift+A and the footer `inbox` link: opens the Inbox sheet, or
    /// closes it when it is already open.
    pub fn toggle_inbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::Inbox, window, cx) {
            return;
        }
        if matches!(self.modal, Some(ActiveModal::Inbox(..))) {
            self.close_modal(window, cx);
        } else {
            self.present_inbox(window, cx);
        }
    }

    /// Ctrl+/: opens the Shortcuts sheet, or closes it when it is already open.
    pub fn toggle_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::Shortcuts, window, cx) {
            return;
        }
        if matches!(self.modal, Some(ActiveModal::Shortcuts(..))) {
            self.close_modal(window, cx);
        } else {
            self.present_shortcuts(window, cx);
        }
    }

    /// Ctrl+Shift+/ and a click on the footer's version: the About sheet.
    pub fn open_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::About, window, cx) {
            return;
        }
        self.present_about(window, cx);
    }

    /// Escape: closes the open sheet. Inert when no sheet is open.
    pub fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::Escape, window, cx) {
            return;
        }
        self.close_modal(window, cx);
    }

    /// Drops the open sheet and hands focus back to the composer.
    pub fn close_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.modal = None;
        self.focus_composer(window, cx);
        cx.notify();
    }

    /// The open sheet as an overlay, or nothing when no sheet is open.
    pub fn render_modal(&self, cx: &App) -> Option<AnyElement> {
        Some(match self.modal.as_ref()? {
            ActiveModal::Jump(sheet, _) => render_filter_sheet(sheet, cx),
            ActiveModal::Link(sheet, _) => render_filter_sheet(sheet, cx),
            ActiveModal::Nick(sheet, _) => render_filter_sheet(sheet, cx),
            ActiveModal::Inbox(sheet, _) => render_inbox_sheet(sheet, cx),
            ActiveModal::Shortcuts(sheet, _) => sheet.clone().into_any_element(),
            ActiveModal::About(sheet, _) => sheet.clone().into_any_element(),
        })
    }

    /// Activating a Jump row expands its network, then selects it. Status rows
    /// are never activatable, so they reach here only through a stale event.
    pub fn jump_to(&mut self, target: &JumpTarget, cx: &mut Context<Self>) {
        if target.kind != JumpKind::Conversation {
            return;
        }
        let Some((network, key)) = target.conversation_id.split_once('\n') else {
            return;
        };
        set_collapsed(&self.world, &mut self.sidebar, network, false);
        self.select_by_ids(network, key, cx);
    }

    /// Link sheet highlight: the transcript scrolls to the row the link is in.
    fn reveal_link_row(&mut self, row: Option<usize>, cx: &mut Context<Self>) {
        let Some(row) = row else {
            return;
        };
        self.transcript.update(cx, |t, cx| {
            t.scroll_to_row(row);
            cx.notify();
        });
    }

    fn present_jump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source = JumpSource {
            world: self.world.clone(),
        };
        let sheet = cx.new(|cx| FilterSheet::new(source, cx));
        let subscriptions = vec![
            track(cx, &sheet),
            cx.subscribe_in(
                &sheet,
                window,
                |shell: &mut Shell, _, event: &SheetEvent<JumpTarget>, window, cx| match event {
                    SheetEvent::Activated(target) => {
                        shell.jump_to(target, cx);
                        shell.close_modal(window, cx);
                    }
                    SheetEvent::Dismissed => shell.close_modal(window, cx),
                    SheetEvent::Highlighted(_) => {}
                },
            ),
        ];
        window.focus(&sheet.read(cx).filter_focus(cx));
        self.modal = Some(ActiveModal::Jump(sheet, subscriptions));
        cx.notify();
    }

    fn present_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source = LinkSource {
            topic: self.current_conversation().topic.clone(),
            rows: self.transcript.read(cx).rows().to_vec(),
        };
        let sheet = cx.new(|cx| FilterSheet::new(source, cx));
        let subscriptions = vec![
            track(cx, &sheet),
            cx.subscribe_in(
                &sheet,
                window,
                |shell: &mut Shell, _, event: &SheetEvent<LinkTarget>, window, cx| match event {
                    SheetEvent::Activated(link) => {
                        let text = link.text.clone();
                        let kind = link.kind;
                        shell.close_modal(window, cx);
                        match kind {
                            LinkKind::Url => {
                                open_web_link(cx, &text);
                            }
                            // Invite rows come only from Status, which this sheet does
                            // not show yet. The Status wiring joins the channel here.
                            LinkKind::Invite => {}
                        }
                    }
                    SheetEvent::Dismissed => shell.close_modal(window, cx),
                    SheetEvent::Highlighted(link) => shell.reveal_link_row(link.row, cx),
                },
            ),
        ];
        window.focus(&sheet.read(cx).filter_focus(cx));
        self.modal = Some(ActiveModal::Link(sheet, subscriptions));
        cx.notify();
    }

    fn present_nick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source = NickSource {
            members: self.current_conversation().members.clone(),
            self_nick: self.current_network().nick.clone(),
        };
        let sheet = cx.new(|cx| FilterSheet::new(source, cx));
        let subscriptions = vec![
            track(cx, &sheet),
            cx.subscribe_in(
                &sheet,
                window,
                |shell: &mut Shell, _, event: &SheetEvent<NickTarget>, window, cx| match event {
                    SheetEvent::Activated(target) => {
                        let nick = target.member.nick.clone();
                        shell.close_modal(window, cx);
                        shell.open_direct_nick(&nick, cx);
                    }
                    SheetEvent::Dismissed => shell.close_modal(window, cx),
                    SheetEvent::Highlighted(_) => {}
                },
            ),
        ];
        window.focus(&sheet.read(cx).filter_focus(cx));
        self.modal = Some(ActiveModal::Nick(sheet, subscriptions));
        cx.notify();
    }

    fn present_inbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.inbox.items().to_vec();
        let sheet = cx.new(|cx| InboxSheet::new(items, cx));
        let subscriptions = vec![
            cx.observe(&sheet, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &sheet,
                window,
                |shell: &mut Shell, _, event: &InboxEvent, window, cx| match event {
                    InboxEvent::Activated(index) => shell.activate_inbox(*index, window, cx),
                    InboxEvent::Dismissed(index) => shell.dismiss_inbox(*index, cx),
                    InboxEvent::Closed => shell.close_modal(window, cx),
                },
            ),
        ];
        window.focus(&sheet.read(cx).focus_handle());
        self.modal = Some(ActiveModal::Inbox(sheet, subscriptions));
        cx.notify();
    }

    fn present_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sheet = cx.new(ShortcutsSheet::new);
        let subscriptions = vec![cx.subscribe_in(
            &sheet,
            window,
            |shell: &mut Shell, _, _: &ShortcutsEvent, window, cx| shell.close_modal(window, cx),
        )];
        window.focus(&sheet.read(cx).focus_handle());
        self.modal = Some(ActiveModal::Shortcuts(sheet, subscriptions));
        cx.notify();
    }

    fn present_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sheet = cx.new(AboutSheet::new);
        let subscriptions = vec![cx.subscribe_in(
            &sheet,
            window,
            |shell: &mut Shell, _, _: &AboutEvent, window, cx| shell.close_modal(window, cx),
        )];
        window.focus(&sheet.read(cx).initial_focus());
        self.modal = Some(ActiveModal::About(sheet, subscriptions));
        cx.notify();
    }
}

/// Re-renders the shell when the sheet notifies (its filter text changed).
fn track<S: FilterSource>(cx: &mut Context<Shell>, sheet: &Entity<FilterSheet<S>>) -> Subscription {
    cx.observe(sheet, |_, _, cx| cx.notify())
}
