---
name: record-omairc-demo
description: Records the Omairc README demo on Omarchy workspace 1 from ./build/omairc --demo-server, injects real keyboard chords so Omakeycast overlays them, then encodes a looping 1600x900 GIF. Use when regenerating omairc.gif, recording the demo walkthrough, or changing the demo GIF script, timings, or key sequence.
---

# Record Omairc demo GIF

This is a **live Omarchy workspace recording**, not the isolated Xvfb path in `verify-omairc`. The README hero is `omairc.gif`. The GIF must be **1600×900** (1600-wide, 16:9).

Do not invent a new driver. Edit and run the scripts in this skill.

## Quick start

From the repo root, with Hyprland already at the resolution and scale the user wants:

```sh
.cursor/skills/record-omairc-demo/scripts/record-demo
```

That switches to workspace 1, launches a private `--demo-server` window, records fullscreen, drives the walkthrough, restores workspace 2 and Cursor, then writes `omairc.gif` at 1600×900.

Re-encode an existing capture without recording:

```sh
.cursor/skills/record-omairc-demo/scripts/record-demo --encode-only /path/to/screenrecording.mp4
```

Change the clip by editing `drive_walkthrough` in [scripts/record-demo](scripts/record-demo). Encode knobs (`GIF_WIDTH`, `GIF_HEIGHT`, `GIF_FPS`, `GIF_DITHER`, `GIF_DIFF_MODE`) live at the top of that script.

## Hard rules

- Drive **only** `$ROOT/build/omairc --demo-server`. Never `/usr/bin/omairc` or any window whose title lacks `irc.example`.
- Launch with `env -u XDG_CONFIG_HOME OMAIRC_ALLOW_MULTI=1`. `--demo-server` isolates XDG **only when `XDG_CONFIG_HOME` is unset**. On Omarchy it is set to `~/.config`; leaving it set writes demo `omarchy` / `oftc` profiles into the user's real `omairc.conf`.
- Record on **workspace 1**. Leave the user's live Omairc and Cursor on workspace 2.
- Do **not** change Hyprland monitor mode or scale. The user sets that up.
- Do **not** use `wtype`, `ydotool`, or `control-omairc` / `xdotool` on the session `$DISPLAY`. Omakeycast's evdev-live mode skips `hl-virtual-keyboard`, so those chords never overlay.
- Inject keys with [scripts/inject-evdev.py](scripts/inject-evdev.py) into a real keyboard (`/dev/input/event*`). The user must be in group `input`.
- Do **not** fake overlays with `omarchy-shell omakeycast combo` unless the user asks.
- Focus the demo window **before every key**. Injected keys go to whatever is focused.
- Always `trap` cleanup: stop the recorder, **unhide the pointer**, kill **only** the demo PID, focus workspace 2 and `class:cursor`. Never `hyprctl reload` (that is only for a monitor change, which this skill does not do).

## Hyprland (Lua compositor)

Classic `hyprctl dispatch workspace 1` fails here. Use:

```sh
hyprctl dispatch 'hl.dsp.focus({ workspace = "1" })'
hyprctl dispatch "hl.dsp.focus({ window = \"pid:$DEMO_PID\" })"
hyprctl dispatch 'hl.dsp.focus({ window = "class:cursor" })'
hyprctl dispatch 'hl.dsp.cursor.move_to_corner({ corner = "topleft" })'
hyprctl eval 'hl.config({ cursor = { invisible = true } })'
# restore the saved bool (usually false) in cleanup:
hyprctl eval 'hl.config({ cursor = { invisible = false } })'
```

`hyprctl keyword cursor:invisible` fails on this Lua compositor. Hide after focusing the demo and before `omarchy screenrecord --fullscreen`. Unhide in `restore()` and on the success path so a crash or Ctrl-C cannot leave the pointer invisible. `--encode-only` must not touch cursor visibility.

Find the demo client in `hyprctl clients -j` by the pid `record-demo` spawned,
then assert its title contains `irc.example` (example:
`#omarchy · irc.example · fred - Omairc`). Do not select it by title alone; the
pid is what guarantees we never drive or kill the user's live client.

## Walkthrough

Seeded start: `#omarchy`, members visible, nick `fred`. `drive_walkthrough` is
the source of truth for timing; every pause there is explicit. The beats that
landed:

1. Hold on `#omarchy` (~1.2s after the screenrecord toast).
2. `Ctrl+/` shortcuts sheet → wait ~1.9s → Escape.
3. `Alt+Down` → `#ricing` (~1.4s).
4. `Ctrl+K`, type `omarchy` (80ms/char), Return.
5. `Ctrl+Shift+M` hide members (~1.2s).
6. `Ctrl+L`, type `mi`, Tab, type ` optional it is.`, Return.
7. `Ctrl+K`, type `anna`, Return (DM).

Omakeycast shows Super/Ctrl/Alt chords (`Ctrl + /`, `Alt + Down`, `Ctrl + K`, `Ctrl + Shift + M`, `Ctrl + L`). Bare keys (typing, Escape, Tab, Return) stay hidden by design.

Wait ~2.4s after `omarchy screenrecord --fullscreen` so the start toast is gone before step 1.

## Recording and encode

```sh
OMARCHY_SCREENRECORD_DIR="$OUTDIR" omarchy screenrecord --fullscreen
# …walkthrough…
OMARCHY_SCREENRECORD_DIR="$OUTDIR" omarchy screenrecord --stop-recording
```

Without `OMARCHY_SCREENRECORD_DIR`, the mp4 lands in Videos. Always point it at a temp dir.

Final GIF is 1600×900, 15 fps, full 256-color palette, no dither, rectangle diffs:

```sh
ffmpeg -y -i "$REC" \
  -vf "fps=15,scale=1600:900:flags=lanczos:force_original_aspect_ratio=decrease,pad=1600:900:(ow-iw)/2:(oh-ih)/2:color=black,split[s0][s1];[s0]palettegen=stats_mode=full[p];[s1][p]paletteuse=dither=none:diff_mode=rectangle" \
  -loop 0 omairc.gif
```

Do not crush to 128 colors unless the user asks. Capture stays native; only the GIF is rescaled. Flat UI keeps `dither=none`; Floyd–Steinberg costs size for little gain here.

## Agent checklist

- [ ] Binary is `./build/omairc` (run `bin/build` if missing). Do not rebuild unless needed.
- [ ] User is on Hyprland/Omarchy; `ffmpeg`, `hyprctl`, `omarchy`, `python3` on `PATH`.
- [ ] `id -nG` includes `input`.
- [ ] Omakeycast plugin enabled (`io.github.fredimachado.omakeycast` in `~/.config/omarchy/shell.json`).
- [ ] Run `scripts/record-demo` (or `--encode-only`). Do not drive keys by hand unless debugging.
- [ ] After restore: pointer visible again, workspace 2, Cursor focused, demo PID gone, no `omarchy`/`oftc` stanzas leaked into `~/.config/omairc/omairc.conf`.
- [ ] `identify omairc.gif` (or `ffprobe`) shows **1600×900**. Copy into the repo root if the script did not already.

## Additional resources

- Encode history, XDG leak, and why `wtype` fails: [reference.md](reference.md)
- Driver and timings: [scripts/record-demo](scripts/record-demo)
- Evdev injector: [scripts/inject-evdev.py](scripts/inject-evdev.py)
