//! What the composer's line does: Enter and SEND. Plain text goes to the selected
//! conversation through the controller. A slash line runs through `model::slash`, and a
//! refused one keeps the typed text. Replies follow Qt's routing: a conversation gets
//! whois rows (`IrcWhoisTranscriptEvent`), and a Status pane gets its own log lines
//! (`IrcStatusEntry::outcome`). Every line lands through a controller intent, and the
//! shell repaints from the surfaces the controller raises. The slash-complete popup's
//! state and keys live here too, since they edit the same line.

use gpui::{App, BorrowAppContext, Context, Window};

use crate::model::chords::Chord;
use crate::model::prefs::Preferences;
use crate::model::slash::{parse, Command, Surface};
use crate::model::Style;
use crate::ui::shell::Shell;
use crate::util::OmaircStyleGlobal;

impl Shell {
    /// The surface the composer types into: Status while it shows, else the conversation.
    pub fn surface(&self) -> Surface {
        if self.status_showing() {
            Surface::Status
        } else {
            Surface::Conversation
        }
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

    /// Enter or SEND. The line goes to the controller, which parses it, runs the verb on the
    /// surface, and says whether the composer clears: a refused conversation line keeps its
    /// text, and Status clears. The shell then follows what the controller raised: a `/list`
    /// opens its overlay, a changed preference redraws the view and is saved, and an accepted
    /// line joins the history. Find mode holds a query, not a line.
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
        // `/list` opens a sheet, and Connect covers the window, so it does not run then.
        if self.connect_open && matches!(command, Command::List(_)) {
            return;
        }

        let before = *self.controller.preferences();
        let surface = self.surface();
        let accepted = self.controller.submit(surface, &text);
        self.apply_preference_changes(before, cx);
        if accepted {
            self.finish_command(&text, cx);
        }
        self.drain_notifications(cx);
        if matches!(command, Command::List(_)) && self.controller.channel_list().open {
            let network_id = self.controller.channel_list().network_id.clone();
            self.present_channel_list(network_id, window, cx);
        }
    }

    /// A composer edit or caret move reached the shell. While a conversation is selected the
    /// controller publishes the typing hint for the line (`NotifyComposerText`). Find mode types
    /// a query, so it publishes nothing.
    pub fn report_composer_text(&mut self, cx: &mut Context<Self>) {
        if self.find.is_some() {
            return;
        }
        let text = self.composer_text(cx);
        self.controller.composer_text_changed(&text);
        self.drain_notifications(cx);
    }

    /// Redraws what a changed preference touches: the avatars and mIRC colors in the
    /// transcript, the nick/network palette, and the view when the membership noise
    /// changed, since its lines fold at read time. The header re-reads prefs each frame.
    pub fn apply_preference_changes(&mut self, before: Preferences, cx: &mut Context<Self>) {
        let after = *self.controller.preferences();
        if after.avatars != before.avatars {
            let avatars = after.avatars;
            self.transcript.update(cx, |t, cx| {
                t.set_avatars(avatars);
                cx.notify();
            });
        }
        if after.colors != before.colors {
            let colors = after.colors;
            self.transcript.update(cx, |t, cx| {
                t.set_colors(colors);
                cx.notify();
            });
        }
        if after.palette != before.palette {
            let palette = after.palette;
            cx.update_global::<OmaircStyleGlobal, _>(|global, _| {
                let theme = global.style.theme.clone();
                let scale = global.style.text_scale;
                global.style = Style::with_color_palette(theme, scale, palette);
            });
            self.transcript.update(cx, |t, cx| {
                t.invalidate_all();
                cx.notify();
            });
            cx.notify();
        }
        if after.joins != before.joins {
            self.rebuild_view_keeping_place(cx);
        }
    }

    /// A line the controller accepted: the composer clears and the line joins its history.
    fn finish_command(&mut self, text: &str, cx: &mut Context<Self>) {
        self.composer_state.record_sent(text);
        self.composer.update(cx, |c, cx| {
            c.clear();
            cx.notify();
        });
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
