//! Desktop notification drain and activation from a toast or D-Bus action.

use gpui::{Context, Window};

use crate::controller::notify::Arrival;
use crate::desktop::notifier::Activation;
use crate::irc::plaintext::plain_irc_text;
use crate::model::title::note_title_mark;
use crate::ui::shell::Shell;

impl Shell {
    pub fn drain_notifications(&mut self, cx: &mut Context<Self>) {
        for arrival in self.controller.take_arrivals() {
            match arrival {
                Arrival::Mention(mention) => {
                    note_title_mark(
                        &mut self.title_mark,
                        self.window_active,
                        &mention.author,
                        &plain_irc_text(&mention.body),
                        &mention.network_id,
                        &mention.target,
                    );
                    if !self.window_active {
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
        self.sync_view(cx);
        let status = self.status_showing();
        let rebuild = surfaces.iter().any(|surface| match surface {
            crate::irc::viewnotify::ViewNotify::Messages
            | crate::irc::viewnotify::ViewNotify::MemberReset => !status,
            crate::irc::viewnotify::ViewNotify::Status => status,
            _ => false,
        });
        if rebuild {
            self.rebuild_view_keeping_place(cx);
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
