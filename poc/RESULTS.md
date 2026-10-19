# UI toolkit PoC — benchmark notes

Recorded on **Windows 10** (build 26200), **12th Gen Intel Core i9-12900K**. All numbers below are **single release runs** on this machine, not averaged lab measurements.

## Build and run

Shared mock data and transcript logic live in the workspace rooted at `poc/Cargo.toml`.

**Iced shell** (workspace member):

```sh
cargo build -p omairc-poc-iced --release --manifest-path poc/Cargo.toml
cargo run -p omairc-poc-iced --release --manifest-path poc/Cargo.toml -- --theme poc/mock/themes/dark.toml
```

Release binary: `poc/target/release/omairc-poc-iced` (`.exe` on Windows).

**GPUI shell** (separate crate tree under `poc/gpui-shell/`):

```sh
cargo build --release --manifest-path poc/gpui-shell/Cargo.toml
cargo run --release --manifest-path poc/gpui-shell/Cargo.toml -- --theme poc/mock/themes/dark.toml
```

Release binary: `poc/gpui-shell/target/release/omairc-poc-gpui` (`.exe` on Windows).

### CLI flags

| Flag | Meaning |
|------|---------|
| `--theme PATH` | Omarchy-style `colors.toml` (defaults exist; runs here used `poc/mock/themes/dark.toml` and `light.toml`). |
| `--scale F` | Text scale multiplier (default `1.0`; GPUI via clap, iced via manual parse). |
| `--lines N` | When `N > 0`, append a generated `#bench` conversation with `N` messages to each network (see gap below). |
| `--bench` | Run automated bench (stdout on iced, stderr on GPUI). |

Bench invocation used for timing rows in the table:

```sh
# iced — prints metrics on stdout then exits
omairc-poc-iced --lines 50000 --bench --theme poc/mock/themes/dark.toml

# GPUI — prints metrics on stderr; process stays open (stop after summary lines)
omairc-poc-gpui --lines 50000 --bench --theme poc/mock/themes/dark.toml
```

## Shared tests

```sh
cargo test -p shared --manifest-path poc/Cargo.toml
```

**39** unit tests passed; **1** doc-test ignored (`theme` example).

## Metrics (release, `--lines 50000 --bench`, dark theme)

| | First frame | Scroll | Reflow (resize) | Append latency | Idle RSS | Idle CPU |
|---|-------------|--------|-----------------|----------------|----------|----------|
| **iced** | 491.87 ms | 62.5 fps | 52.55 ms | 0.50 ms | 163.8 MiB | 0.78 % |
| **GPUI** | 0.86 ms † | 62.9 fps (18 frames) ‡ | 304.51 ms | 0.02 ms | 68.1 MiB | 1.55 % |

Idle RSS/CPU: release binary with **only** `--theme poc/mock/themes/dark.toml` (no `--lines`). After ~3 s for the window to appear, `Get-Process` sampled `WorkingSet64` and `CPU` twice **2 s** apart; CPU % = ΔCPU seconds ÷ wall time. Process names: `omairc-poc-iced`, `omairc-poc-gpui`.

† GPUI records first frame inside `Shell::render` against bench start; this run’s sub‑millisecond value did not match perceived startup and may not be comparable to iced’s first-frame line.

‡ GPUI scroll frame count equals `transcript.rows() + 1` (see methodology and gap).

Raw bench output: `poc/results/iced-bench-out.txt`, `poc/results/gpui-bench-stderr.txt`.

## What `--bench` actually does

The two shells **do not** scroll the transcript the same way.

**Iced** (`poc/iced-shell/src/app.rs`):

1. Wait for first frame, then run **200** scroll steps (not one step per row). Each step unpins and sets scroll anchor to `step * rows / 200`.
2. Bump text scale by 5% and resize the window to 1020×680; reflow time is taken on `WindowResized`.
3. Set composer text to trigger append; measure time until append completes.
4. Print summary on **stdout** and exit (`ExitBench`).

**GPUI** (`poc/gpui-shell/src/main.rs`):

1. After 500 ms, loop `step = 0..=row_count`, calling `list_state.scroll_to` once per step and waiting **8 ms** between steps (so large row counts take minutes).
2. Resize window to 960×640; after 300 ms, record reflow duration.
3. Set composer to `bench ping` and call `send_message`.
4. Print summary on **stderr**; the app **does not exit**.

On this machine, GPUI bench finished in a few seconds with **18** scroll frames because the active transcript had **17** rows (see gap). Iced still executed all **200** scroll steps against that same small transcript.

## Screenshots

Captured with the idle window focused: `Get-Process` → `MainWindowHandle`, `SetForegroundWindow`, then **ffmpeg** `gdigrab` of the window rectangle. Files:

- `poc/results/iced-dark.png`, `poc/results/gpui-dark.png`
- `poc/results/iced-light.png`, `poc/results/gpui-light.png`

Attempts that did **not** work here: PowerShell `Add-Type` + `System.Drawing` (type forwarded to `System.Drawing.Common`, not loaded); ImageMagick `import -window` (X11 delegate missing on this Windows build).

### Visual comparison to Qt (`test-artifacts/send-message.png`)

Both PoCs reproduce the **three-column** Omairc shape (sidebar, transcript, members) and load the bundled **iA Writer Mono** face. Compared to the Qt reference, the PoCs read flatter: fewer avatar treatments, simpler badges, and less polish on spacing and header hierarchy. The iced dark capture showed **overlapping/clipped header text** in the conversation column; GPUI’s dark capture looked closer to the reference layout but still lacks Qt’s member rail density and topic/subtitle styling. Light-theme shots confirm theme.toml switching works on both shells.

## Impressions (single session)

- **Startup:** Iced first bench frame ~492 ms on cold release run; GPUI window appeared quickly but bench first-frame metric was not trustworthy.
- **Scrolling:** Iced bench scroll felt smooth at ~62 fps over 200 synthetic steps; GPUI reported similar fps but only over 18 list steps in this run.
- **Stability:** After GPUI printed bench summary, moving the scroll wheel triggered `RefCell already mutably borrowed` in `gpui::elements::list::ListState::bounds_for_item` and the process **aborted** (see `gpui-bench-stderr.txt` backtrace).
- **Qt parity:** Layout is recognizable; details (avatars, unread pills, composer chrome, member prefixes) still lag the production QML shell.

## Gaps observed while running

1. **`--lines` does not select `#bench`.** Both shells append `bench_channel(N)` but keep the default conversation (`#omarchy`, index 0). The bench therefore ran on **17** mock messages (~17 transcript rows), not 50 000. Iced’s 200-step scroll still runs; GPUI’s per-row loop does not stress a large transcript until the active conversation switches to `#bench`.
2. **GPUI bench scroll fps** includes the fixed **8 ms** timer between steps, so fps is capped by that pacing as much as by layout cost.
3. **GPUI post-bench crash** on wheel input (borrow conflict in list state during transcript render).
4. **Keyboard / selection checklist** not exercised in this pass (parent agent planned separately).

No toolkit winner is implied by these numbers; methodology differences and the `#bench` selection gap limit cross-shell comparison.
