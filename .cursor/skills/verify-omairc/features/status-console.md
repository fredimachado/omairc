# Status console

Status is the network's server window. It shows handshake traffic, errors, notices, and core network commands in the center pane. It is not a sidebar conversation. AUTH and blank-target notices stay here.

## Sub-features

- `status-open-header` opens Status from the network name in the sidebar header.
- `status-edit-sheet` opens the connection sheet from the small edit control beside that name.
- `status-toggle` toggles Status with `Ctrl+``.
- `status-escape` returns to the last conversation with Escape when a conversation exists and setup is not required.
- `status-auth` keeps AUTH notices out of DIRECT MESSAGES.
- `status-composer` sends Status input through the same composer with Enter.

## How to get to it (user POV)

- Click the network name in the sidebar header.
- Press `Ctrl+``.
- Land on Status when nothing is selected yet, including handshake before the first channel.
- Click the small `edit` control in the header to open Connect.
- Type in the composer while Status is showing and press Enter.

## Driving it with control-omairc

Preconditions:

- Handshake lines without a real server need the live QML fixture (`qml-suite`).
- A fresh compiled window is titled `irc.libera.chat Status` with Connect on top of Status (`Offline`, no handshake). Finish Connect before looking for handshake lines on that window, or stay on the offscreen suite.
- After Connect, Status title is `{displayName} Status` or `Status`. Conversation titles stay `{conversation} - Omairc`.

- **First-run Status.** After `control-omairc launch`, the title is `irc.libera.chat Status`. The sidebar name is `irc.libera.chat`, the subtitle is `Offline`, CHANNELS and DIRECT MESSAGES are empty, and Connect sits on top of Status. There are no handshake lines yet. Run `control-omairc screenshot --feature status-console --name first-run-under-connect`.
- **Toggle with no conversation.** Run `control-omairc key --key ctrl+grave`. The title stays `irc.libera.chat Status`. Status cannot close while nothing is selected.
- **Open Status from the header.** After a conversation exists, click the network name. Run `control-omairc click-network`. In the suite this is `networkHeaderButton`. The center header reads Status, the people control is gone, and the list shows server lines such as `*** Looking up your hostname...`.
- **Keep AUTH out of DIRECT MESSAGES.** After opening Status, the sidebar still has no `AUTH` row under DIRECT MESSAGES.
- **Escape.** With a conversation already selected and Connect not required, press Escape. Run `control-omairc key --key Escape`. Status closes and the last conversation returns.
- **Edit control.** Click `edit`. Run `control-omairc click-edit`. The connection sheet opens. Setup-required still opens that sheet on its own.
- **Proof.** Capture Status showing server lines and no AUTH DM. The suite writes `test-artifacts/status-console.png` from the live fixture. When using a desktop instance that already has a conversation, run `control-omairc screenshot --feature status-console --name after-open`.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` clicks `networkHeaderButton` on the live fixture, asserts `*** Looking up your hostname...` in `consoleList`, and keeps AUTH out of DIRECT MESSAGES. This does not prove the compiled-window header click.

## Gotchas

- Status is not a CHANNELS or DIRECT MESSAGES row. Do not look for a sidebar `Status` item.
- There is one composer. Enter goes to the visible pane. Do not look for a second input.
- The network name opens Status. The small `edit` control opens Connect. Clicking the old full header is no longer the sheet path.
- AUTH, `*`, and blank-target notices belong in Status. A DM named AUTH is a failure.
- Status titles do not end with ` - Omairc`. Do not use `wait-title --exact "* - Omairc"` as the Status identity.
- `Ctrl+`` does not close Status when no conversation is selected. First-run Connect is that state.
- The header subtitle is `Offline` on a fresh compiled launch, `mock connected` in the mock window, and `Connected` on the live fixture.
