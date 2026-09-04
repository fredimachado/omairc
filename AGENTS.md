# Omairc contributor notes

## Product boundary

- Keep Omairc dead-simple, keyboard-friendly, and visually native to Omarchy.
- Conversations may be live through `IrcController`. UI tests may still use
  the mock path when the `irc` property is null.
- Prefer a small, calm interface over adding controls for hypothetical future
  features.

## Architecture

- This is a Qt 6 Quick application built with qmake and C++17.
- Keep mock conversation state and presentation logic in
  `src/OmaircWindow.qml`.
- Keep `Backend` limited to desktop integration: Omarchy theme colors, live
  theme watching, text scale, and window geometry.
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
  Test delegate boundaries directly and confirm rendered labels rather than
  relying only on a clean startup.
- Channel switching must update the topic, message model, people count, and
  member list together.
- Show the people count, member toggle, and member panel only for channels.
  Clicking another user in the member panel must open or create a direct
  message with its own local message history.
- Keep the composer single-line. `Enter` sends the message.

## Build and validation

```sh
bin/build
bin/test
bin/test-desktop
QT_QPA_PLATFORM=offscreen timeout 3 ./build/omairc
```

The timeout is expected for the startup smoke check because the GUI event loop
continues running. For visual changes, also inspect the running application;
a warning-free QML startup does not prove that bindings render visible values.
The UI tests exercise the production QML with mouse and keyboard input and
write screenshots to `test-artifacts/`; inspect the relevant screenshot when
changing layout or presentation.
Use `bin/test-desktop` when the optional Xvfb, Xauthority, xdotool, and
ImageMagick dependencies are available to verify the compiled executable as a
black box without interacting with the user's active desktop.
