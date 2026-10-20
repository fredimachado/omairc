use gpui::{actions, App, KeyBinding};

actions!(
    omairc_shell,
    [
        OpenJump,
        JumpUp,
        JumpDown,
        JumpActivate,
        JumpClose,
        TranscriptCopy,
        ComposerSubmit,
        ToggleMembers,
    ]
);

pub fn bind_shell_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-k", OpenJump, None),
        KeyBinding::new("up", JumpUp, Some("JumpSheet")),
        KeyBinding::new("down", JumpDown, Some("JumpSheet")),
        KeyBinding::new("enter", JumpActivate, Some("JumpSheet")),
        KeyBinding::new("escape", JumpClose, Some("JumpSheet")),
        KeyBinding::new("up", JumpUp, Some("JumpFilter")),
        KeyBinding::new("down", JumpDown, Some("JumpFilter")),
        KeyBinding::new("enter", JumpActivate, Some("JumpFilter")),
        KeyBinding::new("escape", JumpClose, Some("JumpFilter")),
        KeyBinding::new("ctrl-c", TranscriptCopy, Some("Composer")),
        KeyBinding::new("enter", ComposerSubmit, Some("Composer")),
    ]);
}
