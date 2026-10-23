//! The in-scope chords and the conditions under which each one is live. The
//! rules mirror each Qt `Shortcut.enabled` in `src/OmaircWindow.qml`
//! (around lines 2650-3186), and the Gating paragraph of the GPUI plan.
//! The UI asks `chord_enabled` before acting, so a chord can never fire
//! behind a sheet that should own the keyboard.

/// A modal that owns the keyboard while it is open. Qt can hold several
/// `opened` flags at once; the GPUI shell shows one at a time, so the model
/// stores a single `Option<Sheet>`. The Connect sheet is not one of these: it is
/// a window-level layer (`ChordContext::connect_open`), and only Shortcuts and
/// About open above it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sheet {
    Shortcuts,
    About,
    Jump,
    Link,
    Inbox,
    Nick,
    /// Asks before a click on a channel name joins a channel the network lacks.
    ChannelOpen,
    /// The `/list` channel directory. Enter or a click joins a channel, and Escape closes it.
    ChannelList,
}

/// What the chord decision can see. Built by the shell from its state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChordContext {
    pub sheet: Option<Sheet>,
    /// The Status pane is showing in place of a conversation. Status is a view,
    /// not an overlay: no-overlay chords stay live. Like Qt's `consoleVisible`,
    /// it hides the member panel and the conversation, so the channel and DM
    /// chords are off.
    pub status_open: bool,
    /// The selected conversation is a channel.
    pub channel: bool,
    /// The selected channel is one the user is in (Qt `currentChannelJoined`). Ctrl+Shift+K
    /// (the nick sheet), Ctrl+Shift+M (the member panel) and Ctrl+Shift+P (member focus) need
    /// one, so on a channel the user has left they do nothing.
    pub joined: bool,
    /// The selection can close: a direct message, or a channel the user has left (Qt
    /// `canCloseSelection`). Ctrl+W closes it. A joined channel is refused.
    pub can_close: bool,
    /// A network header in the server list has keyboard focus.
    pub header_focused: bool,
    /// The member list has keyboard focus.
    pub members_focused: bool,
    /// The transcript has a non-empty text selection.
    pub transcript_selection: bool,
    /// The slash-complete popup is showing above the composer: the typed line
    /// starts a verb or `/pref` name, and no sheet covers it.
    pub slash_open: bool,
    /// Find mode is on: the composer holds a query, and Enter moves to the next match.
    pub find_active: bool,
    /// The Connect sheet is showing. It is a window-level layer, not a `Sheet`: it
    /// owns the keyboard, and only Shortcuts and About open above it. Every other
    /// no-overlay chord waits for it to close.
    pub connect_open: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chord {
    Quit,
    Copy,
    ComposerFocus,
    Jump,
    Link,
    NickSheet,
    Inbox,
    MembersToggle,
    MembersFocus,
    ServerListToggle,
    CloseDirect,
    Shortcuts,
    About,
    /// Ctrl+` (`secondary-\``): opens Status for the selected network, or closes it.
    StatusToggle,
    /// Ctrl+F (`secondary-f`): starts find mode, or moves to the next match while it is on.
    Find,
    /// Up and Down in the composer: walk the sent lines. With no history they fall through.
    ComposerHistory,
    /// Tab and Shift+Tab in the composer: complete a nick. Status has no nicks.
    NickComplete,
    /// Up, Down, Tab, Enter and a click while the slash popup is open: they move
    /// its highlight, insert the highlighted row, or send on an exact name.
    SlashPopup,
    /// Escape while the slash popup is open: it closes and the typed text stays.
    SlashDismiss,
    WalkConversationDown,
    WalkConversationUp,
    WalkNetworkRight,
    WalkNetworkLeft,
    CollapseHeader,
    ExpandHeader,
    CollapseAll,
    ExpandAll,
    MoveNetworkUp,
    MoveNetworkDown,
    NextUnread,
    MarkAllRead,
    /// Alt+U: the unseen jump, to the first line that arrived while the reader was
    /// scrolled up, or to the end while a "New messages" mark is open. Status has
    /// no unseen lines, so it is off there. Qt has no Alt+U: its jump is the `↓`
    /// chip, and the shortcuts sheet does not list this chord.
    Unseen,
    HeaderActivate,
    PageUp,
    PageDown,
    HalfPageUp,
    HalfPageDown,
    TranscriptTop,
    TranscriptBottom,
    MembersTop,
    MembersBottom,
    MembersStep,
    MembersPage,
    MembersHalfPage,
    MembersActivate,
    Escape,
    /// Ctrl+, (and Ctrl+] on Windows): opens Connect, or re-selects the focused
    /// network while it is open. Live with no other sheet, so Shortcuts and About
    /// are the only sheets that can sit above Connect.
    Connect,
    /// Ctrl+Enter and Ctrl+Return while Connect is open: applies the profile. It is
    /// one window-level chord, not one per field, so it works from any focus.
    ConnectApply,
    /// Ctrl+Tab (and Ctrl+PgDn on Windows) while Connect is open: the next tab.
    ConnectTabCycle,
    /// Alt+Right while Connect is open: the next network in the rail. The header
    /// walk's Alt+Right is off then.
    ConnectWalkNetworkRight,
    /// Alt+Left while Connect is open: the previous network in the rail.
    ConnectWalkNetworkLeft,
    /// Ctrl+N while Connect is open: adds a network.
    ConnectAdd,
    /// Ctrl+Shift+Delete while Connect is open: arms the selected network's removal,
    /// and removes it on the next press.
    ConnectRemove,
    /// Ctrl+Shift+U: opens the file picker when the server offers FILEHOST.
    InsertFile,
}

/// Whether the chord is live in `ctx`. "No overlay" chords need no sheet and no
/// Connect sheet to be open, and Status does not count as one. The toggles
/// (Ctrl+K, Ctrl+Shift+O, Ctrl+Shift+A, Ctrl+`) stay live while their own sheet or
/// Status is open, so the same chord closes it.
pub fn chord_enabled(chord: Chord, ctx: &ChordContext) -> bool {
    // No overlay of any kind: no sheet, and not the Connect layer.
    let no_sheet = ctx.sheet.is_none() && !ctx.connect_open;
    // No sheet at all, Connect included. Shortcuts and About open above Connect.
    let sheet_free = ctx.sheet.is_none();
    let sheet_is = |sheet: Sheet| ctx.sheet == Some(sheet);
    // The Connect keys work while Connect shows and no sheet sits above it.
    let connect_keys = ctx.connect_open && sheet_free;
    // Status hides the member panel and the conversation, so these need a
    // conversation view as well as no sheet (Qt `!consoleVisible`).
    let on_conversation = no_sheet && !ctx.status_open;
    match chord {
        // Qt's Ctrl+Q is disabled while any sheet is open.
        Chord::Quit => no_sheet,
        // Copies the transcript selection, or the member list's selection.
        // The composer's own selection is copied by the composer first, so this
        // only sees Ctrl+C when the draft has none. Focus does not matter here:
        // a click in the transcript moves focus to the composer.
        Chord::Copy => no_sheet && (ctx.transcript_selection || ctx.members_focused),
        Chord::ComposerFocus => no_sheet,
        Chord::InsertFile => no_sheet && !ctx.find_active,
        // Toggles Jump, and is blocked by every other sheet.
        Chord::Jump => no_sheet || sheet_is(Sheet::Jump),
        // Toggles Link, and is blocked by every other sheet.
        Chord::Link => no_sheet || sheet_is(Sheet::Link),
        // Toggles Inbox, and is blocked by every other sheet.
        Chord::Inbox => no_sheet || sheet_is(Sheet::Inbox),
        Chord::NickSheet => ctx.channel && ctx.joined && on_conversation,
        Chord::MembersToggle | Chord::MembersFocus => ctx.channel && ctx.joined && on_conversation,
        Chord::ServerListToggle => no_sheet,
        // Qt's closeDirectMessage, Ctrl+W. It closes a direct message or a channel the user
        // has left, and a joined channel is refused.
        Chord::CloseDirect => ctx.can_close && on_conversation,
        // Qt binds Ctrl+/ unconditionally and its handler returns early while
        // another sheet is open, so the result is the same as this gate. It opens
        // above the Connect sheet, as Qt's shortcuts overlay does.
        Chord::Shortcuts => sheet_free || sheet_is(Sheet::Shortcuts),
        // Opens above Connect too.
        Chord::About => sheet_free,
        // Qt binds Ctrl+` with no sheet gate (only the connect and shortcut
        // overlays), so it also closes Status while Status is the view.
        Chord::StatusToggle => no_sheet,
        // Qt's Ctrl+, and Ctrl+] open Connect, and its handler re-selects the
        // focused network when it is already open. A sheet above Connect blocks it.
        Chord::Connect => sheet_free,
        Chord::ConnectApply
        | Chord::ConnectTabCycle
        | Chord::ConnectWalkNetworkRight
        | Chord::ConnectWalkNetworkLeft
        | Chord::ConnectAdd
        | Chord::ConnectRemove => connect_keys,
        // Qt's Ctrl+F is off only under its two overlays. Here any open sheet blocks
        // it, so a search never runs behind a sheet. A press while finding moves to
        // the next match. Status has a transcript too, so it is live there.
        Chord::Find => no_sheet,
        // Qt's composer walks history on Up and Down whichever view is open, so
        // Status keeps its own history. A sheet owns those keys, and so does find,
        // which ignores them.
        Chord::ComposerHistory => no_sheet && !ctx.find_active,
        // Tab completes on a conversation only. Status has no nicks. The key is
        // still consumed there, so focus never leaves the composer. Find mode
        // consumes it too, and completes nothing.
        Chord::NickComplete => on_conversation && !ctx.find_active,
        // The popup owns these keys while it shows and no sheet is open. Status
        // does not hide it: `/` on Status completes Status's verbs.
        Chord::SlashPopup | Chord::SlashDismiss => ctx.slash_open && no_sheet,
        // Qt binds Alt+Shift+A unconditionally and returns early while an
        // overlay is open, so the net gate is "no overlay".
        Chord::MarkAllRead => no_sheet,
        // The unseen jump moves a transcript, so it needs a conversation on screen.
        Chord::Unseen => on_conversation,
        Chord::WalkConversationDown
        | Chord::WalkConversationUp
        | Chord::WalkNetworkRight
        | Chord::WalkNetworkLeft
        | Chord::CollapseHeader
        | Chord::ExpandHeader
        | Chord::CollapseAll
        | Chord::ExpandAll
        | Chord::MoveNetworkUp
        | Chord::MoveNetworkDown
        | Chord::NextUnread
        | Chord::PageUp
        | Chord::PageDown
        | Chord::HalfPageUp
        | Chord::HalfPageDown
        | Chord::TranscriptTop
        | Chord::TranscriptBottom => no_sheet,
        // Return and Enter activate a focused header, which opens that network's
        // Status. The composer must not send while a header is focused. Find mode
        // takes Enter for the next match instead (Qt's Return shortcut is off then).
        Chord::HeaderActivate => ctx.header_focused && no_sheet && !ctx.find_active,
        Chord::MembersTop
        | Chord::MembersBottom
        | Chord::MembersStep
        | Chord::MembersPage
        | Chord::MembersHalfPage
        | Chord::MembersActivate => ctx.members_focused && on_conversation,
        // Escape closes the top layer: a sheet, then Connect, then find mode, then
        // Status for the last conversation. `!no_sheet` covers a sheet and Connect.
        Chord::Escape => !no_sheet || ctx.status_open || ctx.find_active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Column order for the tables below: no sheet, then each sheet kind, then
    /// Status open with no sheet, then Connect open with no sheet.
    const COLUMNS: [(Option<Sheet>, bool, bool); 11] = [
        (None, false, false),
        (Some(Sheet::Shortcuts), false, false),
        (Some(Sheet::About), false, false),
        (Some(Sheet::Jump), false, false),
        (Some(Sheet::Link), false, false),
        (Some(Sheet::Inbox), false, false),
        (Some(Sheet::Nick), false, false),
        (Some(Sheet::ChannelOpen), false, false),
        (Some(Sheet::ChannelList), false, false),
        (None, true, false),
        (None, false, true),
    ];

    /// Live with no overlay, on a conversation or on Status. Not under Connect.
    const NO_SHEET: [bool; 11] = [
        true, false, false, false, false, false, false, false, false, true, false,
    ];
    /// Live with no overlay, but only on a conversation (Status hides it).
    const CONVERSATION_ONLY: [bool; 11] = [
        true, false, false, false, false, false, false, false, false, false, false,
    ];
    /// Live only while Connect shows and no sheet sits above it.
    const CONNECT_ONLY: [bool; 11] = [
        false, false, false, false, false, false, false, false, false, false, true,
    ];
    /// The Connect entry: live with no sheet, on a conversation, on Status, or
    /// under nothing but Connect itself. A sheet above it blocks it.
    const CONNECT_ENTRY: [bool; 11] = [
        true, false, false, false, false, false, false, false, false, true, true,
    ];
    /// Shortcuts opens above Connect, as Qt's overlay does, and toggles itself.
    const SHORTCUTS: [bool; 11] = [
        true, true, false, false, false, false, false, false, false, true, true,
    ];
    /// About opens above Connect too.
    const ABOUT: [bool; 11] = [
        true, false, false, false, false, false, false, false, false, true, true,
    ];
    /// Never live in these contexts.
    const NOWHERE: [bool; 11] = [false; 11];

    /// Checks one chord against every column, with `setup` applied to the base
    /// context first.
    fn check(chord: Chord, setup: fn(&mut ChordContext), expected: [bool; 11]) {
        for (column, (sheet, status_open, connect_open)) in COLUMNS.iter().enumerate() {
            let mut ctx = ChordContext {
                sheet: *sheet,
                status_open: *status_open,
                connect_open: *connect_open,
                ..ChordContext::default()
            };
            setup(&mut ctx);
            assert_eq!(
                chord_enabled(chord, &ctx),
                expected[column],
                "{chord:?} with sheet {sheet:?}, status {status_open}, connect {connect_open} and context {ctx:?}"
            );
        }
    }

    fn nothing(_: &mut ChordContext) {}
    /// A channel the user is in.
    fn joined_channel(ctx: &mut ChordContext) {
        ctx.channel = true;
        ctx.joined = true;
    }
    /// A channel the user has left: it is not joined, and it can close.
    fn left_channel(ctx: &mut ChordContext) {
        ctx.channel = true;
        ctx.can_close = true;
    }
    /// A direct message: it closes, and it has no members.
    fn direct(ctx: &mut ChordContext) {
        ctx.can_close = true;
    }
    fn header(ctx: &mut ChordContext) {
        ctx.header_focused = true;
    }
    fn members(ctx: &mut ChordContext) {
        ctx.channel = true;
        ctx.joined = true;
        ctx.members_focused = true;
    }
    fn selection(ctx: &mut ChordContext) {
        ctx.transcript_selection = true;
    }
    fn slash_popup(ctx: &mut ChordContext) {
        ctx.slash_open = true;
    }
    fn finding(ctx: &mut ChordContext) {
        ctx.find_active = true;
    }

    // Columns: none, Shortcuts, About, Jump, Link, Inbox, Nick, ChannelOpen, ChannelList,
    // Status open, Connect open.

    #[test]
    fn no_overlay_chords_are_live_without_a_sheet_and_on_status() {
        check(Chord::Quit, nothing, NO_SHEET);
        check(Chord::ComposerFocus, nothing, NO_SHEET);
        check(Chord::ServerListToggle, nothing, NO_SHEET);
        check(Chord::MarkAllRead, nothing, NO_SHEET);
        check(Chord::WalkConversationDown, nothing, NO_SHEET);
        check(Chord::WalkConversationUp, nothing, NO_SHEET);
        check(Chord::WalkNetworkRight, nothing, NO_SHEET);
        check(Chord::WalkNetworkLeft, nothing, NO_SHEET);
        check(Chord::CollapseHeader, nothing, NO_SHEET);
        check(Chord::ExpandHeader, nothing, NO_SHEET);
        check(Chord::CollapseAll, nothing, NO_SHEET);
        check(Chord::ExpandAll, nothing, NO_SHEET);
        check(Chord::MoveNetworkUp, nothing, NO_SHEET);
        check(Chord::MoveNetworkDown, nothing, NO_SHEET);
        check(Chord::NextUnread, nothing, NO_SHEET);
        check(Chord::PageUp, nothing, NO_SHEET);
        check(Chord::PageDown, nothing, NO_SHEET);
        check(Chord::HalfPageUp, nothing, NO_SHEET);
        check(Chord::HalfPageDown, nothing, NO_SHEET);
        check(Chord::TranscriptTop, nothing, NO_SHEET);
        check(Chord::TranscriptBottom, nothing, NO_SHEET);
    }

    #[test]
    fn toggles_stay_live_for_their_own_sheet_and_block_the_others() {
        check(
            Chord::Jump,
            nothing,
            [true, false, false, true, false, false, false, false, false, true, false],
        );
        check(
            Chord::Link,
            nothing,
            [true, false, false, false, true, false, false, false, false, true, false],
        );
        check(
            Chord::Inbox,
            nothing,
            [true, false, false, false, false, true, false, false, false, true, false],
        );
    }

    #[test]
    fn shortcuts_sheet_toggles_from_none_or_itself_and_opens_above_connect() {
        check(Chord::Shortcuts, nothing, SHORTCUTS);
    }

    #[test]
    fn about_opens_from_none_or_above_connect_but_not_over_another_sheet() {
        check(Chord::About, nothing, ABOUT);
    }

    #[test]
    fn the_unseen_jump_needs_a_conversation_and_no_sheet() {
        check(Chord::Unseen, nothing, CONVERSATION_ONLY);
        check(Chord::Unseen, joined_channel, CONVERSATION_ONLY);
        check(Chord::Unseen, direct, CONVERSATION_ONLY);
    }

    #[test]
    fn status_toggle_is_live_without_a_sheet_so_it_can_close_status() {
        check(Chord::StatusToggle, nothing, NO_SHEET);
    }

    #[test]
    fn composer_history_is_live_without_a_sheet_and_on_status() {
        check(Chord::ComposerHistory, nothing, NO_SHEET);
    }

    #[test]
    fn find_is_live_without_a_sheet_on_a_conversation_or_status_and_while_finding() {
        check(Chord::Find, nothing, NO_SHEET);
        check(Chord::Find, finding, NO_SHEET);
        check(Chord::Find, joined_channel, NO_SHEET);
    }

    #[test]
    fn find_mode_takes_history_completion_and_header_enter_from_the_composer() {
        check(Chord::ComposerHistory, finding, NOWHERE);
        check(Chord::NickComplete, finding, NOWHERE);
        check(
            Chord::HeaderActivate,
            |ctx| {
                ctx.header_focused = true;
                ctx.find_active = true;
            },
            NOWHERE,
        );
    }

    #[test]
    fn escape_leaves_find_mode_even_with_no_sheet_and_no_status() {
        check(Chord::Escape, finding, [true; 11]);
    }

    #[test]
    fn escape_stays_a_sheet_key_while_finding() {
        // A sheet opened during find (Ctrl+K is not gated by find) closes first.
        let ctx = ChordContext {
            sheet: Some(Sheet::Jump),
            find_active: true,
            ..ChordContext::default()
        };
        assert!(chord_enabled(Chord::Escape, &ctx));
        assert!(!chord_enabled(Chord::Find, &ctx));
    }

    #[test]
    fn the_slash_popup_keys_need_an_open_popup_and_no_sheet() {
        check(Chord::SlashPopup, slash_popup, NO_SHEET);
        check(Chord::SlashDismiss, slash_popup, NO_SHEET);
        check(Chord::SlashPopup, nothing, NOWHERE);
        check(Chord::SlashDismiss, nothing, NOWHERE);
    }

    #[test]
    fn an_open_sheet_keeps_the_slash_popup_out_of_the_keys() {
        // Ctrl+K while "/v" is typed opens Jump: Escape must close Jump, not the popup.
        let ctx = ChordContext {
            sheet: Some(Sheet::Jump),
            slash_open: true,
            ..ChordContext::default()
        };
        assert!(!chord_enabled(Chord::SlashDismiss, &ctx));
        assert!(chord_enabled(Chord::Escape, &ctx));
    }

    #[test]
    fn nick_completion_needs_a_conversation_not_status() {
        check(Chord::NickComplete, nothing, CONVERSATION_ONLY);
        check(Chord::NickComplete, joined_channel, CONVERSATION_ONLY);
        check(Chord::NickComplete, direct, CONVERSATION_ONLY);
    }

    #[test]
    fn escape_closes_any_open_sheet_connect_or_leaves_status() {
        check(
            Chord::Escape,
            nothing,
            [false, true, true, true, true, true, true, true, true, true, true],
        );
    }

    #[test]
    fn member_chords_need_a_joined_channel_and_a_conversation() {
        check(Chord::NickSheet, joined_channel, CONVERSATION_ONLY);
        check(Chord::MembersToggle, joined_channel, CONVERSATION_ONLY);
        check(Chord::MembersFocus, joined_channel, CONVERSATION_ONLY);
        check(Chord::NickSheet, nothing, NOWHERE);
        check(Chord::MembersToggle, nothing, NOWHERE);
        check(Chord::MembersFocus, nothing, NOWHERE);
    }

    #[test]
    fn ctrl_shift_k_m_and_p_do_nothing_on_a_channel_you_have_left() {
        // A left channel is not joined, so the nick sheet and the member chords stay off, and
        // Ctrl+Shift+K opens nothing there.
        check(Chord::NickSheet, left_channel, NOWHERE);
        check(Chord::MembersToggle, left_channel, NOWHERE);
        check(Chord::MembersFocus, left_channel, NOWHERE);
        check(Chord::NickSheet, direct, NOWHERE);
    }

    #[test]
    fn close_needs_a_selection_that_can_close_and_a_conversation() {
        // A direct message and a left channel close. A joined channel is refused.
        check(Chord::CloseDirect, direct, CONVERSATION_ONLY);
        check(Chord::CloseDirect, left_channel, CONVERSATION_ONLY);
        check(Chord::CloseDirect, joined_channel, NOWHERE);
        check(Chord::CloseDirect, nothing, NOWHERE);
    }

    #[test]
    fn header_activation_needs_header_focus() {
        check(Chord::HeaderActivate, header, NO_SHEET);
        check(Chord::HeaderActivate, nothing, NOWHERE);
    }

    #[test]
    fn member_list_navigation_needs_member_focus_and_a_conversation() {
        check(Chord::MembersTop, members, CONVERSATION_ONLY);
        check(Chord::MembersBottom, members, CONVERSATION_ONLY);
        check(Chord::MembersTop, joined_channel, NOWHERE);
        check(Chord::MembersBottom, nothing, NOWHERE);
        for chord in [
            Chord::MembersStep,
            Chord::MembersPage,
            Chord::MembersHalfPage,
            Chord::MembersActivate,
        ] {
            check(chord, members, CONVERSATION_ONLY);
            check(chord, joined_channel, NOWHERE);
        }
    }

    #[test]
    fn copy_needs_a_transcript_selection_or_a_focused_member_list() {
        check(Chord::Copy, selection, NO_SHEET);
        check(Chord::Copy, nothing, NOWHERE);
        check(Chord::Copy, members, NO_SHEET);
    }

    #[test]
    fn copy_works_with_the_composer_focused_after_a_transcript_drag() {
        // A transcript click focuses the composer, so the selection must
        // still copy with the composer focused and its draft unselected.
        let ctx = ChordContext {
            transcript_selection: true,
            ..ChordContext::default()
        };
        assert!(chord_enabled(Chord::Copy, &ctx));
    }

    #[test]
    fn a_sheet_blocks_copy_even_with_a_selection() {
        check(
            Chord::Copy,
            |ctx| {
                ctx.transcript_selection = true;
            },
            NO_SHEET,
        );
    }

    #[test]
    fn status_hides_the_member_panel_and_the_close() {
        // A joined channel with the member list focused, viewed on Status: none of the
        // member chords or Ctrl+W are live.
        let ctx = ChordContext {
            status_open: true,
            channel: true,
            joined: true,
            can_close: true,
            members_focused: true,
            ..ChordContext::default()
        };
        for chord in [
            Chord::NickSheet,
            Chord::MembersToggle,
            Chord::MembersFocus,
            Chord::CloseDirect,
            Chord::MembersTop,
            Chord::MembersActivate,
        ] {
            assert!(
                !chord_enabled(chord, &ctx),
                "{chord:?} must be off on Status"
            );
        }
    }

    /// Asserts that `sheet` holds the keyboard: every chord in the list waits for
    /// it, and only Escape gets through.
    fn blocks_every_chord_but_escape(sheet: Sheet) {
        let ctx = ChordContext {
            sheet: Some(sheet),
            channel: true,
            joined: true,
            can_close: true,
            members_focused: true,
            transcript_selection: true,
            ..ChordContext::default()
        };
        for chord in [
            Chord::Quit,
            Chord::Copy,
            Chord::ComposerFocus,
            Chord::Jump,
            Chord::Link,
            Chord::NickSheet,
            Chord::Inbox,
            Chord::MembersToggle,
            Chord::MembersFocus,
            Chord::ServerListToggle,
            Chord::CloseDirect,
            Chord::Shortcuts,
            Chord::About,
            Chord::StatusToggle,
            Chord::Find,
            Chord::ComposerHistory,
            Chord::NickComplete,
            Chord::NextUnread,
            Chord::MarkAllRead,
            Chord::Unseen,
            Chord::MembersStep,
            Chord::Connect,
            Chord::ConnectApply,
            Chord::ConnectTabCycle,
            Chord::ConnectWalkNetworkRight,
            Chord::ConnectWalkNetworkLeft,
            Chord::ConnectAdd,
            Chord::ConnectRemove,
        ] {
            assert!(
                !chord_enabled(chord, &ctx),
                "{chord:?} must wait for {sheet:?}"
            );
        }
        assert!(chord_enabled(Chord::Escape, &ctx));
    }

    #[test]
    fn the_channel_open_sheet_blocks_every_chord_but_escape() {
        // Enter confirms and Escape cancels; the other keys wait for the sheet.
        blocks_every_chord_but_escape(Sheet::ChannelOpen);
    }

    #[test]
    fn the_channel_list_sheet_blocks_every_chord_but_escape() {
        // Enter joins and Escape closes; the other keys wait for the sheet.
        blocks_every_chord_but_escape(Sheet::ChannelList);
    }

    #[test]
    fn connect_blocks_every_no_overlay_chord_but_its_own() {
        // Every flag set at once, so each gate that looks at them is exercised.
        let ctx = ChordContext {
            connect_open: true,
            channel: true,
            joined: true,
            can_close: true,
            header_focused: true,
            members_focused: true,
            transcript_selection: true,
            slash_open: true,
            ..ChordContext::default()
        };
        for chord in [
            Chord::Quit,
            Chord::Copy,
            Chord::ComposerFocus,
            Chord::Jump,
            Chord::Link,
            Chord::NickSheet,
            Chord::Inbox,
            Chord::MembersToggle,
            Chord::MembersFocus,
            Chord::ServerListToggle,
            Chord::CloseDirect,
            Chord::StatusToggle,
            Chord::Find,
            Chord::ComposerHistory,
            Chord::NickComplete,
            Chord::SlashPopup,
            Chord::SlashDismiss,
            Chord::MarkAllRead,
            Chord::Unseen,
            Chord::NextUnread,
            Chord::WalkConversationDown,
            Chord::WalkConversationUp,
            Chord::WalkNetworkRight,
            Chord::WalkNetworkLeft,
            Chord::CollapseHeader,
            Chord::ExpandHeader,
            Chord::CollapseAll,
            Chord::ExpandAll,
            Chord::MoveNetworkUp,
            Chord::MoveNetworkDown,
            Chord::HeaderActivate,
            Chord::PageUp,
            Chord::HalfPageDown,
            Chord::TranscriptTop,
            Chord::MembersTop,
            Chord::MembersStep,
            Chord::MembersActivate,
        ] {
            assert!(!chord_enabled(chord, &ctx), "{chord:?} must wait for Connect");
        }
        for chord in [
            Chord::Escape,
            Chord::Shortcuts,
            Chord::About,
            Chord::Connect,
            Chord::ConnectApply,
            Chord::ConnectTabCycle,
            Chord::ConnectWalkNetworkRight,
            Chord::ConnectWalkNetworkLeft,
            Chord::ConnectAdd,
            Chord::ConnectRemove,
        ] {
            assert!(chord_enabled(chord, &ctx), "{chord:?} is live under Connect");
        }
    }

    #[test]
    fn the_connect_entry_is_live_unless_another_sheet_holds_the_keyboard() {
        check(Chord::Connect, nothing, CONNECT_ENTRY);
        check(Chord::Connect, joined_channel, CONNECT_ENTRY);
    }

    #[test]
    fn the_connect_keys_need_connect_open_and_no_sheet_above_it() {
        // Each column sets `connect_open` itself, so only the Connect column is live.
        for chord in [
            Chord::ConnectApply,
            Chord::ConnectTabCycle,
            Chord::ConnectWalkNetworkRight,
            Chord::ConnectWalkNetworkLeft,
            Chord::ConnectAdd,
            Chord::ConnectRemove,
        ] {
            check(chord, nothing, CONNECT_ONLY);
            check(chord, joined_channel, CONNECT_ONLY);
        }
    }

    #[test]
    fn the_connect_keys_stay_live_on_status_under_connect() {
        // Connect is a layer over whatever view is showing, Status included.
        let ctx = ChordContext {
            connect_open: true,
            status_open: true,
            ..ChordContext::default()
        };
        assert!(chord_enabled(Chord::ConnectApply, &ctx));
        assert!(chord_enabled(Chord::ConnectAdd, &ctx));
        assert!(!chord_enabled(Chord::StatusToggle, &ctx));
        assert!(chord_enabled(Chord::Escape, &ctx));
    }

    #[test]
    fn the_header_walk_keys_stay_with_the_header_when_connect_is_closed() {
        // Alt+Left and Alt+Right are the header walk with Connect closed, and the
        // Connect walk with it open: never both at once.
        check(Chord::WalkNetworkLeft, nothing, NO_SHEET);
        check(Chord::ConnectWalkNetworkLeft, nothing, CONNECT_ONLY);
    }

    #[test]
    fn gating_table_for_the_default_context() {
        let base = ChordContext::default();
        assert!(chord_enabled(Chord::Quit, &base));
        assert!(chord_enabled(Chord::Jump, &base));
        assert!(chord_enabled(Chord::StatusToggle, &base));
        assert!(chord_enabled(Chord::Connect, &base));
        assert!(!chord_enabled(Chord::Escape, &base));
        assert!(!chord_enabled(Chord::NickSheet, &base));
        assert!(!chord_enabled(Chord::ConnectApply, &base));
    }
}
