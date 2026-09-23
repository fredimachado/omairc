---
name: change-qml
description: Use when adding or moving UI. New rows, sheets, panels, and transcript chrome go in src/qml/; the window only wires them.
metadata:
  internal: true
---

# Change QML

New UI is a file in `src/qml/`, listed in `src/resources.qrc`, imported with `import "qml"` from `src/OmaircWindow.qml`.

The window may instantiate it with explicit props and signals, `style: win.style`, and `property alias` only for ids it already owns (`composer`, `sidebarScroll`, `membersList`, `messageList`, `consoleList`). No `host: win`. Pass `OmaircStyle`, models, and callbacks. `host: win`, `style: style`, a new named `component` besides `PlainUrlHit`, and a third delegate tree are gated by `bin/check-conventions`.

Keep `objectName`s. File-based types stay children of the same parent so `findChild` still works.

Colors come from `backend.themeBackground`, `themeForeground`, `themeAccent`, and `themeSelection`, mixed through `OmaircStyle` (`mixColors` / `style`). Scale with `style.scaledSize` / `backend.textScale`.

Message and console delegates still live in `src/OmaircWindow.qml` because they call `win.plainIrcText`, `win.emphasizedIrcText`, and `PlainUrlHit`. That is leftover, not a pattern to copy. New transcript chrome goes in `src/qml/`. Headers, avatars, and hits already live there: `MessageHeader.qml`, `MessageAvatar.qml`, `TranscriptNickHit.qml`.
