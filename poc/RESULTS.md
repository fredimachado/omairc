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
| `--lines N` | When `N > 0`, append a generated `#bench` conversation with `N` messages to each network and open that channel on network 0. |
| `--bench` | Run automated bench (stdout on iced, stderr + `poc/results/gpui-bench-stderr.txt` on GPUI). |

Bench invocation used for timing rows in the table:

```sh
# iced — prints metrics on stdout then exits
omairc-poc-iced --lines 50000 --bench --theme poc/mock/themes/dark.toml

# GPUI — prints metrics on stderr and appends to poc/results/gpui-bench-stderr.txt; process stays open (stop after summary lines)
omairc-poc-gpui --lines 50000 --bench --theme poc/mock/themes/dark.toml
```

## Shared tests

```sh
cargo test -p shared --manifest-path poc/Cargo.toml
```

**39** unit tests passed; **1** doc-test ignored (`theme` example).

## Metrics (release, `--lines 50000 --bench`, dark theme, `#bench` open)

The first bench table in commit `81007b5` measured **`#omarchy`** (~17 transcript rows) because neither shell selected `#bench` after `--lines`. Those numbers are **withdrawn**.

| | First frame | Scroll | Reflow (resize) | Append latency | Idle RSS † | Idle CPU † |
|---|-------------|--------|-----------------|----------------|------------|------------|
| **iced** | 517.09 ms | 62.5 fps | 87.51 ms | 5.96 ms | 163.8 MiB | 0.78 % |
| **GPUI** | 72.53 ms | 41.7 fps (200 frames) | 301.02 ms | 1.62 ms | 68.1 MiB | 1.55 % |

**Transcript open during scroll:** `#bench` on network 0 with **50 000** rows (GPUI log; iced reports **50 001** on stdout after the bench append adds one row).

**Scroll method:** **Iced** — **200** steps at ~16 ms; anchor `step × rows / 200` across the open transcript. **GPUI** — same **200** step sweep (`step × rows / 200`) with **16 ms** between steps (replacing the old per-row + 8 ms loop).

Idle RSS/CPU †: release binary with **only** `--theme poc/mock/themes/dark.toml` (no `--lines`) on the **short demo window** (`#omarchy`). After ~3 s for the window to appear, `Get-Process` sampled `WorkingSet64` and `CPU` twice **2 s** apart; CPU % = ΔCPU seconds ÷ wall time. Process names: `omairc-poc-iced`, `omairc-poc-gpui`. Not remeasured on this pass.

Raw bench output: `poc/results/iced-bench-out.txt`, `poc/results/gpui-bench-stderr.txt`.

## What `--bench` actually does

**Iced** (`poc/iced-shell/src/app.rs`):

1. With `--lines N > 0`, open `#bench` on network 0 before the first frame.
2. Wait for first frame, then run **200** scroll steps. Each step unpins and sets scroll anchor to `step * rows / 200`.
3. Bump text scale by 5% and resize the window to 1020×680; reflow time is taken on `WindowResized`.
4. Set composer text to trigger append; measure time until append completes.
5. Print summary on **stdout** (including transcript row count) and exit (`ExitBench`).

**GPUI** (`poc/gpui-shell/src/main.rs`):

1. With `--lines N > 0`, open `#bench` on network 0 in `Shell::new`.
2. After 500 ms, run **200** scroll steps with **16 ms** pacing; each step scrolls to `step * rows / 200`.
3. Resize window to 960×640; after 300 ms, record reflow duration.
4. Set composer to `bench ping` and call `send_message`.
5. Print summary on **stderr** and append lines to `poc/results/gpui-bench-stderr.txt`; the app **does not exit**.

## Screenshots

Captured with the idle window focused: `Get-Process` → `MainWindowHandle`, `SetForegroundWindow`, then **ffmpeg** `gdigrab` of the window rectangle. Files:

- `poc/results/iced-dark.png`, `poc/results/gpui-dark.png`
- `poc/results/iced-light.png`, `poc/results/gpui-light.png`

Attempts that did **not** work here: PowerShell `Add-Type` + `System.Drawing` (type forwarded to `System.Drawing.Common`, not loaded); ImageMagick `import -window` (X11 delegate missing on this Windows build).

### Visual comparison to Qt (`test-artifacts/send-message.png`)

Both PoCs reproduce the **three-column** Omairc shape (sidebar, transcript, members) and load the bundled **iA Writer Mono** face. Compared to the Qt reference, the PoCs read flatter: fewer avatar treatments, simpler badges, and less polish on spacing and header hierarchy. The iced dark capture showed **overlapping/clipped header text** in the conversation column; GPUI’s dark capture looked closer to the reference layout but still lacks Qt’s member rail density and topic/subtitle styling. Light-theme shots confirm theme.toml switching works on both shells.

## Impressions (single session)

- **Startup:** Iced first bench frame ~517 ms on cold release run with 50 k rows loaded; GPUI first-frame metric ~73 ms with `#bench` open.
- **Scrolling:** Iced held ~62 fps over 200 steps on the long transcript; GPUI ~42 fps over the same step count (large list layout cost dominates).
- **Stability:** Post-bench wheel scrolling previously hit `RefCell already mutably borrowed` in `ListState::bounds_for_item`; transcript scroll measurement is now deferred to the next turn. **Wheel crash not re-tested** in this pass.
- **Qt parity:** Layout is recognizable; details (avatars, unread pills, composer chrome, member prefixes) still lag the production QML shell.

## Gaps observed while running

1. **Keyboard / selection checklist** not exercised in this pass (parent agent planned separately).
2. **`shared::http_link_ranges`** uses byte indices; `#bench` day dividers with U+2014 panic unless bodies are normalized in the shells (current workaround).

No toolkit winner is implied by these numbers; methodology differences and idle-window RSS still limit cross-shell comparison.
