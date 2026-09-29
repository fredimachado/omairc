# Typing

Typing is the bouncing ellipsis that shows someone is composing. On a channel it sits beside that member's nick in the member panel. On a direct message it sits at the end of the transcript on its own line, indented to the body column. When the peer spoke last **in the current displayed minute** the dots follow their message directly, with no repeated nickname; otherwise the peer's avatar and nick header come back above them. The gate is the minute, not just the author, the same rule that joins consecutive transcript rows: a peer who spoke at an earlier minute gets the nickname back, and the header reappears on its own if the minute rolls over while they keep typing. The dots never share a line with the peer's message text. It is DM-only in the transcript and stays visible through a `typing=paused` hold of 30s (IRCv3 typing client-tag SHOULD). Mock UI shows it for `anna` on `#omarchy` and the `anna` DM. A live session hides it unless the server granted `message-tags`.

The mirror image is outbound: editing the composer in a conversation on a network that granted `message-tags` sends `@+typing=active TAGMSG <target>`, paced to at most one per target every 3s. A non-live line (a slash command, or clearing the composer) withdraws a hint that target was publishing with `@+typing=done TAGMSG <target>`, and a sent message suppresses that done. Moving to another conversation withdraws the previous target's hint. The Status composer never publishes in either client.

## Sub-features

- `typing-member` shows bouncing dots beside a channel member who is composing (`anna` on seeded `#omarchy`).
- `typing-dm` shows the same dots at the end of a direct message transcript (`anna`). The dots take their own line under the peer's last message when that row is the peer's and lands in the current displayed minute, and otherwise appear under the peer's avatar and nick header.
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
- **DM overlay.** Jump to `anna`. Run `control-omairc jump --query anna`, `control-omairc wait-title --exact "anna - Omairc"`, and `control-omairc screenshot --feature typing --name dm-overlay`. There is no member panel. Under anna's last message, a second anna row shows three dots on the indented body column. The seeded line is not in the current minute, so the dots take their own avatar line. The dots always own their line: a narrower window that wraps anna's body must still show the dots below the wrapped text, never appended to its last word.
- **Offscreen suite.** Run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` writes `test-artifacts/typing-member-glyph.png` and `test-artifacts/typing-dm-overlay.png`. `qml-suite` copies them to `test-artifacts/verify/typing/member-glyph.png` and `test-artifacts/verify/typing/dm-overlay.png`. The suite grab must show dots beside `anna` on `#omarchy` and dots at the end of the `anna` transcript. `test_typingChromeFollowsCapabilities` covers `typing-caps`. This is not compiled-window proof.

## Gotchas

- `run typing` skips `launch` when `doctor` is already healthy and `demo=yes`. It does not re-seed typing. A hidden member panel hides the glyph. Recipes that need the fresh demo panel need `cleanup` before `run`.
- `screenshot` retries a near-solid grab and fails the run if the frame stays flat. The member-glyph shot does not send a key first. Escape does not change typing state.
- A `typing=paused` hint stays visible for 30s, matching the IRCv3 typing client-tag recommendation. Do not expect dots to hide the instant the peer pauses. The demo's active hint stays painted until a later event prunes it.
- The transcript indicator is DM-only. Channels keep the member-panel glyph. Transcript dots are 16px; the sidebar DM row and member-panel chrome glyphs stay at the TypingDots default of 12px.
- DM grouping is the same-minute rule, not an author rule. A peer who messaged you ten minutes ago and is typing now gets the nickname and avatar back. Author-only grouping is not the expected behavior, and never was: it is the same rule that joins consecutive transcript rows.
- Clocks are shown on the reader's clock. The seeded `anna` row keeps its fixed server time but its byline reads that instant in your timezone, so do not expect the literal string `10:12` on a non-UTC machine, and the dots still take their own avatar line unless that local minute is the current one. A peer who messaged seconds ago does ride the row.
- Presence dots, away dimming, and status lines are member-presence. This feature is only the ellipsis.
- The identity footer does not follow typing. Live away chrome is identity-footer.
- Mock typing is only `anna`, and only on `#omarchy` or the `anna` DM. `#desktop` has no typing overlay.
- Do not claim live typing on first-run Connect. It is `verified-unreachable` until a session has completed Connect and received `message-tags`. The attempted compiled route is `control-omairc launch` (title `irc.libera.chat Status`, no members).
