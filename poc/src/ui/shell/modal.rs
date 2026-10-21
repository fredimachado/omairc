//! The one open sheet at a time. Each open builds the sheet entity from a
//! snapshot of the shell's state, subscribes to its events, and focuses its
//! filter. Closing (by Escape, a backdrop press, or an activation) returns focus
//! to the composer, as Qt does. Inbox, Shortcuts and About join in step 7.

use gpui::{prelude::*, AnyElement, App, Context, Entity, Subscription, Window};

use crate::model::channel_list::ListRow;
use crate::model::chords::{Chord, Sheet};
use crate::model::jump::{JumpKind, JumpTarget, JumpView};
use crate::model::links::{LinkKind, LinkTarget};
use crate::model::nick_pick::NickTarget;
use crate::ui::sheets::about::{AboutEvent, AboutSheet};
use crate::ui::sheets::channel_list::ChannelListSource;
use crate::ui::sheets::channel_open::{ChannelOpenEvent, ChannelOpenSheet};
use crate::ui::sheets::filter_sheet::{render_filter_sheet, FilterSheet, FilterSource, SheetEvent};
use crate::ui::sheets::inbox::{render_inbox_sheet, InboxEvent, InboxSheet};
use crate::ui::sheets::jump::JumpSource;
use crate::ui::sheets::link::LinkSource;
use crate::ui::sheets::nick::NickSource;
use crate::ui::sheets::shortcuts::{ShortcutsEvent, ShortcutsSheet};
use crate::ui::shell::Shell;
use crate::ui::update::{launch_installer, launch_on_close};
use crate::util::open_web_link;

/// The open sheet and the subscriptions that keep it wired to the shell. The
/// `_subscriptions` fields are never read; they are held so that dropping the
/// sheet unhooks them.
pub enum ActiveModal {
    Jump {
        sheet: Entity<FilterSheet<JumpSource>>,
        _subscriptions: Vec<Subscription>,
    },
    Link {
        sheet: Entity<FilterSheet<LinkSource>>,
        _subscriptions: Vec<Subscription>,
    },
    Nick {
        sheet: Entity<FilterSheet<NickSource>>,
        _subscriptions: Vec<Subscription>,
    },
    Inbox {
        sheet: Entity<InboxSheet>,
        _subscriptions: Vec<Subscription>,
    },
    Shortcuts {
        sheet: Entity<ShortcutsSheet>,
        _subscriptions: Vec<Subscription>,
    },
    About {
        sheet: Entity<AboutSheet>,
        _subscriptions: Vec<Subscription>,
    },
    ChannelOpen {
        sheet: Entity<ChannelOpenSheet>,
        _subscriptions: Vec<Subscription>,
    },
    ChannelList {
        sheet: Entity<FilterSheet<ChannelListSource>>,
        _subscriptions: Vec<Subscription>,
    },
}

impl ActiveModal {
    pub fn sheet(&self) -> Sheet {
        match self {
            Self::Jump { .. } => Sheet::Jump,
            Self::Link { .. } => Sheet::Link,
            Self::Nick { .. } => Sheet::Nick,
            Self::Inbox { .. } => Sheet::Inbox,
            Self::Shortcuts { .. } => Sheet::Shortcuts,
            Self::About { .. } => Sheet::About,
            Self::ChannelOpen { .. } => Sheet::ChannelOpen,
            Self::ChannelList { .. } => Sheet::ChannelList,
        }
    }
}

impl Shell {
    /// Ctrl+K: opens Jump, or closes it when it is already open.
    pub fn toggle_jump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::Jump, window, cx) {
            return;
        }
        if matches!(self.modal, Some(ActiveModal::Jump { .. })) {
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
        if matches!(self.modal, Some(ActiveModal::Link { .. })) {
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
        if matches!(self.modal, Some(ActiveModal::Inbox { .. })) {
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
        if matches!(self.modal, Some(ActiveModal::Shortcuts { .. })) {
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

    /// Escape, in Qt's order: closes the slash popup first, keeping the typed text.
    /// Then it closes the top layer: the open sheet, then the Connect sheet. Then,
    /// with neither, it leaves find mode and restores the draft. Then, with nothing
    /// open, it leaves Status for the conversation. Inert otherwise.
    pub fn escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dismiss_slash_popup(window, cx) {
            return;
        }
        if !self.chord_live(Chord::Escape, window, cx) {
            return;
        }
        if self.modal.is_some() {
            self.close_modal(window, cx);
        } else if self.connect_open {
            self.close_connect(window, cx);
        } else if self.find.is_some() {
            self.leave_find(window, cx);
        } else {
            self.close_status(window, cx);
        }
    }

    /// Drops the open sheet. Focus goes back to the composer, or to Connect when
    /// that sheet is open under it, so keys never reach the composer behind the veil.
    pub fn close_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.modal = None;
        if self.connect_open {
            self.connect.restore_focus();
        } else {
            self.focus_composer(window, cx);
        }
        cx.notify();
    }

    /// The open sheet as an overlay, or nothing when no sheet is open.
    pub fn render_modal(&self, cx: &App) -> Option<AnyElement> {
        Some(match self.modal.as_ref()? {
            ActiveModal::Jump { sheet, .. } => render_filter_sheet(sheet, cx),
            ActiveModal::Link { sheet, .. } => render_filter_sheet(sheet, cx),
            ActiveModal::Nick { sheet, .. } => render_filter_sheet(sheet, cx),
            ActiveModal::Inbox { sheet, .. } => render_inbox_sheet(sheet, cx),
            ActiveModal::Shortcuts { sheet, .. } => sheet.clone().into_any_element(),
            ActiveModal::About { sheet, .. } => sheet.clone().into_any_element(),
            ActiveModal::ChannelOpen { sheet, .. } => sheet.clone().into_any_element(),
            ActiveModal::ChannelList { sheet, .. } => render_filter_sheet(sheet, cx),
        })
    }

    /// `/list [mask]` for `network_id` (Qt `openChannelListSheet`): the channel list
    /// for the mask, from the cache when this mask was asked before. A channel the
    /// network has is focused, and any other is joined, on Enter or a click. Escape
    /// closes it and returns focus to the composer.
    pub fn present_channel_list(
        &mut self,
        network_id: String,
        mask: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let listing = self
            .channel_lists
            .listing(&self.controller, &network_id, mask)
            .to_vec();
        let sheet = cx.new(|cx| FilterSheet::new(ChannelListSource { listing }, cx));
        let subscriptions = vec![
            track(cx, &sheet),
            cx.subscribe_in(
                &sheet,
                window,
                move |shell: &mut Shell, _, event: &SheetEvent<ListRow>, window, cx| match event {
                    SheetEvent::Activated(row) => {
                        let name = row.name.clone();
                        shell.close_modal(window, cx);
                        shell.join_listed_channel(&network_id, &name, cx);
                    }
                    SheetEvent::Dismissed => shell.close_modal(window, cx),
                    SheetEvent::Highlighted(_) => {}
                },
            ),
        ];
        window.focus(&sheet.read(cx).filter_focus(cx));
        self.modal = Some(ActiveModal::ChannelList {
            sheet,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    /// Enter or a click on a list row. A channel the network already has is selected,
    /// as `open_channel_name` does. Any other channel is joined through `join_invite`.
    fn join_listed_channel(&mut self, network_id: &str, name: &str, cx: &mut Context<Self>) {
        match self.find_channel(network_id, name) {
            Some(target) => {
                self.controller.set_network_collapsed(network_id, false);
                self.select(target, cx);
                cx.notify();
            }
            None => self.join_invite(network_id, name, cx),
        }
    }

    /// A click on a channel name, or a Link sheet channel row (Qt
    /// `openChannelName`). A channel the network already has becomes the open
    /// conversation. Any other name asks first, in the Channel open sheet.
    pub fn open_channel_name(
        &mut self,
        channel: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let network_id = self.selected_ref().network_id;
        if let Some(target) = self.find_channel(&network_id, channel) {
            self.controller.set_network_collapsed(&network_id, false);
            self.select(target, cx);
            self.focus_composer(window, cx);
            cx.notify();
        } else {
            self.present_channel_open(network_id, channel.to_string(), window, cx);
        }
    }

    /// The Channel open sheet. Enter joins the channel on the network it opened
    /// for, and closes the sheet; Escape or a press outside cancels.
    fn present_channel_open(
        &mut self,
        network_id: String,
        channel: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sheet = cx.new(|cx| ChannelOpenSheet::new(channel.clone(), cx));
        let subscriptions = vec![cx.subscribe_in(
            &sheet,
            window,
            move |shell: &mut Shell, _, event: &ChannelOpenEvent, window, cx| match event {
                ChannelOpenEvent::Confirmed => {
                    shell.close_modal(window, cx);
                    shell.join_invite(&network_id, &channel, cx);
                }
                ChannelOpenEvent::Dismissed => shell.close_modal(window, cx),
            },
        )];
        window.focus(&sheet.read(cx).focus_handle());
        self.modal = Some(ActiveModal::ChannelOpen {
            sheet,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    /// Activating a Jump row. A conversation expands its network, then selects
    /// it. A Status row opens that network's Status.
    pub fn jump_to(&mut self, target: &JumpTarget, window: &mut Window, cx: &mut Context<Self>) {
        match target.kind {
            JumpKind::Status => self.open_status(&target.network_id, window, cx),
            JumpKind::Conversation => {
                let Some((network, key)) = target.conversation_id.split_once('\n') else {
                    return;
                };
                self.controller.set_network_collapsed(network, false);
                self.select_by_ids(network, key, cx);
            }
        }
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
            view: JumpView::of(&self.controller),
        };
        let sheet = cx.new(|cx| FilterSheet::new(source, cx));
        let subscriptions = vec![
            track(cx, &sheet),
            cx.subscribe_in(
                &sheet,
                window,
                |shell: &mut Shell, _, event: &SheetEvent<JumpTarget>, window, cx| match event {
                    SheetEvent::Activated(target) => {
                        shell.jump_to(target, window, cx);
                        shell.close_modal(window, cx);
                    }
                    SheetEvent::Dismissed => shell.close_modal(window, cx),
                    SheetEvent::Highlighted(_) => {}
                },
            ),
        ];
        window.focus(&sheet.read(cx).filter_focus(cx));
        self.modal = Some(ActiveModal::Jump {
            sheet,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    /// The Link sheet. On a conversation it lists the topic and transcript links;
    /// on Status it lists the Status rows' URLs and INVITE channels.
    fn present_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.transcript.read(cx).rows().to_vec();
        let source = if self.status_showing() {
            LinkSource::Status { rows }
        } else {
            LinkSource::Conversation {
                topic: Some(self.controller.topic()),
                rows,
                features: self.features_of(&self.selected_ref().network_id),
            }
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
                            LinkKind::Invite => shell.join_status_invite(&text, cx),
                            LinkKind::Channel => shell.open_channel_name(&text, window, cx),
                        }
                    }
                    SheetEvent::Dismissed => shell.close_modal(window, cx),
                    SheetEvent::Highlighted(link) => shell.reveal_link_row(link.row, cx),
                },
            ),
        ];
        window.focus(&sheet.read(cx).filter_focus(cx));
        self.modal = Some(ActiveModal::Link {
            sheet,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    fn present_nick(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source = NickSource {
            members: self.controller.members(),
            self_nick: self.current_nick(),
            avatars: self.preferences.avatars,
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
        self.modal = Some(ActiveModal::Nick {
            sheet,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    fn present_inbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.controller.inbox_items().to_vec();
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
        self.modal = Some(ActiveModal::Inbox {
            sheet,
            _subscriptions: subscriptions,
        });
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
        self.modal = Some(ActiveModal::Shortcuts {
            sheet,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }

    /// The footer's version and `Restart to update`. A ready installer starts, and
    /// anything else opens About. Qt `activateVersionControl`.
    pub fn activate_version_control(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.update.read(cx).restart_ready() {
            let _ = launch_installer(&self.update, cx);
            return;
        }
        self.open_about(window, cx);
    }

    /// The window's first close starts a ready installer. Qt `onClosing`.
    pub fn launch_update_on_close(&self, cx: &mut App) {
        let _ = launch_on_close(&self.update, cx);
    }

    fn present_about(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let update = self.update.clone();
        let sheet = cx.new(|cx| AboutSheet::new(update, cx));
        let subscriptions = vec![cx.subscribe_in(
            &sheet,
            window,
            |shell: &mut Shell, _, _: &AboutEvent, window, cx| {
                // Closing the sheet cancels a check still running, as Qt does.
                shell.update.update(cx, |state, _| state.cancel_check());
                shell.close_modal(window, cx);
            },
        )];
        window.focus(&sheet.read(cx).initial_focus());
        self.modal = Some(ActiveModal::About {
            sheet,
            _subscriptions: subscriptions,
        });
        cx.notify();
    }
}

/// Re-renders the shell when the sheet notifies (its filter text changed).
fn track<S: FilterSource>(cx: &mut Context<Shell>, sheet: &Entity<FilterSheet<S>>) -> Subscription {
    cx.observe(sheet, |_, _, cx| cx.notify())
}
