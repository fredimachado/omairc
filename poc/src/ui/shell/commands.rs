use std::time::Instant;

use gpui::{App, Context, Focusable, Window};

use crate::model::directs::{close_direct, open_direct};
use crate::model::sidebar::{set_collapsed, sidebar_rows};
use crate::model::{
    continues_group, local_send_stamp, outgoing_message, row_from_mock_message, Conversation,
    ConversationRef, Network,
};
use crate::ui::shell::Shell;

/// Shell behaviour: selection, sending, focus. Builds no elements.
impl Shell {
    pub fn current_network(&self) -> &Network {
        self.world
            .network(&self.selected.network_id)
            .expect("selected network is in the world")
    }

    pub fn current_conversation(&self) -> &Conversation {
        self.world
            .find(&self.selected)
            .expect("selected conversation is in the world")
    }

    /// Makes `target` the open conversation. Opening reads it, so its unread
    /// count and mention flag clear.
    pub fn select(&mut self, target: ConversationRef, cx: &mut Context<Self>) {
        if let Some(conv) = self.world.find_mut(&target) {
            conv.mark_read();
        }
        self.selected = target;
        let conv = self.current_conversation();
        let self_nick = self.current_network().nick.clone();
        self.transcript
            .update(cx, |t, _| t.set_conversation(conv, &self_nick));
        if !conv.is_channel() {
            self.members_open = false;
        }
    }

    /// A nick click (transcript, member list): open or select that DM on the
    /// selected conversation's network. Self and channel-looking nicks do nothing.
    pub fn open_direct_nick(&mut self, nick: &str, cx: &mut Context<Self>) {
        let network_id = self.selected.network_id.clone();
        if let Some(target) = open_direct(&mut self.world, &network_id, nick) {
            self.select(target, cx);
        }
    }

    /// Ctrl+W on a DM, or a middle click on its row. Closing the selected DM
    /// selects the neighbour (or the first row when none is left). Channels are a no-op.
    pub fn close_direct_message(
        &mut self,
        target: &ConversationRef,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let was_selected = *target == self.selected;
        let Some(next) = close_direct(&mut self.world, &self.sidebar, target) else {
            return;
        };
        if was_selected {
            let fallback = sidebar_rows(&self.world, &self.sidebar)
                .first()
                .map(|row| row.conversation.clone());
            if let Some(row) = next.or(fallback) {
                self.select(row, cx);
            }
            self.focus_composer(window, cx);
        }
        cx.notify();
    }

    pub fn select_by_ids(&mut self, network_id: &str, key: &str, cx: &mut Context<Self>) {
        let target = ConversationRef::new(network_id, key);
        if self.world.find(&target).is_some() {
            self.select(target, cx);
        }
    }

    /// The network header chevron. Collapsing a network never changes the
    /// selection, so a hidden selected row stays selected, as in Qt.
    pub fn toggle_collapsed(&mut self, network_id: &str, cx: &mut Context<Self>) {
        let collapsed = !self.sidebar.is_collapsed(network_id);
        if set_collapsed(&self.world, &mut self.sidebar, network_id, collapsed) {
            cx.notify();
        }
    }

    /// Ctrl+Shift+S: collapse or restore the server list by width. Drops
    /// header focus, as Qt does.
    pub fn toggle_rail(&mut self, cx: &mut Context<Self>) {
        self.sidebar.rail_visible = !self.sidebar.rail_visible;
        self.sidebar.focus = None;
        cx.notify();
    }

    pub fn inbox_count(&self) -> u32 {
        self.current_network()
            .conversations
            .iter()
            .filter(|c| c.unread > 0)
            .count() as u32
    }

    pub fn focus_composer(&self, window: &mut Window, cx: &App) {
        window.focus(&self.composer.read(cx).focus_handle(cx));
    }

    pub fn send_message(&mut self, cx: &mut Context<Self>) {
        let text = self.composer.read(cx).text().trim().to_string();
        if text.is_empty() {
            return;
        }
        let append_start = self.bench.enabled.then(Instant::now);
        let author = self.current_network().nick.clone();
        let (day, time) = local_send_stamp();
        let message = outgoing_message(&text, &author, &time, &day);
        let (continues, id) = {
            let rows = self.transcript.read(cx).rows();
            (
                continues_group(rows.last(), &author, &time),
                rows.len() as u64 + 1,
            )
        };
        let row = row_from_mock_message(id, &message, continues);
        // Stored with the conversation, so switching away and back keeps it.
        let selected = self.selected.clone();
        self.world.record_outgoing(&selected, message);
        self.transcript.update(cx, |t, _| t.append_outgoing(row));
        self.composer.update(cx, |c, cx| {
            c.clear();
            cx.notify();
        });
        if let Some(start) = append_start {
            self.bench.append_latency = Some(start.elapsed());
        }
        cx.notify();
    }

    /// Ends a text drag on mouse-up; notifies only when a drag was running.
    pub fn end_text_drag(&mut self, position: gpui::Point<gpui::Pixels>, cx: &mut Context<Self>) {
        self.transcript.update(cx, |t, cx| {
            if t.finish_drag(position) {
                cx.notify();
            }
        });
    }
}
