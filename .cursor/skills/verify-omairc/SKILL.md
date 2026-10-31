---
name: verify-omairc
description: Feature map and contracts for Omairc desktop and TUI behavior. Use verify-omairc-desktop for compiled GPUI proof or verify-omairc-tui for terminal proof; features/*.md lists shortcuts, preconditions, and observables.
metadata:
  internal: true
---

# Verify Omairc (feature map)

This directory owns the **feature map**: `features/*.md` describes how a user
reaches each behavior (shortcuts, preconditions, observables, and
`desktop-recipe` fences). That map is the shared contract for the Rust GPUI
desktop client and the TUI. Do not duplicate it in product code or CLI help.

## Which driver to use

| Surface | Skill | Control script | Binary |
|---|---|---|---|
| Rust GPUI desktop (product) | `.cursor/skills/verify-omairc-desktop` | `control-omairc-desktop` | `client/target/*/omairc` |
| Terminal UI | `.cursor/skills/verify-omairc-tui` | `control-omairc-tui` | `omairc-tui` |

**Desktop window proof** (title, screenshots, keyboard chords, isolated Xvfb)
always goes through **verify-omairc-desktop**. Read `features/README.md`, then drive with:

```sh
.cursor/skills/verify-omairc-desktop/control-omairc-desktop launch --demo-server
.cursor/skills/verify-omairc-desktop/control-omairc-desktop run <feature-id>
```

Evidence for the desktop client lives under `test-artifacts/verify-desktop/`.
`bin/verify-desktop-recipes` and `bin/test-desktop` replay the same fences at
release quality (`OMAIRC_DESKTOP_PROFILE=release`, `product` features).

The local CLI (`connections`, `status`, `send`, `raise`) is not a substitute
for mapped GUI proof.

## Map maintenance

- Add or edit behavior in `features/<id>.md`, not in a binary catalog.
- Keep `desktop-recipe` fences compatible with both `control-omairc-desktop` and
  `control-omairc-tui` where the behavior exists on both surfaces.
- Retired offscreen suites are not a fallback. The historical note lives in
  `features/README.md`.

For launch, doctor, chords, pointer verbs, and cleanup details, read
`.cursor/skills/verify-omairc-desktop/SKILL.md` in full.
