---
name: verify-omairc
description: Drive the Omairc Qt desktop prototype as a user would (isolated Xvfb + compiled binary). Use when proving the first-run Connect sheet, Status console, channel switching, sending local messages, the member panel, member presence, typing, the identity footer, keyboard shortcuts, closing a direct message, slash commands, slash-command complete, or opening a direct message.
---

# Verify Omairc

Omairc is a Qt 6 Quick desktop app. The compiled window binds a live IRC
controller and a connection sheet unless launched with `--mock`, which leaves
`irc` unset and shows the bundled prototype conversations. UI tests may still
leave `irc` null and exercise that same mock path. There is no web UI or public
API. This skill drives the compiled `build/omairc` window. The local CLI
(`connections`, `status`, `send`, `raise`) is a separate binary path and is
not a mapped feature here.

Read `features/README.md` before driving. Drive the mapped entry points for the feature under proof. A convenient path that skips listed entry points is incomplete.

## Launch

Prefer a private X server and the compiled binary when `Xvfb`, `xauth`, `xdotool`, `import`, and `magick` are on `PATH`. Do not launch on the user's live display.

```sh
.cursor/skills/verify-omairc/control-omairc launch
.cursor/skills/verify-omairc/control-omairc launch --mock
```

Ready when stdout includes a title ending in ` - Omairc` or ` Status`, and `doctor` exits 0. Default launch is `{displayName} Status` (often `irc.libera.chat Status`) with the connection sheet. `--mock` is `#omarchy - Omairc` with the prototype sidebar. After a live conversation exists the title is `{conversation} - Omairc`. Status itself uses `{displayName} Status` or `Status`.

This launch:

- runs `bin/build`
- picks the next free X display from `:110`
- uses a disposable `XDG_*` tree so window geometry is the default 1180x760
- points `DBUS_SESSION_BUS_ADDRESS` at a missing socket so portal text-scale stays 1.0
- passes `--mock` to the binary when requested
- writes run state to `/tmp/omairc-verify-$USER/state`

Two isolated instances can run if they get different displays. Never attach to a window you did not start. Never send `xdotool` to the session `$DISPLAY`.

If those desktop tools are missing, do not invent a drive against the user's session. Launch the offscreen production-QML suite instead. It is short-lived: build happens inside `bin/test`, then each run starts its own window.

```sh
.cursor/skills/verify-omairc/control-omairc doctor-qml
.cursor/skills/verify-omairc/control-omairc qml-suite
```

Teardown for a live Xvfb instance:

```sh
.cursor/skills/verify-omairc/control-omairc cleanup
```

`qml-suite` already tears down its temporary XDG tree. Keep `test-artifacts/verify/`.

## Doctor

Run this first whenever anything looks off:

```sh
.cursor/skills/verify-omairc/control-omairc doctor
```

Require all of:

- `ok isolated Omairc`
- `binary=` is `$ROOT/build/omairc`
- `display=` is the isolated Xvfb from launch, not the user's session
- `app_pid=` and `xvfb_pid=` are alive
- `title=` ends with ` - Omairc` or ` Status`
- `xdg=` is the disposable state directory from this run
- `mock=yes` after `launch --mock`, otherwise `mock=no`

If doctor fails, cleanup, then launch again. Do not continue against a shared or stale instance.

When using the offscreen suite:

```sh
.cursor/skills/verify-omairc/control-omairc doctor-qml
```

Require `ok qml suite` and a working `qmltestrunner=`. This path loads production QML with a fake `Backend` (fixed theme colors, textScale 1.0). It does not open the compiled `build/omairc` window.

## Drive

Use `control-omairc` against the isolated window. Stable handles:

| Handle | Meaning |
|---|---|
| Window title `{name} - Omairc` | Current conversation |
| `click-conversation --name #desktop` | Mock sidebar channel or seeded DM (`#omarchy`, `#desktop`, `#ricing`, `#help`, `anna`, `dax`). Those rows are hidden when `irc` is bound. Use `control-omairc launch --mock` for the prototype. |
| `click-member --name mira` | Member row while the panel is visible |
| `click-people` | Header `12 PEOPLE` / `Hide members` control while the member column is open (`908,36`) |
| `click-people --hidden` | Same control after the column hides (`1124,36`) |
| `click-network` | Sidebar network name. Opens Status. |
| `click-edit` | Small `edit` control beside that name. Opens Connect. Hidden under `--mock`; `click-edit` refuses that window. |
| `focus-composer` | `Ctrl+L` |
| `send --text "..."` | Focus composer, type, `Enter` |
| `key --key ctrl+shift+m` | Toggle members on a channel |
| `key --key ctrl+shift+p` | Focus the member list on a channel. Reopens the panel if it was hidden. |
| `key --key ctrl+w` | Close the selected direct message. No-op on a channel or Status. |
| `key --key ctrl+slash` | Toggle the shortcuts overlay |
| `key --key ctrl+grave` | Toggle Status. Does nothing useful on first-run Connect. |
| `key --key alt+Down` | Next sidebar conversation (channels, then DMs). Status is not in this list. |
| `key --key alt+Up` | Previous sidebar conversation |
| `key --key alt+a` | Next unread conversation, mentions first |
| `key --key Page_Up` | Scroll the visible transcript toward older lines. Composer stays focused. Disabled while Connect is visible. |
| `key --key Page_Down` | Scroll the visible transcript toward newer lines. |
| `key --key Tab` | Complete the nick prefix in the composer after `focus-composer`. |
| `key --key Up` | Previous sent line for the visible conversation or Status. |
| `key --key Down` | Newer sent line, or restore the stashed draft. |
| `key --key ctrl+comma` | Open Connect when a connection exists |
| `key --key ctrl+q` | Quit |

Named clicks are window-relative pixels for 1180x760 at textScale 1.0. They are invalid on a maximized window, a restored user geometry, or a portal text scale other than 1.0. That is why launch isolates XDG and DBus.

QML object names used by `bin/test` (not visible to xdotool): `connectionSheet`, `connectionHost`, `connectionNick`, `conversation-#desktop`, `conversation-anna`, `messageComposer`, `sendButton`, `peopleButton`, `membersPanel`, `membersList`, `member-mira`, `messageList`, `directConversationRepeater`, `networkHeaderButton`, `networkEditButton`, `consoleList`, `selfNickLabel`, `selfPresenceDot`, `selfPresenceLabel`, `presence-dot-anna`, `member-status-anna`, `shortcutsSheet`, `composer-typing`, `member-typing-anna`, `slashCompleteList`, `slashHit-join`.

Typical drive:

```sh
.cursor/skills/verify-omairc/control-omairc launch --mock
.cursor/skills/verify-omairc/control-omairc doctor
.cursor/skills/verify-omairc/control-omairc title
.cursor/skills/verify-omairc/control-omairc click-conversation --name "#desktop"
.cursor/skills/verify-omairc/control-omairc wait-title --exact "#desktop - Omairc"
.cursor/skills/verify-omairc/control-omairc screenshot --feature switch-conversation --name after-desktop
```

That click path needs the mock sidebar. Use `control-omairc launch --mock`. A default compiled launch is Connect with title `{displayName} Status`; use the Connect feature file first.

Inspect the matching feature file for the exact recipe and observables.

When desktop tools are missing, drive the mapped feature through the suite. `qml-suite` runs `bin/test`, which opens the Connect sheet with a fake incomplete profile, then clicks `conversation-#desktop`, `messageComposer`, `membersPanel` / `Ctrl+Shift+M`, `member-mira`, and `networkHeaderButton` with real mouse and key events, then copies screenshots into `test-artifacts/verify/`. That covers every mapped feature except live PREFIX ranks, live typing, and live slash dispatch, which need a completed Connect. It is not a pass on a skipped desktop entry point; say so in the proof notes.

## Evidence

Proof lives in `test-artifacts/verify/<feature-id>/`. Cleanup must not delete it.

Standards:

- Exercise the real window the way a user does: sidebar click, member click, composer, shortcuts. Do not call QML functions or write the mock models from outside the UI.
- Capture the action and the resulting state. A final screenshot alone is not proof.
- Window title is the conversation identity. A screenshot must show the sidebar selection, header name, topic, and (for channels) people count together.
- Messages are local only. Persistence proof is the same session: the row stays after sending, and switching away and back still shows it. There is no server or database.
- `control-omairc compare --before <a> --after <b>` requires a visible pixel change (ImageMagick AE > 100).
- `bin/test` writes `test-artifacts/{connection-sheet,switch-channel,send-message,toggle-members,open-direct-message,status-console,typing-member-glyph,typing-dm-overlay}.png`. Treat those as QML-suite evidence, not desktop-window evidence. `qml-suite` copies them into `test-artifacts/verify/<feature-id>/`, and reuses `switch-channel.png` for member-presence and identity-footer.
- Record the feature ID and entry point on every artifact name.

## Cleanup

```sh
.cursor/skills/verify-omairc/control-omairc cleanup
```

Kills only the `APP_PID` and `XVFB_PID` from the state file, then removes the disposable XDG/Xauth directory and the state file. It does not kill by process name. It does not remove `test-artifacts/verify/`.

After cleanup, confirm the proof files still exist at `test-artifacts/verify/<feature-id>/`.

## Helpers

`control-omairc` is executable. Invoke it from the repo root as shown above. Commands:

```text
launch [--mock] | doctor | title | wait-title --exact TITLE
click --x N --y N
click-conversation --name NAME
click-member --name NICK
click-people [--hidden] | click-network | click-edit | click-send | focus-composer
type --text TEXT | key --key KEY | send --text TEXT
screenshot --feature ID --name STEM
compare --before PATH --after PATH
qml-suite | doctor-qml
cleanup
```

`click-send` assumes the member panel is open (channel, members visible, width >= 980). Prefer `send --text` / `Enter`.
`click-people` assumes the member column is open. After it hides, use `click-people --hidden`.
`key --key ctrl+slash` maps to `Control_L+slash`. `ctrl+shift+m` and `ctrl+shift+p` map to `Control_L+Shift_L+m` / `p`. xdotool's shorter tokens do not reach those Qt shortcuts on the isolated Xvfb.

If Xvfb tools are missing, install `xorg-server-xvfb xorg-xauth xdotool imagemagick` before using this skill. `bin/test` can still run the offscreen QML suite without those packages.
