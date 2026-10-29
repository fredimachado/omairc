# Omairc verification map

This directory is the maintained source for verifying the user-facing behavior of the Omairc desktop client and TUI. Read the index before driving the app, then use the matching feature file as the recipe.

Desktop window proof uses **verify-omairc-desktop**. The TUI uses **verify-omairc-tui**. The Qt `control-omairc` driver, `doctor-qml`, and `qml-suite` are retired; do not treat them as living paths.

## Baseline preconditions

- Prefer `.cursor/skills/verify-omairc-desktop/control-omairc-desktop launch` (Linux) or `control-omairc-desktop.ps1 launch` (Windows) for Connect, or the same with `--demo-server` for seeded conversations. The product binary is `client/target/*/omairc` / `omairc.exe` (debug by default; release + `product` for packager proof).
- Require `control-omairc-desktop` / `control-omairc-desktop.ps1 doctor` to report `ok isolated Omairc`, isolation owned by this run (`display=` on Linux, `localappdata=` / `placement=` on Windows), and a title that ends with ` - Omairc` or ` Status`.
- A default compiled window opens the connection sheet. Title is `{displayName} Status` until a conversation exists. `launch --demo-server` skips Connect and shows `#omarchy · irc.example · fred - Omairc`. Conversation recipes that assume that seeded window use `launch --demo-server`. Saved profiles do not connect unless `Connect automatically on startup` was enabled; Apply on the Connect sheet starts the live session. It does not restore the seeded sidebar.
- If desktop tools are missing (Linux: Xvfb, xdotool, lavapipe, ImageMagick; Windows: ImageMagick `magick`, cargo/MSVC), do not invent a fallback suite. Report the unmet precondition. Do not send input to the user's live window.
- Start every desktop recipe from that baseline unless its preconditions say otherwise.
- Never drive a window that this run did not start. The user's interactive `client/target/*/omairc` is off limits.

## Driving conventions

- Treat every command as literal. Keep quoted names and flags unchanged.
- Prefer chord verbs (`jump`, `walk`, `unread`, `toggle-members`, `focus-members`, `nick-jump`, `status`, `connect`, `composer`, `send`) over `click-conversation` / `click-member`.
- Pixel clicks (`click-version`, `click-people`, `click-edit`, `click-network`, `click-send`, and raw `--x/--y`) are the fallback for mouse-only chrome.
- `control-omairc-desktop` / `control-omairc-desktop.ps1 run <feature>` plays the `desktop-recipe` fence inside `## Driving it with control-omairc`. `run <feature> --dry-run` prints that fence and does not launch. Feature files keep that H2 title for map stability; the living drivers are `control-omairc-desktop` (Linux) and `control-omairc-desktop.ps1` (Windows).
- Desktop evidence lands under `test-artifacts/verify-desktop/`. `bin/verify-desktop-recipes`, `bin/verify-desktop-recipes.ps1`, and `bin/test-desktop` replay the same fences.
- Window title `{conversation} - Omairc` is the conversation identity. Duplicate names use `{conversation} · {displayName} - Omairc`.
- Named clicks assume the isolated 1180x760 window at textScale 1.0. They are invalid off that window.
- Restore the baseline conversation (`#omarchy`) after a mutation if another recipe will reuse the instance.
- Do not remove proof artifacts during cleanup.

## Proof and skip reporting

- Capture the user action and the resulting state, not only the final screen.
- Desktop proof includes the window title and a screenshot that shows Omairc, the selected conversation, and the changed region.
- Mutation proof for a sent message includes a second view after switching away and back.
- Record the feature ID and entry point used with every artifact.
- Report an unreachable path with the attempted command and the unmet precondition.
- Do not report a skipped entry point as verified through a different path.

## Feature entry contract

Each feature file starts with an H1 title and one paragraph describing the user-visible behavior. It then uses exactly four H2 sections in this order.

1. `Sub-features` lists short IDs with one line for each behavior.
2. `How to get to it (user POV)` lists every user entry point.
3. `Driving it with control-omairc` starts with `Preconditions:` and uses labeled bullets that pair each user action with an exact command and observable result. A `desktop-recipe` fence in that section (not a fifth H2) is the executable form `run <feature>` plays. Drive those commands with `control-omairc-desktop` / `control-omairc-desktop.ps1` (or `control-omairc-tui` where the TUI shares the fence).
4. `Gotchas` lists traps that can waste or invalidate a verification run.

Keep implementation details out of the map. Name only user paths, stable handles, required state, commands, and observable proof. Mentions of `qml-suite` / `doctor-qml` in older feature bullets are historical; they are not a fallback.

## Features

- [Connect](./connect.md) covers the first-run Connect sheet, Freenode defaults, and the nick-required state.
- [Switch conversation](./switch-conversation.md) covers sidebar channels, seeded direct messages, topic, people count, and message history.
- [Send a message](./send-message.md) covers composer focus, Enter, SEND, empty input, and `/me` actions.
- [File link](./file-link.md) covers inserting an ordinary web link when a server offers a file host.
- [Toggle members](./toggle-members.md) covers the people control, `Ctrl+Shift+M`, and hiding the panel on direct messages.
- [Open a direct message](./open-direct-message.md) covers opening or creating a DM from a member row and clearing unread state.
- [Status console](./status-console.md) covers the network Status pane, header entry points, AUTH notices, and the shared composer.
- [Member presence](./member-presence.md) covers presence dots, away dimming, status lines, capability gating, and live PREFIX ranks.
- [Identity footer](./identity-footer.md) covers the sidebar nick footer (`fred` on first run, then the connection or live nick), the app version on the right, and live available/away/offline chrome.
- [About](./about.md) covers the About sheet opened from that footer version, the GitHub source link, and Check for Updates.
- [Keyboard](./keyboard.md) covers conversation walk, network-header walk, per-network collapse and reorder, unread jump, `Ctrl+K` jump, `Ctrl+Shift+K` nick jump, nick complete, history, find, member focus, the `Ctrl+Shift+S` server list toggle, `Ctrl+W` close, the shortcut sheet, and `Ctrl+,`.
- [Slash complete](./slash-complete.md) covers the composer slash-command list for `/` plus a character, Tab insert, Escape dismiss, and Up/Down.
- [Typing](./typing.md) covers bouncing ellipsis on channel members and in direct messages, plus the `message-tags` gate.
- [Slash commands](./slash-commands.md) covers live catalog verbs from the composer, including `/kick`, `/kickban`, ban and quiet list verbs, and `/ignore`. Mock treats them as chat except `/me `.
- [Mention notify](./mention-notify.md) covers the desktop notification for an unfocused mention or direct message.
- [Title mention](./title-mention.md) covers the window title for an unfocused mention or direct message.
- [Open links](./open-links.md) covers clicking allowlisted `http`/`https` URLs in transcripts and Status.
- [Open a channel name](./open-channel-name.md) covers a channel name in a message or a topic. An open buffer switches. A missing buffer asks before it joins.
