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

- Mock conversation UI is showing. Use `control-omairc launch --mock`, or `qml-suite` (`irc` left null). A default compiled launch is titled `irc.libera.chat Status`.
- For the desktop recipe below, the current conversation is `#desktop` (`control-omairc wait-title --exact "#desktop - Omairc"`).
- No prior verify line `Hello from verify` is already in this session's `#desktop` transcript.

- **Keyboard focus.** Press `Ctrl+L`. Run `control-omairc focus-composer`. The composer ring uses the accent color.
- **Type and send.** Enter a unique body. Run `control-omairc send --text "Hello from verify"`. The composer is empty and the last visible message is `fred` / `Hello from verify`.
- **Empty submit.** Press `Enter` with an empty composer. Run `control-omairc focus-composer` then `control-omairc key --key Return`. The last message is still `Hello from verify`.
- **SEND button.** Type a second unique body and click SEND while the member panel is open. Run `control-omairc focus-composer`, `control-omairc type --text "Sent with the button"`, and `control-omairc click-send`. The last message is `fred` / `Sent with the button`.
- **Action line.** Send `/me waves`. Run `control-omairc send --text "/me waves"`. The last line is an italic action whose body is `fred waves`.
- **Confirm persistence.** Switch to `#omarchy` and back. Run `control-omairc click-conversation --name "#omarchy"`, `control-omairc wait-title --exact "#omarchy - Omairc"`, `control-omairc click-conversation --name "#desktop"`, and `control-omairc wait-title --exact "#desktop - Omairc"`. `#desktop` still shows the three sent lines.
- **Proof.** Capture the populated transcript. Run `control-omairc screenshot --feature send-message --name after-send` and `control-omairc screenshot --feature send-message --name after-return`. Both images show `Hello from verify` in `#desktop`.
- **Offscreen suite.** When Xvfb tools are missing, run `control-omairc doctor-qml` then `control-omairc qml-suite`. `bin/test` sends from `#omarchy` (not `#desktop`) and writes `test-artifacts/send-message.png`. The last line is `fred` / `Hello from the UI test`. `qml-suite` copies it to `test-artifacts/verify/send-message/after-send.png`. This does not prove the compiled-window composer path.

## Gotchas

- `Enter` sends. There is no multiline composer. Do not hold Shift+Enter expecting a newline.
- Empty or whitespace-only input is ignored. A screenshot of an unchanged transcript is the empty-submit proof.
- `/me ` (note the space) rewrites the body to `fred ...` and uses kind `action`. `/me` without a trailing space is a normal message. Other slashes stay chat on `--mock`. Live catalog verbs are slash-commands.
- Messages are session-local. Relaunching the isolated instance resets the mock models. Prove persistence inside one launch.
- `click-send` is only aimed while the member panel is visible. If the panel is closed, use `Enter`.
