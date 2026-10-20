//! The in-scope chords and the conditions under which each one is live. The
//! rules mirror each Qt `Shortcut.enabled` in `src/OmaircWindow.qml`
//! (around lines 2650-3186), and the Gating paragraph of the GPUI plan.
//! The UI asks `chord_enabled` before acting, so a chord can never fire
//! behind a sheet that should own the keyboard.

/// A modal that owns the keyboard while it is open. Qt can hold several
/// `opened` flags at once; the GPUI shell shows one at a time, so the model
/// stores a single `Option<Sheet>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sheet {
    Shortcuts,
    About,
    Jump,
    Link,
    Inbox,
    Nick,
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
    /// The selected conversation is a channel. Ctrl+Shift+K, M, and P need one.
    pub channel: bool,
    /// The selection is a direct message. Ctrl+W closes it (Qt
    /// `canCloseSelection`; the seed has no unjoined channel to close).
    pub direct: bool,
    /// A network header in the server list has keyboard focus.
    pub header_focused: bool,
    /// The member list has keyboard focus.
    pub members_focused: bool,
    /// The transcript has a non-empty text selection.
    pub transcript_selection: bool,
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
    /// Up and Down in the composer: walk the sent lines. With no history they fall through.
    ComposerHistory,
    /// Tab and Shift+Tab in the composer: complete a nick. Status has no nicks.
    NickComplete,
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
}

/// Whether the chord is live in `ctx`. "No overlay" chords need no sheet to be
/// open, and Status does not count as one. The toggles (Ctrl+K, Ctrl+Shift+O,
/// Ctrl+Shift+A, Ctrl+`) stay live while their own sheet or Status is open, so
/// the same chord closes it.
pub fn chord_enabled(chord: Chord, ctx: &ChordContext) -> bool {
    let no_sheet = ctx.sheet.is_none();
    let sheet_is = |sheet: Sheet| ctx.sheet == Some(sheet);
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
        // Toggles Jump, and is blocked by every other sheet.
        Chord::Jump => no_sheet || sheet_is(Sheet::Jump),
        // Toggles Link, and is blocked by every other sheet.
        Chord::Link => no_sheet || sheet_is(Sheet::Link),
        // Toggles Inbox, and is blocked by every other sheet.
        Chord::Inbox => no_sheet || sheet_is(Sheet::Inbox),
        Chord::NickSheet => ctx.channel && on_conversation,
        Chord::MembersToggle | Chord::MembersFocus => ctx.channel && on_conversation,
        Chord::ServerListToggle => no_sheet,
        Chord::CloseDirect => ctx.direct && on_conversation,
        // Qt binds Ctrl+/ unconditionally and its handler returns early while
        // another sheet is open, so the result is the same as this gate.
        Chord::Shortcuts => no_sheet || sheet_is(Sheet::Shortcuts),
        Chord::About => no_sheet,
        // Qt binds Ctrl+` with no sheet gate (only the connect and shortcut
        // overlays), so it also closes Status while Status is the view.
        Chord::StatusToggle => no_sheet,
        // Qt's composer walks history on Up and Down whichever view is open, so
        // Status keeps its own history. Only a sheet owns those keys.
        Chord::ComposerHistory => no_sheet,
        // Tab completes on a conversation only. Status has no nicks. The key is
        // still consumed there, so focus never leaves the composer.
        Chord::NickComplete => on_conversation,
        // Qt binds Alt+Shift+A unconditionally and returns early while an
        // overlay is open, so the net gate is "no overlay".
        Chord::MarkAllRead => no_sheet,
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
        // Status. The composer must not send while a header is focused.
        Chord::HeaderActivate => ctx.header_focused && no_sheet,
        Chord::MembersTop
        | Chord::MembersBottom
        | Chord::MembersStep
        | Chord::MembersPage
        | Chord::MembersHalfPage
        | Chord::MembersActivate => ctx.members_focused && on_conversation,
        // Escape closes whichever sheet is open. With no sheet, it leaves
        // Status for the last conversation.
        Chord::Escape => !no_sheet || ctx.status_open,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Column order for the tables below: no sheet, then each sheet kind, then
    /// Status open with no sheet.
    const COLUMNS: [(Option<Sheet>, bool); 8] = [
        (None, false),
        (Some(Sheet::Shortcuts), false),
        (Some(Sheet::About), false),
        (Some(Sheet::Jump), false),
        (Some(Sheet::Link), false),
        (Some(Sheet::Inbox), false),
        (Some(Sheet::Nick), false),
        (None, true),
    ];

    /// Live with no sheet, on a conversation or on Status.
    const NO_SHEET: [bool; 8] = [true, false, false, false, false, false, false, true];
    /// Live with no sheet, but only on a conversation (Status hides it).
    const CONVERSATION_ONLY: [bool; 8] = [true, false, false, false, false, false, false, false];
    /// Never live in these contexts.
    const NOWHERE: [bool; 8] = [false; 8];

    /// Checks one chord against every column, with `setup` applied to the base
    /// context first.
    fn check(chord: Chord, setup: fn(&mut ChordContext), expected: [bool; 8]) {
        for (column, (sheet, status_open)) in COLUMNS.iter().enumerate() {
            let mut ctx = ChordContext {
                sheet: *sheet,
                status_open: *status_open,
                ..ChordContext::default()
            };
            setup(&mut ctx);
            assert_eq!(
                chord_enabled(chord, &ctx),
                expected[column],
                "{chord:?} with sheet {sheet:?}, status {status_open} and context {ctx:?}"
            );
        }
    }

    fn nothing(_: &mut ChordContext) {}
    fn channel(ctx: &mut ChordContext) {
        ctx.channel = true;
    }
    fn direct(ctx: &mut ChordContext) {
        ctx.direct = true;
    }
    fn header(ctx: &mut ChordContext) {
        ctx.header_focused = true;
    }
    fn members(ctx: &mut ChordContext) {
        ctx.channel = true;
        ctx.members_focused = true;
    }
    fn selection(ctx: &mut ChordContext) {
        ctx.transcript_selection = true;
    }

    // Columns: none, Shortcuts, About, Jump, Link, Inbox, Nick, Status open.

    #[test]
    fn no_overlay_chords_are_live_without_a_sheet_and_on_status() {
        check(Chord::Quit, nothing, NO_SHEET);
        check(Chord::ComposerFocus, nothing, NO_SHEET);
        check(Chord::ServerListToggle, nothing, NO_SHEET);
        check(Chord::About, nothing, NO_SHEET);
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
            [true, false, false, true, false, false, false, true],
        );
        check(
            Chord::Link,
            nothing,
            [true, false, false, false, true, false, false, true],
        );
        check(
            Chord::Inbox,
            nothing,
            [true, false, false, false, false, true, false, true],
        );
    }

    #[test]
    fn shortcuts_sheet_toggles_from_none_or_itself_only() {
        check(
            Chord::Shortcuts,
            nothing,
            [true, true, false, false, false, false, false, true],
        );
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
    fn nick_completion_needs_a_conversation_not_status() {
        check(Chord::NickComplete, nothing, CONVERSATION_ONLY);
        check(Chord::NickComplete, channel, CONVERSATION_ONLY);
        check(Chord::NickComplete, direct, CONVERSATION_ONLY);
    }

    #[test]
    fn escape_closes_any_open_sheet_or_leaves_status() {
        check(
            Chord::Escape,
            nothing,
            [false, true, true, true, true, true, true, true],
        );
    }

    #[test]
    fn member_chords_need_a_channel_and_a_conversation() {
        check(Chord::NickSheet, channel, CONVERSATION_ONLY);
        check(Chord::MembersToggle, channel, CONVERSATION_ONLY);
        check(Chord::MembersFocus, channel, CONVERSATION_ONLY);
        check(Chord::NickSheet, nothing, NOWHERE);
        check(Chord::MembersToggle, nothing, NOWHERE);
        check(Chord::MembersFocus, nothing, NOWHERE);
    }

    #[test]
    fn close_direct_needs_a_direct_message_and_a_conversation() {
        check(Chord::CloseDirect, direct, CONVERSATION_ONLY);
        check(Chord::CloseDirect, channel, NOWHERE);
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
        check(Chord::MembersTop, channel, NOWHERE);
        check(Chord::MembersBottom, nothing, NOWHERE);
        for chord in [
            Chord::MembersStep,
            Chord::MembersPage,
            Chord::MembersHalfPage,
            Chord::MembersActivate,
        ] {
            check(chord, members, CONVERSATION_ONLY);
            check(chord, channel, NOWHERE);
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
    fn status_hides_the_member_panel_and_the_direct_message_close() {
        // A channel with the member list focused, viewed on Status: none of the
        // member or close-direct chords are live.
        let ctx = ChordContext {
            status_open: true,
            channel: true,
            direct: true,
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

    #[test]
    fn gating_table_for_the_default_context() {
        let base = ChordContext::default();
        assert!(chord_enabled(Chord::Quit, &base));
        assert!(chord_enabled(Chord::Jump, &base));
        assert!(chord_enabled(Chord::StatusToggle, &base));
        assert!(!chord_enabled(Chord::Escape, &base));
        assert!(!chord_enabled(Chord::NickSheet, &base));
    }
}
