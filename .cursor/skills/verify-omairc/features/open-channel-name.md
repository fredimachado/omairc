# Open a channel name

A channel name in a message or a topic opens inside the client. A buffer that is already open switches to it. A channel that is not already a row asks first. Enter joins it. Escape cancels, and nothing is joined until then.

## Sub-features

- `channel-switch` switches to a channel that is already a sidebar row.
- `channel-ask` asks before joining a channel that is not already a row.
- `channel-topic` does the same for a channel name in the topic.
- `channel-types` recognizes only prefixes from the network's advertised channel types.

## How to get to it (user POV)

- Click a channel name in a chat line or a whois line.
- Click a channel name in the topic.
- Press `Ctrl+Shift+O` and choose a channel name from the current conversation or its topic. Enter on a row that is already open switches. Enter on a row that is not open asks. Enter on the ask joins. Escape cancels the ask.
- An `http` or `https` URL still opens in the browser. An invite on Status still joins without this ask. `irc` and `ircs` links do nothing here.

## Driving it with control-omairc

Preconditions:

- The seeded network advertises `CHANTYPES=#`. A compiled window sends `JOIN` only after the ask is confirmed. Do not use a live network as proof.
- `control-omairc launch` without `--demo-server` is first-run Connect. Do not start here.

- **No executable recipe yet.** `run open-channel-name` fails closed. This file has no `desktop-recipe` fence. Do not add an empty fence.
- **What a user sees.** On the seeded network, `#desktop` is a channel name and `&local` is not. Clicking `#Desktop` in a message switches to the open `#desktop` buffer and sends no `JOIN`. Clicking `#brand-new` asks first. Escape cancels and joins nothing. Enter on the ask sends `JOIN`. A channel name in the topic and a channel row in the `Ctrl+Shift+O` sheet follow the same switch-or-ask rule.

## Gotchas

- The prefix comes from the network's `CHANTYPES`. A `#` name is not a channel when the server did not advertise `#`.
- A name glued to a word, or a `#` inside an `http` or `https` URL, is not a channel.
- A trailing quote or an unmatched `>` is outside the name. A PREFIX rank such as `@` in front of the name is outside the span too, and the rank comes from the server, not a hard-coded `@`.
- Status lines are not this path. An invite still joins from its own row.
- Confirming the ask joins. Cancelling it, or clicking outside the ask, does not.
