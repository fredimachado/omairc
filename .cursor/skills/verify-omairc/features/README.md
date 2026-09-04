# Omairc verification map

This directory is the maintained source for verifying the user-facing behavior of the Omairc desktop prototype. Read the index before driving the app, then use the matching feature file as the recipe.

## Baseline preconditions

- Prefer `.cursor/skills/verify-omairc/control-omairc launch` when Xvfb and xdotool are installed.
- Require `control-omairc doctor` to report `ok isolated Omairc`, display owned by this run, and a title that ends with ` - Omairc` or ` Status`.
- A fresh compiled window opens the connection sheet. Title is `{displayName} Status` until a conversation exists. Conversation recipes that assume mock `#omarchy` must use `qml-suite` (`irc` left null). Seeding a profile starts a live session; it does not restore the mock sidebar.
- If desktop tools are missing, require `control-omairc doctor-qml` and drive with `control-omairc qml-suite`. Do not send input to the user's live window.
- Start every desktop recipe from that baseline unless its preconditions say otherwise.
- Never drive a window that this run did not start. The user's interactive `./build/omairc` is off limits.

## Driving conventions

- Treat every command as literal. Keep quoted names and flags unchanged.
- Prefer named helper targets (`click-conversation`, `click-member`, `focus-composer`) over raw `--x/--y`.
- Window title `{conversation} - Omairc` is the conversation identity.
- Named clicks assume the isolated 1180x760 window at textScale 1.0.
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
3. `Driving it with control-omairc` starts with `Preconditions:` and uses labeled bullets that pair each user action with an exact command and observable result.
4. `Gotchas` lists traps that can waste or invalidate a verification run.

Keep implementation details out of the map. Name only user paths, stable handles, required state, commands, and observable proof.

## Features

- [Connect](./connect.md) covers the first-run Connect sheet, Libera defaults, and the nick-required state.
- [Switch conversation](./switch-conversation.md) covers sidebar channels, seeded direct messages, topic, people count, and message history.
- [Send a message](./send-message.md) covers composer focus, Enter, SEND, empty input, and `/me` actions.
- [Toggle members](./toggle-members.md) covers the people control, `Ctrl+Shift+M`, and hiding the panel on direct messages.
- [Open a direct message](./open-direct-message.md) covers opening or creating a DM from a member row and clearing unread state.
- [Status console](./status-console.md) covers the network Status pane, header entry points, AUTH notices, and the shared composer.
