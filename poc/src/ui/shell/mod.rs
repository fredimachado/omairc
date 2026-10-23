//! The window's root view. Owns the controller, the selection's presentation (the view on
//! screen, the sidebar focus, the sheets) and the entities for the transcript and inputs.
//! Rows and widgets only receive data and `Handler`s built here. The shell never writes
//! IRC state: it calls controller intents, then follows the surfaces the controller raises.

pub mod actions;
pub mod avatars;
pub mod bench;
pub mod commands;
pub mod connect;
pub mod find;
pub mod inbox;
pub mod modal;
pub mod net;
pub mod slash;
pub mod unread;
pub mod view;

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    div, prelude::*, App, Context, Entity, FocusHandle, Focusable, MouseButton,
    MouseMoveEvent, MouseUpEvent, Render, ScrollHandle, Subscription, Window,
};

use crate::controller::controller::Controller;
use crate::controller::transports::{NoTransports, TransportFactory};
use crate::irc::plaintext::plain_irc_text;
use crate::irc::viewnotify::ViewNotify;
use crate::model::composer::ComposerState;
use crate::model::connect::{ConnectState, Stop};
use crate::model::find::Find;
use crate::model::format::{format_irc, text_hits, TextHit};
use crate::model::presence::footer_presence;
use crate::model::sidebar::SidebarState;
use crate::model::slash_complete::SlashSession;
use crate::model::status::{connection_line, status_title};
use crate::model::title::{conversation_title, window_title, TitleView};
use crate::model::update_check::UpdateCheck;
use crate::model::ConversationRef;
use crate::net::{NetHub, NetTransports};
use crate::session::clock::RealClock;
use crate::ui::center::header::{people_pill, HeaderView, QueryIdentity, Subtitle};
use crate::ui::center::header_actions::{HeaderActions, HeaderOffers};
use crate::ui::center::render_center;
use crate::ui::center::slash_popup::render_slash_popup;
use crate::ui::input::SingleLineInput;
use crate::ui::members::{render_members, MembersData};
use crate::ui::scrollbar::{end_scrollbar_drag, move_scrollbar_drag, ScrollbarDragCell};
use crate::ui::shell::actions::wire_shell_actions;
use crate::ui::shell::view::{shown_view, ShownView};
use crate::ui::sheets::connect::controls::ConnectControls;
use crate::ui::sheets::connect::{render_connect, ConnectView};
use crate::ui::sidebar::{render_sidebar, SidebarData};
use crate::ui::transcript::view::NickClicked;
use crate::ui::transcript::TranscriptView;
use crate::ui::Handler;
use crate::util::{style, to_gpui};
use crate::version::version;

pub use actions::bind_shell_keys;
pub use bench::BenchState;

pub struct Shell {
    /// The live IRC state: sessions, conversations, Status, the inbox and the selection.
    /// The shell reads it and calls its intents; it never writes it.
    pub controller: Controller,
    /// The socket hub the live sessions dial through, and whose results `deliver_net` takes.
    /// `None` for the demo, which has no sockets (`net.rs`).
    pub net: Option<Rc<NetHub>>,
    /// The view the transcript was last built for (Status of a network, or a conversation).
    pub shown: ShownView,
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
    /// The composer's drafts, sent-line history and Tab cycle, per target.
    pub composer_state: ComposerState,
    /// The slash-complete popup above the composer: its rows and highlight.
    pub slash: SlashSession,
    /// Find mode (Ctrl+F) while it is on: the last query searched and the match row. The
    /// composer holds the query, and `composer_state` holds the draft.
    pub find: Option<Find>,
    /// The Connect sheet's model: the saved networks, the draft, and the sheet's tab. It
    /// lives for the window, so an unapplied draft survives closing the sheet, as Qt's does.
    pub connect: ConnectState,
    /// Whether the Connect sheet is showing. It holds the keyboard, and only Shortcuts and
    /// About open above it.
    pub connect_open: bool,
    /// The focus handle the Connect sheet tracks, so keys reach the window while it shows.
    pub connect_focus: FocusHandle,
    /// The Connect sheet's text fields, focus handles and scroll handles.
    pub connect_ui: ConnectControls,
    /// Whether the window is the active one. While it is not, a line that arrives in the
    /// open conversation counts as unread (Qt `m_windowActive`).
    pub window_active: bool,
    /// The window's activation observer is registered. It needs a window, so it is set up
    /// on the first render.
    activation_watched: bool,
    pub focus_handle: FocusHandle,
    pub typing_phase: u8,
    pub bench: BenchState,
    /// The decoded avatar images the frame draws, by reference (`avatars.rs`).
    pub avatar_memo: avatars::AvatarMemo,
    /// The Windows installer update's state. The About sheet and the footer observe it.
    pub update: Entity<UpdateCheck>,
    _subscriptions: Vec<Subscription>,
}

impl Shell {
    /// The window. `demo` runs the seeded `--demo-server` controller, which reads and writes
    /// nothing. Otherwise the controller loads the user's saved networks and preferences; with no
    /// complete network saved yet, the Connect sheet opens and stays open until one is.
    /// `lines` replaces the selection with the `#bench` channel of that many lines. `net` is the
    /// socket hub a live launch dials through; without one, every network's start is refused.
    pub fn new(
        cx: &mut Context<Self>,
        lines: Option<usize>,
        bench: bool,
        demo: bool,
        net: Option<Rc<NetHub>>,
    ) -> Self {
        let mut controller = if demo {
            // The demo keeps nothing: it never reads or writes the user's files.
            Controller::new_demo().expect("the demo controller starts")
        } else {
            // The saved networks, preferences and open queries come back, and each complete
            // network that asks to connect on launch starts through the socket hub.
            let transports: Box<dyn TransportFactory> = match &net {
                Some(hub) => Box::new(NetTransports::new(hub.clone())),
                None => Box::new(NoTransports),
            };
            let mut controller = Controller::new_user(Rc::new(RealClock::new()), transports);
            controller.activate_startup();
            controller
        };
        if let Some(n) = lines {
            controller.bench_channel(n);
        }

        let connect = if demo {
            ConnectState::seed(&connect_seeds(&controller))
        } else {
            ConnectState::from_stored(&controller.stored_profiles())
        };
        let required = connect.setup_required();
        // With nothing selected, the window opens on Status, as Qt's does: the draft the sheet is
        // editing on a first run, else the first network in the sidebar.
        if controller.selected().is_none() {
            let network = if required {
                Some(connect.selected().to_owned())
            } else {
                controller.network_order().first().cloned()
            };
            if let Some(network) = network.filter(|id| !id.is_empty()) {
                controller.open_status(&network);
            }
        }
        let shown = shown_view(&controller);
        let composer_state = ComposerState::starting_at(shown.draft_key());
        let update = cx.new(|_| {
            UpdateCheck::new(
                version(),
                crate::desktop::installer::running_copy_is_installed(),
            )
        });
        let update_observe = cx.observe(&update, |_, _, cx| cx.notify());
        let scrollbar_drag: ScrollbarDragCell = Default::default();
        let composer = cx.new(|cx| {
            SingleLineInput::new("", "Composer", cx.focus_handle())
                .height(46.)
                .frameless()
        });
        let composer_focus = composer.read(cx).focus_handle(cx);
        let transcript = cx.new(|_| TranscriptView::new(composer_focus, scrollbar_drag.clone()));

        let transcript_events = cx.subscribe(&transcript, |shell: &mut Shell, _, event: &NickClicked, cx| {
            shell.open_direct_nick(&event.0, cx);
        });
        // Every composer edit, caret move and history step re-reads the slash popup. The
        // popup only re-renders when its rows or highlight changed. A changed find query
        // searches again from the top.
        let composer_events = cx.observe(&composer, |shell: &mut Shell, _, cx| {
            shell.follow_find_query(cx);
            shell.report_composer_text(cx);
            if shell.sync_slash_popup(cx) {
                cx.notify();
            }
        });
        // Every Connect field's edit writes the draft, so Apply and Discard see it.
        let connect_ui = ConnectControls::new(cx);
        let connect_edits: Vec<Subscription> = connect_ui
            .fields()
            .into_iter()
            .map(|(field, input)| {
                cx.observe(&input, move |shell: &mut Shell, _, cx| {
                    shell.connect_field_edited(field, cx);
                })
            })
            .collect();

        let mut shell = Self {
            controller,
            net,
            shown,
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
            composer_state,
            slash: SlashSession::new(),
            find: None,
            connect,
            connect_open: required,
            connect_focus: cx.focus_handle(),
            connect_ui,
            window_active: true,
            activation_watched: false,
            focus_handle: cx.focus_handle(),
            typing_phase: 0,
            bench: BenchState {
                enabled: bench,
                ..Default::default()
            },
            avatar_memo: Default::default(),
            update,
            _subscriptions: [transcript_events, composer_events, update_observe]
                .into_iter()
                .chain(connect_edits)
                .collect(),
        };

        let joins_avatars = shell.controller.preferences().avatars;
        shell.transcript.update(cx, |t, _| t.set_avatars(joins_avatars));
        shell.show_shown_view(None, cx);
        shell.sync_send_limit(cx);
        if required {
            // The first field takes focus, as Qt focuses it when the sheet opens.
            let first = shell.connect.initial_focus();
            shell.connect.request_focus(first);
        }
        shell
    }

    /// A network's display name. While Connect holds a network that is not saved yet (the
    /// first run's draft), the draft's name is shown; otherwise the controller names it.
    pub fn network_name(&self, network_id: &str) -> String {
        self.connect
            .draft_name(network_id)
            .unwrap_or_else(|| self.controller.network_name(network_id))
    }

    /// One controller tick: the timers that are due fire, and the shell follows what they
    /// changed. The UI loop calls this on each interval.
    pub fn tick(&mut self, cx: &mut Context<Self>) {
        self.controller.tick();
        self.drain_notifications(cx);
    }

    /// Drains the surfaces the controller raised and brings the shell up to them. A
    /// selection the controller made on its own (a join, a close, a removal) is followed
    /// the same way a click is. The transcript rebuilds for changed lines, and the window
    /// repaints.
    pub fn drain_notifications(&mut self, cx: &mut Context<Self>) {
        let _arrivals = self.controller.take_arrivals();
        let surfaces = self.controller.take_notifications();
        self.sync_view(cx);
        let status = self.status_showing();
        let rebuild = surfaces.iter().any(|surface| match surface {
            ViewNotify::Messages | ViewNotify::MemberReset => !status,
            ViewNotify::Status => status,
            _ => false,
        });
        if rebuild {
            self.rebuild_view_keeping_place(cx);
        }
        if !surfaces.is_empty() {
            cx.notify();
        }
    }
}

/// The networks the Connect sheet starts from: every network the controller holds, with
/// its nick, its CHANTYPES and the channels it has joined. The icon colour follows the
/// network's place, as the seeded profiles number theirs.
fn connect_seeds(controller: &Controller) -> Vec<crate::model::connect::NetworkSeed> {
    use crate::model::connect::{NetworkSeed, ICON_COLOR_COUNT};
    controller
        .network_order()
        .into_iter()
        .enumerate()
        .map(|(index, id)| NetworkSeed {
            nick: controller.current_nick_for(&id),
            icon_color: Some((index as u8 % ICON_COLOR_COUNT) + 1),
            chantypes: controller.features(&id).channel_types().to_owned(),
            channels: controller
                .session(&id)
                .map(|session| session.autojoin_channels())
                .unwrap_or_default(),
            network_id: id,
        })
        .collect()
}

impl Render for Shell {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.bench.enabled && self.bench.first_frame.is_none() {
            self.bench.first_frame = Some(self.bench.started.elapsed());
        }
        // Window focus drives the unread rules. The observer needs the window, so it is
        // registered here, once, and reads the starting focus from it.
        if !self.activation_watched {
            self.activation_watched = true;
            self.window_active = window.is_window_active();
            let subscription = cx.observe_window_activation(
                window,
                |shell: &mut Shell, window: &mut Window, cx: &mut Context<Shell>| {
                    shell.set_window_active(window.is_window_active(), cx);
                },
            );
            self._subscriptions.push(subscription);
        }

        let populated = self.populated_networks();
        let disconnectable = self.connect_disconnectable();
        // Connect's focus. The stops on the showing tab get their handles, then the focus
        // request a Tab, an open or a tab cycle left is applied. The stop that holds focus
        // is read back, and the rail's Enter arming resets when the rail lets go. Finally
        // the fields show the draft.
        let mut connect_focused: Option<Stop> = None;
        if self.connect_open {
            let stops = self.connect.stops(&populated, disconnectable);
            self.connect_ui.ensure(cx, &stops);
            if let Some(stop) = self.connect.take_focus_request() {
                self.connect_ui.ensure(cx, std::slice::from_ref(&stop));
                match self.connect_ui.handle(&stop) {
                    Some(handle) => window.focus(handle),
                    None => window.focus(&self.connect_focus),
                }
            }
            connect_focused = self.connect_ui.focused(&stops, window);
            // A click focuses a stop without the keys, so the model learns it here. Focus on
            // Shortcuts or About leaves the last stop in place for the restore.
            if let Some(stop) = &connect_focused {
                self.connect.set_focus(Some(stop.clone()));
            }
            if !matches!(
                connect_focused,
                Some(Stop::Network(_) | Stop::AddNetwork)
            ) {
                self.connect.rail_left();
            }
            self.sync_connect_inputs(cx);
        }

        let app_style = style(cx);
        let entity = cx.entity();
        // Qt gates the panel on `win.width >= scaledSize(980)`. Status hides it.
        let wide_enough = f32::from(window.viewport_size().width)
            >= app_style.scaled(980.) as f32;
        // Qt shows the panel only on a channel the user is in (`membersPanel.visible`).
        let show_members = self.members_open
            && self.controller.is_channel()
            && self.controller.channel_joined()
            && !self.status_showing()
            && wide_enough;
        // A hidden member list cannot keep focus (a DM, a narrow window, or a closed panel):
        // hand focus to the composer, as Ctrl+Shift+M does.
        if !show_members && self.member_focus.is_focused(window) {
            self.focus_composer(window, cx);
        }
        let selected = self.selected_ref();
        let status = self.status_network();
        let wanted_window_title = match &status {
            Some(network_id) => window_title(&self.controller, TitleView::Status(&self.network_name(network_id))),
            None => window_title(&self.controller, TitleView::Conversation(Some(&selected))),
        };
        if wanted_window_title != self.window_title {
            window.set_window_title(&wanted_window_title);
            self.window_title = wanted_window_title;
        }

        // The header reads the Status pane's title and its connection line, or the
        // conversation's title and people count. Status shows the connection line from the
        // controller: the network's last error while one is set, else its connection word.
        let (title, people, actions) = match &status {
            Some(network_id) => (
                status_title(&self.network_name(network_id)),
                None,
                None,
            ),
            None => {
                let channel = self.controller.is_channel();
                let joined = self.controller.channel_joined();
                let people = people_pill(channel, joined, self.controller.people_count());
                let actions = HeaderActions {
                    offers: HeaderOffers::for_selection(
                        channel,
                        joined,
                        self.controller.can_close_selection(),
                    ),
                    on_leave: leave_handler(entity.clone()),
                    on_join: join_handler(entity.clone()),
                    on_close: close_selection_handler(entity.clone()),
                };
                (
                    conversation_title(&self.controller, &selected).unwrap_or_default(),
                    people,
                    Some(actions),
                )
            }
        };
        // A direct message's header names its peer: the presence dot and labels beside the
        // nick, and the peer's real name as the second line (Qt's `queryPeerHeader`). A
        // channel's second line is its topic, with clickable URLs and channel names, checked
        // against the network's CHANTYPES. Status has neither.
        let peer = status
            .is_none()
            .then(|| self.controller.peer_header())
            .flatten();
        let away_visible = self.controller.has_away_presence();
        let query = peer.as_ref().map(|peer| QueryIdentity {
            presence: peer.presence,
            labels: &peer.labels,
            away_visible,
        });
        let peer_realname = peer
            .as_ref()
            .map(|peer| plain_irc_text(&peer.realname))
            .unwrap_or_default();
        let topic = (status.is_none() && peer.is_none()).then(|| {
            let features = self.features_of(&selected.network_id);
            let text = format_irc(&self.controller.topic());
            let hits = text_hits(&text, &features);
            (text, hits)
        });
        let connection = status
            .as_deref()
            .map(|network_id| connection_line(&self.controller, network_id))
            .unwrap_or_default();
        let subtitle = match (&peer, &topic) {
            (Some(_), _) => Subtitle::Plain(&peer_realname),
            (None, Some((text, hits))) => Subtitle::Topic {
                text,
                hits,
                on_hit: topic_handler(entity.clone()),
            },
            (None, None) => Subtitle::Plain(&connection),
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

        let update_ready = self.update.read(cx).restart_ready();
        let conversations = self.controller.conversations();
        let networks = self.network_headers(&conversations);
        // The member list is read only while its panel shows. Its avatars, with the rest
        // this frame draws, get their photos here, and the transcript and a Nick sheet
        // are handed the same set.
        let members = if show_members {
            self.controller.members()
        } else {
            Vec::new()
        };
        // The member rows' typing dots read the controller only while the panel shows, and
        // only on a network that can show typing (`HasTyping`). Away is gated by away-notify.
        let typing_nicks = if show_members && self.controller.has_typing() {
            self.controller.typing_nicks()
        } else {
            Vec::new()
        };
        let photos = self.avatar_photos_for_frame(&networks, &conversations, &members, cx);
        self.push_photos(&photos, cx);
        // The footer is the focused network's identity (Qt's `identityNetworkId`): its nick,
        // and the presence the controller reads for that network.
        let footer_network = self.focused_network();
        let presence = footer_presence(&self.controller);
        let sidebar = render_sidebar(
            SidebarData {
                update_ready,
                networks: &networks,
                conversations: &conversations,
                state: &self.sidebar,
                selected: &selected,
                footer_network: &footer_network,
                presence,
                status_open: status.is_some(),
                inbox_count: self.controller.inbox_items().len() as u32,
                typing_phase: self.typing_phase,
                avatars: self.controller.preferences().avatars,
                photos: &photos,
                on_status: status_handler(entity.clone()),
                on_edit: edit_handler(entity.clone()),
                on_inbox: inbox_handler(entity.clone()),
                on_about: about_handler(entity.clone()),
                on_select: select_handler(entity.clone()),
                on_close: close_handler(entity.clone()),
                on_toggle: collapse_handler(entity.clone()),
            },
            &app_style,
            &self.sidebar_scroll,
            entity.clone(),
            &self.scrollbar_drag,
        );
        let header = HeaderView {
            title: &title,
            subtitle,
            query,
            people,
            actions,
            members_visible: show_members,
        };
        // The composer shell takes the accent border while its draft has focus in an
        // active window, and SEND lights only when the draft has text (Qt's `composerShell`).
        let composer_focused =
            self.composer.read(cx).focus_handle(cx).is_focused(window) && window.is_window_active();
        let has_text = !self.composer.read(cx).text().trim().is_empty();
        let center = render_center(
            header,
            self.transcript.clone(),
            self.composer.clone(),
            &app_style,
            toggle_members_handler(entity.clone()),
            send_handler(entity.clone()),
            slash_popup,
            composer_focused,
            has_text,
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
            .capture_any_mouse_up(cx.listener(|shell, ev: &MouseUpEvent, window, cx| {
                if ev.button == MouseButton::Left {
                    end_scrollbar_drag(&shell.scrollbar_drag);
                    shell.end_text_drag(ev.position, window, cx);
                }
            }))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|shell, ev: &MouseUpEvent, window, cx| {
                    shell.end_text_drag(ev.position, window, cx);
                }),
            )
            .child(sidebar)
            .child(center)
            .when(show_members, |this| {
                let people = self.controller.people_count();
                this.child(render_members(
                    MembersData {
                        members: &members,
                        people,
                        self_nick: &self.current_nick(),
                        on_nick: nick_handler(entity.clone()),
                        on_heading: heading_handler(entity.clone()),
                        focus: &self.member_focus,
                        cursor: members_cursor,
                        avatars: self.controller.preferences().avatars,
                        photos: &photos,
                        typing: &typing_nicks,
                        typing_phase: self.typing_phase,
                        away_visible,
                    },
                    &app_style,
                    &self.members_scroll,
                    entity.clone(),
                    &self.scrollbar_drag,
                ))
            })
            // Connect sits above the window and below Shortcuts and About, which open after it.
            .when(self.connect_open, |this| {
                this.child(render_connect(&ConnectView {
                    state: &self.connect,
                    populated: &populated,
                    disconnectable,
                    prefs: self.controller.preferences(),
                    controls: &self.connect_ui,
                    focused: connect_focused.as_ref(),
                    card_focus: &self.connect_focus,
                    style: &app_style,
                    on_stop: connect_stop_handler(entity.clone()),
                    on_dismiss: connect_dismiss_handler(entity.clone()),
                }))
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

/// A click on a URL or channel name in the header's topic.
fn topic_handler(entity: Entity<Shell>) -> Handler<TextHit> {
    Rc::new(move |hit: TextHit, window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| shell.activate_text_hit(hit, window, cx));
    })
}

/// A click on a Connect control: the shell acts on the stop it landed on.
fn connect_stop_handler(entity: Entity<Shell>) -> Handler<Stop> {
    Rc::new(move |stop: Stop, window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| shell.connect_click(stop, window, cx));
    })
}

/// A click on the Connect dimmer. Closing is refused while the sheet is required.
fn connect_dismiss_handler(entity: Entity<Shell>) -> Handler<()> {
    Rc::new(move |_: (), window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| shell.close_connect(window, cx));
    })
}

/// A network's `edit` label: opens the Connect sheet on that network.
fn edit_handler(entity: Entity<Shell>) -> Handler<String> {
    Rc::new(move |network_id: String, window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| shell.open_connect_for(&network_id, window, cx));
    })
}

/// The footer's `inbox` link: opens the Inbox sheet, or closes it when open.
fn inbox_handler(entity: Entity<Shell>) -> Handler<()> {
    Rc::new(move |_: (), window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| shell.toggle_inbox(window, cx));
    })
}

/// The footer's version link: a ready installer starts, and anything else opens About.
fn about_handler(entity: Entity<Shell>) -> Handler<()> {
    Rc::new(move |_: (), window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| shell.activate_version_control(window, cx));
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

/// SEND: send the draft, then focus the composer. In find mode SEND moves to the next
/// match instead, as Qt's `sendMessage` does. A command that opens a sheet (`/list`)
/// keeps the focus there.
fn send_handler(entity: Entity<Shell>) -> Handler<()> {
    Rc::new(move |_: (), window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| {
            if shell.find.is_some() {
                shell.begin_or_advance_find(window, cx);
            } else {
                shell.send_message(window, cx);
            }
            if shell.modal.is_none() {
                shell.focus_composer(window, cx);
            }
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

/// A middle click on a sidebar row: closes that conversation.
fn close_handler(entity: Entity<Shell>) -> Handler<ConversationRef> {
    Rc::new(move |target: ConversationRef, window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| {
            shell.close_conversation_row(&target, window, cx);
        });
    })
}

/// A click on the header's LEAVE button.
fn leave_handler(entity: Entity<Shell>) -> Handler<()> {
    Rc::new(move |_: (), window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| shell.leave_selected_channel(window, cx));
    })
}

/// A click on the header's JOIN button.
fn join_handler(entity: Entity<Shell>) -> Handler<()> {
    Rc::new(move |_: (), window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| shell.join_selected_channel(window, cx));
    })
}

/// A click on the header's CLOSE button.
fn close_selection_handler(entity: Entity<Shell>) -> Handler<()> {
    Rc::new(move |_: (), window: &mut Window, cx: &mut App| {
        entity.update(cx, |shell, cx| shell.close_selection(window, cx));
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

/// The interval of the controller's timers (autoaway, typing, reconnect), in milliseconds.
pub const CONTROLLER_TICK: Duration = Duration::from_millis(100);
