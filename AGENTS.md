# Omairc contributor notes

## Product boundary

- Keep Omairc dead-simple, keyboard-friendly, and visually native to Omarchy.
- Conversations are live through `IrcController`. `OmaircWindow` requires
  `irc`. Binding it to null does not load a prototype sidebar.
  `./build/omairc --demo-server` seeds that controller in-process and skips
  Connect. UI tests use `SeededIrcFixture` or a small fake controller.
- Prefer a small, calm interface over adding controls for hypothetical future
  features.

## Architecture

- This is a Qt 6 Quick application built with qmake and C++17.
- Keep window presentation logic in `src/OmaircWindow.qml`. Seed the demo
  world from `IrcDemoServer`, not from QML `ListModel`s.
- Keep `Backend` limited to desktop integration: Omarchy theme colors, live
  theme watching, text scale, window geometry, and desktop notifications.
- Keep IRC networking, protocol, session, and model code in `src/irc/`.
- Keep system light/dark mode and portal text-scale detection in
  `SystemTheme`.
- Use the bundled `iA Writer Mono S` font for all custom interface text.
- The public CLI skill lives in `skills/omairc/`. Install it with
  `npx skills add fredimachado/omairc/skills -g`. Proving the GUI uses
  `.cursor/skills/verify-omairc`; that skill is internal.

## Visual conventions

- Derive UI colors from `backend.themeBackground`, `themeForeground`,
  `themeAccent`, and `themeSelection`; do not introduce a separate fixed
  palette for structural UI.
- Derive secondary surfaces and muted text with `mixColors()` so new Omarchy
  themes continue to work.
- Scale dimensions and text through `scaledSize()` and `backend.textScale`.
- Keep conversation counts consistent between the header and member panel.

## QML conventions

- Dynamic conversation navigation uses an unambiguous `conversation` role.
  Test the live `MessageListModel` and the compiled window. Confirm rendered
  labels. A warning-free QML startup does not prove that bindings render
  visible values.
- The Connect sheet is tabbed: `Connection` holds the per-network profile and
  `Preferences` holds global settings such as reopening direct messages on
  startup. Only queries the user opened or replied to are restored, and
  restore waits until after ISUPPORT (`376` / `422`). Keep the
  `Ctrl+/` shortcut hint in its rail so new users find the shortcuts sheet.
- The Connect sheet is keyboard-first. `Enter` walks to the next field and
  `Ctrl+Enter` applies, so a stray keypress cannot commit a half-typed
  profile. `Ctrl+Enter` is one window-level `Shortcut`, not a branch inside
  each control: wiring it per control meant it silently did nothing wherever
  focus sat on something without its own handler. The network rail owns
  `Up`/`Down`/`Home`/`End` and scrolls the focused row into view; focus does
  not scroll a `Flickable` on its own. Focusable rows must stay reachable by
  `Tab` as well. Closing the sheet returns focus to the composer.
- Channel switching must update the topic, message model, people count, and
  member list together.
- Show the people count, member toggle, and member panel only for channels.
  Clicking another user in the member panel must open or create a direct
  message with its own local message history.
- Order member rows by the server's `PREFIX` rank, highest first, then nick.
  Sort in `MemberListModel` through `IrcEventReducer::orderedMembers`, never in
  QML, and keep the panel and `omairc names` in that same order.
- Keep the composer single-line. `Enter` sends the message.

## Review lessons

`bin/check-conventions` already gates the mechanical repeats. These still
need judgment.

- Inventing a conversation requires `IrcConversationCause` on
  `ensureConversation`. QuietSend and InboundSelf never invent; UserOpen
  invents DMs; ChannelState invents channels; InboundOther invents
  channels and non-service DMs; Restore invents non-service DMs after
  ISUPPORT so engaged queries survive a restart. Persist only when the
  user opened the query, sent in it, or a self-authored line arrives.
  Cover `/msg` with `echo-message`, incoming NickServ PRIVMSG, and
  incoming human PRIVMSG in the same test matrix.
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
launches `./build/omairc --demo-server` as a black box and is not in CI.
`bin/test-live` needs Docker. It drives Ergo, Solanum, and ngIRCd, then the
dual-network production-QML proof. It fails if `docker` or `docker compose`
is missing.

For visual changes, inspect the running window and the screenshots under
`test-artifacts/`.

On Windows, `bin\build.bat` finds a Qt 6 kit, runs qmake and nmake (or jom,
or mingw32-make), and deploys Qt next to `build\release\omairc.exe`. Set
`QMAKE` to pick a kit. `bin/build`, `bin/test`, `bin/test-desktop`, and
`bin/test-live` stay Unix scripts. The C++ suite still builds from
`tests/tests.pro` with the same kit. Portal text-scale stays 1.0, the window
icon stays empty, and the Omarchy `colors.toml` watch does nothing when that
file is missing.
