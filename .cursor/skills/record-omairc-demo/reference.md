# Demo GIF reference

## Demo binary vs live client

| | Live | Demo |
|---|---|---|
| Binary | `/usr/bin/omairc` | `$ROOT/build/omairc --demo-server` |
| Title | real network names | contains `irc.example` |
| Config | `~/.config/omairc` | temp XDG **only if `XDG_CONFIG_HOME` is unset** |
| Socket | default user socket | `OMAIRC_ALLOW_MULTI=1` so it does not steal it |

`src/main.cpp` isolates XDG for `--demo-server` only when `XDG_CONFIG_HOME` is empty. Omarchy exports `XDG_CONFIG_HOME=$HOME/.config`, so the launch line must be:

```sh
env -u XDG_CONFIG_HOME OMAIRC_ALLOW_MULTI=1 ./build/omairc --demo-server
```

Theme still comes from `$HOME/.local/state/omarchy/current/theme/colors.toml` (home is unchanged). Window default size in QML is 1180×760; on a live workspace it is a normal Hyprland client.

After a leak, demo `omarchy` / `oftc` blocks appeared in `~/.config/omairc/omairc.conf`. `record-demo` fails if those stanzas appear after a run. Do not delete the user's other networks.

## Why wtype is invisible

Omakeycast (`io.github.fredimachado.omakeycast`) in evdev-live mode reads `/dev/input/event*` and skips devices named `hl-virtual-keyboard`. `wtype` and similar virtual keyboards never overlay.

Write `input_event` structs to a real keyboard (`AT Translated Set 2 keyboard`, `/dev/input/event3` on this laptop). Requires group `input`:

```sh
sudo usermod -aG input "$USER"   # then log out
```

Bare keys and Shift-only typing do not overlay. Super/Ctrl/Alt chords do. Do not use `omarchy-shell omakeycast combo` unless the user wants a fake HUD.

Current overlay in `~/.config/omarchy/shell.json`: duration 2s, bottom-right, fontSize 36. Do not change those unless asked.

Injector CLI:

```sh
python3 scripts/inject-evdev.py chord ctrl+slash
python3 scripts/inject-evdev.py chord alt+down
python3 scripts/inject-evdev.py chord ctrl+k
python3 scripts/inject-evdev.py chord ctrl+shift+m
python3 scripts/inject-evdev.py chord ctrl+l
python3 scripts/inject-evdev.py chord escape
python3 scripts/inject-evdev.py type --delay 0.08 "omarchy"
```

The injector only auto-picks a device whose name contains `keyboard`, and fails
rather than guessing at some other event node. Override with
`--device /dev/input/eventN` or `OMAIRC_EVDEV_DEVICE`.

If a recording shows bare keys but no Super/Ctrl/Alt overlays, the auto-picked
keyboard may not be the one Omakeycast is watching (e.g. an integrated or USB
keyboard node rather than the AT keyboard). List candidates with
`ls -l /dev/input/event*` (or their names via `inject-evdev.py`'s pick) and
pass `--device` for the correct node.

## Isolated Xvfb is the wrong path

`verify-omairc` / `control-omairc` records 1180×760 on a private X display. It cannot show the Omarchy bar, workspace 1, or Omakeycast. Use it for feature proof, not this GIF.

## Monitor setup

Do not change mode or scale from this skill. Historical one-off (restored with `hyprctl reload`):

```sh
hyprctl eval 'hl.monitor({ output = "eDP-1", mode = "1280x800@60", position = "0x0", scale = 1 })'
```

Last native capture was eDP-1 **3840×2160 scale 3**. 16:9 maps exactly to 1600×900. Non-16:9 sources are padded with black by `record-demo`.

## Encode history

| Take | Capture | GIF | Notes |
|------|---------|-----|--------|
| Isolated Xvfb | 1180×760 | 800×515, 10fps, 128 colors, dither=none | First README GIF |
| Live 4K scale 2 | 3840×2160 | 960×540, 10fps, 128 colors | UI too small |
| Live 1280×800 | 1280×800 | 800×515, 10fps, 128 colors | Commit `310fb74` |
| Native HQ | 3840×2160 | 3840×2160, 15fps, 256 colors, floyd_steinberg | ~28MB, no downscale |
| 1080p floyd | native (do not change) | 1920×1080, 15fps, 256 colors, floyd_steinberg | ~10MB |
| 1080p none | native (do not change) | 1920×1080, 15fps, 256 colors, dither=none, rectangle | ~8MB; flat UI |
| **Current** | native (do not change) | **1600×900**, 15fps, 256 colors, dither=none, diff_mode=rectangle | rescale at encode only |

Old compact filter (do not use unless asked):

```sh
fps=10,scale=800:-1:flags=lanczos,split[s0][s1];[s0]palettegen=max_colors=128:stats_mode=diff[p];[s1][p]paletteuse=dither=none:diff_mode=rectangle
```

## Pointer hide

`hyprctl keyword` cannot set cursor options on this compositor. Hide with a runtime Lua config (not a file edit, not `hyprctl reload`):

```sh
hyprctl eval 'hl.config({ cursor = { invisible = true } })'
hyprctl eval 'hl.config({ cursor = { invisible = false } })'
```

`record-demo` saves the current `cursor:invisible` bool, hides after focusing the demo, and restores that bool in `restore()` and on success. A leftover invisible pointer after a crash is the same eval with `false`.

## Screenrecord

`omarchy screenrecord --fullscreen` then `--stop-recording`. Start toast lasts ~2s; the driver waits 2.4s. Stop prints the mp4 path when `OMARCHY_SCREENRECORD_DIR` is set. Hide the pointer **before** this starts.

## Seeded demo content

`IrcDemoServer` seeds in-process IRC. Walkthrough assumes:

- Start conversation `#omarchy` on `irc.example`, nick `fred`
- Next channel `#ricing` via `Alt+Down`
- Member `mira` completes from `mi`+Tab
- DM `anna` via jump (`Ctrl+K`)

If the seed changes, edit `drive_walkthrough` in `scripts/record-demo`.
