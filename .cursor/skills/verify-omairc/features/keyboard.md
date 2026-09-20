# Keyboard

Keyboard is the window chord map: walk conversations, walk network headers, collapse or reorder those networks, jump to a named conversation, jump to a channel nick, jump unread, complete nicks, recall sent lines, find in the buffer, focus members, toggle the left server list column, close a direct message, and show this list. Clicks still work. Status is not a sidebar row.

## Sub-features

- `keyboard-walk` moves CHANNELS then DIRECT MESSAGES with `Alt+Down` / `Alt+Up`, wrapping at both ends. Status is not in that list. Rows under a collapsed network are skipped; the walk does not expand them. If the current conversation is hidden, the next/previous *visible* row is used. If nothing is visible, the chord is a no-op.
- `keyboard-networks` moves network headers with `Alt+Left` / `Alt+Right`, wrapping at both ends. A network with no conversations stays in that walk. Headers stay in this walk when their section is collapsed. Enter opens that network's Status. `Ctrl+,` opens Connect for that header. Restores the server list column when it is collapsed.
- `keyboard-network-collapse` collapses or expands the focused network with `Alt+Shift+Left` / `Alt+Shift+Right`. The header stays; CHANNELS, DIRECT MESSAGES, and their rows hide (height 0). A small chevron on the header toggles the same state on click. Header click and Enter still open Status. Unread and mention marks stay on the collapsed header. These chords need a focused network header (`sidebarNetworkFocusId`); they no-op in the composer.
- `keyboard-network-collapse-all` collapses or expands every network with `Ctrl+Alt+Shift+Left` / `Ctrl+Alt+Shift+Right`. Header focus is not required. Collapse-all does not steal header focus.
- `keyboard-network-move` reorders the focused network with `Alt+Shift+Up` / `Alt+Shift+Down`. No wrap at the ends. Focus stays on the moved network. Needs a focused header. No drag handles.
- `keyboard-jump` opens a filter overlay with `Ctrl+K`. It lists channels, direct messages, and each network's Status, including conversations under collapsed networks. Type filters the list. Up/Down highlight, Enter jumps, Escape dismisses and returns to the composer. Activating a conversation expands that network then selects it. It is a no-op while Connect is visible.
- `keyboard-nick` opens a filter overlay with `Ctrl+Shift+K` on a channel. It lists that channel's members in panel order. Type filters by nick substring. Up/Down highlight, Enter opens or creates that DM, Escape dismisses and returns to the composer. The member panel `ONLINE` heading opens the same sheet. It is a no-op on a direct message or Status, and it works with the member panel hidden.
- `keyboard-unread` jumps to the next unread with `Alt+A`, mentions first. Muted conversations are skipped while hunting mentions. Hidden conversations under collapsed networks still count; landing expands that network.
- `keyboard-complete` completes a nick prefix in the composer with `Tab`.
- `keyboard-history` recalls sent lines with `Up` / `Down` and restores a typed draft on `Down`.
- `keyboard-drafts` keeps unsent composer text per conversation and Status. Switching targets restores that draft. It does not follow you.
- `keyboard-members` focuses the member list with `Ctrl+Shift+P` and reopens a hidden panel. Enter on a focused member opens a DM.
- `keyboard-server-list` collapses and restores the left server list column with `Ctrl+Shift+S`. That is the whole rail, not a per-network section. Collapsing is not hiding: `Alt+Down` / `Alt+Up` still walk conversations while the column is collapsed. It also drops the focused network header, so Enter in the composer sends again. `Alt+Left` / `Alt+Right` restores the column so the header highlight is visible.
- `keyboard-close` closes the selected direct message with `Ctrl+W` and selects the next DM, or the previous row when that was the last. It is a no-op on a channel, Status, or the shortcut sheet.
- `keyboard-sheet` toggles the shortcut list with `Ctrl+/`. Escape closes the sheet before Status. The list includes `Ctrl+W`, `Ctrl+F`, `Ctrl+K`, `Ctrl+Shift+K`, `Ctrl+Shift+S`, per-network collapse/expand/move, collapse-all / expand-all, and `/disconnect`. `Ctrl+Q` still quits the process.
- `keyboard-connect` opens Connect with `Ctrl+,` when a connection exists.
- `keyboard-scroll` pages the visible transcript with `Page Up` / `Page Down` while the composer stays focused.
- `keyboard-find` finds text in the current conversation or Status with `Ctrl+F`. Empty first press enters find and waits. The composer shows Find and holds the query. Matches include author and body, or Status label and text. The current row uses the selection color. Enter or another `Ctrl+F` goes to the next match and wraps. Escape leaves find and restores the unsent draft.

## How to get to it (user POV)

- Press `Ctrl+/` to read the list in the app.
- Press `Ctrl+K` to jump to a channel, direct message, or Status.
- Press `Ctrl+Shift+K` on a channel to jump to a nick and open or create that DM. Click `ONLINE` in the member panel for the same sheet.
- Press `Alt+Down` / `Alt+Up` or `Alt+A` while a conversation exists.
- Press `Alt+Left` / `Alt+Right` to land on a network header, including an empty one.
- Press `Alt+Shift+Left` / `Alt+Shift+Right` on a focused header to collapse or expand that network. Click the header chevron for the same toggle.
- Press `Ctrl+Alt+Shift+Left` / `Ctrl+Alt+Shift+Right` to collapse or expand every network.
- Press `Alt+Shift+Up` / `Alt+Shift+Down` on a focused header to move that network.
- Press `Tab`, `Up`, or `Down` in the composer.
- Press `Ctrl+Shift+P` on a channel, then arrows and Enter.
- Press `Ctrl+Shift+S` to collapse or restore the left server list column.
- Press `Ctrl+W` on a direct message to close it.
- Press `Ctrl+,` to reopen Connect after a profile exists.
- Press `Page Up` / `Page Down` to scroll without leaving the composer.
- Press `Ctrl+F` to find in the current conversation or Status.

## Driving it with control-omairc

Preconditions:

- Conversation walk, unread jump, nick complete, history, member focus, `Ctrl+W` close, network collapse/reorder, and the sheet's Escape-vs-Status behavior need the seeded UI (`control-omairc launch --demo-server` or `qml-suite`, seeded `IrcController`). The demo has two networks (omarchy, then oftc).
- A default compiled window is titled `irc.libera.chat Status` with Connect. `Ctrl+/` still opens the sheet on that window. `Alt+Down` has nowhere to walk until a conversation exists.
- `Ctrl+,` opens Connect on `--demo-server` because a connection is bound. The suite also covers it with `test_openConnectSheetWithShortcut`.

- **Shortcut sheet on first run.** After `control-omairc launch`, run `control-omairc key --key ctrl+slash` then `control-omairc screenshot --feature keyboard --name after-ctrl-slash`. The sheet lists walk conversations, walk networks, collapse/expand network, collapse/expand all networks, move network, jump to conversation, jump to nick, unread, Status, Connect, members panel, focus members, server list, close direct message, composer, find, send, scroll, nick complete, history, Escape, this sheet, `/disconnect`, and quit. If the first send does not open the sheet, send `ctrl+slash` again. Run `control-omairc key --key Escape`. Connect stays open.
- **Walk, unread, complete, history, members, close.** Run `control-omairc doctor-qml` then `control-omairc qml-suite`. The suite covers `Alt+Down` / `Alt+Up` (including wrap, leaving Status, and skipping collapsed sections), `Alt+Left` / `Alt+Right` (including an empty header, Enter → Status, `Ctrl+,`, and walking a collapsed section's header), `Ctrl+K` jump (filter, Enter, Escape, duplicate `#omarchy`, Connect no-op, and expanding a collapsed network on activate), `Ctrl+Shift+K` nick jump (filter, Enter creating `mira`, Escape, hidden panel, Status and DM no-op, `ONLINE` heading), `Alt+A` (mention then unread, including expand-on-land), `Tab` on `mi` → `mira: `, Up/Down history, per-conversation composer drafts, `Ctrl+F` find, Page Up / Page Down, `Ctrl+Shift+P` plus Enter on `mira`, `Ctrl+W` closing a DM, and `Ctrl+/` Escape keeping the conversation and Status. The sheet lists `walk networks`, collapse/expand/move network, `Ctrl+K`, `Ctrl+Shift+K`, and `Ctrl+Shift+S`. On a fresh `launch --demo-server`, `control-omairc key --key alt+a` jumps to `#ricing` (mention 12). A second `alt+a` jumps to `anna`, who is still a mention until that DM is opened. Create `mira` from her member row, then `control-omairc key --key ctrl+w`. The title becomes `dax - Omairc` and the `mira` sidebar row is gone. Do not `click-conversation --name mira`; that helper only knows seeded rows.
- **Network collapse and reorder.** Offscreen, `qml-suite` covers `test_collapseFocusedNetworkWithShortcut`, `test_networkCollapseChordNeedsHeaderFocus`, `test_collapseAllNetworksWithoutHeaderFocus`, `test_altWalkSkipsCollapsedNetworkRows`, `test_jumpActivatesCollapsedNetworkConversation`, `test_unreadJumpExpandsCollapsedNetwork`, `test_moveFocusedNetworkWithShortcut`, and `test_collapsedSectionKeepsHeaderWalkAndServerListToggle`. On `--demo-server`, `control-omairc key --key alt+Right` focuses oftc, then `alt+shift+Left` hides that network's channels and DMs while the header stays. `alt+shift+Right` restores them. `ctrl+alt+shift+Left` collapses every section without header focus; `ctrl+alt+shift+Right` expands them. `alt+shift+Up` / `alt+shift+Down` swap omarchy and oftc while the same header stays focused. Click the header chevron (`networkCollapseButton-<id>`) for the mouse path. Header click still opens Status.
- **Connect chord.** The same suite run covers `Ctrl+,` on the fallback window with a connection. That does not prove the compiled-window chord after Apply.
- **Server list toggle.** On a seeded window run `control-omairc key --key ctrl+shift+s` then `control-omairc screenshot --feature keyboard --name server-list-collapsed`. The left column is gone and the transcript is full width with no leftover rail text. Press `ctrl+shift+s` again; the column returns with the networks and the identity footer. This is independent of per-network collapse. Offscreen, `doctor-qml` then `qml-suite` covers `test_toggleServerListWithShortcut` (writing `test-artifacts/server-list-collapsed.png` and `test-artifacts/server-list-restored.png`), `test_collapsedServerListKeepsWalking`, and `test_collapsingServerListDropsNetworkSelection`, and copies those two to `test-artifacts/verify/keyboard/`.

## Gotchas

- Status stays out of `Alt+Down` / `Alt+Up`. Opening Status then `Alt+Down` leaves Status for the next *visible* sidebar row.
- `Alt+Left` / `Alt+Right` do not change the open conversation. Enter on a focused header opens Status. `Ctrl+L` clears header focus so Enter sends again. Per-network collapse/expand/move need that header focus; collapse-all / expand-all do not.
- Per-network collapse (`Alt+Shift+Left`) is not `Ctrl+Shift+S`. The former hides that network's CHANNELS/DM rows and keeps the header. The latter collapses the whole left rail by width. They do not affect each other.
- `Alt+Down` / `Alt+Up` skip hidden rows under collapsed networks and do not auto-expand while walking. `Ctrl+K` still lists those conversations; activating one expands the network. `Alt+A` also considers hidden unread and expands on land.
- Unsent composer text stays with the conversation or Status. Switching away and back restores it. Status uses its own key, not the conversation name.
- `Ctrl+Shift+M` still toggles the panel. That is toggle-members. `Ctrl+Shift+P` focuses the list.
- `Ctrl+Shift+S` collapses the left column instead of hiding it, so `Alt+Down` / `Alt+Up` keep walking conversations while it is out of view. Collapsing drops the focused network header, so Enter in the composer sends again. `Alt+Left` / `Alt+Right` brings the column back because it highlights a rail row.
- `Ctrl+W` is disabled on channels and Status. Typed `/close` on a channel is a rejected slash command and stays in the composer.
- `Tab` completes a nick prefix in the composer. Prove it with `qml-suite` (`mi` → `mira: `).
- Page Up / Page Down are disabled while Connect is visible.
- `Alt+Down` / `Alt+Up`, `Alt+Left` / `Alt+Right`, `Alt+Shift+Left` / `Right` / `Up` / `Down`, `Ctrl+Alt+Shift+Left` / `Right`, `Alt+A`, `Ctrl+L`, `Ctrl+W`, `Ctrl+Shift+S`, and `Ctrl+`` are disabled while Connect is visible. Connect is a window-level modal; those chords must not walk servers or members behind it. `Ctrl+/` still opens the shortcuts overlay.
- `Ctrl+F` enters find even with an empty composer. It jumps the current transcript to the match and leaves follow-the-end so the match stays put. Escape restores the draft, not the old scroll position.
- Escape closes the sheet before Status. `Ctrl+/` toggles it.
- `Ctrl+K` is disabled while Connect is visible, the same rule as `Ctrl+F`. Duplicate channel names show the network display name.
- `Ctrl+Shift+K` is disabled on direct messages and Status. It does not open the member panel. Filter is a nick substring, same style as `Ctrl+K`. The sheet lists members in `PREFIX` rank order, including your own nick; Enter on yourself is a no-op.
- `control-omairc` maps `ctrl+slash` to `Control_L+slash` and `ctrl+shift+p` / `ctrl+shift+k` to `Control_L+Shift_L+p` / `k`. Per-network collapse uses `alt+shift+Left` → `Alt_L+Shift_L+Left` (and Right/Up/Down). Collapse-all uses `ctrl+alt+shift+Left` → `Control_L+Alt_L+Shift_L+Left`. xdotool's shorter tokens do not reach those Qt shortcuts on the isolated Xvfb.
- Create `mira` from her member row, then `Ctrl+W`. Do not `click-conversation --name mira`; that helper only knows seeded rows (`#omarchy`, `#desktop`, `#ricing`, `#help`, `anna`, `dax`).
- Do not Apply on the compiled window to "get a conversation" for walk proof. That starts a real session.
