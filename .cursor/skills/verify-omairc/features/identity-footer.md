# Identity footer

The sidebar footer always shows who this window is. It is the initials chip, the nick, a presence mark, and `available` or `away`. It is not a conversation and it does not open a sheet. The mark and label follow live `irc.selfAway`. They do not follow member presence or `hasAwayPresence`. Mock stays `available`.

## Sub-features

- `identity-fallback-empty` shows an empty nick and `?` initials, with `available`, when no nick is set yet. There is no `fred` fallback; `fred` only appears under `--demo-server`, where it is the seeded live nick.
- `identity-connection-nick` shows the Connect nick draft as soon as Nick is non-empty, including before Apply.
- `identity-live-nick` shows the live session nick when one is present.
- `identity-self-away` paints the footer mark amber `#d6a552` and the word `away` when live `irc.selfAway` is true. Otherwise the mark is green `#69b978` and the word is `available`. The nick is not dimmed.

## How to get to it (user POV)

- Look at the bottom of the sidebar. It is on every screen, including first-run Connect.

## Driving it with control-omairc

Preconditions:

- A default compiled launch shows an empty footer nick (`?` initials) and `available` because Nick is still empty. `fred` only appears under `--demo-server`, where it is the seeded live nick.
- Demo chrome is also on `control-omairc launch --demo-server`. Live-fixture nicks are proven by `qml-suite`. Do not type a nick and Apply on the compiled window; that starts a real session.

- **First-run fallback.** After `control-omairc launch`, run `control-omairc screenshot --feature identity-footer --name first-run-empty`. The footer nick is empty (the initials chip shows `?`) and the line under it is `available`. There is no `fred` fallback.
- **Offscreen suite.** When proving the seeded and live labels, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` asserts `selfNickLabel` is `fred` on the seeded window and `live-nick` on the live fixture. The seeded `selfPresenceLabel` is `available` and `selfPresenceDot` is `#69b978`. A live fixture with `selfAway` true and `hasAwayPresence` false shows `away` and `#d6a552`. `qml-suite` copies the `#desktop` grab to `test-artifacts/verify/identity-footer/mock-fred.png`. That image must show `fred` / `available` in the sidebar footer. This does not prove the compiled-window first-run footer.

## Gotchas

- The seeded footer stays `available` while `fred` is present. Live chrome follows `irc.selfAway` only.
- Member dots still gate on `hasAwayPresence`. The footer mark does not.
- An empty first-run Nick shows an empty footer nick and `?` initials. That is the fallback, not a saved profile. `fred` only appears on the demo server, where it is the seeded live nick.
- `irc.currentNick` is empty only when nothing is selected. First-run (empty selection) then shows the Connect nick draft if you typed one without Apply. Opening Status over a selected conversation keeps the live nick.
- The network name and `edit` control are the Status / Connect entry points. Clicking the footer does nothing.
