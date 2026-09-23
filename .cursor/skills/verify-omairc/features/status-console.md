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

- A default compiled launch (`control-omairc launch`) is titled `irc.libera.chat Status` with Connect on top of Status (`Offline`, no handshake). Finish Connect before looking for handshake lines on that window. That launch is not this fence. One fence cannot also `launch --demo-server`.
- `control-omairc launch --demo-server` starts on `#omarchy · irc.example · fred - Omairc` with no Connect overlay. The sidebar subtitle is `Connected`. Open Status with `Ctrl+``. Demo Status already includes `-AUTH- *** Looking up your hostname...`. The omarchy network's display name clashes with OFTC, so the Status title is `irc.example · fred Status`.
- After Connect, the window title and center header are `{displayName} Status`. Conversation titles stay `{conversation} - Omairc`, or `{conversation} · {displayName} - Omairc` when two conversations share a name.

```desktop-recipe
launch --demo-server
wait-title --exact "#omarchy · irc.example · fred - Omairc"
status
wait-title --exact "irc.example · fred Status"
screenshot --feature status-console --name after-open
key --key Escape
wait-title --exact "#omarchy · irc.example · fred - Omairc"
```

- **Open Status.** Press `Ctrl+``. Run `control-omairc status` then `control-omairc wait-title --exact "irc.example · fred Status"`. The people control is gone. The list shows server lines including `-AUTH- *** Looking up your hostname...`. There is no `AUTH` row under DIRECT MESSAGES. Capture it with `control-omairc screenshot --feature status-console --name after-open`.
- **Escape.** Press Escape. Run `control-omairc key --key Escape` then `control-omairc wait-title --exact "#omarchy · irc.example · fred - Omairc"`. Status closes and the last conversation returns.
- **First-run Status.** After `control-omairc launch` (no `--demo-server`), the title is `irc.libera.chat Status`. Connect sits on top of Status, the subtitle is `Offline`, and there are no handshake lines. `status` does not close Status while nothing is selected. That path is not this fence.
- **Mouse path.** `control-omairc click-network` opens Status from the network name. `control-omairc click-edit` opens Connect from the small edit control. There is no chord for those pixels. They are not this recipe. The recipe uses `status`.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` clicks `networkHeaderButton` on the live fixture, asserts `-AUTH- *** Looking up your hostname...` in `consoleList`, and keeps AUTH out of DIRECT MESSAGES. The suite writes `test-artifacts/status-console.png`. This is not compiled-window proof.

## Gotchas

- `run status-console` skips `launch` when `doctor` is already healthy and `demo=yes`. It does not reset which pane is open. Recipes that need Status closed on a fresh demo need `cleanup` before `run`.
- Status is not a CHANNELS or DIRECT MESSAGES row. Do not look for a sidebar `Status` item.
- There is one composer. Enter goes to the visible pane. Do not look for a second input.
- The network name opens Status. The small `edit` control opens Connect. Clicking the old full header is no longer the sheet path.
- AUTH, `*`, and blank-target notices belong in Status. A DM named AUTH is a failure.
- Status titles do not end with ` - Omairc`. Do not use `wait-title --exact "* - Omairc"` as the Status identity.
- `Ctrl+`` does not close Status when no conversation is selected. First-run Connect is that state.
- The header subtitle is `Offline` on a default compiled launch and `Connected` on `--demo-server` and the live fixture.
