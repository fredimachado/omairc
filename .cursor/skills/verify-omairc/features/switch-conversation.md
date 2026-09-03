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

## Driving it with control-omairc

Preconditions:

- Isolated Omairc is healthy and titled `#omarchy - Omairc`.
- `control-omairc doctor` reports the disposable XDG directory and default-sized window.

- **Open #desktop.** Choose the `#desktop` channel. Run `control-omairc click-conversation --name "#desktop"` then `control-omairc wait-title --exact "#desktop - Omairc"`. The title is `#desktop - Omairc`, the header topic is `Desktops should feel personal, fast, and calm.`, and the people control reads `8 PEOPLE`.
- **Open #ricing.** Choose the `#ricing` channel. Run `control-omairc click-conversation --name "#ricing"` then `control-omairc wait-title --exact "#ricing - Omairc"`. The topic is `Themes, type, wallpapers, and the tiny details.` and the people control reads `10 PEOPLE`.
- **Open #help.** Choose the `#help` channel. Run `control-omairc click-conversation --name "#help"` then `control-omairc wait-title --exact "#help - Omairc"`. The topic is `Ask a clear question. Share what you already tried.` and the people control reads `5 PEOPLE`.
- **Return to #omarchy.** Choose `#omarchy`. Run `control-omairc click-conversation --name "#omarchy"` then `control-omairc wait-title --exact "#omarchy - Omairc"`. The topic is `A cozy corner for Omarchy users and builders.` and the people control reads `12 PEOPLE`.
- **Open anna.** Choose `anna` under DIRECT MESSAGES. Run `control-omairc click-conversation --name anna` then `control-omairc wait-title --exact "anna - Omairc"`. The topic is `Direct message with anna`. The people control and member panel are absent.
- **Open dax.** Choose `dax`. Run `control-omairc click-conversation --name dax` then `control-omairc wait-title --exact "dax - Omairc"`. The topic is `Direct message with dax`.
- **Proof.** Return to `#desktop` and capture the channel state. Run `control-omairc click-conversation --name "#desktop"`, `control-omairc wait-title --exact "#desktop - Omairc"`, and `control-omairc screenshot --feature switch-conversation --name after-desktop`. The screenshot shows `#desktop` selected, topic about desktops, `8 PEOPLE`, and the `#desktop` unread badge gone.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` clicks `conversation-#desktop` and writes `test-artifacts/verify/switch-conversation/after-desktop.png`. The image must show `#desktop`, the desktop topic, and `8 PEOPLE`. This does not prove the compiled-window click path.

## Gotchas

- `#desktop` starts with unread `3` and `#ricing` with mention `12`. Opening the channel clears that row. Prove the badge is gone after the click, not before.
- Direct messages do not show a people count or member panel. Do not treat a missing panel as a toggle failure.
- Sidebar labels render as `#  omarchy` without repeating the hash in the name string. Assert the window title (`#omarchy - Omairc`), not the clipped label text.
- Clicking the CHANNELS `+` opens a toast about real IRC connectivity. That is not a conversation switch.
