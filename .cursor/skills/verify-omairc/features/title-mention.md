# Title mention

Title mention is the window title Omairc shows when a mention or a direct message arrives and the window is not focused. The title names the author and the text with IRC formatting stripped, then the conversation. A focused window keeps the open conversation, or that network's Status. A muted conversation stays on that plain title. Focusing the window, or opening the conversation the title names, clears it.

## Sub-features

- `title-unfocused` replaces the window title with an unfocused mention.
- `title-dm-unfocused` replaces the window title with an unfocused direct message, including one that does not contain the nick.
- `title-focused` leaves the title unchanged when the window is already focused.
- `title-muted` leaves the title unchanged for a muted channel or direct message.
- `title-plain` strips IRC formatting from the title (`hey \x02fred` becomes `hey fred`).
- `title-clear-focus` restores the open conversation or Status when the window gains focus.
- `title-clear-open` restores the plain title when that conversation is opened.
- `title-invite-quiet` leaves the title unchanged for an invite. The invite stays in the session inbox.

## How to get to it (user POV)

- Leave the Omairc window unfocused during a live session. Be mentioned in a channel, or receive a direct message, including one that does not contain the nick. The window title shows that line.
- Keep the window focused. The same mention or direct message leaves the title on the open conversation or Status.
- Mute the conversation. A later mention or direct message there leaves the title unchanged. Chat still arrives.
- Focus the window. The title returns to the open conversation or Status.
- While the window is still unfocused, open the conversation the title names. The title returns to that conversation.

## Driving it with control-omairc

Preconditions:

- Live `title-unfocused` needs a completed Connect, a registered session, and an unfocused window. Do not Apply on the compiled first-run window for this proof.
- `control-omairc launch` without `--demo-server` is first-run Connect titled `irc.libera.chat Status`. That window has no session and no mentions.
- `--demo-server` has no new inbound mentions after launch. There is no compiled-window title recipe there.

- **No compiled-window recipe.** `control-omairc run title-mention` fails closed. This file has no `desktop-recipe` fence. Do not add an empty fence or a `qml-suite` fence.
- **Offscreen suite.** Run `control-omairc doctor-qml` then `control-omairc qml-suite`. `test_unfocusedTitleMarksMention` calls `noteUnfocusedTitle(false, "alice", "hey \x02fred")` on `#ricing` and expects the title `alice: hey fred · #ricing - Omairc`, expects a focused call to leave the open title, expects opening `#ricing` and `windowFocusGained()` to clear it, and expects a direct message from `dax` to title `dax: hello there - Omairc`. `test_focusedArrivalLeavesTitle` injects a live mention while the window is active and expects the open title to stay. The offscreen window cannot be deactivated, so the unfocused live arrival is the direct `noteUnfocusedTitle(false, ...)` call. `qml-suite` is not compiled-window proof.
- **Live desktop title.** Do not claim this on first-run Connect or `--demo-server`. It is `verified-unreachable` until a session has completed Connect and the isolated window is unfocused.

## Gotchas

- Desktop notifications are a separate feature. This one is only the window title. The terminal client uses the same words in the title it already sets.
- A muted conversation does not change the title. Chat in that conversation still arrives.
- An invite does not change the title. It stays in the session inbox.
- The plain title is still `{conversation} - Omairc`, `{conversation} · {displayName} - Omairc` when that name is on more than one network, or `{displayName} Status`.
