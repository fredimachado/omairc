//! One harness for GPUI view tests: the seeded `--demo-server` shell in a
//! `TestAppContext`, fixed window size, and helpers for settle, inject, select
//! scroll, and chords. Every view test that needs a live shell uses this.

use gpui::{
    px, size, Entity, Focusable, ListOffset, MouseButton, Modifiers, Point, TestAppContext,
    VisualTestContext,
};

use crate::model::theme::Theme;
use crate::model::transcript::RowKind;
use crate::model::{ConversationRef, Style};
use crate::storage::scrollplace::ScrollPlace;
use crate::ui::input::bind_input_keys;
use crate::ui::shell::{bind_shell_keys, Shell};
use crate::util::OmaircStyleGlobal;

pub const OMARCHY: &str = "omarchy";

/// Fixed window used by every shell view test so list layout is comparable.
pub const WINDOW_WIDTH: f32 = 1024.;
pub const WINDOW_HEIGHT: f32 = 768.;

/// The seeded demo shell in a window, ready for layout and input.
pub struct SeededShell<'a> {
    pub shell: Entity<Shell>,
    pub cx: &'a mut VisualTestContext,
}

/// Builds the demo shell, binds keys, focuses the composer, and settles one frame.
pub fn open(cx: &mut TestAppContext) -> SeededShell<'_> {
    cx.update(|app| {
        app.set_global(OmaircStyleGlobal {
            style: Style::new(Theme::defaults_dark(), 1.0),
        });
        bind_input_keys(app);
        bind_shell_keys(app);
    });
    let (activation_tx, _) = async_channel::unbounded();
    let (shell, vcx) = cx.add_window_view(|_window, cx| {
        Shell::new(cx, false, true, None, activation_tx)
    });
    vcx.simulate_resize(size(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)));
    // Drop seed mention arrivals so settle does not raise desktop notifications.
    shell.update(vcx, |shell, _| {
        shell.controller.take_arrivals();
    });
    vcx.update(|window, cx| {
        shell.update(cx, |shell, cx| shell.focus_on_launch(window, cx));
    });
    let mut seeded = SeededShell { shell, cx: vcx };
    seeded.settle();
    seeded
}

impl SeededShell<'_> {
    /// Paints and drains the executor so list measure and focus settle.
    pub fn settle(&mut self) {
        self.cx.update(|window, _| window.refresh());
        self.cx.run_until_parked();
    }

    pub fn resize(&mut self, width: f32, height: f32) {
        self.cx.simulate_resize(size(px(width), px(height)));
        self.settle();
    }

    /// Selects a conversation on `OMARCHY` and refreshes the transcript.
    pub fn select(&mut self, key: &str) {
        let target = ConversationRef::new(OMARCHY, key);
        self.shell.update(self.cx, |shell, cx| {
            shell.select(target, cx);
        });
        self.settle();
    }

    /// Delivers one server line to `network_id`, ticks the controller, refreshes.
    pub fn inject(&mut self, network_id: &str, line: &str) {
        self.shell.update(self.cx, |shell, cx| {
            let mut bytes = line.as_bytes().to_vec();
            bytes.extend_from_slice(b"\r\n");
            shell
                .controller
                .demo()
                .expect("demo controller")
                .inject(network_id, &bytes);
            shell.controller.tick();
            shell.refresh_shown_conversation(cx);
        });
        self.settle();
    }

    /// Injects a PRIVMSG into the open omarchy conversation.
    pub fn inject_chat(&mut self, nick: &str, target: &str, body: &str) {
        self.inject(
            OMARCHY,
            &format!(":{nick}!u@h PRIVMSG {target} :{body}"),
        );
    }

    /// Appends chat lines until the open transcript overflows the viewport.
    pub fn fill_until_scrollable(&mut self, nick: &str, target: &str, prefix: &str) {
        for index in 0..40 {
            self.inject_chat(nick, target, &format!("{prefix} {index}"));
            if self.overflows() {
                break;
            }
        }
        self.shell.update(self.cx, |shell, cx| {
            shell.transcript.update(cx, |t, cx| t.scroll_to_bottom(cx));
        });
        self.settle();
        assert!(
            self.overflows(),
            "expected the transcript to overflow the viewport"
        );
        assert!(self.at_end(), "fill must leave the reader following the end");
    }

    pub fn at_end(&self) -> bool {
        self.shell
            .read_with(self.cx, |shell, cx| shell.transcript.read(cx).at_end())
    }

    pub fn overflows(&self) -> bool {
        self.shell.read_with(self.cx, |shell, cx| {
            shell.transcript.read(cx).content_overflows_viewport()
        })
    }

    pub fn scroll_top(&self) -> ListOffset {
        self.shell.read_with(self.cx, |shell, cx| {
            shell.transcript.read(cx).logical_scroll_top()
        })
    }

    /// Whether the transcript last reported that it follows the end.
    pub fn reported_end(&self) -> bool {
        self.shell
            .read_with(self.cx, |shell, _| shell.controller.transcript_follow_end())
    }

    /// The place the open transcript's conversation `key` was saved at, as the settings file keeps it.
    pub fn saved_place(&self, key: &str) -> ScrollPlace {
        self.shell
            .read_with(self.cx, |shell, _| shell.controller.place_for(OMARCHY, key))
            .expect("a place is saved")
    }

    /// Jumps to the first row whose text is `body`, as a find or link match does, and renders.
    pub fn jump_to(&mut self, body: &str) {
        self.shell.update(self.cx, |shell, cx| {
            shell.transcript.update(cx, |transcript, cx| {
                let rows = transcript.rows();
                let row = rows.iter().position(|row| row.body.plain == body);
                transcript.scroll_to_row(row.expect("the row is shown"), cx);
            });
        });
        self.settle();
    }

    /// Scrolls the transcript with the wheel over its middle. Positive pixels move up.
    pub fn wheel(&mut self, pixels: f32) {
        let centre = gpui::point(px(WINDOW_WIDTH / 2.), px(WINDOW_HEIGHT / 2.));
        self.cx.simulate_event(gpui::ScrollWheelEvent {
            position: centre,
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(pixels))),
            modifiers: Modifiers::default(),
            touch_phase: gpui::TouchPhase::Moved,
        });
        self.settle();
    }

    /// Saves the open conversation's place, as leaving it does, and returns it as stored.
    pub fn save_place(&mut self, key: &str) -> ScrollPlace {
        self.shell
            .update(self.cx, |shell, cx| shell.remember_open_transcript(cx));
        self.saved_place(key)
    }

    /// The kind of the row at the top of the transcript, or `None` when the list is at its end.
    pub fn top_kind(&self) -> Option<RowKind> {
        self.shell.read_with(self.cx, |shell, cx| {
            let transcript = shell.transcript.read(cx);
            transcript
                .rows()
                .get(transcript.logical_scroll_top().item_ix)
                .map(|row| row.kind)
        })
    }

    pub fn page_down(&mut self) {
        self.shell.update(self.cx, |shell, cx| {
            shell.transcript.update(cx, |t, cx| t.scroll_page(true, false, cx));
        });
        self.settle();
    }

    pub fn page_up(&mut self) {
        self.shell.update(self.cx, |shell, cx| {
            shell.transcript.update(cx, |t, cx| t.scroll_page(false, false, cx));
        });
        self.settle();
    }

    pub fn row_count(&self) -> usize {
        self.shell
            .read_with(self.cx, |shell, cx| shell.transcript.read(cx).rows().len())
    }

    pub fn list_item_count(&self) -> usize {
        self.shell.read_with(self.cx, |shell, cx| {
            shell.transcript.read(cx).list_item_count()
        })
    }

    pub fn typing_peer(&self) -> Option<String> {
        self.shell.read_with(self.cx, |shell, cx| {
            shell
                .transcript
                .read(cx)
                .typing_peer()
                .map(str::to_owned)
        })
    }

    pub fn first_unseen(&self) -> Option<usize> {
        self.shell.read_with(self.cx, |shell, cx| {
            shell.transcript.read(cx).first_unseen_index()
        })
    }

    pub fn selected_key(&self) -> String {
        self.shell
            .read_with(self.cx, |shell, _| shell.selected_ref().key)
    }

    pub fn composer_focused(&mut self) -> bool {
        self.cx.update(|window, cx| {
            self.shell
                .read(cx)
                .composer
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        })
    }

    pub fn connect_open(&self) -> bool {
        self.shell.read_with(self.cx, |shell, _| shell.connect_open)
    }

    pub fn about_open(&self) -> bool {
        self.shell.read_with(self.cx, |shell, _| {
            matches!(
                shell.modal.as_ref().map(|m| m.sheet()),
                Some(crate::model::chords::Sheet::About)
            )
        })
    }

    /// Space-separated keystrokes (gpui parse rules), then settle.
    pub fn keys(&mut self, keystrokes: &str) {
        self.cx.simulate_keystrokes(keystrokes);
        self.settle();
    }

    pub fn click(&mut self, x: f32, y: f32) {
        self.cx
            .simulate_click(Point::new(px(x), px(y)), Modifiers::default());
        self.settle();
    }

    pub fn mouse_down(&mut self, x: f32, y: f32, button: MouseButton) {
        self.cx.simulate_mouse_down(
            Point::new(px(x), px(y)),
            button,
            Modifiers::default(),
        );
        self.settle();
    }

    pub fn mouse_up(&mut self, x: f32, y: f32, button: MouseButton) {
        self.cx
            .simulate_mouse_up(Point::new(px(x), px(y)), button, Modifiers::default());
        self.settle();
    }
}
