# Identity footer

The sidebar footer always shows who this window is. It is the initials chip, the nick, a presence mark, and `available`, `away`, or `offline`. The app version sits on the right of that same footer, on the presence line. Clicking that version opens About. The mark and label follow live `irc.connectionStatus` and `irc.selfAway`. They do not follow member presence or `hasAwayPresence`. A connected session stays `available` unless away; anything other than `Connected` is `offline`.

## Sub-features

- `identity-fallback-empty` shows an empty nick and `?` initials, with `offline`, when no nick is set yet and the network is not connected. There is no `fred` fallback; `fred` only appears under `--demo-server`, where it is the seeded live nick.
- `identity-connection-nick` shows the Connect nick draft as soon as Nick is non-empty, including before Apply.
- `identity-live-nick` shows the live session nick when one is present.
- `identity-self-away` paints the footer mark amber `#d6a552` and the word `away` when the focused network is `Connected` and live `irc.selfAway` is true. When `Connected` and not away, the mark is green `#69b978` and the word is `available`. When the focused network is not `Connected`, the mark is muted and the word is `offline`, even if a prior away fact remains. The nick is not dimmed.
- `identity-app-version` shows the running app version on the right of the footer, muted, on the same baseline as `available` / `away` / `offline`. It is present on every screen, including first-run Connect. Clicking it opens About.

## How to get to it (user POV)

- Look at the bottom of the sidebar. It is on every screen, including first-run Connect.

## Driving it with control-omairc

Preconditions:

- A default compiled launch shows an empty footer nick (`?` initials) and `offline` because Nick is still empty and the network is Offline. `fred` only appears under `--demo-server`, where it is the seeded live nick.
- Demo chrome is also on `control-omairc launch --demo-server`. Live-fixture nicks are proven by `qml-suite`. Do not type a nick and Apply on the compiled window; that starts a real session.
- The first `import` after a fresh `launch` can grab a blank frame. This recipe sends one `Tab` first. Tab walks focus inside Connect and does not Apply or dismiss the sheet.

```desktop-recipe
launch
wait-title --exact "irc.libera.chat Status"
key --key Tab
wait-title --exact "irc.libera.chat Status"
screenshot --feature identity-footer --name first-run-empty
```

- **First-run fallback.** After `control-omairc launch`, run `control-omairc wait-title --exact "irc.libera.chat Status"`, `control-omairc key --key Tab`, and `control-omairc screenshot --feature identity-footer --name first-run-empty`. The footer nick is empty (the initials chip shows `?`) and the line under it is `offline`. There is no `fred` fallback. The app version is still on the right. Connect stays open on top of that footer.
- **Offscreen suite.** When proving the seeded and live labels, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` asserts `selfNickLabel` is `fred` on the seeded window and `live-nick` on the live fixture. The seeded `selfPresenceLabel` is `available` and `selfPresenceDot` is `#69b978`. A live fixture with `selfAway` true and `hasAwayPresence` false shows `away` and `#d6a552` while `connectionStatus` is `Connected`. A live fixture with `connectionStatus` `Offline` shows `offline` and the muted mark. `selfVersionLabel` is the non-empty app version on the seeded window. `qml-suite` copies the `#desktop` grab to `test-artifacts/verify/identity-footer/mock-fred.png`. That image must show `fred` / `available` and the version on the right of the sidebar footer. This is not compiled-window proof.

## Gotchas

- `run identity-footer` skips `launch` when `doctor` is already healthy and `demo=no`. It does not clear a nick you typed. A healthy `--demo-server` instance makes plain `launch` fail. Recipes that need the empty first-run footer need `cleanup` before `run`. Do not Apply.
- A screenshot taken before any key on a fresh launch can be a blank frame. The recipe sends `Tab` first.
- The seeded footer stays `available` while `fred` is present and the demo session is Connected. Live chrome follows `irc.connectionStatus` first, then `irc.selfAway`.
- Other members' dots still gate on `hasAwayPresence`. Our own member row and the footer mark do not, because the `305` / `306` numerics are authoritative for us when Connected.
- An empty first-run Nick shows an empty footer nick and `?` initials. That is the fallback, not a saved profile. `fred` only appears on the demo server, where it is the seeded live nick.
- `irc.currentNick` is empty only when nothing is selected. First-run (empty selection) then shows the Connect nick draft if you typed one without Apply. Opening Status over a selected conversation keeps the live nick.
- The network name and `edit` control are the Status / Connect entry points. The nick and presence words do not open a sheet. The version on the right opens About. When a Windows installer download is ready, that label reads `Restart to update` and starts the installer instead.
