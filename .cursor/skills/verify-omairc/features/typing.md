# Typing

Typing is the bouncing ellipsis that shows someone is composing. On a channel it sits beside that member's nick. On a direct message it sits above the composer. Mock UI shows it for `anna` on `#omarchy` and the `anna` DM. A live session hides it unless the server granted `message-tags`.

## Sub-features

- `typing-member` shows bouncing dots beside a channel member who is composing (`anna` on seeded `#omarchy`).
- `typing-dm` shows the same dots above the composer in a direct message (`anna`).
- `typing-caps` hides both on a live session that did not get `message-tags`.

## How to get to it (user POV)

- Open `#omarchy` with the member panel visible and read `anna`'s row.
- Open the `anna` direct message and look just above the composer.

## Driving it with control-omairc

Preconditions:

- Seeded conversation UI is showing (`#omarchy · irc.example · fred - Omairc`, members visible). Use `control-omairc launch --demo-server`, or `qml-suite` (seeded `IrcController`).
- `control-omairc launch` without `--demo-server` is first-run Connect titled `irc.libera.chat Status` with no member list. Do not start this recipe there.
- Live `typing-caps` needs a completed Connect and `message-tags`. That is not this recipe.

- **Member glyph and DM overlay.** Run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` writes `test-artifacts/typing-member-glyph.png` and `test-artifacts/typing-dm-overlay.png`. `qml-suite` copies them to `test-artifacts/verify/typing/member-glyph.png` and `test-artifacts/verify/typing/dm-overlay.png`. The member grab must show dots beside `anna` on `#omarchy`. The DM grab must show `anna` selected, no member panel, and dots above the composer.
- **Capability gate.** The same suite run covers `typing-caps` in `test_typingChromeFollowsCapabilities`. There is no compiled-window screenshot for that path.

## Gotchas

- Presence dots, away dimming, and status lines are member-presence. This feature is only the ellipsis.
- The identity footer does not follow typing. Live away chrome is identity-footer.
- Mock typing is only `anna`, and only on `#omarchy` or the `anna` DM. `#desktop` has no typing overlay.
- Do not claim live typing on first-run Connect. It is `verified-unreachable` until a session has completed Connect and received `message-tags`. The attempted compiled route is `control-omairc launch` (title `irc.libera.chat Status`, no members).
