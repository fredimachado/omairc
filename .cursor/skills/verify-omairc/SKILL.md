---
name: verify-omairc
description: Drive the Omairc Qt desktop prototype as a user would (isolated Xvfb + compiled binary). Use when proving the first-run Connect sheet, Status console, channel switching, sending local messages, the member panel, member presence, typing, the identity footer, keyboard shortcuts, closing a direct message, slash commands, slash-command complete, opening a direct message, unfocused mention or direct-message notifications, or http(s) links in transcripts.
metadata:
  internal: true
---

# Verify Omairc

Omairc is a Qt 6 Quick desktop app. The compiled window always binds a live
IRC controller. A default launch shows the connection sheet. `--demo-server`
seeds that controller in-process and skips Connect. `OmaircWindow` requires
`irc`; binding it to null does not load a prototype sidebar. There is no web
UI or public API. This skill drives the compiled `build/omairc` window. The
local CLI (`connections`, `status`, `send`, `raise`) is a separate binary path
and is not a mapped feature here.

Read `features/README.md` before driving. Drive the mapped entry points for the feature under proof. A convenient path that skips listed entry points is incomplete.

## Launch

Prefer a private X server and the compiled binary when `Xvfb`, `xauth`, `xdotool`, `import`, and `magick` are on `PATH`. Do not launch on the user's live display.

```sh
.cursor/skills/verify-omairc/control-omairc launch
.cursor/skills/verify-omairc/control-omairc launch --demo-server
```

Ready when stdout includes a title ending in ` - Omairc` or ` Status`, and `doctor` exits 0. Default launch is `{displayName} Status` (often `irc.libera.chat Status`) with the connection sheet. `--demo-server` is `#omarchy · irc.example · fred - Omairc` with the seeded sidebar. After a live conversation exists the title is `{conversation} - Omairc`, or `{conversation} · {displayName} - Omairc` when two conversations share a name. Status itself uses `{displayName} Status` or `Status`.

This launch:

- runs `bin/build`
- picks the next free X display from `:110`
- uses a disposable `XDG_*` tree so window geometry is the default 1180x760
- points `DBUS_SESSION_BUS_ADDRESS` at a missing socket so portal text-scale stays 1.0
- sets `OMAIRC_ALLOW_MULTI=1` so the isolated window does not take the user socket
- passes `--demo-server` to the binary when requested
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
- `demo=yes` after `launch --demo-server`, otherwise `demo=no`

If doctor fails, cleanup, then launch again. Do not continue against a shared or stale instance.

When using the offscreen suite:

```sh
.cursor/skills/verify-omairc/control-omairc doctor-qml
```

Require `ok qml suite`. `bin/test` runs `tst_omairc.qml` through `seeded_qml_tests` (`Omairc.Test 1.0`). Default-window tests bind a fake or seeded `IrcController`. It does not open the compiled `build/omairc` window.

## Drive

Use `control-omairc` against the isolated window. Shortcuts come first. Pixel clicks are last, and only for mouse-only chrome.

Agents should `launch` (if needed) then `run <feature>` instead of assembling raw `key --key` chords. `run <feature>` plays the `desktop-recipe` fence in that feature file (under `## Driving it with control-omairc`, before `## Gotchas`). `run <feature> --dry-run` prints the fence and exits without launching. A missing file, a missing fence, or an empty fence exits non-zero. `qml-suite` and `doctor-qml` inside a fence are rejected. They are not compiled-window proof.

`run` skips a fence `launch` when `doctor` is already healthy and the demo flag matches (`--demo-server` needs `demo=yes`; plain `launch` needs `demo=no`). A healthy instance in the other mode is an error. The runner does not clean it up. Any command failure stops the recipe. `screenshot` retries a failed, missing, empty, or near-solid grab, treats a non-finite stddev as flat, and exits non-zero without keeping an older PNG at that path if the latest grab failed or the frame stays flat. After the sequence, `run` still fails when a screenshot path is empty or the file is missing. `wait-title` already fails on a title miss.

Named chord verbs wrap the same keysym path as `key`:

| Handle | Meaning |
|---|---|
| Window title `{name} - Omairc` | Current conversation. Duplicate names use `{name} · {displayName} - Omairc`. |
| `jump --query TEXT` | `Ctrl+K`, wait for the jump field, type a substring of the jump label (`#desktop`, `anna`), Return. Empty `--query` fails. `#omarchy` is duplicated on the demo; the first sidebar match is the omarchy network. |
| `walk --down` / `walk --up` | One `Alt+Down` or `Alt+Up`. `--times N` repeats that direction (default 1). |
| `unread` | `Alt+A`. Next unread, mentions first. |
| `toggle-members` | `Ctrl+Shift+M` on a channel. |
| `focus-members` | `Ctrl+Shift+P`. Reopens the member list if it was hidden. |
| `nick-jump --query TEXT` | `Ctrl+Shift+K`, wait, type, Return. Same shape as `jump`. No-op on a direct message or Status. |
| `status` | `Ctrl+grave`. Toggles Status. |
| `connect` | `Ctrl+,`. Opens Connect when a connection exists. |
| `composer` | `Ctrl+L`. `focus-composer` is the same chord. Both wait after the chord so a later `type` is not racing focus. |
| `send --text "..."` | Focus the composer, type, `Enter`. |

Chords that do not have a named verb still use `key --key`:

| Handle | Meaning |
|---|---|
| `key --key ctrl+shift+s` | Collapse or restore the left server list column |
| `key --key ctrl+w` | Close the selected direct message. No-op on a channel or Status. |
| `key --key ctrl+slash` | Toggle the shortcuts overlay |
| `key --key alt+Right` | Next network header. Headers stay walkable when that section is collapsed. |
| `key --key alt+Left` | Previous network header |
| `key --key alt+shift+Left` | Collapse the focused network section. No-op without header focus. |
| `key --key alt+shift+Right` | Expand the focused network section |
| `key --key ctrl+alt+shift+Left` | Collapse every network section. Does not need header focus. |
| `key --key ctrl+alt+shift+Right` | Expand every network section |
| `key --key alt+shift+Up` | Move the focused network up. No wrap. |
| `key --key alt+shift+Down` | Move the focused network down |
| `key --key Page_Up` | Scroll the visible transcript toward older lines. Composer stays focused. Disabled while Connect is visible. |
| `key --key Page_Down` | Scroll the visible transcript toward newer lines. |
| `key --key Tab` | Complete the nick prefix in the composer after `composer`. |
| `key --key Up` | Previous sent line for the visible conversation or Status. |
| `key --key Down` | Newer sent line, or restore the stashed draft. |
| `key --key ctrl+q` | Quit |

Pixel clicks are the fallback for mouse-only chrome. Prefer a chord verb when one exists.

| Handle | Meaning |
|---|---|
| `click-conversation --name #desktop` | Seeded sidebar channel or DM (`#desktop`, `#help`, `#omarchy`, `#ricing`, `anna`, `dax`) on `--demo-server`. Channels are alphabetical, then DMs. Created DMs such as `mira` are not in that list; open them with `click-member` from a channel. |
| `click-member --name mira` | Member row while the panel is visible. Seeded `#omarchy` rows are ordered by rank: `~fred`, `&anna`, `@dax`, `@mira`, `%kai`, `+teo`, then the plain nicks. Seeded `#desktop` has no ranks and stays alphabetical. |
| `click-people` | Header `12 PEOPLE` / `Hide members` control while the member column is open (`908,36`) |
| `click-people --hidden` | Same control after the column hides (`1124,36`) |
| `click-network` | Sidebar network name. Opens Status. |
| `click-edit` | Small `edit` control beside that name. Opens Connect. Visible whenever `connection` is bound, including `--demo-server`. |
| `click-version` | App version on the right of the sidebar footer. Opens About. |
| `click-send` | SEND button. No chord. Assumes the member panel is open. Prefer `send --text`. |

Named clicks are window-relative pixels for 1180x760 at textScale 1.0. They are invalid on a maximized window, a restored user geometry, or a portal text scale other than 1.0. That is why launch isolates XDG and DBus.

QML object names used by `bin/test` (not visible to xdotool): `connectionSheet`, `connectionName`, `connectionHost`, `connectionNick`, `connectionPort`, `connectionTls`, `connectionUsername`, `connectionRealname`, `connectionAutojoin`, `connectionConnectOnStartup`, `connectionPassword`, `connectionNickServ`, `connectionProblem`, `connectionCredentialStatus`, `connectionForgetPassword`, `connectionForgetNickServ`, `connectionAddNetwork`, `connectionRemove`, `connectionDisconnect`, `connectionApply`, `connectionDiscard`, `networkChoiceList`, `connectionSheetTab-connection`, `connectionSheetTab-preferences`, `connectionPreferencesPanel`, `connectionReopenDirects`, `connectionOpenAtUnread`, `conversation-omarchy-#desktop`, `conversation-omarchy-anna`, `messageComposer`, `sendButton`, `peopleButton`, `membersPanel`, `membersList`, `membersHeadingButton`, `member-mira`, `messageList`, `messageBody`, `urlHit`, `messageUnseenJump`, `directConversationRepeater`, `networkHeaderButton-omarchy`, `consoleList`, `consoleUnseenJump`, `selfNickLabel`, `selfPresenceDot`, `selfPresenceLabel`, `selfVersionLabel`, `selfVersionHit`, `aboutSheet`, `aboutSheetCard`, `aboutSheetDimmer`, `aboutTitle`, `aboutName`, `aboutVersion`, `aboutLogo`, `aboutOpenSource`, `aboutGithubLink`, `aboutCopyright`, `aboutCheckUpdates`, `aboutOk`, `aboutUpdateStatus`, `aboutUpdateCheck`, `presence-dot-anna`, `member-status-anna`, `shortcutsSheet`, `jumpSheet`, `jumpFilter`, `jumpFilterPlaceholder`, `jumpModel`, `nickSheet`, `nickFilter`, `nickFilterPlaceholder`, `nickModel`, `nickList`, `typingTranscript`, `typingTranscriptDots`, `typingTranscriptAvatar`, `typingTranscriptHeader`, `member-typing-anna`, `slashCompleteList`, `slashHit-join`.

Typical drive:

```sh
.cursor/skills/verify-omairc/control-omairc launch --demo-server
.cursor/skills/verify-omairc/control-omairc run switch-conversation
```

That recipe needs the seeded sidebar. Use `control-omairc launch --demo-server`, or let `run` launch it from the fence. A default compiled launch is Connect with title `{displayName} Status`; use the Connect feature file first.

Inspect the matching feature file for the exact recipe and observables.

When desktop tools are missing, do not invent a drive against the user's session. `doctor-qml` then `qml-suite` is the fallback. `qml-suite` runs `bin/test`, which opens the Connect sheet with a fake incomplete profile, then clicks seeded `conversation-omarchy-#desktop`, `messageComposer`, `membersPanel` / `Ctrl+Shift+M`, `member-mira`, and `networkHeaderButton-omarchy` with real mouse and key events, then copies screenshots into `test-artifacts/verify/`. That covers every mapped feature except live typing, live slash dispatch, a real unfocused desktop mention, and rank glyphs arriving from a real server handshake, which need a completed Connect (and, for mentions, an unfocused window). It is not compiled-window proof and it is not a pass on a skipped desktop entry point; say so in the proof notes.

## Evidence

Proof lives in `test-artifacts/verify/<feature-id>/`. Cleanup must not delete it.

Standards:

- Exercise the real window the way a user does: shortcuts and the composer first, pixel clicks only for mouse-only chrome. Do not call QML functions from outside the UI.
- Capture the action and the resulting state. A final screenshot alone is not proof.
- Window title is the conversation identity. A screenshot must show the sidebar selection, header name, topic, and (for channels) people count together.
- Messages are session-local. Persistence proof is the same session: the row stays after sending, and switching away and back still shows it.
- `control-omairc compare --before <a> --after <b>` requires a visible pixel change (ImageMagick AE > 100).
- `bin/test` writes `test-artifacts/{connection-sheet,switch-channel,send-message,toggle-members,server-list-collapsed,server-list-restored,open-direct-message,status-console,typing-member-glyph,typing-dm-overlay,about-sheet}.png`. Treat those as QML-suite evidence, not desktop-window evidence. `qml-suite` copies them into `test-artifacts/verify/<feature-id>/`, and reuses `switch-channel.png` for member-presence and identity-footer.
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
launch [--demo-server] | doctor | title | wait-title --exact TITLE
run FEATURE [--dry-run]
jump --query TEXT | nick-jump --query TEXT
walk --down | walk --up [--times N]
unread | toggle-members | focus-members | status | connect | composer
focus-composer | type --text TEXT | key --key KEY | send --text TEXT
screenshot --feature ID --name STEM
compare --before PATH --after PATH
click --x N --y N
click-conversation --name NAME
click-member --name NICK
click-people [--hidden] | click-network | click-edit | click-version | click-send
qml-suite | doctor-qml
cleanup
```

`click-send` assumes the member panel is open (channel, members visible, width >= 980). Prefer `send --text` / `Enter`.
`click-people` assumes the member column is open. After it hides, use `click-people --hidden`.
`key --key ctrl+slash` maps to `Control_L+slash`. `ctrl+k`, `ctrl+l`, `ctrl+grave`, and `ctrl+comma` map to `Control_L+k` / `l` / `grave` / `comma`. `ctrl+shift+m`, `ctrl+shift+p`, `ctrl+shift+k`, and `ctrl+shift+s` map to `Control_L+Shift_L+m` / `p` / `k` / `s`. `alt+shift+Left` / `Right` / `Up` / `Down` map to `Alt_L+Shift_L+Left` and the matching arrows. `ctrl+alt+shift+Left` / `Right` map to `Control_L+Alt_L+Shift_L+Left` / `Right`. xdotool's shorter tokens do not reach those Qt shortcuts on the isolated Xvfb. `composer` and `focus-composer` use that `ctrl+l` path, then wait before a later `type`.

If Xvfb tools are missing, install `xorg-server-xvfb xorg-xauth xdotool imagemagick` before using this skill. `bin/test` can still run the offscreen QML suite without those packages.
