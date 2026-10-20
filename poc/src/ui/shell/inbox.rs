//! What the Inbox does with a row. The sheet itself lives in `ui/sheets/inbox.rs`
//! and the open and close paths in `modal.rs`; this file applies a chosen row
//! to the shell. Ports Qt `IrcController::activateInboxItem` and
//! `dismissInboxItem`: a conversation row reveals its conversation, an invite
//! joins the channel and selects it.

use gpui::{Context, Window};

use crate::model::inbox::{join_channel, InboxItem, InboxKind};
use crate::model::sidebar::set_collapsed;
use crate::model::ConversationRef;
use crate::ui::shell::modal::ActiveModal;
use crate::ui::shell::Shell;

impl Shell {
    /// Enter or a click on an inbox row: removes it, acts on it, and closes the
    /// sheet. Focus returns to the composer, as for every sheet.
    pub fn activate_inbox(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.inbox.activate(index) else {
            return;
        };
        self.close_modal(window, cx);
        self.act_on_inbox_item(&item, cx);
        cx.notify();
    }

    /// The delete button or Delete on the sheet: drops the row without acting
    /// on it, and keeps the sheet's selection in step with the list.
    pub fn dismiss_inbox(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.inbox.dismiss(index) {
            return;
        }
        if let Some(ActiveModal::Inbox(sheet, _)) = &self.modal {
            let items = self.inbox.items().to_vec();
            sheet.update(cx, |sheet, cx| sheet.sync(items, Some(index), cx));
        }
        cx.notify();
    }

    /// Joins `channel` on `network_id` and selects it. Joining clears that
    /// channel's invite row, as Qt's `IrcInbox::consumeInvite` does. Used by the
    /// inbox and by an INVITE line in Status.
    pub fn join_invite(&mut self, network_id: &str, channel: &str, cx: &mut Context<Self>) {
        self.inbox.consume_invite(network_id, channel);
        if let Some(target) = join_channel(&mut self.world, network_id, channel) {
            self.reveal(target, cx);
        }
    }

    fn act_on_inbox_item(&mut self, item: &InboxItem, cx: &mut Context<Self>) {
        match item.kind {
            InboxKind::Invite => self.join_invite(&item.network_id, &item.target, cx),
            InboxKind::MonitorOnline => self.reveal_by_name(&item.network_id, &item.actor, cx),
            InboxKind::Mention | InboxKind::Highlight | InboxKind::Direct | InboxKind::Kick => {
                self.reveal_by_name(&item.network_id, &item.target, cx)
            }
        }
    }

    /// Looks a conversation up by name, as Qt does with the row's target, and
    /// reveals it. A name the network does not have is a no-op.
    fn reveal_by_name(&mut self, network_id: &str, name: &str, cx: &mut Context<Self>) {
        let key = self.world.network(network_id).and_then(|network| {
            network
                .conversations
                .iter()
                .find(|c| c.name.eq_ignore_ascii_case(name))
                .map(|c| c.key())
        });
        if let Some(key) = key {
            self.reveal(ConversationRef::new(network_id, key), cx);
        }
    }

    /// Expands the conversation's network so the row is visible, then selects it.
    fn reveal(&mut self, target: ConversationRef, cx: &mut Context<Self>) {
        set_collapsed(&self.world, &mut self.sidebar, &target.network_id, false);
        self.select(target, cx);
    }
}
