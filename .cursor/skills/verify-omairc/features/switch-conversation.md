# Switch conversation

Switch conversation lets a user leave the current mock chat and open another channel or seeded direct message from the sidebar, with the header, topic, people count, and transcript changing together.

## Sub-features

- `switch-channel` opens `#desktop`, `#ricing`, and `#help` from the CHANNELS list.
- `switch-seeded-dm` opens the seeded `anna` and `dax` direct messages.
- `switch-topic` updates the header topic for the selected conversation.
- `switch-people` shows a people count only on channels.
- `switch-unread` clears a channel row's unread badge when that channel is opened.

## How to get to it (user POV)

- Click `#omarchy`, `#desktop`, `#ricing`, or `#help` under CHANNELS.
- Click `anna` or `dax` under DIRECT MESSAGES.
- Press `Alt+Down` / `Alt+Up` to walk CHANNELS then DIRECT MESSAGES. Press `Alt+A` for the next unread (mentions first, skipping muted).

## Driving it with control-omairc

Preconditions:

- Seeded conversation UI is showing (`#omarchy · irc.example · fred - Omairc`, sidebar includes `#desktop` and `anna`). Use `control-omairc launch --demo-server`. When Xvfb tools are missing, `qml-suite` (seeded `IrcController`) is the fallback and is not compiled-window proof.
- `control-omairc launch` without `--demo-server` is first-run Connect titled `irc.libera.chat Status`. Do not start this recipe there.
- For a desktop instance titled `#omarchy · irc.example · fred - Omairc`, `control-omairc doctor` must report `demo=yes`, the disposable XDG directory, and the default-sized window.

```desktop-recipe
launch --demo-server
wait-title --exact "#omarchy · irc.example · fred - Omairc"
screenshot --feature switch-conversation --name before-switch
walk --down
wait-title --exact "#ricing - Omairc"
unread
wait-title --exact "anna - Omairc"
jump --query "#desktop"
wait-title --exact "#desktop - Omairc"
jump --query "#ricing"
wait-title --exact "#ricing - Omairc"
jump --query "#help"
wait-title --exact "#help - Omairc"
jump --query "#omarchy"
wait-title --exact "#omarchy · irc.example · fred - Omairc"
jump --query anna
wait-title --exact "anna - Omairc"
jump --query dax
wait-title --exact "dax - Omairc"
jump --query "#desktop"
wait-title --exact "#desktop - Omairc"
screenshot --feature switch-conversation --name after-desktop
compare --before test-artifacts/verify/switch-conversation/before-switch.png --after test-artifacts/verify/switch-conversation/after-desktop.png
```

- **Capture the start conversation.** Stay on the demo's omarchy `#omarchy`. Run `control-omairc wait-title --exact "#omarchy · irc.example · fred - Omairc"` then `control-omairc screenshot --feature switch-conversation --name before-switch`. The title is `#omarchy · irc.example · fred - Omairc` because both networks have `#omarchy`.
- **Walk to #ricing.** Press `Alt+Down` once. Run `control-omairc walk --down` then `control-omairc wait-title --exact "#ricing - Omairc"`. From that fresh start title, one step lands on `#ricing`. The topic is `Themes, type, wallpapers, and the tiny details.` and the people control reads `10 PEOPLE`.
- **Unread to anna.** Press `Alt+A`. Run `control-omairc unread` then `control-omairc wait-title --exact "anna - Omairc"`. After `#ricing` is open, the next unread is `anna`. The topic is `Direct message with anna`. The people control and member panel are absent.
- **Open #desktop.** Press `Ctrl+K` and jump to `#desktop`. Run `control-omairc jump --query "#desktop"` then `control-omairc wait-title --exact "#desktop - Omairc"`. The header topic is `Desktops should feel personal, fast, and calm.`, and the people control reads `8 PEOPLE`.
- **Open #ricing.** Jump to `#ricing`. Run `control-omairc jump --query "#ricing"` then `control-omairc wait-title --exact "#ricing - Omairc"`. The topic is `Themes, type, wallpapers, and the tiny details.` and the people control reads `10 PEOPLE`.
- **Open #help.** Jump to `#help`. Run `control-omairc jump --query "#help"` then `control-omairc wait-title --exact "#help - Omairc"`. The topic is `Ask a clear question. Share what you already tried.` and the people control reads `5 PEOPLE`.
- **Return to #omarchy.** Jump to `#omarchy`. Run `control-omairc jump --query "#omarchy"` then `control-omairc wait-title --exact "#omarchy · irc.example · fred - Omairc"`. The query is a substring of the jump label. The first sidebar match is the omarchy network, not the OFTC one. The topic is `A cozy corner for Omarchy users and builders.` and the people control reads `12 PEOPLE`.
- **Open anna.** Jump to `anna`. Run `control-omairc jump --query anna` then `control-omairc wait-title --exact "anna - Omairc"`. The topic is `Direct message with anna`. The people control and member panel are absent.
- **Open dax.** Jump to `dax`. Run `control-omairc jump --query dax` then `control-omairc wait-title --exact "dax - Omairc"`. The topic is `Direct message with dax`.
- **Proof.** Jump back to `#desktop` and capture the channel state. Run `control-omairc jump --query "#desktop"`, `control-omairc wait-title --exact "#desktop - Omairc"`, `control-omairc screenshot --feature switch-conversation --name after-desktop`, and `control-omairc compare --before test-artifacts/verify/switch-conversation/before-switch.png --after test-artifacts/verify/switch-conversation/after-desktop.png`. The screenshot shows `#desktop` selected, topic `Desktops should feel personal, fast, and calm.`, `8 PEOPLE`, and the `#desktop` unread badge gone. `compare` must report a pixel change.
- **Mouse fallback.** `control-omairc click-conversation --name` still aims the seeded rows (`#desktop`, `#help`, `#omarchy`, `#ricing`, `anna`, `dax`) when a chord cannot. It is not this recipe.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` clicks `conversation-omarchy-#desktop` and writes `test-artifacts/switch-channel.png`. `qml-suite` copies it to `test-artifacts/verify/switch-conversation/after-desktop.png`. The image must show `#desktop`, the desktop topic, and `8 PEOPLE`. This is not compiled-window proof.

## Gotchas

- `#desktop` starts with unread `3` and `#ricing` with mention `12`. Opening the channel clears that row. The before screenshot is still the start conversation, so prove the `#desktop` badge is gone on `after-desktop`, not on `before-switch`.
- From a fresh `#omarchy · irc.example · fred - Omairc`, `walk --down` lands on `#ricing - Omairc`. The following `unread` lands on `anna - Omairc`. Opening `#ricing` clears its mention, so a later `unread` does not return there.
- Direct messages do not show a people count or member panel. Do not treat a missing panel as a toggle failure.
- Sidebar labels render as `#  omarchy` without repeating the hash in the name string. Assert the window title (`#omarchy · irc.example · fred - Omairc`), not the clipped label text. Do not assert the stale title `#omarchy - Omairc` while both networks have `#omarchy`.
- `Alt+Down` / `Alt+Up` / `Alt+A` are keyboard. Status is not in that walk. A default compiled launch has no sidebar rows. `qml-suite` is not compiled-window proof.
