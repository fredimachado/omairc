//! Desktop notification drain and activation from a toast or D-Bus action.

use gpui::{Context, Window};

use crate::controller::notify::Arrival;
use crate::desktop::notifier::Activation;
use crate::irc::plaintext::plain_irc_text;
use crate::model::title::note_title_mark;
use crate::model::ConversationRef;
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
        let key = self
            .controller
            .conversations()
            .into_iter()
            .find(|row| {
                row.key.network_id == activation.network_id
                    && row.target.eq_ignore_ascii_case(&activation.target)
            })
            .map(|row| ConversationRef::from_key(&row.key));
        if let Some(target) = key {
            self.reveal(target, cx);
            let open_at_unread = self.controller.preferences().unread;
            let msgid = activation.msgid.clone();
            self.transcript.update(cx, |transcript, cx| {
                if open_at_unread {
                    transcript.land_on_unread_mark();
                } else if !msgid.is_empty() {
                    transcript.scroll_to_msgid(&msgid);
                }
                cx.notify();
            });
        }
        self.focus_composer(window, cx);
        cx.notify();
    }
}
