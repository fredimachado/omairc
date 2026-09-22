# Keyboard

Keyboard is the window chord map: walk conversations, walk network headers, collapse or reorder those networks, jump to a named conversation, jump to a channel nick, jump unread, complete nicks, recall sent lines, find in the buffer, focus members, toggle the left server list column, close a direct message, and show this list. Clicks still work. Status is not a sidebar row.

## Sub-features

- `keyboard-walk` moves CHANNELS then DIRECT MESSAGES with `Alt+Down` / `Alt+Up`, wrapping at both ends. Status is not in that list. Rows under a collapsed network are skipped; the walk does not expand them. If the current conversation is hidden, walking continues from its place in the full sidebar order and lands on the next or previous visible row, wrapping among visible rows only. If nothing is visible, the chord is a no-op.
- `keyboard-networks` moves network headers with `Alt+Left` / `Alt+Right`, wrapping at both ends. A network with no conversations stays in that walk. Headers stay in this walk when their section is collapsed. Enter opens that network's Status. `Ctrl+,` opens Connect for that header. Restores the server list column when it is collapsed.
- `keyboard-network-collapse` collapses or expands the focused network with `Alt+Shift+Left` / `Alt+Shift+Right`. The header stays; CHANNELS, DIRECT MESSAGES, and their rows hide (height 0). A small chevron on the header toggles the same state on click. Header click and Enter still open Status. Unread and mention marks stay on the collapsed header. These chords need a focused network header (`sidebarNetworkFocusId`); they no-op in the composer.
- `keyboard-network-collapse-all` collapses or expands every network with `Ctrl+Alt+Shift+Left` / `Ctrl+Alt+Shift+Right`. Header focus is not required. Collapse-all does not steal header focus.
- `keyboard-network-move` reorders the focused network with `Alt+Shift+Up` / `Alt+Shift+Down`. No wrap at the ends. Focus stays on the moved network. Needs a focused header. No drag handles.
- `keyboard-jump` opens a filter overlay with `Ctrl+K`. It lists channels, direct messages, and each network's Status, including conversations under collapsed networks. Type filters the list. Up/Down highlight, Enter jumps, Escape dismisses and returns to the composer. Activating a conversation expands that network then selects it. It is a no-op while Connect is visible.
- `keyboard-nick` opens a filter overlay with `Ctrl+Shift+K` on a channel. It lists that channel's members in panel order. Type filters by nick substring. Up/Down highlight, Enter opens or creates that DM, Escape dismisses and returns to the composer. The member panel `ONLINE` heading opens the same sheet. It is a no-op on a direct message or Status, and it works with the member panel hidden.
- `keyboard-unread` jumps to the next unread with `Alt+A`, mentions first. Muted conversations are skipped while hunting mentions. Hidden conversations under collapsed networks still count; landing expands that network.
- `keyboard-inbox` toggles the session inbox sheet with `Ctrl+Shift+A`. Up/Down walk rows, Enter activates, Escape dismisses and returns to the composer. It is a no-op while Connect is visible.
- `keyboard-complete` completes a nick prefix in the composer with `Tab`.
- `keyboard-history` recalls sent lines with `Up` / `Down` and restores a typed draft on `Down`.
- `keyboard-drafts` keeps unsent composer text per conversation and Status. Switching targets restores that draft. It does not follow you.
- `keyboard-members` focuses the member list with `Ctrl+Shift+P` and reopens a hidden panel. Enter on a focused member opens a DM.
- `keyboard-server-list` collapses and restores the left server list column with `Ctrl+Shift+S`. That is the whole rail, not a per-network section. Collapsing is not hiding: `Alt+Down` / `Alt+Up` still walk conversations while the column is collapsed. It also drops the focused network header, so Enter in the composer sends again. `Alt+Left` / `Alt+Right` restores the column so the header highlight is visible.
- `keyboard-close` closes the selected direct message with `Ctrl+W` and selects the next DM, or the previous row when that was the last. It is a no-op on a channel, Status, or the shortcut sheet.
- `keyboard-sheet` toggles the shortcut list with `Ctrl+/`. Escape closes the sheet before Status. The list includes `Ctrl+W`, `Ctrl+F`, `Ctrl+K`, `Ctrl+Shift+K`, `Ctrl+Shift+A`, `Ctrl+Shift+S`, per-network collapse/expand/move, and collapse-all / expand-all. `Ctrl+Q` still quits the process.
- `keyboard-connect` opens Connect with `Ctrl+,` when a connection exists.
- `keyboard-scroll` pages the visible transcript with `Page Up` / `Page Down` while the composer stays focused. `Shift+Page Up` / `Shift+Page Down` hop about half as far. `Ctrl+Home` / `Ctrl+End` jump to the top / bottom.
- `keyboard-find` finds text in the current conversation or Status with `Ctrl+F`. Empty first press enters find and waits. The composer shows Find and holds the query. Matches include author and body, or Status label and text. The current row uses the selection color. Enter or another `Ctrl+F` goes to the next match and wraps. Escape leaves find and restores the unsent draft.

## How to get to it (user POV)

- Press `Ctrl+/` to read the list in the app.
- Press `Ctrl+K` to jump to a channel, direct message, or Status.
- Press `Ctrl+Shift+K` on a channel to jump to a nick and open or create that DM. Click `ONLINE` in the member panel for the same sheet.
- Press `Alt+Down` / `Alt+Up` or `Alt+A` while a conversation exists.
- Press `Ctrl+Shift+A` to open or close the session inbox sheet.
- Press `Alt+Left` / `Alt+Right` to land on a network header, including an empty one.
- Press `Alt+Shift+Left` / `Alt+Shift+Right` on a focused header to collapse or expand that network. Click the header chevron for the same toggle.
- Press `Ctrl+Alt+Shift+Left` / `Ctrl+Alt+Shift+Right` to collapse or expand every network.
- Press `Alt+Shift+Up` / `Alt+Shift+Down` on a focused header to move that network.
- Press `Tab`, `Up`, or `Down` in the composer.
- Press `Ctrl+Shift+P` on a channel, then arrows and Enter.
- Press `Ctrl+Shift+S` to collapse or restore the left server list column.
- Press `Ctrl+W` on a direct message to close it.
- Press `Ctrl+,` to reopen Connect after a profile exists.
- Press `Page Up` / `Page Down` to scroll without leaving the composer. `Shift+Page Up` / `Shift+Page Down` hop about half a page. `Ctrl+Home` / `Ctrl+End` jump to the top / bottom.
- Press `Ctrl+F` to find in the current conversation or Status.

## Driving it with control-omairc

Preconditions:

- Conversation walk, unread jump, nick complete, history, member focus, `Ctrl+W` close, network collapse/reorder, and the sheet's Escape-vs-Status behavior need the seeded UI (`control-omairc launch --demo-server`). When Xvfb tools are missing, `qml-suite` (seeded `IrcController`) is the fallback and is not compiled-window proof. The demo has two networks (omarchy, then oftc).
- A default compiled window is titled `irc.libera.chat Status` with Connect. `Ctrl+/` still opens the sheet on that window. `Alt+Down` has nowhere to walk until a conversation exists. That launch is not this fence.
- `Ctrl+,` opens Connect on `--demo-server` because a connection is bound. The offscreen suite also covers it with `test_openConnectSheetWithShortcut`. That suite is not compiled-window proof.
- `unread` on a fresh demo lands on `#ricing`. A later `unread` does not. This fence needs `cleanup` before `run` when the demo is not fresh.

```desktop-recipe
launch --demo-server
wait-title --exact "#omarchy · irc.example · fred - Omairc"
unread
wait-title --exact "#ricing - Omairc"
screenshot --feature keyboard --name after-unread
jump --query "#desktop"
wait-title --exact "#desktop - Omairc"
nick-jump --query mira
wait-title --exact "mira - Omairc"
key --key ctrl+w
wait-title --exact "dax - Omairc"
screenshot --feature keyboard --name after-close
key --key ctrl+shift+s
screenshot --feature keyboard --name server-list-collapsed
key --key ctrl+shift+s
screenshot --feature keyboard --name server-list-restored
```

- **Unread, jump, nick jump, close.** On a fresh `launch --demo-server`, press `Alt+A`. Run `control-omairc unread` then `control-omairc wait-title --exact "#ricing - Omairc"`. Capture it with `control-omairc screenshot --feature keyboard --name after-unread`. `#ricing` starts with mention `12`. Press `Ctrl+K` and jump to `#desktop`. Run `control-omairc jump --query "#desktop"` then `control-omairc wait-title --exact "#desktop - Omairc"`. Press `Ctrl+Shift+K`, type `mira`, and press Enter. Run `control-omairc nick-jump --query mira` then `control-omairc wait-title --exact "mira - Omairc"`. Press `Ctrl+W`. Run `control-omairc key --key ctrl+w` then `control-omairc wait-title --exact "dax - Omairc"`. The `mira` sidebar row is gone. Sidebar order is alphabetical, so `mira` is last among the omarchy direct messages and close selects `dax`. Capture that with `control-omairc screenshot --feature keyboard --name after-close`.
- **Server list.** Press `Ctrl+Shift+S`. Run `control-omairc key --key ctrl+shift+s` then `control-omairc screenshot --feature keyboard --name server-list-collapsed`. The left column is gone and the transcript is full width. Press `Ctrl+Shift+S` again. Run `control-omairc key --key ctrl+shift+s` then `control-omairc screenshot --feature keyboard --name server-list-restored`. The column returns with the networks and the identity footer. This is independent of per-network collapse.
- **Chords with no verb.** `ctrl+w` and `ctrl+shift+s` are in the fence. These stay `key --key` and are not in the fence: `ctrl+slash`, `ctrl+shift+a`, `alt+Left`, `alt+Right`, `alt+shift+Left`, `alt+shift+Right`, `alt+shift+Up`, `alt+shift+Down`, `ctrl+alt+shift+Left`, `ctrl+alt+shift+Right`, `Escape`, `Tab`, `Page_Up`, `Page_Down`, and `ctrl+f`. Named verbs cover the rest: `walk`, `unread`, `jump`, `nick-jump`, `focus-members`, `status`, `connect`, and `composer`.
- **Shortcut sheet on first run.** After `control-omairc launch` (no `--demo-server`), `control-omairc key --key ctrl+slash` opens the sheet and `control-omairc key --key Escape` leaves Connect open. That is a different launch from this fence. Do not put both launches in one recipe.
- **Mouse path.** The header chevron (`networkCollapseButton-<id>`) toggles one network the way `Alt+Shift+Left` / `Alt+Shift+Right` do. There is no chord helper for that pixel. It is not this recipe.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. The suite covers walk, network headers, collapse and reorder, `Ctrl+K`, `Ctrl+Shift+K`, `Ctrl+Shift+A`, `Alt+A`, nick complete, history, drafts, find, paging, `Ctrl+Shift+P`, `Ctrl+W`, and `Ctrl+/`. It also covers `test_toggleServerListWithShortcut`. This is not compiled-window proof.

## Gotchas

- `run keyboard` skips `launch` when `doctor` is already healthy and `demo=yes`. It does not reset unread badges or created direct messages. `unread` lands on `#ricing` only on a fresh demo. Recipes that need that landing need `cleanup` before `run`.
- Status stays out of `Alt+Down` / `Alt+Up`. Opening Status then `walk --down` leaves Status for the next visible sidebar row.
- `Alt+Left` / `Alt+Right` do not change the open conversation. Enter on a focused header opens Status. `Ctrl+L` clears header focus so Enter sends again. Per-network collapse/expand/move need that header focus; collapse-all / expand-all do not.
- Per-network collapse (`Alt+Shift+Left`) is not `Ctrl+Shift+S`. The former hides that network's CHANNELS/DM rows and keeps the header. The latter collapses the whole left rail by width. They do not affect each other.
- `Alt+Down` / `Alt+Up` skip hidden rows under collapsed networks and do not auto-expand while walking. `Ctrl+K` still lists those conversations; activating one expands the network. `Alt+A` also considers hidden unread and expands on land.
- Unsent composer text stays with the conversation or Status. Switching away and back restores it. Status uses its own key, not the conversation name.
- `Ctrl+Shift+M` still toggles the panel. That is toggle-members. `Ctrl+Shift+P` focuses the list.
- `Ctrl+Shift+S` collapses the left column instead of hiding it, so `Alt+Down` / `Alt+Up` keep walking conversations while it is out of view. Collapsing drops the focused network header, so Enter in the composer sends again. `Alt+Left` / `Alt+Right` brings the column back because it highlights a rail row.
- `Ctrl+W` is disabled on channels and Status. Typed `/close` on a channel is a rejected slash command and stays in the composer.
- `Tab` completes a nick prefix in the composer. Prove it with `qml-suite` (`mi` → `mira: `).
- Page Up / Page Down, Shift+Page Up / Shift+Page Down, and Ctrl+Home / Ctrl+End are disabled while Connect is visible or the shortcuts overlay is open. Plain Home / End stay composer caret (and Connect's network rail).
- `Alt+Down` / `Alt+Up`, `Alt+Left` / `Alt+Right`, `Alt+Shift+Left` / `Right` / `Up` / `Down`, `Ctrl+Alt+Shift+Left` / `Right`, `Alt+A`, `Ctrl+Shift+A`, `Ctrl+L`, `Ctrl+W`, `Ctrl+Shift+S`, and `Ctrl+`` are disabled while Connect is visible. Connect is a window-level modal; those chords must not walk servers or members behind it. `Ctrl+/` still opens the shortcuts overlay.
- `Ctrl+F` enters find even with an empty composer. It jumps the current transcript to the match and leaves follow-the-end so the match stays put. Escape restores the draft, not the old scroll position.
- Escape closes the sheet before Status. `Ctrl+/` toggles it.
- `Ctrl+K` is disabled while Connect is visible, the same rule as `Ctrl+F`. Duplicate channel names show the network display name.
- `Ctrl+Shift+K` is disabled on direct messages and Status. It does not open the member panel. Filter is a nick substring, same style as `Ctrl+K`. The sheet lists members in `PREFIX` rank order, including your own nick; Enter on yourself is a no-op.
- `control-omairc` maps `ctrl+slash` to `Control_L+slash` and `ctrl+shift+p` / `ctrl+shift+k` to `Control_L+Shift_L+p` / `k`. Per-network collapse uses `alt+shift+Left` → `Alt_L+Shift_L+Left` (and Right/Up/Down). Collapse-all uses `ctrl+alt+shift+Left` → `Control_L+Alt_L+Shift_L+Left`. xdotool's shorter tokens do not reach those Qt shortcuts on the isolated Xvfb.
- `nick-jump --query mira` creates `mira`, then `key --key ctrl+w` selects `dax - Omairc` and drops the `mira` row. `click-conversation` does not know `mira`.
- Do not Apply on the compiled window to "get a conversation" for walk proof. That starts a real session.
