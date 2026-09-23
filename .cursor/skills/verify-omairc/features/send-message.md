# Send a message

Send a message lets a user add a local line to the current mock transcript from the single-line composer, then see that line stay after switching away and back.

## Sub-features

- `send-focus` focuses the composer from click or `Ctrl+L`.
- `send-enter` submits the trimmed text with `Enter`.
- `send-button` submits the same text with SEND.
- `send-empty` leaves the transcript unchanged when the composer is blank or whitespace.
- `send-action` turns a `/me ` prefix into an action line attributed to `fred`.

## How to get to it (user POV)

- Click the composer, type, and press `Enter`.
- Press `Ctrl+L`, type, and press `Enter`.
- Type text and click `SEND`.
- Press `Tab` to complete a nick, or `Up` / `Down` to recall sent lines. Those chords are keyboard.

## Driving it with control-omairc

Preconditions:

- Seeded conversation UI is showing. Use `control-omairc launch --demo-server`. When Xvfb tools are missing, `qml-suite` (seeded `IrcController`) is the fallback and is not compiled-window proof. A default compiled launch is titled `irc.libera.chat Status`.
- For the desktop recipe below, jump to `#desktop` first. Run `control-omairc jump --query "#desktop"` then `control-omairc wait-title --exact "#desktop - Omairc"`. `launch --demo-server` starts on `#omarchy · irc.example · fred - Omairc`.
- No prior verify line `Hello from verify` is already in this session's `#desktop` transcript.
- `run send-message` skips `launch` when `doctor` is already healthy and `demo=yes`. It does not reset the member panel. `click-send` aims at the SEND button while that panel is open. A hidden panel makes the click miss, and the draft stays in the composer. Run `cleanup` before `run` when reusing a dirty instance.

```desktop-recipe
launch --demo-server
jump --query "#desktop"
wait-title --exact "#desktop - Omairc"
screenshot --feature send-message --name before-send
composer
send --text "Hello from verify"
focus-composer
key --key Return
focus-composer
type --text "Sent with the button"
click-send
screenshot --feature send-message --name after-button
send --text "/me waves"
screenshot --feature send-message --name after-send
jump --query "#omarchy"
wait-title --exact "#omarchy · irc.example · fred - Omairc"
jump --query "#desktop"
wait-title --exact "#desktop - Omairc"
screenshot --feature send-message --name after-return
compare --before test-artifacts/verify/send-message/before-send.png --after test-artifacts/verify/send-message/after-send.png
compare --before test-artifacts/verify/send-message/after-button.png --after test-artifacts/verify/send-message/after-send.png
```

- **Open #desktop.** Press `Ctrl+K` and jump to `#desktop`. Run `control-omairc jump --query "#desktop"` then `control-omairc wait-title --exact "#desktop - Omairc"`. Capture the transcript before sending with `control-omairc screenshot --feature send-message --name before-send`.
- **Keyboard focus.** Press `Ctrl+L`. Run `control-omairc composer` (`control-omairc focus-composer` is the same chord). The composer ring uses the accent color.
- **Type and send.** Enter a unique body and press `Enter`. Run `control-omairc send --text "Hello from verify"`. The composer is empty and the last visible message is `fred` / `Hello from verify`.
- **Empty submit.** Press `Enter` with an empty composer. Run `control-omairc focus-composer` then `control-omairc key --key Return`. The last message is still `Hello from verify`.
- **SEND button.** Type a second unique body and click SEND while the member panel is open. There is no chord for the button. Run `control-omairc focus-composer`, `control-omairc type --text "Sent with the button"`, and `control-omairc click-send`. Capture that result before any further send. Run `control-omairc screenshot --feature send-message --name after-button`. The last message is `fred` / `Sent with the button` and the composer is empty.
- **Action line.** Send `/me waves` only after that shot, as its own `send`. Run `control-omairc send --text "/me waves"`. The last line is an italic action: author `fred`, body `waves`. A leftover button draft would be prefixed onto `/me waves` in this line.
- **Confirm persistence.** Jump to `#omarchy` and back to `#desktop`. Run `control-omairc jump --query "#omarchy"`, `control-omairc wait-title --exact "#omarchy · irc.example · fred - Omairc"`, `control-omairc jump --query "#desktop"`, and `control-omairc wait-title --exact "#desktop - Omairc"`. `#desktop` still shows the three sent lines.
- **Proof.** Capture the populated transcript after `/me` and again after returning. Run `control-omairc screenshot --feature send-message --name after-send` before the jump away, and `control-omairc screenshot --feature send-message --name after-return` after coming back. Both images show `Hello from verify` in `#desktop`. Run `control-omairc compare --before test-artifacts/verify/send-message/before-send.png --after test-artifacts/verify/send-message/after-send.png` and `control-omairc compare --before test-artifacts/verify/send-message/after-button.png --after test-artifacts/verify/send-message/after-send.png`. Both compares must report a pixel change. The second pair is the button line against the action line. A missed `click-send` leaves the draft, and the later `send` then posts one combined line; that pair is how the miss shows up.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` sends from `#omarchy` (not `#desktop`) and writes `test-artifacts/send-message.png`. The last line is `fred` / `Hello from the UI test`. `qml-suite` copies it to `test-artifacts/verify/send-message/after-send.png`. This is not compiled-window proof.

## Gotchas

- `Enter` sends. There is no multiline composer. Do not hold Shift+Enter expecting a newline.
- Empty or whitespace-only input is ignored. A screenshot of an unchanged transcript is the empty-submit proof.
- `/me` is a live ACTION. Other catalog verbs are slash-commands. A rejected command such as `/close` on a channel stays in the composer. A successful `/join` or `/query` is consumed before the window switches, so it cannot remain as a draft.
- Messages are session-local. Relaunching the isolated instance resets the seeded session. Prove persistence inside one launch.
- `run send-message` skips `launch` when `doctor` is already healthy and `demo=yes`. It does not reset the member panel. A hidden panel makes `click-send` miss and leaves `Sent with the button` in the composer. The following `send --text "/me waves"` would append to that draft. Run `cleanup` before `run` when reusing a dirty instance.
- `click-send` is only aimed while the member panel is visible. `/me waves` is a separate `send` after the `after-button` shot, so it is not typed into a draft the button was supposed to clear.
- If the panel is closed, use `Enter`.
