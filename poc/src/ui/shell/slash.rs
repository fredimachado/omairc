//! What the composer's line does: Enter and SEND. Plain text goes to the selected
//! conversation. A slash line runs through `model::slash`, and a refused one
//! keeps the typed text. Replies follow Qt's routing: a conversation gets whois
//! rows (`IrcWhoisTranscriptEvent`), and a Status pane gets its own log lines
//! (`IrcStatusEntry::outcome`). The slash-complete popup's state and keys live
//! here too, since they edit the same line.

use std::time::Instant;

use gpui::{App, Context, Window};

use crate::model::chords::Chord;
use crate::model::mock::{MockMessage, StatusLine};
use crate::model::slash::{
    help_text, parse, pref_reply, version_reply_text, version_request, Command, PrefRequest,
    Rejection, Selected, Surface,
};
use crate::model::status::{ctcp_reply_line, outcome_line, row_from_status_line, status_rows};
use crate::model::{
    local_log_stamp, local_send_stamp, outgoing_action, outgoing_message, row_for_message,
    whois_message,
};
use crate::ui::shell::Shell;

impl Shell {
    /// The surface the composer types into: Status while it shows, else the conversation.
    pub fn surface(&self) -> Surface {
        if self.status.is_some() {
            Surface::Status
        } else {
            Surface::Conversation
        }
    }

    /// The network that owns the open view: Status's network, else the selected
    /// conversation's. Qt's `errorNetworkId` picks the same one.
    pub fn surface_network(&self) -> String {
        self.status
            .clone()
            .unwrap_or_else(|| self.selected.network_id.clone())
    }

    /// Whether the slash-complete popup shows: its state is open and no sheet covers the composer.
    pub fn slash_popup_showing(&self) -> bool {
        self.modal.is_none() && self.slash.is_open()
    }

    /// Recomputes the popup after the composer's text, the surface or a history
    /// walk changed. Returns whether the popup changed, so the caller re-renders
    /// only then.
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

    /// A click on a popup row: inserts that row, as Tab does. The shell focuses
    /// the composer afterwards.
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

    /// Up or Down while the popup is open: moves its highlight, with wrap.
    /// Returns whether the key was used.
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

    /// Tab while the popup is open: inserts the highlighted row. Returns false
    /// when Tab should fall through to nick completion.
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

    /// Enter while the popup is open: inserts the highlighted row when the typed
    /// word is partial. Returns false on an exact name, so Enter sends it.
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

    /// Escape while the popup is open: closes it and keeps the text. Returns
    /// false when the popup is shut, so Escape reaches the other handlers.
    pub fn dismiss_slash_popup(&mut self, window: &Window, cx: &mut Context<Self>) -> bool {
        if !self.chord_live(Chord::SlashDismiss, window, cx) || !self.slash.dismiss() {
            return false;
        }
        cx.notify();
        true
    }

    /// Enter or SEND. A line of plain text goes to the selected conversation, and
    /// a slash line runs its command. A refused line keeps the composer text. A
    /// line that runs clears the composer and joins its history. `/list` opens a
    /// sheet, which takes the keyboard from the composer, so callers check for one
    /// before they refocus the composer.
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
            Some(rejection) => Err(rejection),
            None => self.run_command(command, window, cx),
        };
        match outcome {
            Ok(()) => self.finish_command(&text, cx),
            Err(rejection) => self.refuse_command(&rejection, cx),
        }
    }

    /// Runs a command that may run on this surface. An `Err` is a refusal found
    /// while running it, such as `/version` with a channel target.
    fn run_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Result<(), Rejection> {
        match command {
            // Blank input never reaches here, and an unknown word is rejected first.
            Command::Empty | Command::Unknown(_) => {}
            Command::Say(text) => self.send_say(&text, cx),
            Command::Action(text) => {
                let (author, day, time) = self.author_and_stamp();
                self.append_selected_line(outgoing_action(&text, &author, &time, &day), cx);
            }
            Command::Clear => self.clear_view(cx),
            Command::Help => self.reply_lines(help_text(), cx),
            Command::Pref(request) => self.run_pref(&request, cx),
            Command::Version(argument) => self.run_version(&argument, cx)?,
            Command::List(mask) => self.open_channel_list(&mask, window, cx),
        }
        Ok(())
    }

    /// Plain text to the selected conversation. Its send latency is the bench's append timing.
    fn send_say(&mut self, text: &str, cx: &mut Context<Self>) {
        let append_start = self.bench.enabled.then(Instant::now);
        let (author, day, time) = self.author_and_stamp();
        self.append_selected_line(outgoing_message(text, &author, &time, &day), cx);
        if let Some(start) = append_start {
            self.bench.append_latency = Some(start.elapsed());
        }
    }

    /// `/clear`: empties the open Status log, or the selected conversation's lines.
    fn clear_view(&mut self, cx: &mut Context<Self>) {
        match self.status.clone() {
            Some(network_id) => {
                self.world.clear_status(&network_id);
                let rows = status_rows(&self.world, &network_id);
                self.transcript.update(cx, |t, _| t.set_status(rows));
            }
            None => {
                let selected = self.selected.clone();
                self.world.clear_conversation(&selected);
                self.show_selected_conversation(cx);
            }
        }
    }

    /// `/pref`: the model applies it and says what it printed. A change is saved.
    /// A new `joins` setting refolds the open conversation at once, and a new
    /// `avatars` setting redraws its photos.
    fn run_pref(&mut self, request: &PrefRequest, cx: &mut Context<Self>) {
        let joins_before = self.preferences.joins;
        let avatars_before = self.preferences.avatars;
        let reply = pref_reply(request, &mut self.preferences);
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
            self.rebuild_selected_keeping_place(cx);
        }
        self.reply_lines(&reply.text, cx);
    }

    /// `/version [nick]`. A conversation gets the reply as a whois row, and the
    /// network's Status gets a CTCP line. On Status only the Status line is written.
    fn run_version(&mut self, argument: &str, cx: &mut Context<Self>) -> Result<(), Rejection> {
        let network_id = self.surface_network();
        let chantypes = self
            .world
            .network(&network_id)
            .map(|network| network.chantypes.clone())
            .unwrap_or_default();
        let conv = self.current_conversation();
        let peer = conv.name.clone();
        let selected = if conv.is_channel() {
            Selected::Channel
        } else {
            Selected::Direct(peer.as_str())
        };
        let request = version_request(argument, self.surface(), Some(selected), &chantypes)?;
        let reply = version_reply_text(&request.nick);
        if request.in_conversation {
            self.append_selected_line(whois_message(&reply), cx);
        }
        self.log_status(&network_id, ctcp_reply_line(&local_log_stamp(), &reply), cx);
        Ok(())
    }

    /// `/list [mask]` (Qt `dispatchList`): opens the channel list for the network
    /// the command was typed on, which is Status's network or the selected
    /// conversation's. It does nothing while Connect is open.
    pub fn open_channel_list(&mut self, mask: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.connect_open {
            return;
        }
        let network_id = self.surface_network();
        if self.world.network(&network_id).is_none() {
            return;
        }
        self.present_channel_list(network_id, mask, window, cx);
    }

    /// A command ran: the composer clears, the line joins its history, and the
    /// network's last error clears, as Qt's `report` does on success.
    fn finish_command(&mut self, text: &str, cx: &mut Context<Self>) {
        let network_id = self.surface_network();
        self.world.set_last_error(&network_id, None);
        self.composer_state.record_sent(text);
        self.composer.update(cx, |c, cx| {
            c.clear();
            cx.notify();
        });
        cx.notify();
    }

    /// A command was refused. On a conversation it becomes the network's last
    /// error, shown in the header. On Status it is a Status line. The text stays.
    fn refuse_command(&mut self, rejection: &Rejection, cx: &mut Context<Self>) {
        let text = rejection.text();
        match self.status.clone() {
            Some(network_id) => {
                self.log_status(&network_id, outcome_line(&local_log_stamp(), &text), cx);
            }
            None => {
                let network_id = self.surface_network();
                self.world.set_last_error(&network_id, Some(text));
            }
        }
        cx.notify();
    }

    /// Shows a reply one line at a time: a whois row in the conversation, or an
    /// outcome line on Status.
    fn reply_lines(&mut self, text: &str, cx: &mut Context<Self>) {
        let status = self.status.clone();
        for line in text.lines() {
            match &status {
                Some(network_id) => {
                    self.log_status(network_id, outcome_line(&local_log_stamp(), line), cx);
                }
                None => self.append_selected_line(whois_message(line), cx),
            }
        }
    }

    /// Adds a line to a network's Status log, and to the Status pane when it shows that network.
    fn log_status(&mut self, network_id: &str, line: StatusLine, cx: &mut Context<Self>) {
        let row = if self.status.as_deref() == Some(network_id) {
            let id = self.transcript.read(cx).rows().len() as u64 + 1;
            Some(row_from_status_line(id, &line))
        } else {
            None
        };
        self.world.push_status(network_id, line);
        if let Some(row) = row {
            self.transcript.update(cx, |t, _| t.append_line(row));
        }
    }

    /// Stores a line in the selected conversation and appends its row to the transcript.
    fn append_selected_line(&mut self, message: MockMessage, cx: &mut Context<Self>) {
        let row = {
            let rows = self.transcript.read(cx).rows();
            row_for_message(rows.len() as u64 + 1, &message, rows.last())
        };
        let selected = self.selected.clone();
        self.world.record_outgoing(&selected, message);
        self.transcript.update(cx, |t, _| t.append_line(row));
    }

    /// The author nick and the local `(day, HH:MM)` stamp for a line sent now.
    fn author_and_stamp(&self) -> (String, String, String) {
        let author = self.current_network().nick.clone();
        let (day, time) = local_send_stamp();
        (author, day, time)
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
