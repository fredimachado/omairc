//! Desktop notification drain and activation from a toast or D-Bus action.

use chrono::TimeDelta;
use gpui::{Context, Window};

use crate::controller::notify::Arrival;
use crate::desktop::notifier::Activation;
use crate::irc::plaintext::plain_irc_text;
use crate::irc::viewnotify::ViewNotify;
use crate::model::title::note_title_mark;
use crate::model::ConversationRef;
use crate::ui::shell::Shell;

/// How long a saved place waits for its history while a request is in flight. A request that
/// never settles (a FAIL that names no target, a silent server) gives the wait up at this.
pub(crate) const RESTORE_WAIT: TimeDelta = TimeDelta::seconds(12);

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
        self.retire_wait_for_sent_lines(&surfaces, cx);
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
        // A restore waiting on history is given up once the shown conversation's request has
        // settled and its line never arrived, so the place lands as it does with no history.
        // A request that never settles gives the wait up at its deadline the same way.
        let waiting = self.transcript.read(cx).has_pending_restore();
        let since = *self.restore_wait_since.get_or_insert_with(|| self.controller.now());
        let expired = self.controller.now() - since >= RESTORE_WAIT;
        if !waiting {
            self.restore_wait_since = None;
        } else if expired || !self.controller.transcript_history_awaited() {
            self.restore_wait_since = None;
            self.transcript.update(cx, |t, cx| t.give_up_pending_restore(cx));
            let follow_end = self.transcript.read(cx).reports_follow_end();
            self.report_transcript_viewport_from_shell(follow_end, cx);
        }
        if !surfaces.is_empty() {
            cx.notify();
        }
    }

    /// A line the controller accepted to the conversation the transcript shows retires a
    /// waiting place, so the live page is what gets saved. The target of each accepted line
    /// (`ViewNotify::Sent`) decides it, whatever sent it (composer, `/me`, `/msg`, `/notice`,
    /// or the CLI). A line to another target leaves the wait, and so does any line while
    /// Status shows. The viewport is reported as it was for a composer line.
    fn retire_wait_for_sent_lines(&mut self, surfaces: &[ViewNotify], cx: &mut Context<Self>) {
        if self.shown.status.is_some() {
            return;
        }
        let shown = &self.shown.conversation;
        let retires = surfaces.iter().any(|surface| {
            matches!(surface, ViewNotify::Sent(key) if ConversationRef::from_key(key) == *shown)
        });
        if !retires {
            return;
        }
        self.transcript.update(cx, |t, _| t.clear_pending_restore());
        let follow_end = self.transcript.read(cx).reports_follow_end();
        self.report_transcript_viewport_from_shell(follow_end, cx);
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
