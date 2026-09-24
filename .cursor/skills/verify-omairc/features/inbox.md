# Session inbox

The session inbox is a waiting list of background arrivals: mentions, highlight words, channel invites, monitored nicks coming online, and self-kicks. Plain direct messages do not append rows; mentions and highlight hits in a direct message still do. It lives in the identity footer as an `inbox` mark with a count badge. `Ctrl+Shift+A` opens a sheet to walk and activate rows. Opening the matching conversation from the sidebar consumes related rows and clears the badge when nothing remains.

## Sub-features

- `inbox-mark` shows a count badge only when `irc.inboxCount` is greater than zero. Clicking the mark opens the sheet.
- `inbox-sheet` toggles with `Ctrl+Shift+A`. Up/Down walk rows, Enter activates, Delete dismisses the selected row without navigating, Escape closes the sheet and returns focus to the composer. Each row also has a `delete` control. Connect blocks the chord.
- `inbox-mention-activate` on a mention or highlight row reveals that conversation and scrolls to the stored `msgid` when present.
- `inbox-invite-activate` on an invite row sends `JOIN` for that channel on the stored network.
- `inbox-consume` removes conversation rows when that conversation is selected from the sidebar, invite rows after a matching self-join, and monitor rows after opening the watched nick as a direct message.

## How to get to it (user POV)

- Receive a mention, highlight, invite, monitor online edge, or self-kick while another conversation is open. A mention or highlight inside a direct message counts too.
- Look at the identity footer. When the count is non-zero, click `inbox` or press `Ctrl+Shift+A`.
- Press Enter on a row to jump, join, or open as appropriate.

## Driving it with control-omairc

Preconditions:

- Use the seeded UI (`control-omairc launch --demo-server` or `qml-suite`). The demo seeds live conversations but does not produce new inbox rows after launch unless you inject frames or switch away first. Do not inject frames for this proof.

- **No compiled-window recipe.** `control-omairc run inbox` fails closed. This file has no `desktop-recipe` fence. Do not add an empty fence or a `qml-suite` fence. `Ctrl+Shift+A` can open an empty sheet on `--demo-server`, and that is not the waiting list.
- **Offscreen suite.** Run `control-omairc doctor-qml` then `control-omairc qml-suite`. `test_inboxSheetTogglesWithShortcut` covers `Ctrl+Shift+A` toggle, Escape, and Connect no-op. `test_inboxMarkShowsCountAndOpensSheet` covers the footer badge and click path. `test_inboxMentionEnterJumpsToMsgid` and `test_inboxInviteEnterJoinsChannel` cover Enter activation. `test_inboxSelectingConversationClearsMatchingRows` covers sidebar consume. `qml-suite` is not compiled-window proof.

## Gotchas

- The inbox is separate from unread badges, mention badges, and desktop notifications. A muted conversation still arrives in chat but does not append inbox rows.
- Hydration and replay do not append inbox rows; only live edges do, except invites recorded from incoming `INVITE` and monitor online edges after hydration.
- Replacing invite or monitor rows keeps one row per channel or watched nick on the same network.
- The inbox caps at fifty rows; older rows drop from the tail.
