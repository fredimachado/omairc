# Keyboard

Keyboard is the window chord map: walk conversations, walk network headers, jump to a named conversation, jump to a channel nick, jump unread, complete nicks, recall sent lines, find in the buffer, focus members, close a direct message, and show this list. Clicks still work. Status is not a sidebar row.

## Sub-features

- `keyboard-walk` moves CHANNELS then DIRECT MESSAGES with `Alt+Down` / `Alt+Up`, wrapping at both ends. Status is not in that list.
- `keyboard-networks` moves network headers with `Alt+Left` / `Alt+Right`, wrapping at both ends. A network with no conversations stays in that walk. Enter opens that network's Status. `Ctrl+,` opens Connect for that header.
- `keyboard-jump` opens a filter overlay with `Ctrl+K`. It lists channels, direct messages, and each network's Status. Type filters the list. Up/Down highlight, Enter jumps, Escape dismisses and returns to the composer. It is a no-op while Connect is visible.
- `keyboard-nick` opens a filter overlay with `Ctrl+Shift+K` on a channel. It lists that channel's members in panel order. Type filters by nick substring. Up/Down highlight, Enter opens or creates that DM, Escape dismisses and returns to the composer. The member panel `ONLINE` heading opens the same sheet. It is a no-op on a direct message or Status, and it works with the member panel hidden.
- `keyboard-unread` jumps to the next unread with `Alt+A`, mentions first. Muted conversations are skipped while hunting mentions.
- `keyboard-complete` completes a nick prefix in the composer with `Tab`.
- `keyboard-history` recalls sent lines with `Up` / `Down` and restores a typed draft on `Down`.
- `keyboard-drafts` keeps unsent composer text per conversation and Status. Switching targets restores that draft. It does not follow you.
- `keyboard-members` focuses the member list with `Ctrl+Shift+P` and reopens a hidden panel. Enter on a focused member opens a DM.
- `keyboard-close` closes the selected direct message with `Ctrl+W` and selects the next DM, or the previous row when that was the last. It is a no-op on a channel, Status, or the shortcut sheet.
- `keyboard-sheet` toggles the shortcut list with `Ctrl+/`. Escape closes the sheet before Status. The list includes `Ctrl+W`, `Ctrl+F`, `Ctrl+K`, `Ctrl+Shift+K`, and `/disconnect`. `Ctrl+Q` still quits the process.
- `keyboard-connect` opens Connect with `Ctrl+,` when a connection exists.
- `keyboard-scroll` pages the visible transcript with `Page Up` / `Page Down` while the composer stays focused.
- `keyboard-find` finds text in the current conversation or Status with `Ctrl+F`. Empty first press enters find and waits. The composer shows Find and holds the query. Matches include author and body, or Status label and text. The current row uses the selection color. Enter or another `Ctrl+F` goes to the next match and wraps. Escape leaves find and restores the unsent draft.

## How to get to it (user POV)

- Press `Ctrl+/` to read the list in the app.
- Press `Ctrl+K` to jump to a channel, direct message, or Status.
- Press `Ctrl+Shift+K` on a channel to jump to a nick and open or create that DM. Click `ONLINE` in the member panel for the same sheet.
- Press `Alt+Down` / `Alt+Up` or `Alt+A` while a conversation exists.
- Press `Alt+Left` / `Alt+Right` to land on a network header, including an empty one.
- Press `Tab`, `Up`, or `Down` in the composer.
- Press `Ctrl+Shift+P` on a channel, then arrows and Enter.
- Press `Ctrl+W` on a direct message to close it.
- Press `Ctrl+,` to reopen Connect after a profile exists.
- Press `Page Up` / `Page Down` to scroll without leaving the composer.
- Press `Ctrl+F` to find in the current conversation or Status.

## Driving it with control-omairc

Preconditions:

- Conversation walk, unread jump, nick complete, history, member focus, `Ctrl+W` close, and the sheet's Escape-vs-Status behavior need the seeded UI (`control-omairc launch --demo-server` or `qml-suite`, seeded `IrcController`).
- A default compiled window is titled `irc.libera.chat Status` with Connect. `Ctrl+/` still opens the sheet on that window. `Alt+Down` has nowhere to walk until a conversation exists.
- `Ctrl+,` opens Connect on `--demo-server` because a connection is bound. The suite also covers it with `test_openConnectSheetWithShortcut`.

- **Shortcut sheet on first run.** After `control-omairc launch`, run `control-omairc key --key ctrl+slash` then `control-omairc screenshot --feature keyboard --name after-ctrl-slash`. The sheet lists walk conversations, walk networks, jump to conversation, jump to nick, unread, Status, Connect, members panel, focus members, close direct message, composer, find, send, scroll, nick complete, history, Escape, this sheet, `/disconnect`, and quit. If the first send does not open the sheet, send `ctrl+slash` again. Run `control-omairc key --key Escape`. Connect stays open.
- **Walk, unread, complete, history, members, close.** Run `control-omairc doctor-qml` then `control-omairc qml-suite`. The suite covers `Alt+Down` / `Alt+Up` (including wrap and leaving Status), `Alt+Left` / `Alt+Right` (including an empty header, Enter → Status, and `Ctrl+,`), `Ctrl+K` jump (filter, Enter, Escape, duplicate `#omarchy`, Connect no-op), `Ctrl+Shift+K` nick jump (filter, Enter creating `mira`, Escape, hidden panel, Status and DM no-op, `ONLINE` heading), `Alt+A` (mention then unread), `Tab` on `mi` → `mira: `, Up/Down history, per-conversation composer drafts, `Ctrl+F` find, Page Up / Page Down, `Ctrl+Shift+P` plus Enter on `mira`, `Ctrl+W` closing a DM, and `Ctrl+/` Escape keeping the conversation and Status. The sheet lists `walk networks`, `Ctrl+K`, and `Ctrl+Shift+K`. On a fresh `launch --demo-server`, `control-omairc key --key alt+a` jumps to `#ricing` (mention 12). A second `alt+a` jumps to `anna`, who is still a mention until that DM is opened. Create `mira` from her member row, then `control-omairc key --key ctrl+w`. The title becomes `dax - Omairc` and the `mira` sidebar row is gone. Do not `click-conversation --name mira`; that helper only knows seeded rows.
- **Connect chord.** The same suite run covers `Ctrl+,` on the fallback window with a connection. That does not prove the compiled-window chord after Apply.

## Gotchas

- Status stays out of `Alt+Down` / `Alt+Up`. Opening Status then `Alt+Down` leaves Status for the next sidebar row.
- `Alt+Left` / `Alt+Right` do not change the open conversation. Enter on a focused header opens Status. `Ctrl+L` clears header focus so Enter sends again.
- Unsent composer text stays with the conversation or Status. Switching away and back restores it. Status uses its own key, not the conversation name.
- `Ctrl+Shift+M` still toggles the panel. That is toggle-members. `Ctrl+Shift+P` focuses the list.
- `Ctrl+W` is disabled on channels and Status. Typed `/close` on a channel is a rejected slash command and stays in the composer.
- `Tab` completes a nick prefix in the composer. Prove it with `qml-suite` (`mi` → `mira: `).
- Page Up / Page Down are disabled while Connect is visible.
- `Alt+Down` / `Alt+Up`, `Alt+Left` / `Alt+Right`, `Alt+A`, `Ctrl+L`, `Ctrl+W`, and `Ctrl+`` are disabled while Connect is visible. Connect is a window-level modal; those chords must not walk servers or members behind it. `Ctrl+/` still opens the shortcuts overlay.
- `Ctrl+F` enters find even with an empty composer. It jumps the current transcript to the match and leaves follow-the-end so the match stays put. Escape restores the draft, not the old scroll position.
- Escape closes the sheet before Status. `Ctrl+/` toggles it.
- `Ctrl+K` is disabled while Connect is visible, the same rule as `Ctrl+F`. Duplicate channel names show the network display name.
- `Ctrl+Shift+K` is disabled on direct messages and Status. It does not open the member panel. Filter is a nick substring, same style as `Ctrl+K`. The sheet lists members in `PREFIX` rank order, including your own nick; Enter on yourself is a no-op.
- `control-omairc` maps `ctrl+slash` to `Control_L+slash` and `ctrl+shift+p` / `ctrl+shift+k` to `Control_L+Shift_L+p` / `k`. xdotool's shorter tokens do not reach those Qt shortcuts on the isolated Xvfb.
- Create `mira` from her member row, then `Ctrl+W`. Do not `click-conversation --name mira`; that helper only knows seeded rows (`#omarchy`, `#desktop`, `#ricing`, `#help`, `anna`, `dax`).
- Do not Apply on the compiled window to "get a conversation" for walk proof. That starts a real session.
