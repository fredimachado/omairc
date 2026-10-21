//! What the composer's line does: Enter and SEND. Plain text goes to the selected
//! conversation through the controller. A slash line runs through `model::slash`, and a
//! refused one keeps the typed text. Replies follow Qt's routing: a conversation gets
//! whois rows (`IrcWhoisTranscriptEvent`), and a Status pane gets its own log lines
//! (`IrcStatusEntry::outcome`). Every line lands through a controller intent, and the
//! shell repaints from the surfaces the controller raises. The slash-complete popup's
//! state and keys live here too, since they edit the same line.

use std::time::Instant;

use gpui::{App, Context, Window};

use crate::controller::channellist::ListOutcome;
use crate::model::chords::Chord;
use crate::model::slash::{
    command_outcome_text, ctcp_target, help_text, parse, pref_reply, version_reply_text, Command,
    CommandOutcome, PrefRequest, SelectedView, Surface, Words,
};
use crate::ui::shell::Shell;

impl Shell {
    /// The surface the composer types into: Status while it shows, else the conversation.
    pub fn surface(&self) -> Surface {
        if self.status_showing() {
            Surface::Status
        } else {
            Surface::Conversation
        }
    }

    /// The network that owns the open view: Status's network, else the selected
    /// conversation's. Qt's `errorNetworkId` picks the same one.
    pub fn surface_network(&self) -> String {
        self.focused_network()
    }

    /// Whether the slash-complete popup shows: its state is open and no sheet covers the composer.
    pub fn slash_popup_showing(&self) -> bool {
        self.modal.is_none() && self.slash.is_open()
    }

    /// Recomputes the popup after the composer's text, the surface or a history walk
    /// changed. Returns whether the popup changed, so the caller re-renders only then.
    pub fn sync_slash_popup(&mut self, cx: &App) -> bool {
        let before = self.slash.clone();
        // Find mode types a query, not a command, so the popup stays shut.
        if self.find.is_some() {
            self.slash.close();
        } else {
            let text = self.composer_text(cx);
            let surface = self.surface();
            let browsing = self.composer_state.is_browsing();
            self.slash.sync(&text, surface, browsing);
        }
        self.slash != before
    }

    /// A click on a popup row: inserts that row, as Tab does. The shell focuses the
    /// composer afterwards.
    pub fn pick_slash_hit(&mut self, index: usize, window: &Window, cx: &mut Context<Self>) {
        if !self.chord_live(Chord::SlashPopup, window, cx) {
            return;
        }
        let text = self.composer_text(cx);
        if let Some(next) = self.slash.activate(index, &text) {
            self.set_composer_text(next, cx);
        }
        cx.notify();
    }

    /// Up or Down while the popup is open: moves its highlight, with wrap. Returns
    /// whether the key was used.
    pub fn move_slash_selection(
        &mut self,
        delta: isize,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.chord_live(Chord::SlashPopup, window, cx) {
            return false;
        }
        self.slash.move_selection(delta);
        cx.notify();
        true
    }

    /// Tab while the popup is open: inserts the highlighted row. Returns false when Tab
    /// should fall through to nick completion.
    pub fn complete_slash_verb(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if !self.chord_live(Chord::SlashPopup, window, cx) {
            return false;
        }
        let text = self.composer_text(cx);
        let Some(next) = self.slash.tab(&text) else {
            return false;
        };
        self.set_composer_text(next, cx);
        cx.notify();
        true
    }

    /// Enter while the popup is open: inserts the highlighted row when the typed word is
    /// partial. Returns false on an exact name, so Enter sends it.
    pub fn enter_slash_verb(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if !self.chord_live(Chord::SlashPopup, window, cx) {
            return false;
        }
        let text = self.composer_text(cx);
        let Some(next) = self.slash.enter(&text) else {
            return false;
        };
        self.set_composer_text(next, cx);
        cx.notify();
        true
    }

    /// Escape while the popup is open: closes it and keeps the text. Returns false when
    /// the popup is shut, so Escape reaches the other handlers.
    pub fn dismiss_slash_popup(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if !self.chord_live(Chord::SlashDismiss, window, cx) || !self.slash.dismiss() {
            return false;
        }
        cx.notify();
        true
    }

    /// Enter or SEND. A line of plain text goes to the selected conversation, and a slash
    /// line runs its command. A refused line keeps the composer text. A line that runs
    /// clears the composer and joins its history. `/list` opens a sheet, which takes the
    /// keyboard from the composer, so callers check for one before they refocus the
    /// composer.
    pub fn send_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Find mode holds a query in the field. Its callers move to the next match.
        if self.find.is_some() {
            return;
        }
        let text = self.composer_text(cx).trim().to_string();
        if text.is_empty() {
            return;
        }
        let command = parse(&text);
        let outcome = match command.rejection(self.surface()) {
            Some(outcome) => Err(outcome),
            None => self.run_command(&command, window, cx),
        };
        match outcome {
            Ok(()) => self.finish_command(&text, cx),
            Err(outcome) => {
                let refusal = command_outcome_text(outcome, &command);
                self.refuse_command(&refusal, cx);
            }
        }
    }

    /// Runs a command that may run on this surface. An `Err` is a refusal found while
    /// running it, such as `/version` with a channel target.
    fn run_command(
        &mut self,
        command: &Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), CommandOutcome> {
        match command {
            // Blank input never reaches here, and an unknown word is rejected first.
            Command::Empty | Command::Unknown(_) => {}
            Command::Say(text) => self.send_say(text, cx),
            Command::Action(text) => self.send_action(text, cx),
            Command::Clear => self.clear_view(cx),
            Command::Help => self.reply_lines(&help_text(), cx),
            Command::Pref(request) => self.run_pref(request, cx),
            Command::Version(words) => self.run_version(words, cx)?,
            Command::List(mask) => self.open_channel_list(mask, window, cx),
            // The rest of the catalog runs through the controller's dispatcher, which
            // lands next. Until then a typed one is refused, never sent.
            _ => return Err(CommandOutcome::Unsupported),
        }
        Ok(())
    }

    /// Plain text to the selected conversation. Its send latency is the bench's append timing.
    fn send_say(&mut self, text: &str, cx: &mut Context<Self>) {
        let append_start = self.bench.enabled.then(Instant::now);
        let network_id = self.surface_network();
        let target = self.controller.selected_target().to_owned();
        self.controller.send_to_target(&network_id, &target, text);
        self.drain_notifications(cx);
        if let Some(start) = append_start {
            self.bench.append_latency = Some(start.elapsed());
        }
    }

    /// `/me`: a CTCP ACTION to the selected conversation.
    fn send_action(&mut self, text: &str, cx: &mut Context<Self>) {
        let network_id = self.surface_network();
        let target = self.controller.selected_target().to_owned();
        self.controller.send_action(&network_id, &target, text);
        self.drain_notifications(cx);
    }

    /// `/clear`: empties the open Status log, or the selected conversation's lines.
    fn clear_view(&mut self, cx: &mut Context<Self>) {
        match self.status_network() {
            Some(network_id) => self.controller.clear_status(&network_id),
            None => {
                let network_id = self.surface_network();
                let target = self.controller.selected_target().to_owned();
                self.controller.clear_conversation(&network_id, &target);
            }
        }
        self.drain_notifications(cx);
    }

    /// `/pref`: the model applies it and says what it printed. A change is saved. A new
    /// `joins` setting goes to the controller and refolds the open view at once, and a
    /// new `avatars` setting redraws its photos.
    fn run_pref(&mut self, request: &PrefRequest, cx: &mut Context<Self>) {
        let joins_before = self.preferences.joins;
        let avatars_before = self.preferences.avatars;
        let reply = pref_reply(*request, &mut self.preferences);
        if reply.changed {
            if let Err(error) = self.preferences.save() {
                eprintln!("omairc-poc: could not save preferences: {error}");
            }
        }
        if self.preferences.avatars != avatars_before {
            let avatars = self.preferences.avatars;
            self.transcript.update(cx, |t, cx| {
                t.set_avatars(avatars);
                cx.notify();
            });
        }
        if self.preferences.joins != joins_before {
            self.controller.set_membership_noise(self.preferences.joins);
            self.rebuild_view_keeping_place(cx);
        }
        self.reply_lines(&reply.text, cx);
    }

    /// `/version [nick]`. A conversation gets the reply as a whois row, and the network's
    /// Status gets a CTCP line. On Status only the Status line is written.
    fn run_version(&mut self, words: &Words, cx: &mut Context<Self>) -> Result<(), CommandOutcome> {
        let network_id = self.surface_network();
        let chantypes = self.features_of(&network_id).channel_types().to_owned();
        let peer = self.controller.selected_target().to_owned();
        let selected = if self.controller.is_channel() {
            SelectedView::Channel
        } else {
            SelectedView::Direct(peer.as_str())
        };
        let request = ctcp_target(words, self.surface(), Some(selected), &chantypes)?;
        let reply = version_reply_text(&request.nick);
        if request.in_conversation {
            self.controller.reply_in_selection(&reply);
        }
        self.controller.log_ctcp_reply(&network_id, &reply);
        self.drain_notifications(cx);
        Ok(())
    }

    /// `/list [mask]` (Qt `dispatchList`): opens the channel list for the network the
    /// command was typed on, which is Status's network or the selected conversation's.
    /// It does nothing while Connect is open.
    pub fn open_channel_list(&mut self, mask: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.connect_open {
            return;
        }
        let network_id = self.surface_network();
        if !self.controller.knows_network(&network_id) {
            return;
        }
        if self.controller.list_channels(&network_id, mask) != ListOutcome::Sent {
            return;
        }
        self.present_channel_list(network_id, window, cx);
    }

    /// A command ran: the composer clears, the line joins its history, and the network's
    /// last error clears, as Qt's `report` does on success.
    fn finish_command(&mut self, text: &str, cx: &mut Context<Self>) {
        let network_id = self.surface_network();
        self.controller.set_last_error(&network_id, None);
        self.composer_state.record_sent(text);
        self.composer.update(cx, |c, cx| {
            c.clear();
            cx.notify();
        });
        self.drain_notifications(cx);
    }

    /// A command was refused. On a conversation it becomes the network's last error,
    /// shown in the header. On Status it is a Status line. The text stays.
    fn refuse_command(&mut self, text: &str, cx: &mut Context<Self>) {
        match self.status_network() {
            Some(network_id) => self.controller.log_outcome(&network_id, text),
            None => {
                let network_id = self.surface_network();
                self.controller
                    .set_last_error(&network_id, Some(text.to_string()));
            }
        }
        self.drain_notifications(cx);
    }

    /// Shows a reply one line at a time: a whois row in the conversation, or an outcome
    /// line on Status.
    fn reply_lines(&mut self, text: &str, cx: &mut Context<Self>) {
        let status = self.status_network();
        for line in text.lines() {
            match &status {
                Some(network_id) => self.controller.log_outcome(network_id, line),
                None => {
                    self.controller.reply_in_selection(line);
                }
            }
        }
        self.drain_notifications(cx);
    }

    /// Replaces the composer's text. The caret goes to the end, as after typing.
    fn set_composer_text(&self, text: String, cx: &mut Context<Self>) {
        self.composer.update(cx, |c, cx| {
            c.set_text(text);
            cx.notify();
        });
    }

    fn composer_text(&self, cx: &App) -> String {
        self.composer.read(cx).text().to_string()
    }
}
