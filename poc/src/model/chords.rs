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
    /// The selected conversation is a channel. Ctrl+Shift+K, M, and P need one.
    pub channel: bool,
    /// The selection is a direct message. Ctrl+W closes it (Qt
    /// `canCloseSelection`; the seed has no unjoined channel to close).
    pub direct: bool,
    /// A network header in the server list has keyboard focus.
    pub header_focused: bool,
    /// The member list has keyboard focus.
    pub members_focused: bool,
    /// The composer has keyboard focus.
    pub composer_focused: bool,
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
/// open. The toggles (Ctrl+K, Ctrl+Shift+O, Ctrl+Shift+A) stay live while
/// their own sheet is open, so the same chord closes it.
pub fn chord_enabled(chord: Chord, ctx: &ChordContext) -> bool {
    let no_sheet = ctx.sheet.is_none();
    let sheet_is = |sheet: Sheet| ctx.sheet == Some(sheet);
    match chord {
        // Qt's Ctrl+Q is disabled while any sheet is open.
        Chord::Quit => no_sheet,
        // Copies the transcript selection, or the member list's selection.
        Chord::Copy => {
            no_sheet && ((ctx.transcript_selection && !ctx.composer_focused) || ctx.members_focused)
        }
        Chord::ComposerFocus => no_sheet,
        // Toggles Jump, and is blocked by every other sheet.
        Chord::Jump => no_sheet || sheet_is(Sheet::Jump),
        // Toggles Link, and is blocked by every other sheet.
        Chord::Link => no_sheet || sheet_is(Sheet::Link),
        // Toggles Inbox, and is blocked by every other sheet.
        Chord::Inbox => no_sheet || sheet_is(Sheet::Inbox),
        Chord::NickSheet => ctx.channel && no_sheet,
        Chord::MembersToggle | Chord::MembersFocus => ctx.channel && no_sheet,
        Chord::ServerListToggle => no_sheet,
        Chord::CloseDirect => ctx.direct && no_sheet,
        // Qt binds Ctrl+/ unconditionally and its handler returns early while
        // another sheet is open, so the result is the same as this gate.
        Chord::Shortcuts => no_sheet || sheet_is(Sheet::Shortcuts),
        Chord::About => no_sheet,
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
        // Return and Enter activate a focused header. Its handler is a no-op
        // for now, but the composer must not send while a header is focused.
        Chord::HeaderActivate => ctx.header_focused && no_sheet,
        Chord::MembersTop
        | Chord::MembersBottom
        | Chord::MembersStep
        | Chord::MembersPage
        | Chord::MembersHalfPage
        | Chord::MembersActivate => ctx.members_focused && no_sheet,
        // Escape closes whichever sheet is open.
        Chord::Escape => !no_sheet,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Column order for the tables below: no sheet, then each sheet kind.
    const COLUMNS: [Option<Sheet>; 7] = [
        None,
        Some(Sheet::Shortcuts),
        Some(Sheet::About),
        Some(Sheet::Jump),
        Some(Sheet::Link),
        Some(Sheet::Inbox),
        Some(Sheet::Nick),
    ];

    /// Checks one chord against every sheet column, with `setup` applied to
    /// the base context first.
    fn check(chord: Chord, setup: fn(&mut ChordContext), expected: [bool; 7]) {
        for (column, sheet) in COLUMNS.iter().enumerate() {
            let mut ctx = ChordContext {
                sheet: *sheet,
                ..ChordContext::default()
            };
            setup(&mut ctx);
            assert_eq!(
                chord_enabled(chord, &ctx),
                expected[column],
                "{chord:?} with sheet {sheet:?} and context {ctx:?}"
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

    // Columns: none, Shortcuts, About, Jump, Link, Inbox, Nick.

    #[test]
    fn no_overlay_chords_are_live_only_without_a_sheet() {
        let only_none = [true, false, false, false, false, false, false];
        check(Chord::Quit, nothing, only_none);
        check(Chord::ComposerFocus, nothing, only_none);
        check(Chord::ServerListToggle, nothing, only_none);
        check(Chord::About, nothing, only_none);
        check(Chord::MarkAllRead, nothing, only_none);
        check(Chord::WalkConversationDown, nothing, only_none);
        check(Chord::WalkConversationUp, nothing, only_none);
        check(Chord::WalkNetworkRight, nothing, only_none);
        check(Chord::WalkNetworkLeft, nothing, only_none);
        check(Chord::CollapseHeader, nothing, only_none);
        check(Chord::ExpandHeader, nothing, only_none);
        check(Chord::CollapseAll, nothing, only_none);
        check(Chord::ExpandAll, nothing, only_none);
        check(Chord::MoveNetworkUp, nothing, only_none);
        check(Chord::MoveNetworkDown, nothing, only_none);
        check(Chord::NextUnread, nothing, only_none);
        check(Chord::PageUp, nothing, only_none);
        check(Chord::PageDown, nothing, only_none);
        check(Chord::HalfPageUp, nothing, only_none);
        check(Chord::HalfPageDown, nothing, only_none);
        check(Chord::TranscriptTop, nothing, only_none);
        check(Chord::TranscriptBottom, nothing, only_none);
    }

    #[test]
    fn toggles_stay_live_for_their_own_sheet_and_block_the_others() {
        check(
            Chord::Jump,
            nothing,
            [true, false, false, true, false, false, false],
        );
        check(
            Chord::Link,
            nothing,
            [true, false, false, false, true, false, false],
        );
        check(
            Chord::Inbox,
            nothing,
            [true, false, false, false, false, true, false],
        );
    }

    #[test]
    fn shortcuts_sheet_toggles_from_none_or_itself_only() {
        check(
            Chord::Shortcuts,
            nothing,
            [true, true, false, false, false, false, false],
        );
    }

    #[test]
    fn escape_closes_any_open_sheet_and_is_inert_otherwise() {
        check(
            Chord::Escape,
            nothing,
            [false, true, true, true, true, true, true],
        );
    }

    #[test]
    fn member_chords_need_a_channel_and_no_sheet() {
        check(
            Chord::NickSheet,
            channel,
            [true, false, false, false, false, false, false],
        );
        check(
            Chord::MembersToggle,
            channel,
            [true, false, false, false, false, false, false],
        );
        check(
            Chord::MembersFocus,
            channel,
            [true, false, false, false, false, false, false],
        );
        check(Chord::NickSheet, nothing, [false; 7]);
        check(Chord::MembersToggle, nothing, [false; 7]);
        check(Chord::MembersFocus, nothing, [false; 7]);
    }

    #[test]
    fn close_direct_needs_a_direct_message_and_no_sheet() {
        check(
            Chord::CloseDirect,
            direct,
            [true, false, false, false, false, false, false],
        );
        check(Chord::CloseDirect, channel, [false; 7]);
        check(Chord::CloseDirect, nothing, [false; 7]);
    }

    #[test]
    fn header_activation_needs_header_focus() {
        check(
            Chord::HeaderActivate,
            header,
            [true, false, false, false, false, false, false],
        );
        check(Chord::HeaderActivate, nothing, [false; 7]);
    }

    #[test]
    fn member_list_navigation_needs_member_focus() {
        check(
            Chord::MembersTop,
            members,
            [true, false, false, false, false, false, false],
        );
        check(
            Chord::MembersBottom,
            members,
            [true, false, false, false, false, false, false],
        );
        check(Chord::MembersTop, channel, [false; 7]);
        check(Chord::MembersBottom, nothing, [false; 7]);
        for chord in [
            Chord::MembersStep,
            Chord::MembersPage,
            Chord::MembersHalfPage,
            Chord::MembersActivate,
        ] {
            check(
                chord,
                members,
                [true, false, false, false, false, false, false],
            );
            check(chord, channel, [false; 7]);
        }
    }

    #[test]
    fn copy_needs_a_selection_outside_the_composer_or_a_focused_member_list() {
        check(
            Chord::Copy,
            selection,
            [true, false, false, false, false, false, false],
        );
        check(Chord::Copy, nothing, [false; 7]);
        check(
            Chord::Copy,
            members,
            [true, false, false, false, false, false, false],
        );

        let mut composer = ChordContext {
            transcript_selection: true,
            composer_focused: true,
            ..ChordContext::default()
        };
        assert!(!chord_enabled(Chord::Copy, &composer));
        composer.composer_focused = false;
        assert!(chord_enabled(Chord::Copy, &composer));
    }

    #[test]
    fn a_sheet_blocks_copy_even_with_a_selection() {
        check(
            Chord::Copy,
            |ctx| {
                ctx.transcript_selection = true;
            },
            [true, false, false, false, false, false, false],
        );
    }

    #[test]
    fn gating_table_for_the_default_context() {
        let base = ChordContext::default();
        assert!(chord_enabled(Chord::Quit, &base));
        assert!(chord_enabled(Chord::Jump, &base));
        assert!(!chord_enabled(Chord::Escape, &base));
        assert!(!chord_enabled(Chord::NickSheet, &base));
    }
}
