---
name: verify-omairc-desktop
description: Drive the Omairc Rust GPUI product client as a user would (isolated Xvfb on Linux, or control-omairc-desktop.ps1 on Windows, against the compiled binary). Use when proving the seeded demo shell, window title, conversation switching, Status console, composer, or keyboard chords against client/target/*/omairc(.exe).
metadata:
  internal: true
---

# Verify Omairc desktop (Rust GPUI)

The **product desktop client** lives in `client/`. It is a GPUI app shipped as
the `omairc` binary (`product` is the default Cargo feature for install
identity; `dev` uses a separate `omairc-dev` folder for local runs). This
skill is the official black-box driver for that window, not a throwaway harness.

`--demo-server` seeds the same two-network demo world as the retired Qt client.
Drive the compiled binary under an isolated X server with `xdotool`. The shared
feature map `.cursor/skills/verify-omairc/features/*.md` is the contract. Do not
point the legacy Qt `control-omairc` script at this binary, and do not invent
a new control protocol. In-process `#[gpui::test]` input is a separate path; it
does not press keys into a real window.

## Launch

Prefer a private X server when `Xvfb`, `xauth`, `xdotool`, `import`, `magick`,
`cargo`, and lavapipe (`vulkan-swrast`) are available. Do not launch on the
user's live display.

```sh
.cursor/skills/verify-omairc-desktop/control-omairc-desktop launch
.cursor/skills/verify-omairc-desktop/control-omairc-desktop launch --demo-server
```

Ready when stdout includes a title ending in ` - Omairc` or ` Status`, and
`doctor` exits 0. Default launch is first-run Connect
(`chat.freenode.net Status`). `--demo-server` is
`#omarchy · irc.example · fred - Omairc` with the seeded sidebar.

**Agents always use the default (Xvfb).** Do not pass `--visible` from a
recipe or an agent drive. That flag is only for a human who wants to watch.

This launch:

- runs `cargo build` in `client/` (debug profile by default). Set
  `OMAIRC_DESKTOP_PROFILE=release` and optionally `OMAIRC_DESKTOP_FEATURES=product`
  (as `bin/test-desktop` does) for the packager binary under
  `client/target/release/omairc`. Default Cargo features already include
  `product`; pass `--features dev` only when you intentionally need the
  `omairc-dev` home folder.
- picks the next free X display from `:110`
- uses a disposable `XDG_*` tree so window geometry is the default 1180×760
- points `DBUS_SESSION_BUS_ADDRESS` at a missing socket
- sets `OMAIRC_ALLOW_MULTI=1` so the isolated window does not take the user socket
- **unsets `WAYLAND_DISPLAY` / `WAYLAND_SOCKET`** and sets `XDG_SESSION_TYPE=x11`
  (GPUI prefers Wayland whenever `WAYLAND_DISPLAY` is set; without this the
  window opens on the live session and nested-X `xdotool` never sees it)
- forces lavapipe via `VK_ICD_FILENAMES` (nested X has no GPU; Blade needs a
  software Vulkan ICD). Override discovery with `OMAIRC_LAVAPIPE_ICD=/path/to/lvp_icd.json`
  when the system package is not installed
- passes `--demo-server` when requested
- writes run state to `/tmp/omairc-verify-desktop-$USER/state`

Never attach to a window you did not start. Never send `xdotool` to the
session `$DISPLAY`.

### Watching a run (`--visible`)

When *you* want to see the driven window on your desktop, launch with
`--visible`. That starts **Xephyr** (a nested X server) as a normal window
on your session; the client and `xdotool` still talk only to that nested
display. Control verbs are unchanged.

```sh
.cursor/skills/verify-omairc-desktop/control-omairc-desktop launch --demo-server --visible
.cursor/skills/verify-omairc-desktop/control-omairc-desktop run switch-conversation
```

Needs `xorg-server-xephyr` and a host `DISPLAY` (XWayland on Omarchy).
On Hyprland the host window is floated and centered under the reserved
top bar at 1600×1000 logical pixels. The nested framebuffer matches the
physical size (logical × host scale), the client is launched with matching
`--scale`, and its window is resized to fill Xephyr so fonts stay
readable on HiDPI.
`doctor` reports `visible=yes` and `xserver=Xephyr`. Cleanup closes the
Xephyr window. Do not put `--visible` in `desktop-recipe` fences.

Teardown:

```sh
.cursor/skills/verify-omairc-desktop/control-omairc-desktop cleanup
```

Keep `test-artifacts/verify-desktop/`.

## Doctor

```sh
.cursor/skills/verify-omairc-desktop/control-omairc-desktop doctor
```

Require all of:

- `ok isolated Omairc`
- `binary=` is `$ROOT/client/target/debug/omairc` (or `release`)
- `display=` is the nested X server from launch, not the user's session
- `app_pid=` and `xserver_pid=` are alive
- `xserver=` is `Xvfb` (default) or `Xephyr` (`--visible`)
- `visible=no` for agent launches; `visible=yes` only after `--visible`
- `title=` ends with ` - Omairc` or ` Status`
- `lavapipe=` is a real `lvp_icd*.json` path
- `xdg=` is the disposable state directory from this run
- `demo=yes` after `launch --demo-server`, otherwise `demo=no`

If doctor fails, cleanup, then launch again. Do not continue against a shared
or stale instance.

On Arch/Omarchy, install lavapipe with `vulkan-swrast` when doctor reports it
missing. Without it the process panics on `Failed to create surface` under
Xvfb.

## Windows

On Windows, use the PowerShell driver (pwsh preferred; Windows PowerShell 5.1
also works). There is no private display: the client runs on the same session
and desktop as the agent, with a disposable `LOCALAPPDATA` tree.

```powershell
pwsh .cursor/skills/verify-omairc-desktop/control-omairc-desktop.ps1 launch --demo-server
```

Isolation for that launch:

- overrides `LOCALAPPDATA` under a temp state directory (seeded
  `[migration] qtRegistryImported=true` so Connect geometry is controlled)
- sets `OMAIRC_ALLOW_MULTI=1` so the isolated window does not take the user
  named pipe
- sets `OMAIRC_SKIP_NOTIFICATIONS=1`
- default placement is off-screen with the taskbar button hidden; the process
  still shares the interactive desktop

**Agents always use the default (hidden).** Do not pass `--visible` from a
recipe or an agent drive. That flag is only for a human who wants to watch: it
centers the window on the primary work area. Never put `--visible` in
`desktop-recipe` fences.

At launch the driver briefly activates the window (so demo attention titles do
not stick), then restores the caller's foreground. Windows often clamps the
seeded off-screen geometry, so a short on-screen flash before off-screen
placement is expected.

`doctor` on Windows reports `ok isolated Omairc`, `binary=`, `app_pid=`,
`window=`, `title=`, `localappdata=`, `evidence=`, `demo=`, `visible=`, and
`placement=` (`offscreen` or `onscreen`). It does not report `display=`,
`xserver=`, or `lavapipe=`.

Requirements: `pwsh` (or Windows PowerShell 5.1), ImageMagick `magick` on
`PATH`, and a cargo/MSVC toolchain for `launch`.

Replay every feature with a `desktop-recipe` fence:

```powershell
pwsh bin\verify-desktop-recipes.ps1
```

Evidence stays in `test-artifacts/verify-desktop/`. Cleanup does not remove it.

## Drive

Use `control-omairc-desktop` against the isolated window. Shortcuts and the
composer first. Pointer verbs use an XTEST/XI2 click path (below).

Agents should `launch` (if needed) then `run <feature>` instead of assembling
raw `key --key` chords. `run <feature>` plays the same
`## Driving it with control-omairc` / `desktop-recipe` fence in
`.cursor/skills/verify-omairc/features/<feature>.md` that the TUI uses.
`run <feature> --dry-run` prints the fence and exits without launching.
`bin/verify-desktop-recipes` (Linux) and `bin/verify-desktop-recipes.ps1`
(Windows) run every feature with a `desktop-recipe` fence and keep evidence
in `test-artifacts/verify-desktop/`.

`run` skips a fence `launch` when `doctor` is already healthy and the demo
flag matches. A healthy instance in the other mode is an error. Any command
failure stops the recipe. Fence paths under `test-artifacts/verify/` are
remapped to `test-artifacts/verify-desktop/` for `compare`.

Named chord verbs wrap the same keysym path as `key` (including the
`Control_L+k` rewrite):

| Handle | Meaning |
|---|---|
| Window title `{name} - Omairc` | Current conversation. Duplicate names use `{name} · {displayName} - Omairc`. |
| `jump --query TEXT` | `Ctrl+K`, wait, type, Return. |
| `walk --down` / `walk --up` | `Alt+Down` / `Alt+Up`. `--times N` repeats. |
| `unread` | `Alt+A`. |
| `toggle-members` | People-pill click on a channel (XI2); `Ctrl+Shift+M` on a DM (no-op). |
| `focus-members` | `Ctrl+Shift+P`. |
| `nick-jump --query TEXT` | `Ctrl+Shift+K`, wait, type, Return. |
| `status` | `Ctrl+grave`. |
| `connect` | `Ctrl+,`. |
| `composer` / `focus-composer` | `Ctrl+L`, then wait before a later `type`. |
| `send --text "..."` | Focus the composer, type, `Enter`. |

Chords without a named verb still use `key --key` (same map as the retired Qt
driver: `ctrl+shift+s`, `ctrl+w`, `ctrl+slash`,
`ctrl+shift+slash` for About, network walk / collapse, paging, Tab,
history, `ctrl+q`, …).

### Pointer verbs (XI2 via XTEST)

GPUI on X11 reads keyboard input from core `KeyPress` events, so
`xdotool key --window` works. Pointer input is XInput2 only.
`xdotool click --window WID` uses XSendEvent (core) and GPUI drops it.

Pointer verbs instead:

1. `xdotool mousemove --sync --window WID x y` (window-relative move)
2. Confirm `Virtual core XTEST pointer` exists via `xinput list`
3. `xdotool click --window 0 1` (XTEST button events delivered as XI2)

Named pixel clicks assume the isolated 1180×760 window at textScale 1.0
(same coordinates as the retired Qt driver). Prefer chord verbs when a recipe
has one. `click-version` also sends `Ctrl+Shift+/` so About opens when the
footer hit misses under lavapipe. For SEND, prefer `send --text` / Enter.

Typical drive:

```sh
.cursor/skills/verify-omairc-desktop/control-omairc-desktop launch --demo-server
.cursor/skills/verify-omairc-desktop/control-omairc-desktop run switch-conversation
```

## Evidence

Proof lives in `test-artifacts/verify-desktop/<feature-id>/`. Cleanup must not
delete it.

Standards match the feature map: exercise the real window with shortcuts and
the composer; capture action and result; window title is conversation
identity; `compare` requires a visible pixel change (AE > 100).

## Cleanup

```sh
.cursor/skills/verify-omairc-desktop/control-omairc-desktop cleanup
```

Kills only the `APP_PID` and `XVFB_PID` from the state file, then removes the
disposable XDG/Xauth directory and the state file. It does not remove
`test-artifacts/verify-desktop/`.

## Helpers

`control-omairc-desktop` is executable. Invoke it from the repo root.

```text
launch [--demo-server] [--visible] | doctor | title | wait-title --exact TITLE
run FEATURE [--dry-run]
jump --query TEXT | nick-jump --query TEXT
walk --down | walk --up [--times N]
unread | toggle-members | focus-members | status | connect | composer
focus-composer | type --text TEXT | key --key KEY | send --text TEXT
click --x N --y N
click-conversation --name NAME | click-member --name NICK
click-people [--hidden] | click-network | click-edit | click-version | click-send
screenshot --feature ID --name STEM
compare --before PATH --after PATH
cleanup
```

If Xvfb tools are missing, install
`xorg-server-xvfb xorg-xauth xdotool xorg-xinput imagemagick`. If lavapipe
is missing, install `vulkan-swrast`. Do not invent a drive against the
user's session.
