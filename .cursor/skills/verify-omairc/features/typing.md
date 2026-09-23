# Typing

Typing is the bouncing ellipsis that shows someone is composing. On a channel it sits beside that member's nick in the member panel. On a direct message it sits at the end of the transcript. It groups with the peer's last message when the peer spoke last, and takes its own avatar line otherwise. It is DM-only in the transcript and stays visible through a `typing=paused` hold of 30s (IRCv3 typing client-tag SHOULD). Mock UI shows it for `anna` on `#omarchy` and the `anna` DM. A live session hides it unless the server granted `message-tags`.

## Sub-features

- `typing-member` shows bouncing dots beside a channel member who is composing (`anna` on seeded `#omarchy`).
- `typing-dm` shows the same dots at the end of a direct message transcript (`anna`). The dots share the peer's last message row when that message is the peer's, and otherwise show the peer's avatar and nick header above them.
- `typing-caps` hides both on a live session that did not get `message-tags`.

## How to get to it (user POV)

- Open `#omarchy` with the member panel visible and read `anna`'s row.
- Open the `anna` direct message and look at the end of the transcript, just below `anna`'s message.

## Driving it with control-omairc

Preconditions:

- Seeded conversation UI is showing (`#omarchy · irc.example · fred - Omairc`, members visible). Use `control-omairc launch --demo-server`. When Xvfb tools are missing, `qml-suite` (seeded `IrcController`) is the fallback and is not compiled-window proof.
- `control-omairc launch` without `--demo-server` is first-run Connect titled `irc.libera.chat Status` with no member list. Do not start this recipe there.
- Live `typing-caps` needs a completed Connect and `message-tags`. That is not this recipe. Do not inject typing frames.

```desktop-recipe
launch --demo-server
wait-title --exact "#omarchy · irc.example · fred - Omairc"
screenshot --feature typing --name member-glyph
jump --query anna
wait-title --exact "anna - Omairc"
screenshot --feature typing --name dm-overlay
```

- **Member glyph.** Capture the member panel. Run `control-omairc screenshot --feature typing --name member-glyph`. Three dots sit beside `&anna`. The other rows do not.
- **DM overlay.** Jump to `anna`. Run `control-omairc jump --query anna`, `control-omairc wait-title --exact "anna - Omairc"`, and `control-omairc screenshot --feature typing --name dm-overlay`. There is no member panel. Under anna's last message, a second anna row shows three dots. The seeded line is not in the current minute, so the dots take their own avatar line.
- **Offscreen suite.** Run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` writes `test-artifacts/typing-member-glyph.png` and `test-artifacts/typing-dm-overlay.png`. `qml-suite` copies them to `test-artifacts/verify/typing/member-glyph.png` and `test-artifacts/verify/typing/dm-overlay.png`. The suite grab must show dots beside `anna` on `#omarchy` and dots at the end of the `anna` transcript. `test_typingChromeFollowsCapabilities` covers `typing-caps`. This is not compiled-window proof.

## Gotchas

- `run typing` skips `launch` when `doctor` is already healthy and `demo=yes`. It does not re-seed typing. A hidden member panel hides the glyph. Recipes that need the fresh demo panel need `cleanup` before `run`.
- `screenshot` retries a near-solid grab and fails the run if the frame stays flat. The member-glyph shot does not send a key first. Escape does not change typing state.
- A `typing=paused` hint stays visible for 30s, matching the IRCv3 typing client-tag recommendation. Do not expect dots to hide the instant the peer pauses. The demo's active hint stays painted until a later event prunes it.
- The transcript indicator is DM-only. Channels keep the member-panel glyph. Transcript dots are 16px; the sidebar DM row and member-panel chrome glyphs stay at the TypingDots default of 12px.
- Presence dots, away dimming, and status lines are member-presence. This feature is only the ellipsis.
- The identity footer does not follow typing. Live away chrome is identity-footer.
- Mock typing is only `anna`, and only on `#omarchy` or the `anna` DM. `#desktop` has no typing overlay.
- Do not claim live typing on first-run Connect. It is `verified-unreachable` until a session has completed Connect and received `message-tags`. The attempted compiled route is `control-omairc launch` (title `irc.libera.chat Status`, no members).
