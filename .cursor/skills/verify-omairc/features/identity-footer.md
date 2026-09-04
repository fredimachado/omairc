# Identity footer

The sidebar footer always shows who this window is. It is the initials chip, the nick, a green mark, and the word `available`. It is not a conversation and it does not open a sheet.

## Sub-features

- `identity-fallback-fred` shows `fred` / `available` when no nick is set yet.
- `identity-connection-nick` shows the Connect nick once a profile nick exists.
- `identity-live-nick` shows the live session nick when one is present.

## How to get to it (user POV)

- Look at the bottom of the sidebar. It is on every screen, including first-run Connect.

## Driving it with control-omairc

Preconditions:

- A fresh compiled launch shows `fred` / `available` because Nick is still empty.
- Mock and live-fixture nicks are proven by `qml-suite`. Do not type a nick and Apply on the compiled window; that starts a real session.

- **First-run fallback.** After `control-omairc launch`, run `control-omairc screenshot --feature identity-footer --name first-run-fred`. The footer nick is `fred` and the line under it is `available`.
- **Offscreen suite.** When proving the mock and live labels, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` asserts `selfNickLabel` is `fred` on the mock window and `live-nick` on the live fixture. `qml-suite` copies the `#desktop` grab to `test-artifacts/verify/identity-footer/mock-fred.png`. That image must show `fred` / `available` in the sidebar footer. This does not prove the compiled-window first-run footer.

## Gotchas

- The footer always says `available`. It does not follow away or member presence.
- An empty first-run Nick still shows `fred`. That is the fallback, not a saved profile.
- The network name and `edit` control are the Status / Connect entry points. Clicking the footer does nothing.
