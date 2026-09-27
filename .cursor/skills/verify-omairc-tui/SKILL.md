---
name: verify-omairc-tui
description: Drive the Omairc TUI (omairc-tui) in a PTY with control-omairc-tui. Use when proving the seeded demo shell, the window title, the sidebar, the transcript, or the composer in the Go terminal client.
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

In phase 4 the only launch that reaches a shell is `launch --demo-server`: a
plain `launch` (no `--demo-server`) runs a no-argument `omairc-tui`, which
exits non-zero because the Connect sheet (and the live IRC path) does not land
until phase 5. The PTY defaults to `118x30` (columns x rows) to match the Qt
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
- On the seeded demo, both networks resolve to host `irc.example`, so their
  display names are `irc.example · fred` and `irc.example · oak`. `#omarchy`
  exists on both networks, so its title carries the display name; `#ricing`
  and `dax` do not.

`title` prints the current title. `wait-title --exact TEXT` polls until the
title matches exactly and fails on a miss.

## Drive

The gate is keyboard-first. It REJECTS pixel/mouse verbs (`click-*`) and
`compare`, `qml-suite`, and `doctor-qml` in a recipe: `omairc-tui` has no
pointer path and no QML suite. Phase 4 keeps only `Ctrl+Q` / `Ctrl+C` quit and
`Enter`-to-send; the rest of the chord map lands in later phases (see Limits).

Use `run <feature>` instead of assembling raw `key` / `type` chords. Play the
verbs directly when a feature has no fence yet:

```sh
.cursor/skills/verify-omairc-tui/control-omairc-tui screenshot --feature send-message --name before-send
.cursor/skills/verify-omairc-tui/control-omairc-tui type --text "Hello from verify"
.cursor/skills/verify-omairc-tui/control-omairc-tui key --key return
.cursor/skills/verify-omairc-tui/control-omairc-tui send --text "Hello from verify"
# walk and unread are accepted but NO-OPS in the phase-4 shell (phase 6 chords):
.cursor/skills/verify-omairc-tui/control-omairc-tui walk --down --times 2
.cursor/skills/verify-omairc-tui/control-omairc-tui unread
```

Verb table:

| Verb | Meaning |
|---|---|
| `launch [--demo-server] [--size 118x30]` | Start the detached PTY daemon and the shell. Phase 4 needs `--demo-server` to seed a world; a plain `launch` exits the child non-zero. `--size` is columns x rows; default `118x30`. |
| `doctor` | Print `ok omairc-tui` plus `binary=`, `pid=`, `title=`, `size=`, `demo=`. |
| `title` | Print the current OSC 2 title. |
| `text` | Print the reconstructed grid text (the rows actually on screen). |
| `wait-title --exact TEXT` | Poll until the title equals `TEXT`; fail on a miss. |
| `key --key NAME` | Write one key chord (for example `ctrl+q`, `ctrl+c`, `return`, `escape`). |
| `type --text TEXT` | Type literal text into the focused input. |
| `send --text TEXT` | Write `TEXT` into the focused composer, then press `Enter`. The composer already holds focus in phase 4. |
| `walk --down` / `walk --up [--times N]` | Accepted, but a NO-OP in the phase-4 shell: it writes `Alt+Down` / `Alt+Up` `--times N` (default 1), and the chord map lands in phase 6. |
| `unread` | Accepted, but a NO-OP in the phase-4 shell: it writes `Alt+A`, and the chord map lands in phase 6. |
| `screenshot [--feature NAME] [--name NAME]` | Render the current grid to a PNG. Default feature `shell`, default name `screenshot`. |
| `run <feature> [--dry-run]` | Replay the feature's `desktop-recipe` fence. `--dry-run` prints it and exits. |
| `cleanup` | Kill the daemon and the PTY; removes the state file. Keeps evidence. |
| `--help` | Print the verb list. |

Key names are case-sensitive Bubble Tea key names, not `xdotool` keysyms.
Accepted names are `ctrl+q`, `ctrl+c`, `ctrl+l`, `ctrl+slash`, `enter` /
`Return` / `return`, `tab`, `space`, `backspace`, `Escape` / `escape`, `Up` /
`Down` / `Left` / `Right` / `up` / `down` / `left` / `right`, `Page_Up` /
`Page_Down` / `page_up` / `page_down`, `Home` / `End` / `home` / `end`,
`alt+Down` / `alt+Up` / `alt+Right` / `alt+Left`, any `alt+<letter>` /
`alt+<Letter>`, any `ctrl+<letter>`, and `shift+Tab`. An unknown name fails
the verb with `unknown key "NAME"` instead of sending nothing.

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
whose verb the TUI does not support yet (for example `jump`, `composer`,
`compare`, or any `click-*`) FAILS the run rather than silently skipping, so a
fence that moves on to later-phase chords is expected to fail until that phase
lands. Do not soften a fence to make it pass.

## Evidence

PNG screenshots and the reconstructed grid text land under
`test-artifacts/verify-tui/<feature>/` relative to the repo root. Keep them.
The screenshots are rendered from the VT text grid, so they show what the
terminal actually contained, not a mock. Capture the action and the resulting
state: a final screenshot alone is not proof.

## Limits

Phase 4 is the shell only. It does render all three columns: the sidebar
(network sections with presence marks and typing ellipses), the transcript,
and, for channels, a member column with status and away text. Do not claim
what has not landed:

- No Connect sheet. It lands in phase 5; a plain `launch` (no `--demo-server`)
  exits non-zero rather than showing a shell.
- No slash commands. They land in phase 7.
- No full keyboard map. It lands in phase 6; phase 4 keeps only `Ctrl+Q` /
  `Ctrl+C` quit and `Enter`-to-send.
- No avatars, links, notifications, preferences, or a Connect sheet yet.

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
