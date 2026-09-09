# Incoming direct reply

Incoming direct reply is the live path where another nick PRIVMSGs you, a DIRECT MESSAGES row appears, you open that chat, and Enter in the composer reaches the peer.

## Sub-features

- `dm-inbound-row` adds a DIRECT MESSAGES row when a peer PRIVMSGs the connected nick.
- `dm-inbound-select` opens that chat with `Alt+Down` so the title is `{peer} - Omairc`.
- `dm-inbound-send` submits a composer line with Enter and shows the local echo.
- `dm-inbound-peer` is the peer receiving `PRIVMSG {peer} :dm reply from ui`.

## How to get to it (user POV)

- Connect to a server and join a channel.
- Wait until a DIRECT MESSAGES row appears for an incoming private message.
- Press `Alt+Down` to open that chat, or click the row.
- Type a reply and press Enter.

## Driving it with control-omairc

Preconditions:

- This path needs a live ircd and the compiled window. `launch --mock` and `qml-suite` never receive a real PRIVMSG.
- Run `bin/test-live-ui`. That script starts Ergo, launches an isolated window, fills Connect for `127.0.0.1` plaintext, and drives the recipe below.
- A default `control-omairc launch` is first-run Connect titled `irc.libera.chat Status`. Do not start this recipe there without a reachable ircd.

- **Connect.** After `control-omairc launch`, fill Host `127.0.0.1`, the published plaintext port, TLS off, and Nick `omau`. Click Apply. Run `control-omairc wait-title --exact "#omarchy - Omairc"`. The sidebar shows `#omarchy` and Connected.
- **Inbound DM.** A peer nick `uireply` sends `PRIVMSG omau :dm ping from peer`. Run `control-omairc screenshot --feature incoming-direct-reply --name after-dm`. A `uireply` row appears under DIRECT MESSAGES. The title stays `#omarchy - Omairc` until you open it.
- **Open the chat.** Press `Alt+Down`. Run `control-omairc key --key alt+Down` then `control-omairc wait-title --exact "uireply - Omairc"`. The topic is `Direct message with uireply`. The inbound line `dm ping from peer` is visible.
- **Reply.** Type and send. Run `control-omairc send --text "dm reply from ui"`. The composer is empty and the last line is `omau` / `dm reply from ui`. The peer's socket receives `PRIVMSG uireply :dm reply from ui`.
- **Proof.** Capture the selected DM after send. Run `control-omairc screenshot --feature incoming-direct-reply --name after-send` and `control-omairc compare --before after-select.png --after after-send.png`. The image must show `uireply` selected and both the inbound ping and the local reply.
- **Offscreen suite.** `qml-suite` does not cover this path. Report it skipped for mock QML.

## Gotchas

- `click-conversation` only knows mock names (`anna`, `dax`, the four channels). A live inbound nick is not in that map. Use `Alt+Down` after one autojoined channel, or a raw click whose Y you measured on that sidebar.
- A successful local echo is not delivery. The peer log line `PEER_GOT_REPLY` is the delivery proof.
- `bin/test-live` talks to `IrcController` in-process. It does not focus the composer or press Enter.
- Connect field clicks assume the isolated 1180x760 window at textScale 1.0. Apply is at `857,637` on that layout.
