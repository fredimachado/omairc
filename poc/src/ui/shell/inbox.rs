//! What the Inbox does with a row. The sheet itself lives in `ui/sheets/inbox.rs` and
//! the open and close paths in `modal.rs`; this file applies a chosen row to the shell.
//! Ports Qt `IrcController::activateInboxItem` and `dismissInboxItem`: a conversation row
//! reveals its conversation, an invite joins the channel and selects it. The rows live in
//! the controller, which removes them when the shell takes them.

use gpui::{Context, Window};

use crate::irc::inbox::{InboxItem, InboxKind};
use crate::model::ConversationRef;
use crate::ui::shell::modal::ActiveModal;
use crate::ui::shell::Shell;

impl Shell {
    /// Enter or a click on an inbox row: removes it, acts on it, and closes the sheet.
    /// Focus returns to the composer, as for every sheet.
    pub fn activate_inbox(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = self.controller.take_inbox_item(index) else {
            return;
        };
        self.close_modal(window, cx);
        self.act_on_inbox_item(&item, cx);
        self.drain_notifications(cx);
        cx.notify();
    }

    /// The delete button or Delete on the sheet: drops the row without acting on it, and
    /// keeps the sheet's selection in step with the list.
    pub fn dismiss_inbox(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.controller.take_inbox_item(index).is_none() {
            return;
        }
        if let Some(ActiveModal::Inbox { sheet, .. }) = &self.modal {
            let items = self.controller.inbox_items().to_vec();
            sheet.update(cx, |sheet, cx| sheet.sync(items, Some(index), cx));
        }
        cx.notify();
    }

    /// Joins `channel` on `network_id` and selects it. Joining clears that channel's
    /// invite row, as Qt's `IrcInbox::consumeInvite` does. Used by the inbox, an INVITE
    /// line in Status, and the Channel open sheet. The join is the controller's.
    pub fn join_invite(&mut self, network_id: &str, channel: &str, cx: &mut Context<Self>) {
        self.controller.join_channel(network_id, channel);
        self.drain_notifications(cx);
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

    /// Looks a conversation up by name, as Qt does with the row's target, and reveals it.
    /// A name the network does not have is a no-op.
    fn reveal_by_name(&mut self, network_id: &str, name: &str, cx: &mut Context<Self>) {
        let key = self
            .controller
            .conversations()
            .into_iter()
            .find(|row| row.key.network_id == network_id && row.target.eq_ignore_ascii_case(name))
            .map(|row| ConversationRef::from_key(&row.key));
        if let Some(target) = key {
            self.reveal(target, cx);
        }
    }

    /// Expands the conversation's network so the row is visible, then selects it.
    pub fn reveal(&mut self, target: ConversationRef, cx: &mut Context<Self>) {
        self.controller.set_network_collapsed(&target.network_id, false);
        self.select(target, cx);
    }
}
