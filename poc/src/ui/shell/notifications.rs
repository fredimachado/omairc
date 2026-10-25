//! Desktop notification drain and activation from a toast or D-Bus action.

use gpui::{Context, Window};

use crate::controller::notify::Arrival;
use crate::desktop::notifier::Activation;
use crate::irc::plaintext::plain_irc_text;
use crate::irc::viewnotify::ViewNotify;
use crate::model::title::note_title_mark;
use crate::ui::shell::Shell;

impl Shell {
    pub fn drain_notifications(&mut self, cx: &mut Context<Self>) {
        for arrival in self.controller.take_arrivals() {
            match arrival {
                Arrival::Mention(mention) => {
                    let arrival_active = self.arrival_active();
                    note_title_mark(
                        &mut self.title_mark,
                        arrival_active,
                        &mention.author,
                        &plain_irc_text(&mention.body),
                        &mention.network_id,
                        &mention.target,
                    );
                    if !arrival_active {
                        let msgid = mention.msgid.value.clone();
                        self.notifier.notify(
                            &mention.author,
                            &plain_irc_text(&mention.body),
                            &mention.network_id,
                            &mention.target,
                            &msgid,
                        );
                    }
                }
                Arrival::Monitor {
                    display,
                    body,
                    network_id,
                } => self.notifier.notify(&display, &body, &network_id, &display, ""),
            }
        }
        let surfaces = self.controller.take_notifications();
        let mut connect_synced = false;
        for network_id in self.controller.take_background_profile_syncs() {
            if let Some(profile) = self.controller.profile(&network_id) {
                self.connect.sync_background_profile(profile);
                connect_synced = true;
            }
        }
        if connect_synced {
            self.sync_connect_inputs(cx);
        }
        self.sync_view(cx);
        self.sync_channel_list_sheet(cx);
        let status = self.status_showing();
        let refresh_messages = surfaces.iter().any(|surface| match surface {
            ViewNotify::Messages | ViewNotify::MemberReset => !status,
            ViewNotify::Status => status,
            _ => false,
        });
        let refresh_typing = !status
            && surfaces.iter().any(|surface| {
                matches!(surface, ViewNotify::Typing | ViewNotify::RearmTyping)
            });
        if surfaces.iter().any(|surface| *surface == ViewNotify::Inbox) {
            self.sync_inbox_sheet(cx);
        }
        if refresh_messages {
            self.refresh_shown_conversation(cx);
        } else if refresh_typing {
            self.refresh_shown_typing(cx);
        }
        if !surfaces.is_empty() {
            cx.notify();
        }
    }

    /// Raises the window and reveals the conversation a notification named.
    pub fn activate_notification(
        &mut self,
        activation: Activation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if activation.network_id.is_empty() || activation.target.is_empty() {
            return;
        }
        window.activate_window();
        self.title_mark = None;
        let was_same = self
            .controller
            .selected_network_id()
            .is_some_and(|network_id| network_id == activation.network_id)
            && self
                .controller
                .selected_target()
                .eq_ignore_ascii_case(&activation.target);
        self.controller
            .set_network_collapsed(&activation.network_id, false);
        self.controller
            .reveal_conversation(&activation.network_id, &activation.target);
        self.sync_view(cx);
        if !self.status_showing() {
            let open_at_unread = self.controller.preferences().unread;
            let msgid = activation.msgid.clone();
            self.transcript.update(cx, |transcript, cx| {
                if !was_same && open_at_unread {
                    transcript.land_on_unread_mark(cx);
                } else if !open_at_unread {
                    if !msgid.is_empty() && transcript.scroll_to_msgid(&msgid, cx) {
                        // The notification's msgid wins over a saved scroll place.
                    } else if !was_same {
                        transcript.scroll_to_bottom(cx);
                    }
                }
                cx.notify();
            });
        }
        self.focus_composer(window, cx);
        cx.notify();
    }
}
