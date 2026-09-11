# Omairc contributor notes

## Product boundary

- Keep Omairc dead-simple, keyboard-friendly, and visually native to Omarchy.
- Conversations may be live through `IrcController`. UI tests may still use
  the mock path when the `irc` property is null. `./build/omairc --mock` opens
  that same prototype in the compiled window, without Connect or a session.
- Prefer a small, calm interface over adding controls for hypothetical future
  features.

## Architecture

- This is a Qt 6 Quick application built with qmake and C++17.
- Keep mock conversation state and presentation logic in
  `src/OmaircWindow.qml`. That file is also the live window.
- Keep `Backend` limited to desktop integration: Omarchy theme colors, live
  theme watching, text scale, window geometry, and desktop notifications.
- Keep IRC networking, protocol, session, and model code in `src/irc/`.
- Keep system light/dark mode and portal text-scale detection in
  `SystemTheme`.
- Use the bundled `iA Writer Mono S` font for all custom interface text.

## Visual conventions

- Derive UI colors from `backend.themeBackground`, `themeForeground`,
  `themeAccent`, and `themeSelection`; do not introduce a separate fixed
  palette for structural UI.
- Derive secondary surfaces and muted text with `mixColors()` so new Omarchy
  themes continue to work.
- Scale dimensions and text through `scaledSize()` and `backend.textScale`.
- Keep conversation counts consistent between the header and member panel.

## QML conventions

- For fixed mock channel navigation, prefer explicit `ConversationRow`
  instances. A `ListModel`/`Repeater` boundary using a `name` role resolved to
  empty strings in this interface even though the QML tree started without
  errors.
- Dynamic direct-message navigation uses an unambiguous `conversation` role.
  Test the live `MessageListModel` and the compiled window, not only the mock
  `ListModel`. Confirm rendered labels. A warning-free QML startup does not
  prove that bindings render visible values.
- Channel switching must update the topic, message model, people count, and
  member list together.
- Show the people count, member toggle, and member panel only for channels.
  Clicking another user in the member panel must open or create a direct
  message with its own local message history.
- Keep the composer single-line. `Enter` sends the message.

## Review lessons

`bin/check-conventions` already gates the mechanical repeats. These still
need judgment.

- Put a product rule in one place. `/msg` must not open a DM was gated in
  the controller, then the reducer, then the translator (#23, #85, #88). A
  new inbound or echo path that calls `ensureConversation` must cover
  `/msg` with `echo-message`, incoming NickServ PRIVMSG, and incoming human
  PRIVMSG in the same test matrix.
- Do not hard-code CHANTYPES, CHANMODES, or PREFIX. Call
  `IrcServerFeatures`.
- Fail closed when redacting secrets for Status or error previews. Do not
  enumerate one more well-formed bypass.

## Build and validation

`bin/test` is the default gate. It runs `bin/check-conventions`, builds,
checks CLI help and version, runs the C++ suite, and runs the offscreen QML
tests. CI runs `bin/test`, `bin/test-san`, and `bin/test-live` on every pull
request, and rejects a pull request whose base is not `master`.

```sh
bin/test
bin/test-san
bin/test-install
bin/test-desktop
bin/test-live
```

`bin/test-desktop` needs Xvfb, Xauthority, xdotool, and ImageMagick. It
launches `./build/omairc --mock` as a black box and is not in CI.
`bin/test-live` needs Docker. It drives Ergo, Solanum, and ngIRCd, then the
dual-network production-QML proof. It fails if `docker` or `docker compose`
is missing.

For visual changes, inspect the running window and the screenshots under
`test-artifacts/`.
