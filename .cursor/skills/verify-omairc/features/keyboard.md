# Keyboard

Keyboard is the window chord map: walk conversations, jump unread, complete nicks, recall sent lines, find in the buffer, focus members, close a direct message, and show this list. Clicks still work. Status is not a sidebar row.

## Sub-features

- `keyboard-walk` moves CHANNELS then DIRECT MESSAGES with `Alt+Down` / `Alt+Up`, wrapping at both ends. Status is not in that list.
- `keyboard-unread` jumps to the next unread with `Alt+A`, mentions first.
- `keyboard-complete` completes a nick prefix in the composer with `Tab`.
- `keyboard-history` recalls sent lines with `Up` / `Down` and restores a typed draft on `Down`.
- `keyboard-drafts` keeps unsent composer text per conversation and Status. Switching targets restores that draft. It does not follow you.
- `keyboard-members` focuses the member list with `Ctrl+Shift+P` and reopens a hidden panel. Enter on a focused member opens a DM.
- `keyboard-close` closes the selected direct message with `Ctrl+W` and selects the next DM, or the previous row when that was the last. It is a no-op on a channel, Status, or the shortcut sheet.
- `keyboard-sheet` toggles the shortcut list with `Ctrl+/`. Escape closes the sheet before Status. The list includes `Ctrl+W` and `Ctrl+F`.
- `keyboard-connect` opens Connect with `Ctrl+,` when a connection exists.
- `keyboard-scroll` pages the visible transcript with `Page Up` / `Page Down` while the composer stays focused.
- `keyboard-find` finds text in the current conversation or Status with `Ctrl+F`. The composer holds the query. Enter or another `Ctrl+F` goes to the next match and wraps. Escape leaves find and restores the unsent draft.

## How to get to it (user POV)

- Press `Ctrl+/` to read the list in the app.
- Press `Alt+Down` / `Alt+Up` or `Alt+A` while a conversation exists.
- Press `Tab`, `Up`, or `Down` in the composer.
- Press `Ctrl+Shift+P` on a channel, then arrows and Enter.
- Press `Ctrl+W` on a direct message to close it.
- Press `Ctrl+,` to reopen Connect after a profile exists.
- Press `Page Up` / `Page Down` to scroll without leaving the composer.
- Press `Ctrl+F` to find in the current conversation or Status.

## Driving it with control-omairc

Preconditions:

- Conversation walk, unread jump, nick complete, history, member focus, `Ctrl+W` close, and the sheet's Escape-vs-Status behavior need mock UI (`control-omairc launch --mock` or `qml-suite`, `irc` left null).
- A default compiled window is titled `irc.libera.chat Status` with Connect. `Ctrl+/` still opens the sheet on that window. `Alt+Down` has nowhere to walk until a conversation exists.
- `Ctrl+,` is a no-op on the mock window (`connection` is null). Prove it with `qml-suite` (`test_openConnectSheetWithShortcut`) or a compiled window that already has a connection.

- **Shortcut sheet on first run.** After `control-omairc launch`, run `control-omairc key --key ctrl+slash` then `control-omairc screenshot --feature keyboard --name after-ctrl-slash`. The sheet lists walk, unread, Status, Connect, members panel, focus members, close direct message, composer, find, send, scroll, nick complete, history, Escape, this sheet, and quit. If the first send does not open the sheet, send `ctrl+slash` again. Run `control-omairc key --key Escape`. Connect stays open.
- **Walk, unread, complete, history, members, close.** Run `control-omairc doctor-qml` then `control-omairc qml-suite`. The suite covers `Alt+Down` / `Alt+Up` (including wrap and leaving Status), `Alt+A` (mention then unread), `Tab` on `mi` → `mira: `, Up/Down history, per-conversation composer drafts, `Ctrl+F` find, Page Up / Page Down, `Ctrl+Shift+P` plus Enter on `mira`, `Ctrl+W` closing a DM, and `Ctrl+/` Escape keeping the conversation and Status. On a fresh `launch --mock`, `control-omairc key --key alt+a` jumps to `#ricing` (mention 12). A second `alt+a` jumps to `anna`, who is still a mention until that DM is opened. Create `mira` from her member row, then `control-omairc key --key ctrl+w`. The title becomes `dax - Omairc` and the `mira` sidebar row is gone. Do not `click-conversation --name mira`; that helper only knows seeded rows.
- **Connect chord.** The same suite run covers `Ctrl+,` on the fallback window with a connection. That does not prove the compiled-window chord after Apply.

## Gotchas

- Status stays out of `Alt+Down` / `Alt+Up`. Opening Status then `Alt+Down` leaves Status for the next sidebar row.
- Unsent composer text stays with the conversation or Status. Switching away and back restores it. Status uses its own key, not the conversation name.
- `Ctrl+Shift+M` still toggles the panel. That is toggle-members. `Ctrl+Shift+P` focuses the list.
- `Ctrl+W` is disabled on channels and Status. On `--mock`, typed `/close` stays a chat line. Live `/close` is slash-commands.
- `Tab` completes a nick prefix in the composer. Prove it with `qml-suite` (`mi` → `mira: `).
- Page Up / Page Down are disabled while Connect is visible.
- `Ctrl+F` jumps the current transcript to the match and leaves follow-the-end so the match stays put. Escape restores the draft, not the old scroll position.
- Escape closes the sheet before Status. `Ctrl+/` toggles it.
- `control-omairc` maps `ctrl+slash` to `Control_L+slash` and `ctrl+shift+p` to `Control_L+Shift_L+p`. xdotool's shorter tokens do not reach those Qt shortcuts on the isolated Xvfb.
- Create `mira` from her member row, then `Ctrl+W`. Do not `click-conversation --name mira`; that helper only knows seeded rows (`#omarchy`, `#desktop`, `#ricing`, `#help`, `anna`, `dax`).
- Do not Apply on the compiled window to "get a conversation" for walk proof. That starts a real session.
