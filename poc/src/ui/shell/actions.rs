//! The shell's key table. Every chord is an action; `wire_shell_actions`
//! attaches one listener per action to the window root, and each listener
//! asks `model::chords::chord_enabled` (through `Shell`) before acting.
//! `secondary-` is Ctrl on Windows and Linux and Cmd on macOS.

use gpui::{actions, App, Context, Div, KeyBinding, prelude::*};

use crate::model::chords::Chord;
use crate::ui::input::{Copy as CopySelection, Paste};
use crate::ui::sheets::about::bind_about_keys;
use crate::ui::sheets::channel_open::bind_channel_open_keys;
use crate::ui::sheets::connect::{
    bind_connect_keys, ConnectArrowLeft, ConnectArrowRight, ConnectEnter, ConnectNextStop,
    ConnectPrevStop, ConnectRailDown, ConnectRailEnd, ConnectRailHome, ConnectRailUp, ConnectSpace,
};
use crate::ui::sheets::filter_sheet::bind_filter_keys;
use crate::ui::sheets::inbox::bind_inbox_keys;
use crate::ui::shell::Shell;

actions!(
    omairc_shell,
    [
        OpenJump,
        OpenLink,
        OpenNick,
        OpenInbox,
        ToggleShortcuts,
        OpenAbout,
        CloseSheet,
        ComposerSubmit,
        ToggleMembers,
        ToggleSidebarRail,
        CloseDirect,
        WalkConversationUp,
        WalkConversationDown,
        HeaderLeft,
        HeaderRight,
        CollapseHeader,
        ExpandHeader,
        CollapseAllNetworks,
        ExpandAllNetworks,
        MoveNetworkUp,
        MoveNetworkDown,
        NextUnread,
        MarkAllRead,
        JumpUnseen,
        FocusComposer,
        FocusMembers,
        Quit,
        TranscriptPageUp,
        TranscriptPageDown,
        TranscriptHalfPageUp,
        TranscriptHalfPageDown,
        TranscriptTop,
        TranscriptBottom,
        MemberUp,
        MemberDown,
        MemberPageUp,
        MemberPageDown,
        MemberHalfPageUp,
        MemberHalfPageDown,
        MemberTop,
        MemberBottom,
        MemberActivate,
        ToggleStatus,
        ComposerHistoryUp,
        ComposerHistoryDown,
        CompleteNickForward,
        CompleteNickBackward,
        OpenFind,
        OpenConnect,
        ConnectApply,
        ConnectCycleTab,
        ConnectAdd,
        ConnectRemove,
        InsertFile,
    ]
);

pub fn bind_shell_keys(cx: &mut App) {
    bind_filter_keys(cx);
    bind_inbox_keys(cx);
    bind_about_keys(cx);
    bind_channel_open_keys(cx);
    bind_connect_keys(cx);
    let bindings = vec![
        KeyBinding::new("secondary-,", OpenConnect, None),
        // Ctrl+Enter applies while Connect shows. The listener checks that.
        KeyBinding::new("secondary-enter", ConnectApply, None),
        KeyBinding::new("secondary-tab", ConnectCycleTab, None),
        KeyBinding::new("secondary-n", ConnectAdd, None),
        KeyBinding::new("secondary-shift-delete", ConnectRemove, None),
        KeyBinding::new("secondary-/", ToggleShortcuts, None),
        KeyBinding::new("secondary-shift-/", OpenAbout, None),
        // Shift+/ can arrive as "?" on some layouts (Windows SendKeys does).
        KeyBinding::new("secondary-?", OpenAbout, None),
        KeyBinding::new("secondary-k", OpenJump, None),
        KeyBinding::new("secondary-shift-a", OpenInbox, None),
        KeyBinding::new("secondary-shift-o", OpenLink, None),
        KeyBinding::new("secondary-shift-u", InsertFile, None),
        KeyBinding::new("secondary-shift-k", OpenNick, None),
        KeyBinding::new("escape", CloseSheet, None),
        KeyBinding::new("secondary-shift-s", ToggleSidebarRail, None),
        KeyBinding::new("secondary-w", CloseDirect, None),
        KeyBinding::new("secondary-l", FocusComposer, None),
        KeyBinding::new("secondary-`", ToggleStatus, None),
        KeyBinding::new("secondary-f", OpenFind, None),
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("secondary-shift-m", ToggleMembers, None),
        KeyBinding::new("secondary-shift-p", FocusMembers, None),
        KeyBinding::new("alt-down", WalkConversationDown, None),
        KeyBinding::new("alt-up", WalkConversationUp, None),
        KeyBinding::new("alt-left", HeaderLeft, None),
        KeyBinding::new("alt-right", HeaderRight, None),
        KeyBinding::new("alt-shift-left", CollapseHeader, None),
        KeyBinding::new("alt-shift-right", ExpandHeader, None),
        KeyBinding::new("secondary-alt-shift-left", CollapseAllNetworks, None),
        KeyBinding::new("secondary-alt-shift-right", ExpandAllNetworks, None),
        KeyBinding::new("alt-shift-up", MoveNetworkUp, None),
        KeyBinding::new("alt-shift-down", MoveNetworkDown, None),
        KeyBinding::new("alt-a", NextUnread, None),
        KeyBinding::new("alt-shift-a", MarkAllRead, None),
        KeyBinding::new("alt-u", JumpUnseen, None),
        KeyBinding::new("pageup", TranscriptPageUp, None),
        KeyBinding::new("pagedown", TranscriptPageDown, None),
        KeyBinding::new("shift-pageup", TranscriptHalfPageUp, None),
        KeyBinding::new("shift-pagedown", TranscriptHalfPageDown, None),
        KeyBinding::new("secondary-home", TranscriptTop, None),
        KeyBinding::new("secondary-end", TranscriptBottom, None),
        KeyBinding::new("up", MemberUp, Some("MemberList")),
        KeyBinding::new("down", MemberDown, Some("MemberList")),
        KeyBinding::new("pageup", MemberPageUp, Some("MemberList")),
        KeyBinding::new("pagedown", MemberPageDown, Some("MemberList")),
        KeyBinding::new("shift-pageup", MemberHalfPageUp, Some("MemberList")),
        KeyBinding::new("shift-pagedown", MemberHalfPageDown, Some("MemberList")),
        KeyBinding::new("home", MemberTop, Some("MemberList")),
        KeyBinding::new("end", MemberBottom, Some("MemberList")),
        KeyBinding::new("enter", MemberActivate, Some("MemberList")),
        // The composer binds secondary-c to `Copy` itself (bind_input_keys). When its
        // draft has no selection it propagates, and the shell's listener below
        // copies the transcript or member nick. The member list uses the same key.
        KeyBinding::new("secondary-c", CopySelection, Some("MemberList")),
        KeyBinding::new("enter", ComposerSubmit, Some("Composer")),
        // Up and Down walk the sent lines. With none they fall through to the field.
        KeyBinding::new("up", ComposerHistoryUp, Some("Composer")),
        KeyBinding::new("down", ComposerHistoryDown, Some("Composer")),
        // Tab completes a nick and never moves focus.
        KeyBinding::new("tab", CompleteNickForward, Some("Composer")),
        KeyBinding::new("shift-tab", CompleteNickBackward, Some("Composer")),
    ];
    // Windows Terminal keeps Ctrl+, for its Settings, and Ctrl+PgDn for its tabs, so
    // Qt adds Ctrl+] and Ctrl+PgDn on Windows only.
    #[cfg(target_os = "windows")]
    {
        bindings.push(KeyBinding::new("secondary-]", OpenConnect, None));
        bindings.push(KeyBinding::new("secondary-pagedown", ConnectCycleTab, None));
    }
    cx.bind_keys(bindings);
}

/// Attaches every shell action to the window root. Each listener defers to a
/// `Shell` method that checks `chord_enabled` first.
pub fn wire_shell_actions(root: Div, cx: &mut Context<Shell>) -> Div {
    root.on_action(cx.listener(|shell, _: &OpenJump, window, cx| {
        shell.toggle_jump(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &OpenLink, window, cx| {
        shell.toggle_link(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &OpenNick, window, cx| {
        shell.open_nick_sheet(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &OpenInbox, window, cx| {
        shell.toggle_inbox(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &ToggleShortcuts, window, cx| {
        shell.toggle_shortcuts(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &OpenAbout, window, cx| {
        shell.open_about(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &CloseSheet, window, cx| {
        shell.escape(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &ToggleSidebarRail, window, cx| {
        shell.toggle_rail(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &CloseDirect, window, cx| {
        shell.close_selected_direct(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &CopySelection, window, cx| {
        shell.copy_focused(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &ComposerSubmit, window, cx| {
        shell.composer_submit(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &ToggleMembers, window, cx| {
        shell.toggle_members(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &FocusMembers, window, cx| {
        shell.focus_members(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &FocusComposer, window, cx| {
        shell.clear_header_and_focus_composer(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &ToggleStatus, window, cx| {
        shell.toggle_status(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &Quit, window, cx| {
        shell.quit(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &WalkConversationDown, window, cx| {
        shell.walk_conversation(1, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &WalkConversationUp, window, cx| {
        shell.walk_conversation(-1, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &HeaderLeft, window, cx| {
        shell.walk_header(-1, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &HeaderRight, window, cx| {
        shell.walk_header(1, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &CollapseHeader, window, cx| {
        shell.set_focused_header_collapsed(true, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &ExpandHeader, window, cx| {
        shell.set_focused_header_collapsed(false, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &CollapseAllNetworks, window, cx| {
        shell.set_all_collapsed(true, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &ExpandAllNetworks, window, cx| {
        shell.set_all_collapsed(false, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &MoveNetworkUp, window, cx| {
        shell.move_focused_header(-1, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &MoveNetworkDown, window, cx| {
        shell.move_focused_header(1, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &NextUnread, window, cx| {
        shell.select_next_unread(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &MarkAllRead, window, cx| {
        shell.read_all(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &JumpUnseen, window, cx| {
        shell.jump_to_unseen(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &TranscriptPageUp, window, cx| {
        shell.transcript_scroll(Chord::PageUp, false, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &TranscriptPageDown, window, cx| {
        shell.transcript_scroll(Chord::PageDown, true, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &TranscriptHalfPageUp, window, cx| {
        shell.transcript_scroll(Chord::HalfPageUp, false, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &TranscriptHalfPageDown, window, cx| {
        shell.transcript_scroll(Chord::HalfPageDown, true, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &TranscriptTop, window, cx| {
        shell.transcript_jump(false, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &TranscriptBottom, window, cx| {
        shell.transcript_jump(true, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &MemberUp, window, cx| {
        shell.member_move(-1, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &MemberDown, window, cx| {
        shell.member_move(1, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &MemberPageUp, window, cx| {
        shell.member_page(false, false, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &MemberPageDown, window, cx| {
        shell.member_page(true, false, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &MemberHalfPageUp, window, cx| {
        shell.member_page(false, true, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &MemberHalfPageDown, window, cx| {
        shell.member_page(true, true, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &MemberTop, window, cx| {
        shell.member_jump(false, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &MemberBottom, window, cx| {
        shell.member_jump(true, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &MemberActivate, window, cx| {
        shell.member_activate(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &ComposerHistoryUp, window, cx| {
        if !shell.recall_history(-1, window, cx) {
            cx.propagate();
        }
    }))
    .on_action(cx.listener(|shell, _: &ComposerHistoryDown, window, cx| {
        if !shell.recall_history(1, window, cx) {
            cx.propagate();
        }
    }))
    .on_action(cx.listener(|shell, _: &CompleteNickForward, window, cx| {
        shell.complete_nick(false, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &CompleteNickBackward, window, cx| {
        shell.complete_nick(true, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &OpenFind, window, cx| {
        shell.begin_or_advance_find(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &OpenConnect, window, cx| {
        shell.open_connect(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &ConnectApply, window, cx| {
        shell.connect_apply(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &ConnectCycleTab, window, cx| {
        shell.connect_cycle_tab(1, window, cx);
    }))
    .on_action(cx.listener(|shell, _: &ConnectAdd, window, cx| {
        shell.connect_add(window, cx);
    }))
    .on_action(cx.listener(|shell, _: &ConnectRemove, window, cx| {
        shell.connect_remove(window, cx);
    }))
    // The Connect card's own keys. Each one passes on when the focused stop does not use it,
    // so a field keeps Space and the arrows, and the composer keeps its keys.
    .on_action(cx.listener(|shell, _: &ConnectNextStop, window, cx| {
        if !shell.connect_step(false, window, cx) {
            cx.propagate();
        }
    }))
    .on_action(cx.listener(|shell, _: &ConnectPrevStop, window, cx| {
        if !shell.connect_step(true, window, cx) {
            cx.propagate();
        }
    }))
    .on_action(cx.listener(|shell, _: &ConnectEnter, window, cx| {
        if !shell.connect_enter(window, cx) {
            cx.propagate();
        }
    }))
    .on_action(cx.listener(|shell, _: &ConnectSpace, window, cx| {
        if !shell.connect_space(window, cx) {
            cx.propagate();
        }
    }))
    .on_action(cx.listener(|shell, _: &ConnectArrowLeft, window, cx| {
        if !shell.connect_arrow(-1, window, cx) {
            cx.propagate();
        }
    }))
    .on_action(cx.listener(|shell, _: &ConnectArrowRight, window, cx| {
        if !shell.connect_arrow(1, window, cx) {
            cx.propagate();
        }
    }))
    .on_action(cx.listener(|shell, _: &ConnectRailUp, window, cx| {
        if !shell.connect_rail_step(-1, window, cx) {
            cx.propagate();
        }
    }))
    .on_action(cx.listener(|shell, _: &ConnectRailDown, window, cx| {
        if !shell.connect_rail_step(1, window, cx) {
            cx.propagate();
        }
    }))
    .on_action(cx.listener(|shell, _: &ConnectRailHome, window, cx| {
        if !shell.connect_rail_edge(false, window, cx) {
            cx.propagate();
        }
    }))
    .on_action(cx.listener(|shell, _: &ConnectRailEnd, window, cx| {
        if !shell.connect_rail_edge(true, window, cx) {
            cx.propagate();
        }
    }))
    .on_action(cx.listener(|shell, _: &InsertFile, window, cx| {
        if !shell.chord_live(Chord::InsertFile, window, cx) || !shell.file_upload_offered() {
            cx.propagate();
            return;
        }
        shell.prompt_for_file(window, cx);
    }))
    .capture_action(cx.listener(|shell, action: &Paste, window, cx| {
        // gpui's capture phase runs on unless a listener stops it, so a consumed file
        // paste must stop here or the composer pastes the path as well.
        if shell.handle_composer_paste(action, window, cx) {
            cx.stop_propagation();
        }
    }))
}
