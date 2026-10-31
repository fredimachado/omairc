//! The shell's reads of the controller, and the view the transcript shows. Every value
//! here comes from `Controller` or from presentation state the shell owns (the composer
//! the sidebar focus, the Connect model). Nothing here writes controller state.
//! `sync_view` is the one place the transcript and the composer follow the controller's
//! selection. Every intent calls it, and so does every change the controller raises as a
//! surface, so a selection the controller makes on its own (a join, a close, a removal)
//! shows the same way a click does.

use gpui::{App, Context};

use crate::controller::controller::Controller;
use crate::controller::snapshots::ConversationRow;
use crate::model::connect::ConnectState;
use crate::irc::serverfeatures::IrcServerFeatures;
use crate::model::composer::DraftKey;
use crate::model::sidebar::{sidebar_rows, NetworkHeader, SidebarRow};
use crate::model::status::{connection_line, status_rows};
use crate::model::scroll_place::SavedPlace;
use crate::model::title::clear_title_mark_if_opened;
use crate::model::ConversationRef;
use crate::ui::shell::Shell;
use crate::ui::transcript::view::ConversationLines;
use crate::ui::update::launch_on_close;

/// The view the transcript was last built for: a network's Status, or a conversation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShownView {
    /// The network whose Status is showing, or `None` while the conversation shows.
    pub status: Option<String>,
    /// The selected conversation. With nothing selected it names the focused network
    /// with an empty key, which no row matches.
    pub conversation: ConversationRef,
}

impl ShownView {
    /// The composer draft this view owns (Status for its network, else the conversation).
    pub fn draft_key(&self) -> DraftKey {
        DraftKey::for_view(self.status.as_deref(), &self.conversation)
    }
}

/// The view the controller shows right now: Status of the focused network while Status
/// shows, and the selected conversation (or an empty one) either way.
pub fn shown_view(controller: &Controller) -> ShownView {
    let conversation = match controller.selected() {
        Some(key) => ConversationRef::from_key(key),
        None => ConversationRef::new(controller.focused_network_id(), ""),
    };
    ShownView {
        status: controller
            .shows_status()
            .then(|| controller.focused_network_id().to_owned()),
        conversation,
    }
}

/// The sidebar network headers the shell renders (`NetworkSection` inputs).
pub(crate) fn build_network_headers(
    controller: &Controller,
    connect: &ConnectState,
    rows: &[ConversationRow],
) -> Vec<NetworkHeader> {
    controller
        .network_order()
        .into_iter()
        .map(|id| {
            let nick = controller.current_nick_for(&id);
            let account = controller.display_account(&id, &nick);
            let avatar = controller
                .nick_facts(&id)
                .get(&nick)
                .and_then(|facts| facts.avatar.clone());
            let icon_color = connect
                .saved()
                .iter()
                .find(|profile| profile.network_id == id)
                .and_then(|profile| profile.icon_color);
            NetworkHeader {
                name: controller.network_name(&id),
                nick,
                account,
                avatar,
                icon_color,
                icon: controller.features(&id).icon_url().map(str::to_owned),
                unread: controller.unread_count_for(&id) > 0,
                mention: controller.mention_for(&id),
                alerts: controller.alerts_for(&id),
                show_edit: controller.session(&id).is_some(),
                has_channels: rows.iter().any(|row| row.key.network_id == id && !row.direct),
                has_directs: rows.iter().any(|row| row.key.network_id == id && row.direct),
                collapsed: controller.is_network_collapsed(&id),
                status: connection_line(controller, &id),
                id,
            }
        })
        .collect()
}

impl Shell {
    /// The view on screen right now, as the controller's selection and Status say.
    pub fn current_view(&self) -> ShownView {
        shown_view(&self.controller)
    }

    /// The selected conversation as a reference (`ConversationRef::from_key`).
    pub fn selected_ref(&self) -> ConversationRef {
        match self.controller.selected() {
            Some(key) => ConversationRef::from_key(key),
            None => ConversationRef::new(self.controller.focused_network_id(), ""),
        }
    }

    /// The network whose Status pane is showing, or `None` while a conversation shows.
    pub fn status_network(&self) -> Option<String> {
        self.controller
            .shows_status()
            .then(|| self.controller.focused_network_id().to_owned())
    }

    /// Whether the transcript shows a Status pane.
    pub fn status_showing(&self) -> bool {
        self.controller.shows_status()
    }

    /// The network the keyboard and the composer point at (`FocusedNetworkID`).
    pub fn focused_network(&self) -> String {
        self.controller.focused_network_id().to_owned()
    }

    /// The nick the focused network's session holds.
    pub fn current_nick(&self) -> String {
        self.controller
            .current_nick_for(self.controller.focused_network_id())
    }

    /// The features the network's ISUPPORT advertised (CHANTYPES, PREFIX, LINELEN).
    pub fn features_of(&self, network_id: &str) -> IrcServerFeatures {
        self.controller.features(network_id).clone()
    }

    /// The sidebar rows in sidebar order, collapsed networks included. The walks use them.
    pub fn sidebar_walk(&self) -> Vec<SidebarRow> {
        let rows = self.controller.conversations();
        sidebar_rows(&rows, |id| self.controller.is_network_collapsed(id))
    }

    /// The networks in sidebar order, each with its header and badges.
    pub fn network_headers(&self, rows: &[ConversationRow]) -> Vec<NetworkHeader> {
        build_network_headers(&self.controller, &self.connect, rows)
    }

    /// The networks that hold at least one conversation (the Connect sheet's Remove
    /// check needs them).
    pub fn populated_networks(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for row in self.controller.conversations() {
            if !ids.contains(&row.key.network_id) {
                ids.push(row.key.network_id);
            }
        }
        ids
    }

    /// The channel row `name` names on `network_id`, if the network has one. Channel names
    /// match ignoring ASCII case, and a direct message never matches.
    pub fn find_channel(&self, network_id: &str, name: &str) -> Option<ConversationRef> {
        self.controller
            .conversations()
            .into_iter()
            .find(|row| {
                !row.direct
                    && row.key.network_id == network_id
                    && row.target.eq_ignore_ascii_case(name)
            })
            .map(|row| ConversationRef::from_key(&row.key))
    }

    /// Brings the transcript, the composer's draft and the send limit to the view the
    /// controller now shows. A view that is already on screen is left alone; a change
    /// remembers the place and the draft of the view it leaves.
    pub fn sync_view(&mut self, cx: &mut Context<Self>) {
        let view = self.current_view();
        if view == self.shown {
            return;
        }
        let previous = std::mem::replace(&mut self.shown, view.clone());
        clear_title_mark_if_opened(
            &mut self.title_mark,
            view.status.is_some(),
            self.controller.focused_network_id(),
            self.controller.selected_target(),
        );
        if previous.status.is_none() {
            let place = self.transcript.update(cx, |t, _| t.capture_scroll_place());
            self.remember_place(previous.conversation, place);
        }
        let text = self.composer.read(cx).text().to_string();
        self.composer_state.stash(&text);
        self.end_find_mode(cx);
        self.member_cursor = 0;
        let draft = self.composer_state.switch_to(view.draft_key());
        self.composer.update(cx, |c, cx| {
            c.set_text(draft);
            cx.notify();
        });
        self.show_shown_view(None, cx);
        self.sync_send_limit(cx);
        cx.notify();
    }

    /// Stores where a conversation's transcript was left. The controller keeps it in
    /// `scrollplaces.ini`, so it is there after a restart.
    fn remember_place(&mut self, conversation: ConversationRef, place: SavedPlace) {
        self.controller
            .remember_place(&conversation.network_id, &conversation.key, place.to_stored());
    }

    /// Where a conversation's transcript was last left, as `scrollplaces.ini` has it.
    fn saved_place(&self, conversation: &ConversationRef) -> Option<SavedPlace> {
        self.controller
            .place_for(&conversation.network_id, &conversation.key)
            .and_then(|stored| SavedPlace::from_stored(&stored))
    }

    /// Rebuilds the transcript for the view on screen, keeping the reader's place. Used
    /// when the lines or the settings that shape them change.
    pub fn rebuild_view_keeping_place(&mut self, cx: &mut Context<Self>) {
        let place = self.transcript.update(cx, |t, _| t.capture_scroll_place());
        self.show_shown_view(Some(place), cx);
    }

    /// Refreshes the open transcript from the controller without resetting interaction.
    pub fn refresh_shown_conversation(&mut self, cx: &mut Context<Self>) {
        if self.status_showing() {
            let network_id = self.shown.status.clone().expect("status");
            let lines = self.controller.status_lines(&network_id);
            let rows = status_rows(&lines);
            self.transcript.update(cx, |t, _| t.set_status(rows));
        } else {
            let network_id = self.shown.conversation.network_id.clone();
            let features = self.features_of(&network_id);
            let self_nick = self.controller.current_nick_for(&network_id);
            let typing_peer = (!self.controller.is_channel()
                && !self.controller.typing_nicks().is_empty())
            .then(|| self.controller.selected_target().to_owned());
            let lines = ConversationLines {
                messages: self.controller.messages(),
                mark: self.controller.unread_mark(),
                facts: self.controller.nick_facts(&network_id),
                self_nick: &self_nick,
                features: &features,
                typing_peer,
                today: self.transcript_calendar.local_date,
            };
            self.transcript.update(cx, |t, cx| t.refresh_conversation(lines, cx));
        }
        let at_end = self.transcript.read(cx).at_end();
        self.report_transcript_viewport_from_shell(at_end, cx);
    }

    /// Updates only the typing dots on the open direct message.
    pub fn refresh_shown_typing(&mut self, cx: &mut Context<Self>) {
        if self.status_showing() || self.controller.is_channel() {
            return;
        }
        let peer = (!self.controller.typing_nicks().is_empty())
            .then(|| self.controller.selected_target().to_owned());
        self.transcript
            .update(cx, |t, cx| t.refresh_typing_peer(peer, cx));
    }

    /// Fills the transcript with the shown view's rows. A Status view shows its log. A
    /// conversation shows its lines, its "New messages" mark and its people's presence
    /// and the list lands at `place`, or at the mark or the end.
    pub fn show_shown_view(&mut self, place: Option<crate::model::scroll_place::SavedPlace>, cx: &mut Context<Self>) {
        match self.shown.status.clone() {
            Some(network_id) => {
                let lines = self.controller.status_lines(&network_id);
                let rows = status_rows(&lines);
                self.transcript.update(cx, |t, _| t.set_status(rows));
            }
            None => {
                let network_id = self.shown.conversation.network_id.clone();
                let features = self.features_of(&network_id);
                let self_nick = self.controller.current_nick_for(&network_id);
                // A DM whose peer is typing ends the list with the typing dots.
                let typing_peer = (!self.controller.is_channel()
                    && !self.controller.typing_nicks().is_empty())
                .then(|| self.controller.selected_target().to_owned());
                let lines = ConversationLines {
                    messages: self.controller.messages(),
                    mark: self.controller.unread_mark(),
                    facts: self.controller.nick_facts(&network_id),
                    self_nick: &self_nick,
                    features: &features,
                    typing_peer,
                    today: self.transcript_calendar.local_date,
                };
                let open_at_unread = place.is_none() && self.controller.preferences().unread;
                let place = place.or_else(|| self.saved_place(&self.shown.conversation));
                self.transcript.update(cx, |t, _| {
                    t.set_conversation(lines);
                    t.restore_scroll_place(place.as_ref(), open_at_unread);
                });
            }
        }
        let at_end = self.transcript.read(cx).at_end();
        self.report_transcript_viewport_from_shell(at_end, cx);
    }

    /// Qt `rememberOpenTranscript`: stores where the open conversation's transcript
    /// was left. Status saves nothing.
    pub fn remember_open_transcript(&mut self, cx: &mut App) {
        if self.status_showing() {
            return;
        }
        let conversation = self.shown.conversation.clone();
        let place = self.transcript.update(cx, |t, _| t.capture_scroll_place());
        self.remember_place(conversation, place);
    }

    /// Qt `onClosing`: remembers the open transcript, then may start a ready installer.
    pub fn launch_update_on_close(&mut self, cx: &mut App) {
        self.remember_open_transcript(cx);
        let _ = launch_on_close(&self.update, cx);
    }
}

#[cfg(test)]
mod tests {
    use async_channel;
    use gpui::{px, size, TestAppContext};

    use crate::model::{Style, theme::Theme};
    use crate::util::OmaircStyleGlobal;

    use super::Shell;

    const OMARCHY: &str = "omarchy";

    fn inject_line(shell: &mut Shell, line: &str) {
        let mut bytes = line.as_bytes().to_vec();
        bytes.extend_from_slice(b"\r\n");
        shell
            .controller
            .demo()
            .expect("demo controller")
            .inject(OMARCHY, &bytes);
        shell.controller.tick();
    }

    #[gpui::test]
    fn closing_persists_the_open_conversation_scroll_anchor(cx: &mut TestAppContext) {
        let (activation_tx, _) = async_channel::unbounded();
        cx.update(|app| {
            app.set_global(OmaircStyleGlobal {
                style: Style::new(Theme::defaults_dark(), 1.0),
            });
        });
        let (shell, vcx) = cx.add_window_view(|_window, cx| {
            Shell::new(cx, false, true, None, activation_tx)
        });
        shell.update(vcx, |shell, cx| {
            assert!(
                shell.controller.bench_channel(60),
                "the #bench seed must inject on the demo controller"
            );
            // Selection alone leaves `shown` on the demo seed; close remembers
            // `shown.conversation`, so follow the controller onto #bench first.
            shell.sync_view(cx);
            inject_line(
                shell,
                "@msgid=close-anchor :user0!u@h PRIVMSG #bench :anchored",
            );
            for index in 0..25 {
                inject_line(
                    shell,
                    &format!(":user1!u@h PRIVMSG #bench :filler {index}"),
                );
            }
            shell.refresh_shown_conversation(cx);
        });
        vcx.simulate_resize(size(px(1024.), px(768.)));
        vcx.run_until_parked();
        shell.update(vcx, |shell, cx| {
            assert!(
                shell
                    .controller
                    .messages()
                    .iter()
                    .any(|message| message.msgid.value == "close-anchor"),
                "the injected line keeps its msgid"
            );
            shell.transcript.update(cx, |transcript, cx| {
                for _ in 0..8 {
                    if !transcript.at_end() {
                        break;
                    }
                    transcript.scroll_page(false, false, cx);
                }
                assert!(
                    !transcript.at_end(),
                    "the bench transcript must scroll off the tail"
                );
                assert!(
                    transcript.scroll_to_msgid("close-anchor", cx),
                    "the tagged line is on screen"
                );
            });
        });
        vcx.update(|window, _| window.refresh());
        vcx.run_until_parked();
        shell.update(vcx, |shell, cx| {
            shell.transcript.update(cx, |transcript, _| {
                assert!(
                    !transcript.at_end(),
                    "refresh must keep the reader off the tail"
                );
                let saved = transcript.capture_scroll_place();
                assert_eq!(
                    saved.to_stored().msg_id,
                    "close-anchor",
                    "capture must read the msgid row before close"
                );
            });
            shell.launch_update_on_close(cx);
            let place = shell
                .controller
                .place_for(OMARCHY, "#bench")
                .expect("close path must remember the open transcript");
            assert_eq!(place.msg_id, "close-anchor");
            assert!(
                !place.follow_end,
                "close must not save follow_end when scrolled off the tail"
            );
        });
    }

    #[gpui::test]
    fn pref_palette_updates_the_style_global(cx: &mut TestAppContext) {
        use crate::model::color_palette::ColorPaletteId;
        use crate::model::slash::Surface;

        let (activation_tx, _) = async_channel::unbounded();
        cx.update(|app| {
            app.set_global(OmaircStyleGlobal {
                style: Style::new(Theme::defaults_dark(), 1.0),
            });
        });
        let (shell, vcx) = cx.add_window_view(|_window, cx| {
            Shell::new(cx, false, true, None, activation_tx)
        });
        shell.update(vcx, |shell, cx| {
            assert_eq!(
                cx.global::<OmaircStyleGlobal>().style.color_palette(),
                ColorPaletteId::Classic
            );
            let before = *shell.controller.preferences();
            assert!(shell.controller.submit(Surface::Conversation, "/pref palette vivid"));
            shell.apply_preference_changes(before, cx);
            assert_eq!(
                cx.global::<OmaircStyleGlobal>().style.color_palette(),
                ColorPaletteId::Vivid
            );
            assert_eq!(
                shell.controller.preferences().palette,
                ColorPaletteId::Vivid
            );
        });
    }

    #[gpui::test]
    fn connect_preference_writes_go_through_one_path(cx: &mut TestAppContext) {
        use crate::model::color_palette::ColorPaletteId;
        use crate::model::prefs::PrefName;

        let (activation_tx, _) = async_channel::unbounded();
        cx.update(|app| {
            app.set_global(OmaircStyleGlobal {
                style: Style::new(Theme::defaults_dark(), 1.0),
            });
        });
        let (shell, vcx) = cx.add_window_view(|_window, cx| {
            Shell::new(cx, false, true, None, activation_tx)
        });
        shell.update(vcx, |shell, cx| {
            assert_eq!(
                cx.global::<OmaircStyleGlobal>().style.color_palette(),
                ColorPaletteId::Classic
            );
            let palette = shell.controller.preferences().next_value(PrefName::Palette);
            shell.set_connect_preference(PrefName::Palette, palette, cx);
            assert_eq!(
                cx.global::<OmaircStyleGlobal>().style.color_palette(),
                ColorPaletteId::Vivid
            );
            let palette = shell.controller.preferences().next_value(PrefName::Palette);
            shell.set_connect_preference(PrefName::Palette, palette, cx);
            assert_eq!(
                cx.global::<OmaircStyleGlobal>().style.color_palette(),
                ColorPaletteId::Classic
            );

            assert!(shell.controller.preferences().colors);
            let colors = shell.controller.preferences().next_value(PrefName::Colors);
            shell.set_connect_preference(PrefName::Colors, colors, cx);
            assert!(!shell.controller.preferences().colors);
        });
    }

    #[gpui::test]
    fn transcript_calendar_roll_refreshes_open_conversation(cx: &mut TestAppContext) {
        use chrono::Local;

        use crate::irc::reducer::zero_time;
        use crate::model::transcript::{day_caption, rows_from_messages_on};
        use crate::model::transcript_calendar::TranscriptCalendarSnapshot;
        use crate::model::{ConversationRef, RowKind};

        let (activation_tx, _) = async_channel::unbounded();
        cx.update(|app| {
            app.set_global(OmaircStyleGlobal {
                style: Style::new(Theme::defaults_dark(), 1.0),
            });
        });
        let (shell, vcx) = cx.add_window_view(|_window, cx| {
            Shell::new(cx, false, true, None, activation_tx)
        });
        shell.update(vcx, |shell, cx| {
            shell.select(ConversationRef::new(OMARCHY, "#ricing"), cx);
            let messages = shell.controller.messages().to_vec();
            let mark = shell.controller.unread_mark();
            let message_day = messages
                .iter()
                .find_map(|message| {
                    (message.time != zero_time())
                        .then(|| message.time.with_timezone(&Local).date_naive())
                })
                .expect("#ricing seed has a dated line");
            // Sit one day after the seed's local day so the leading rule reads Yesterday,
            // then roll forward so the same rule becomes the short date.
            let today = message_day.succ_opt().expect("seed day has a next day");
            let next_day = today.succ_opt().expect("next day fits in calendar");
            let offset = shell.transcript_calendar.utc_offset_secs;
            shell.transcript_calendar = TranscriptCalendarSnapshot {
                local_date: today,
                utc_offset_secs: offset,
            };
            shell.refresh_shown_conversation(cx);
            let expected_before = rows_from_messages_on(&messages, mark, today)
                .into_iter()
                .find(|row| row.kind == RowKind::DaySeparator)
                .and_then(|row| row.separator_text);
            assert_eq!(
                expected_before.as_deref(),
                Some(day_caption(message_day, today).as_str())
            );
            assert_eq!(
                leading_day_separator(shell, cx),
                expected_before,
                "refresh must build captions from the calendar snapshot, not Local::now()"
            );

            shell.tick_transcript_calendar(
                TranscriptCalendarSnapshot {
                    local_date: next_day,
                    utc_offset_secs: offset,
                },
                cx,
            );
            let expected_after = rows_from_messages_on(&messages, mark, next_day)
                .into_iter()
                .find(|row| row.kind == RowKind::DaySeparator)
                .and_then(|row| row.separator_text);
            assert_ne!(
                expected_after, expected_before,
                "calendar roll must change the leading day caption"
            );
            assert_eq!(
                leading_day_separator(shell, cx),
                expected_after,
                "day roll must rebuild captions from the advanced snapshot local_date"
            );
        });
    }

    #[gpui::test]
    fn transcript_calendar_offset_tick_refreshes_open_conversation(cx: &mut TestAppContext) {
        use chrono::Local;

        use crate::irc::reducer::zero_time;
        use crate::model::transcript::rows_from_messages_on;
        use crate::model::transcript_calendar::TranscriptCalendarSnapshot;
        use crate::model::{ConversationRef, RowKind};

        let (activation_tx, _) = async_channel::unbounded();
        cx.update(|app| {
            app.set_global(OmaircStyleGlobal {
                style: Style::new(Theme::defaults_dark(), 1.0),
            });
        });
        let (shell, vcx) = cx.add_window_view(|_window, cx| {
            Shell::new(cx, false, true, None, activation_tx)
        });
        shell.update(vcx, |shell, cx| {
            shell.select(ConversationRef::new(OMARCHY, "#ricing"), cx);
            let messages = shell.controller.messages().to_vec();
            let mark = shell.controller.unread_mark();
            let message_day = messages
                .iter()
                .find_map(|message| {
                    (message.time != zero_time())
                        .then(|| message.time.with_timezone(&Local).date_naive())
                })
                .expect("#ricing seed has a dated line");
            let today = message_day.succ_opt().expect("seed day has a next day");
            let offset = shell.transcript_calendar.utc_offset_secs;
            shell.transcript_calendar = TranscriptCalendarSnapshot {
                local_date: today,
                utc_offset_secs: offset,
            };
            shell.refresh_shown_conversation(cx);
            let expected = rows_from_messages_on(&messages, mark, today)
                .into_iter()
                .find(|row| row.kind == RowKind::DaySeparator)
                .and_then(|row| row.separator_text);
            assert_eq!(leading_day_separator(shell, cx), expected);

            shell.tick_transcript_calendar(
                TranscriptCalendarSnapshot {
                    local_date: today,
                    utc_offset_secs: offset + 3_600,
                },
                cx,
            );
            assert_eq!(
                leading_day_separator(shell, cx),
                expected,
                "offset-only tick must still rebuild through the snapshot local_date"
            );
        });
    }

    fn leading_day_separator(shell: &Shell, cx: &gpui::App) -> Option<String> {
        use crate::model::RowKind;

        shell
            .transcript
            .read(cx)
            .rows()
            .iter()
            .find(|row| row.kind == RowKind::DaySeparator)
            .and_then(|row| row.separator_text.clone())
    }
}
