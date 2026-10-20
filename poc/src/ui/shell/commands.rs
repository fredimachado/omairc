use std::time::Instant;

use gpui::{App, Context, Focusable, Window};

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
        self.transcript.update(cx, |t, _| t.set_conversation(conv));
        if !conv.is_channel() {
            self.members_open = false;
        }
    }

    pub fn select_by_ids(&mut self, network_id: &str, key: &str, cx: &mut Context<Self>) {
        let target = ConversationRef::new(network_id, key);
        if self.world.find(&target).is_some() {
            self.select(target, cx);
        }
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
