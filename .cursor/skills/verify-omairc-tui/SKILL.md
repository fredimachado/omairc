---
name: verify-omairc-tui
description: Drive the Omairc TUI (omairc-tui) in a PTY with control-omairc-tui. Use when proving the seeded demo shell, the first-run Connect sheet, the window title, conversation switching (walk/unread/jump), the Status console, the sidebar, the transcript, or the composer in the Go terminal client.
metadata:
  internal: true
---

# Verify Omairc TUI

`omairc-tui` is the Go terminal client. `--demo-server` seeds the same
two-network demo world as Qt and now runs the interactive shell on the
alternate screen. This skill drives the compiled `tui/bin/omairc-tui` inside a
PTY with `control-omairc-tui`. The shared feature map
`.cursor/skills/verify-omairc/features/*.md` is the contract for both drivers.
There is no web UI and no display: this is a terminal program, so there is no
Xvfb, no `xdotool`, and nothing to click. Never attach to a Qt instance.

## Launch

The wrapper builds nothing itself. It runs `tui/bin/build` only when
`tui/bin/control-omairc-tui` is missing, then execs the driver from the repo
root so evidence paths stay repo-relative. `launch` starts a detached daemon
that owns the PTY; every later verb talks to that daemon.

```sh
.cursor/skills/verify-omairc-tui/control-omairc-tui launch --demo-server
.cursor/skills/verify-omairc-tui/control-omairc-tui launch --demo-server --size 118x30
```

A plain `launch` (no `--demo-server`) runs a no-argument `omairc-tui`, which
starts on the Connect sheet over an empty controller and is titled
`irc.libera.chat Status`. `launch --demo-server` skips Connect and shows the
seeded sidebar. The PTY defaults to `118x30` (columns x rows) to match the Qt
window's character budget. `--size WxH` overrides it.

Run state lives in `$OMAIRC_TUI_STATE` when set, otherwise
`/tmp/omairc-verify-tui-$USER`. The daemon owns the PTY, so a second `launch`
while one is live is an error, not a second instance. `cleanup` tears it down.

## Doctor

Run this first whenever anything looks off:

```sh
.cursor/skills/verify-omairc-tui/control-omairc-tui doctor
```

Require all of:

- `ok omairc-tui`
- `binary=` is `$ROOT/tui/bin/omairc-tui`
- `pid=` is the live daemon pid
- `title=` is the OSC 2 title the shell last emitted
- `size=` is the PTY geometry from launch
- `demo=yes` after `launch --demo-server`, otherwise `demo=no`

`--demo-server` is ready when the title is
`#omarchy · irc.example · fred - Omairc`. If doctor fails, `cleanup`, then
launch again. Do not drive a stale daemon.

## Title

The terminal title is emitted as an OSC 2 window-title sequence from the
Bubble Tea v2 `View.WindowTitle` field. It is the TUI parity surface for the Qt
window title, computed by `internal/ui/title.go` (mirroring
`OmaircWindow.qml`'s `conversationTitleText` / `statusTitleText`) and
`internal/irc.RosterDisplayName` (mirroring `IrcConnection::rosterDisplayName`).

```sh
.cursor/skills/verify-omairc-tui/control-omairc-tui wait-title --exact "#omarchy · irc.example · fred - Omairc"
.cursor/skills/verify-omairc-tui/control-omairc-tui title
```

Contract:

- `{conversation} - Omairc`, or `{conversation} · {displayName} - Omairc` when
  two conversations share a name across networks.
- Status is `{displayName} Status`, or `Status` when no connection is bound.
  On a plain `launch` (first-run Connect, no live session) it falls back to
  the sheet's draft display name, so it is `irc.libera.chat Status`.
- On the seeded demo, both networks resolve to host `irc.example`, so their
  display names are `irc.example · fred` and `irc.example · oak`. `#omarchy`
  exists on both networks, so its title carries the display name; `#ricing`
  and `dax` do not.

`title` prints the current title. `wait-title --exact TEXT` polls until the
title matches exactly and fails on a miss.

## Drive

The gate is keyboard-first. It REJECTS pixel/mouse verbs (`click-*`) and the
QML suite verbs (`qml-suite`, `doctor-qml`) in a recipe: `omairc-tui` has no
pointer path and no QML suite. The full Phase 6 chord map is live
(`internal/ui/keys.go` is the chord table): `Ctrl+Q` is the only quit chord,
`Ctrl+C` copies via OSC 52, and `Enter` sends. Phase 7 adds the slash catalog,
the completer, and the `/list` overlay.

Use `run <feature>` instead of assembling raw `key` / `type` chords. Play the
verbs directly when a feature has no fence yet:

```sh
.cursor/skills/verify-omairc-tui/control-omairc-tui screenshot --feature send-message --name before-send
.cursor/skills/verify-omairc-tui/control-omairc-tui type --text "Hello from verify"
.cursor/skills/verify-omairc-tui/control-omairc-tui key --key return
.cursor/skills/verify-omairc-tui/control-omairc-tui send --text "Hello from verify"
# walk, unread, jump, and status are real:
.cursor/skills/verify-omairc-tui/control-omairc-tui walk --down --times 2
.cursor/skills/verify-omairc-tui/control-omairc-tui unread
.cursor/skills/verify-omairc-tui/control-omairc-tui jump --query "#desktop"
.cursor/skills/verify-omairc-tui/control-omairc-tui status
```

Verb table:

| Verb | Meaning |
|---|---|
| `launch [--demo-server] [--size 118x30]` | Start the detached PTY daemon and the shell. A plain `launch` starts on the Connect sheet (`irc.libera.chat Status`); `--demo-server` seeds the two-network world. `--size` is columns x rows; default `118x30`. |
| `doctor` | Print `ok omairc-tui` plus `binary=`, `pid=`, `title=`, `size=`, `demo=`. |
| `title` | Print the current OSC 2 title. |
| `text` | Print the reconstructed grid text (the rows actually on screen). |
| `wait-title --exact TEXT` | Poll until the title equals `TEXT`; fail on a miss. |
| `key --key NAME` | Write one key chord (for example `ctrl+q`, `ctrl+c`, `ctrl+k`, `ctrl+``, `return`, `Escape`, `alt+Down`). |
| `type --text TEXT` | Type literal text into the focused input. |
| `send --text TEXT` | Write `TEXT` into the focused composer, then press `Enter`. |
| `walk --down` / `walk --up [--times N]` | Write `Alt+Down` / `Alt+Up` `--times N` (default 1) to walk conversations in sidebar order, wrapping. Status is not in the walk. |
| `unread` | Write `Alt+A` for the next unread conversation (mentions first, muted skipped). |
| `jump --query TEXT` | Write `Ctrl+K`, type `TEXT` into the filter, then press `Enter` to open the first match (conversation or Status). |
| `nick-jump --query TEXT` | Write `Ctrl+Shift+K`, type `TEXT` into the nick filter, then press `Enter` to open or create that direct message (channels only). |
| `status` | Write `Ctrl+`` to toggle the Status console. |
| `connect` | Write `Ctrl+,` to open the Connect sheet (a no-op with no connection model, as in `--demo-server`). |
| `composer` | Focus the composer. The TUI composer owns focus unless a modal is open, so this is a no-op that keeps the shared desktop recipes runnable. |
| `compare --before PATH --after PATH` | Assert two PNGs differ. The fence's `test-artifacts/verify/` paths are remapped to `test-artifacts/verify-tui/`. |
| `screenshot [--feature NAME] [--name NAME]` | Render the current grid to a PNG. Default feature `shell`, default name `screenshot`. |
| `run <feature> [--dry-run]` | Replay the feature's `desktop-recipe` fence. `--dry-run` prints it and exits. |
| `cleanup` | Kill the daemon and the PTY; removes the state file. Keeps evidence. |
| `--help` | Print the verb list. |

Key names are case-sensitive Bubble Tea key names, not `xdotool` keysyms.
Accepted names are `ctrl+q`, `ctrl+c`, `ctrl+l`, `ctrl+slash`, `ctrl+k`,
`ctrl+``, `ctrl+,`, `ctrl+enter`, `ctrl+tab`, `ctrl+shift+delete`,
`ctrl+shift+s`, `ctrl+shift+p`, `ctrl+shift+k`, `ctrl+shift+a`,
`ctrl+shift+o`, `ctrl+shift+m`, `ctrl+home`, `ctrl+end`, `enter` / `Return` /
`return`, `tab`, `space`, `backspace`, `Escape` / `escape`, `Up` / `Down` /
`Left` / `Right` / `up` / `down` / `left` / `right`, `Page_Up` / `Page_Down` /
`page_up` / `page_down`, `shift+Page_Up` / `shift+Page_Down`, `Home` / `End` /
`home` / `end`, `alt+Down` / `alt+Up` / `alt+Right` / `alt+Left`,
`alt+shift+Left` / `alt+shift+Right` / `alt+shift+Up` / `alt+shift+Down`,
`ctrl+alt+shift+Left` / `ctrl+alt+shift+Right`, any `alt+<letter>` /
`alt+<Letter>`, any `ctrl+<letter>`, and `shift+Tab`. An unknown name fails
the verb with `unknown key "NAME"` instead of sending nothing. `ctrl+slash`
is sent as the Kitty CSI-u sequence, not the legacy `0x1F` control byte,
because the decoder does not map `0x1F` to `ctrl+/`.

## Run

`run <feature>` parses the same `## Driving it with control-omairc` section and
```` ```desktop-recipe ```` fence in
`.cursor/skills/verify-omairc/features/*.md` that the Qt driver uses.

```sh
.cursor/skills/verify-omairc-tui/control-omairc-tui run send-message
.cursor/skills/verify-omairc-tui/control-omairc-tui run send-message --dry-run
```

`run <feature> --dry-run` prints the fence and exits without launching. A
missing file, a missing fence, or an empty fence exits non-zero. A recipe line
whose verb the TUI does not support yet (for example `click-*`) FAILS the run
rather than silently skipping, so a fence that moves on to a later-phase verb
is expected to fail until that phase lands. The `run keyboard`,
`run slash-commands`, and `run slash-complete` fences pass end to end over
`--demo-server`. Do not soften a fence to make it pass.

## Evidence

PNG screenshots and the reconstructed grid text land under
`test-artifacts/verify-tui/<feature>/` relative to the repo root. Keep them.
The screenshots are rendered from the VT text grid, so they show what the
terminal actually contained, not a mock. Capture the action and the resulting
state: a final screenshot alone is not proof.

## Limits

Phase 7 lands the slash catalog and completer: `run slash-commands` and
`run slash-complete` pass end to end over `--demo-server` (`/join`, refused
`/close`, `/query`, CTCP `/ping`/`/time`/`/version`, `/list`, and the
Tab/Escape/Up/Down completer). Phase 6 landed the full `keyboard.md` chord map;
Phase 5 landed the Connect sheet and core conversation navigation. The shell
renders all three columns: the sidebar (network sections with presence marks
and typing ellipses), the transcript, and, for channels, a member column with
status and away text.

The store-backed verbs (`/pref /avatar /status /autoaway /monitor /ignore
/mute /highlight`) are in-memory for this phase; disk persistence lands later.
`/list` is a real overlay with streaming rows, a fuzzy filter, a users
descending sort, and Enter to join. Do not claim what has not landed:

- The Connect sheet is in-memory only: a plain `launch` shows first-run
  Connect, but there is no profile store, so nothing survives a restart
  (phase 11). There is no credential/keychain store and no Disconnect proof in
  a fence yet.
- The chord map is real: `Ctrl+K` jump, `Alt+Down`/`Alt+Up` walk,
  `Alt+Left`/`Alt+Right` network walk, per-network and collapse-all collapse,
  network reorder, `Alt+A` unread, `Ctrl+`` Status, `Ctrl+,` Connect (with a
  bound connection), `Ctrl+Shift+K` nick jump (which opens or creates a DM),
  `Ctrl+Shift+P` member focus with member paging, `Ctrl+Shift+S` server-list
  collapse, `Ctrl+W` close, `Ctrl+F` find, `Ctrl+C` copy, transcript and
  member paging, Tab nick complete, history, drafts, and send/receive.
- The `Ctrl+Shift+O` link sheet and `Ctrl+Shift+A` inbox sheet open, walk, and
  close with real chords and modal gating, but their backing data stores are
  empty: URL extraction lands in phase 10 and `InboxStore` in phase 9.
- No typing indicators, presence-driven navigation, notifications, avatars,
  or preferences beyond the Connect sheet's in-memory toggles. They land in
  later phases.

There is no Xvfb, `xdotool`, or X display involved, and never attach to a Qt
instance. If a mapped feature needs a later phase, report the unmet
precondition; do not invent a proof.

## Cleanup

```sh
.cursor/skills/verify-omairc-tui/control-omairc-tui cleanup
```

Kills only the daemon from the state file and removes the state file. It does
not remove `test-artifacts/verify-tui/`.

## Helpers

`control-omairc-tui` is a thin POSIX `sh` wrapper around
`tui/bin/control-omairc-tui`, which `tui/bin/build` builds. The wrapper builds
only when the driver binary is missing, then execs it from the repo root.
There is no display to install; nothing else is required.
